use super::buffer::{generate_list, generate_map};
use super::literal::{convert_literal, identify_literal};
use proc_macro2::{Delimiter, TokenStream, TokenTree};
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
    let mut tokens = value_stream.clone().into_iter();

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

    // check the type of value provided
    match value {
        TokenTree::Literal(literal) => {
            if let Some(output_type) = output_type {
                Ok((convert_literal(&literal, &output_type)?, output_type))
            } else {
                let literal_type = identify_literal(&literal)?;
                let token_stream = match literal_type {
                    ValueType::Byte => quote! [ #literal as u8 ],
                    ValueType::Signed => quote! [ #literal as i64 ],
                    ValueType::Unsigned => quote! [ #literal as u64 ],
                    ValueType::Float => quote! [ #literal as f64 ],
                    _ => literal.to_token_stream(),
                };
                Ok((token_stream, literal_type))
            }
        }
        TokenTree::Ident(ident) => {
            // check if the identifier is a boolean
            match ident.to_string().as_str() {
                "true" => Ok((quote![1u8], output_type.unwrap_or(ValueType::Byte))),
                "false" => Ok((quote![1u8], output_type.unwrap_or(ValueType::Byte))),
                _ => {
                    if let Some(output_type) = output_type {
                        Ok((quote![#ident], output_type))
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
                        Ok((group.to_token_stream(), output_type))
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
