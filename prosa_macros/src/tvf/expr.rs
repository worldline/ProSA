use crate::tvf::error;
use core::fmt;
use proc_macro2::{Span, TokenStream};
use syn::spanned::Spanned;

/// Define a field expression which includes the following elements
/// - a target field identifier
/// - a modifier (unary operator)
/// - a literal / variable / rust-expression
/// - a implicit or explicit target type
pub(crate) struct TvfExpr {
    /// Field identifier to use
    pub id: TvfId,

    /// Modifier for the value
    pub modifier: Modifier,

    /// Value to use for the field
    pub value: TvfValue,

    /// Output type to insert the field in the buffer
    pub explicit_type: Option<TvfType>,
}

impl TvfExpr {
    /// Analyze the expression to deduce the type of the field.
    /// If the type could not be deduced, then fail with an error.
    pub(crate) fn get_type(&self) -> Result<TvfType, syn::Error> {
        if let Some(explicit) = self.explicit_type {
            Ok(explicit)
        } else if let Some(deduced) = self.value.deduce_type() {
            Ok(deduced)
        } else {
            Err(error(
                self.value.span(),
                "Could not deduce field type for field",
            ))
        }
    }
}

/// Identifier of a field
pub(crate) enum TvfId {
    /// We directly have an integer value
    Int(usize),

    /// Rust expression (surrounded by parenthesis)
    Expr(TokenStream),
}

/// Define a value to insert into the buffer
pub(crate) enum TvfValue {
    /// Simple literal which can be used to implicitly identify the type of the value
    Lit(syn::Lit),

    /// Identifier of a variable
    Ident(syn::Ident),

    /// Format expression (format! macro)
    Format(TokenStream),

    /// Rust expression (surrounded by parenthesis)
    Expr(TokenStream),

    /// Sub-buffer
    Buffer(Vec<TvfExpr>, Span),
}

impl TvfValue {
    #[rustfmt::skip]
    pub(crate) fn span(&self) -> Span {
        match self {
            Self::Lit    (lit    ) => lit   .span(),
            Self::Ident  (ident  ) => ident .span(),
            Self::Format (stream ) => stream.span(),
            Self::Expr   (stream ) => stream.span(),
            Self::Buffer (_, span) => *span,
        }
    }

    /// Deduce the type of a TVF value is possible
    pub(crate) fn deduce_type(&self) -> Option<TvfType> {
        match self {
            Self::Lit(lit) => match lit {
                syn::Lit::Bool(_) | syn::Lit::Byte(_) | syn::Lit::Char(_) => Some(TvfType::Byte),
                syn::Lit::Int(int) => {
                    let suffix = int.suffix();
                    if suffix == "u8" {
                        Some(TvfType::Byte)
                    } else if suffix.starts_with('u') {
                        Some(TvfType::Unsigned)
                    } else {
                        Some(TvfType::Signed)
                    }
                }
                syn::Lit::Float(_) => Some(TvfType::Float),
                syn::Lit::Str(_) | syn::Lit::CStr(_) => Some(TvfType::String),
                syn::Lit::ByteStr(_) => Some(TvfType::Bytes),
                _ => None,
            },
            Self::Ident(ident) => {
                if ident == "false" || ident == "true" {
                    Some(TvfType::Byte)
                } else {
                    None
                }
            }
            Self::Format(_) => Some(TvfType::String),
            Self::Expr(_) => None,
            Self::Buffer(_, _) => Some(TvfType::Buffer),
        }
    }
}

/// The types that can be added to a TVF buffer
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::IntoStaticStr)]
#[strum(serialize_all = "lowercase")]
pub(crate) enum TvfType {
    Byte,
    Signed,
    Unsigned,
    Float,
    String,
    Bytes,
    Date,
    DateTime,
    Buffer,
}

impl fmt::Display for TvfType {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s: &'static str = self.into();
        f.write_str(s)
    }
}

/// Modifier for a value
/// Usually a not for boolean or a minus sign for numbers, etc..
#[repr(u8)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, strum::IntoStaticStr)]
pub(crate) enum Modifier {
    /// No operator
    #[default]
    #[strum(serialize = "")]
    None,

    /// + sign, results in no operator being added
    #[strum(serialize = "+")]
    Positive,

    /// - sign, negate the following value
    #[strum(serialize = "-")]
    Negative,

    /// logical not for boolean values
    #[strum(serialize = "!")]
    LogicalNot,

    /// * dereference
    #[strum(serialize = "*")]
    Dereference,

    /// & borrow
    #[strum(serialize = "&")]
    Borrow,
}

impl fmt::Display for Modifier {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s: &'static str = self.into();
        f.write_str(s)
    }
}

impl Modifier {
    /// Check if the modifier is intended for numerical/logical types (+, -, !)
    #[inline]
    pub(crate) fn is_numeric(self) -> bool {
        matches!(self, Self::LogicalNot | Self::Positive | Self::Negative)
    }

    /// Filter out numerical/logical operator (+, -, !)
    #[inline]
    pub(crate) fn filter_out_numeric(self) -> Self {
        if matches!(self, Self::Borrow | Self::Dereference) {
            self
        } else {
            Self::None
        }
    }
}

/// Simple sequence of bytes to serialize
#[derive(Debug, Default, Clone)]
pub(crate) struct Bytes(pub Vec<u8>);
