use chrono::{NaiveDate, NaiveDateTime};
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

/// Identifier of a field
pub(crate) enum TvfId {
    /// We directly have an integer value
    Int(usize),

    /// Identifier of a variable
    Ident(syn::Ident),

    /// Rust expression (surrounded by parenthesis)
    Expr(TokenStream),
}

/// Define a value to insert into the buffer
pub(crate) enum TvfValue {
    /// Simple literal which can be used to implicitely identify the type of the value
    Lit(syn::Lit),

    /// Identifier of a variable
    Ident(syn::Ident),

    /// Rust expression (surrounded by parenthesis)
    Expr(TokenStream),

    /// Sub-buffer
    Buffer(Vec<TvfExpr>, Span),
}

impl TvfValue {
    pub(crate) fn span(&self) -> Span {
        match self {
            TvfValue::Lit(lit) => lit.span(),
            TvfValue::Ident(ident) => ident.span(),
            TvfValue::Expr(stream) => stream.span(),
            TvfValue::Buffer(_, span) => *span,
        }
    }
}

/// The types that can be added to a TVF buffer
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

/// Modifier for a value
/// Usually a not for boolean or a minus sign for numbers, etc..
#[repr(u8)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Modifier {
    /// No operator
    #[default]
    None,

    /// + sign, results in no operator being added
    Positive,

    /// - sign, negate the following value
    Negative,

    /// logical not for boolean values
    LogicalNot,

    /// * dereference
    Dereference,

    /// & borrow
    Borrow,
}

/// Simple sequence of bytes to serialize
#[derive(Debug, Default, Clone)]
pub(crate) struct Bytes(pub Vec<u8>);

/// Values that have been identified
pub(crate) enum Value<'e> {
    Bool(bool),
    Byte(u8),
    Signed(i64),
    Unsigned(u64),
    Float(f64),
    String(String),
    Bytes(Bytes),
    Date(NaiveDate),
    DateTime(NaiveDateTime),
    Buffer(&'e [TvfExpr]),
}

impl<'e> Value<'e> {
    /// Process the TvfValue to identify its type
    #[rustfmt::skip]
    pub(crate) fn identify(&self) -> TvfType {
        match self {
            Self::Bool     (_) => TvfType::Byte,
            Self::Byte     (_) => TvfType::Byte,
            Self::Signed   (_) => TvfType::Signed,
            Self::Unsigned (_) => TvfType::Unsigned,
            Self::Float    (_) => TvfType::Float,
            Self::String   (_) => TvfType::String,
            Self::Bytes    (_) => TvfType::Bytes,
            Self::Date     (_) => TvfType::Date,
            Self::DateTime (_) => TvfType::DateTime,
            Self::Buffer   (_) => TvfType::Buffer,
        }
    }
}
