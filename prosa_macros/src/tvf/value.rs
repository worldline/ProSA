use super::buffer::{generate_list, generate_map};
use super::literal::{convert_literal, identify_literal};
use proc_macro2::{Delimiter, Punct, TokenStream, TokenTree};
use quote::{ToTokens, quote};
use syn::{Error, Ident};

/// The types that can be added to a TVF buffer
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ValueType {
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

impl From<&ValueType> for TokenStream {
    fn from(value: &ValueType) -> Self {
        match value {
            ValueType::Byte => quote![put_byte],
            ValueType::Signed => quote![put_signed],
            ValueType::Unsigned => quote![put_unsigned],
            ValueType::Float => quote![put_float],
            ValueType::String => quote![put_string],
            ValueType::Bytes => quote![put_bytes],
            ValueType::Date => quote![put_date],
            ValueType::DateTime => quote![put_datetime],
            ValueType::Buffer => quote![put_buffer],
        }
    }
}

/// Deduce the value type from the type provided in a `as` cast
impl TryFrom<Ident> for ValueType {
    type Error = Error;

    fn try_from(ident: Ident) -> Result<Self, Self::Error> {
        match ident.to_string().to_ascii_lowercase().as_str() {
            "byte" | "u8" => Ok(ValueType::Byte),
            "signed" | "i8" | "i16" | "i32" | "i64" | "isize" => Ok(ValueType::Signed),
            "unsigned" | "u16" | "u32" | "u64" | "usize" => Ok(ValueType::Unsigned),
            "float" | "f32" | "f64" => Ok(ValueType::Float),
            "string" => Ok(ValueType::String),
            "bytes" => Ok(ValueType::Bytes),
            "date" | "naivedate" => Ok(ValueType::Date),
            "datetime" | "naivedatetime" => Ok(ValueType::DateTime),
            "buffer" => Ok(ValueType::Buffer),
            _ => Err(Error::new_spanned(ident, "Invalid value type")),
        }
    }
}

/// At this point the token stream has been preparsed such that the value is:
/// - {} or []
/// - a single literal
/// - a single literal followed by a `as` cast
/// - a path followed by a `as` cast
/// - an expression surrounded by () and followed by a `as` cast
pub(crate) fn generate_value(
    buffer_type: &Ident,
    value_stream: &TokenStream,
) -> Result<(TokenStream, ValueType), Error> {
    // Process the token tree
    let mut tokens = value_stream.clone().into_iter().peekable();

    // Check if the first element is a sign (+ or -)
    let unary_op = if let Some(TokenTree::Punct(punct)) = tokens.peek() {
        let unary_op = UnaryOp::new(punct)?;
        tokens.next(); // move to next token
        unary_op
    } else {
        UnaryOp::None
    };

    // in all cases, we expect to find a value
    let value = tokens.next().ok_or(Error::new_spanned(
        value_stream,
        "Expected a value, none found.",
    ))?;

    // Check if the value is followed by an `as` cast
    let output_type = if let Some(TokenTree::Ident(ident)) = tokens.next() {
        if ident == "as" {
            // check if the value is followed by a type
            if let Some(TokenTree::Ident(ident)) = tokens.next() {
                Some(ValueType::try_from(ident)?)
            } else {
                return Err(Error::new_spanned(ident, "Expected a type identifier"));
            }
        } else {
            return Err(Error::new_spanned(ident, "Expected an `as` keyword"));
        }
    } else {
        None
    };

    // Raise an error if there are unexpected tokens
    if let Some(token) = tokens.next() {
        return Err(Error::new_spanned(token, "Unexpected token"));
    }

    let sign = unary_op.make_sign();

    // check the type of value provided
    match value {
        TokenTree::Literal(literal) => {
            if let Some(output_type) = output_type {
                Ok((
                    convert_literal(&literal, &output_type, unary_op)?,
                    output_type,
                ))
            } else {
                let literal_type = identify_literal(&literal)?;
                let token_stream = match literal_type {
                    ValueType::Byte => quote! [ #sign #literal as u8 ],
                    ValueType::Signed => quote! [ #sign #literal as i64 ],
                    ValueType::Unsigned => quote! [ #sign #literal as u64 ],
                    ValueType::Float => quote! [ #sign #literal as f64 ],
                    _ => literal.to_token_stream(),
                };
                Ok((token_stream, literal_type))
            }
        }
        TokenTree::Ident(ident) => {
            // check if the identifier is a boolean
            match ident.to_string().as_str() {
                "true" => {
                    let t = if unary_op == UnaryOp::LogicalNot {
                        quote![0u8]
                    } else {
                        quote![1u8]
                    };
                    Ok((t, output_type.unwrap_or(ValueType::Byte)))
                }
                "false" => {
                    let t = if unary_op == UnaryOp::LogicalNot {
                        quote![1u8]
                    } else {
                        quote![0u8]
                    };
                    Ok((t, output_type.unwrap_or(ValueType::Byte)))
                }
                _ => {
                    if let Some(output_type) = output_type {
                        Ok((quote![#sign #ident], output_type))
                    } else {
                        Err(Error::new_spanned(
                            ident,
                            "Could not deduce the type of the variable. Please use `as` cast.",
                        ))
                    }
                }
            }
        }
        // handle {}, [] and () expressions
        TokenTree::Group(group) => {
            match group.delimiter() {
                Delimiter::Brace => {
                    // if a `as` cast was used, verify that it is valid
                    if output_type.is_some_and(|t| t != ValueType::Buffer) {
                        return Err(Error::new_spanned(group, "Invalid type for `{}` value."));
                    }
                    Ok((generate_map(buffer_type, &group)?, ValueType::Buffer))
                }
                Delimiter::Bracket => {
                    // if a `as` cast was used, verify that it is valid
                    if output_type.is_some_and(|t| t != ValueType::Buffer) {
                        return Err(Error::new_spanned(group, "Invalid type for `[]` value."));
                    }
                    Ok((generate_list(buffer_type, &group)?, ValueType::Buffer))
                }
                _ => {
                    if let Some(output_type) = output_type {
                        Ok((quote![ #sign #group ], output_type))
                    } else {
                        Err(Error::new_spanned(
                            group,
                            "Type cannot be deduced from an expression in parenthesis. Please use `as` cast.",
                        ))
                    }
                }
            }
        }
        TokenTree::Punct(_) => Err(Error::new_spanned(value, "Unexpected punctuation.")),
    }
}

/// Define a unary operator that may precede a literal, a variable or an expression
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UnaryOp {
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

impl UnaryOp {
    /// Identify unary operation from punctuation mark
    pub(crate) fn new(punct: &Punct) -> Result<Self, Error> {
        match punct.as_char() {
            '+' => Ok(Self::Positive),
            '-' => Ok(Self::Negative),
            '!' => Ok(Self::LogicalNot),
            '*' => Ok(Self::Dereference),
            '&' => Ok(Self::Borrow),
            _ => Err(Error::new_spanned(punct, "Unsupported punctuation")),
        }
    }

    /// Generate token for current unary operator
    #[rustfmt::skip]
    pub(crate) fn make_sign(self) -> TokenStream {
        match self {
            UnaryOp::None        => TokenStream::new(),
            UnaryOp::Positive    => TokenStream::new(),
            UnaryOp::Negative    => quote![ - ],
            UnaryOp::LogicalNot  => quote![ ! ],
            UnaryOp::Dereference => quote![ * ],
            UnaryOp::Borrow      => quote![ & ],
        }
    }
}
