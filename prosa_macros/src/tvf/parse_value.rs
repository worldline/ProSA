use crate::tvf::{
    error,
    expr::{Bytes, Modifier, TvfValue},
};
use chrono::{NaiveDate, NaiveDateTime};
use core::{fmt, str::FromStr};
use num_traits::FromPrimitive;
use proc_macro2::{Span, TokenStream};
use quote::{ToTokens, quote};
use syn::spanned::Spanned;

/// Get an integer value from a literal
pub(crate) fn parse_int<I>(literal: &syn::Lit) -> Result<I, syn::Error>
where
    I: FromPrimitive + FromStr,
    <I as FromStr>::Err: fmt::Display,
{
    let span = literal.span();
    match literal {
        syn::Lit::Byte(byte) => I::from_u8(byte.value())
            .ok_or_else(|| syn::Error::new(span, "Could not deduce integer from byte")),
        syn::Lit::Char(chr) => I::from_u32(chr.value() as u32)
            .ok_or_else(|| syn::Error::new(span, "Could not deduce integer from character")),
        syn::Lit::Int(int) => Ok(int.base10_parse()?),
        _ => Err(syn::Error::new(span, "Invalid integer literal")),
    }
}

/// Get a float value from a literal
pub(crate) fn parse_float(literal: &syn::Lit) -> Result<f64, syn::Error> {
    let span = literal.span();
    match literal {
        syn::Lit::Byte(byte) => Ok(byte.value() as f64),
        syn::Lit::Char(chr) => Ok(chr.value() as u32 as f64),
        syn::Lit::Int(int) => Ok(int.base10_parse()?),
        syn::Lit::Float(float) => {
            let float: f64 = float.base10_parse()?;
            if float.is_infinite() || float.is_nan() {
                Err(syn::Error::new(span, "Float literal is infinite or NaN"))
            } else {
                Ok(float)
            }
        }
        _ => Err(syn::Error::new(span, "Invalid float literal")),
    }
}

/// Given a literal deduce a Date
pub(crate) fn parse_date(literal: &syn::Lit) -> Result<NaiveDate, syn::Error> {
    const FORMAT: &str = "%Y-%m-%d";
    let span = literal.span();

    let string = parse_str(literal)?;
    NaiveDate::parse_from_str(&string, FORMAT)
        .map_err(|err| syn::Error::new(span, format!("Failed to parse date: {}", err)))
}

/// Given a literal deduce a DateTime
pub(crate) fn parse_datetime(literal: &syn::Lit) -> Result<NaiveDateTime, syn::Error> {
    const FORMAT: &str = "%Y-%m-%d %H:%M:%S%.3f";
    let span = literal.span();

    let string = parse_str(literal)?;
    NaiveDateTime::parse_from_str(&string, FORMAT)
        .map_err(|err| syn::Error::new(span, format!("Failed to parse date: {}", err)))
}

/// Given a literal deduce a String.
/// This function is used to parse a Date and a DateTime.
fn parse_str(literal: &syn::Lit) -> Result<String, syn::Error> {
    let span = literal.span();

    // Try to convert the literal into a string
    let string = match literal {
        syn::Lit::Str(s) => s.value(),
        syn::Lit::ByteStr(s) => match String::from_utf8(s.value()) {
            Ok(s) => s,
            Err(err) => {
                return Err(syn::Error::new(
                    span,
                    format!("Failed to parse UTF-8 string: {}", err),
                ));
            }
        },
        syn::Lit::CStr(s) => s.value().to_string_lossy().to_string(),
        _ => {
            return Err(syn::Error::new(span, "Unsupported literal"));
        }
    };
    Ok(string)
}

/// Numeric literals, variables and expressions can be converted into strings
pub(crate) fn convert_value_to_string(
    value: &TvfValue,
    modifier: Modifier,
) -> Result<TokenStream, syn::Error> {
    match value {
        TvfValue::Lit(lit) => {
            let s = convert_literal_to_string(lit, modifier)?;
            Ok(s.to_token_stream())
        }
        TvfValue::Ident(ident) => {
            let operator = modifier.to_token();
            Ok(quote! { (#operator #ident).to_string() })
        }
        TvfValue::Format(format_expr) => {
            if modifier.is_numeric() {
                Err(error(
                    format_expr.span(),
                    format!(
                        "Operator {} is not compatible with `format` expression",
                        modifier
                    ),
                ))
            } else {
                Ok(quote! { format!(#format_expr) })
            }
        }
        TvfValue::Expr(expr) => {
            let operator = modifier.to_token();
            Ok(quote! { (#operator (#expr)).to_string() })
        }
        TvfValue::Buffer(_, span) => Err(error(*span, "Cannot convert buffer into string")),
    }
}

/// Convert a literal into a string while applying necessary modifier
pub(crate) fn convert_literal_to_string(
    literal: &syn::Lit,
    modifier: Modifier,
) -> Result<String, syn::Error> {
    let span = literal.span();

    match literal {
        syn::Lit::Str(s) => {
            if modifier.is_numeric() {
                Err(error(span, "Unsupported operator on string literal"))
            } else {
                Ok(s.value())
            }
        }
        syn::Lit::ByteStr(s) => {
            if modifier.is_numeric() {
                Err(error(span, "Unsupported operator on byte string literal"))
            } else {
                match String::from_utf8(s.value()) {
                    Ok(s) => Ok(s),
                    Err(err) => Err(error(
                        span,
                        format!("Failed to parse UTF-8 string: {}", err),
                    )),
                }
            }
        }
        syn::Lit::CStr(s) => {
            if modifier.is_numeric() {
                Err(error(span, "Unsupported operator on C-string literal"))
            } else {
                Ok(s.value().to_string_lossy().to_string())
            }
        }
        syn::Lit::Bool(s) => {
            let value = match modifier {
                Modifier::None | Modifier::Positive | Modifier::Dereference => s.value,
                Modifier::LogicalNot | Modifier::Negative => !s.value,
                _ => return Err(syn::Error::new(span, "Unsupported operator for boolean")),
            };
            Ok(if value { "true" } else { "false" }.to_string())
        }
        syn::Lit::Byte(s) => {
            let value = s.value();
            match modifier {
                Modifier::None | Modifier::Positive | Modifier::Dereference => {
                    Ok(value.to_string())
                }
                Modifier::LogicalNot => Ok((!value).to_string()),
                Modifier::Negative => Ok((-(value as i32)).to_string()),
                _ => Err(syn::Error::new(span, "Unsupported operator for byte")),
            }
        }
        syn::Lit::Char(s) => {
            let value = s.value();
            match modifier {
                Modifier::None | Modifier::Positive | Modifier::Dereference => {
                    Ok(value.to_string())
                }
                _ => Err(syn::Error::new(span, "Unsupported operator for character")),
            }
        }
        syn::Lit::Int(s) => {
            let value = s.base10_parse::<u128>()?;
            match modifier {
                Modifier::None | Modifier::Positive | Modifier::Dereference => {
                    Ok(value.to_string())
                }
                Modifier::LogicalNot => Ok((!value).to_string()),
                Modifier::Negative => Ok((-(value as i128)).to_string()),
                _ => Err(syn::Error::new(span, "Unsupported operator for integer")),
            }
        }
        syn::Lit::Float(s) => {
            let value = s.base10_parse::<f64>()?;
            match modifier {
                Modifier::None | Modifier::Positive | Modifier::Dereference => {
                    Ok(value.to_string())
                }
                Modifier::Negative => Ok((-value).to_string()),
                _ => Err(syn::Error::new(span, "Unsupported operator for float")),
            }
        }
        _ => Err(syn::Error::new(span, "Unsupported literal")),
    }
}

impl Bytes {
    /// Parse a integer literal an build a sequence of bytes from it
    pub(crate) fn from_literal(literal: &syn::Lit) -> Result<Self, syn::Error> {
        let span = literal.span();

        match literal {
            syn::Lit::Str(lit) => Ok(Self(lit.value().into_bytes())),
            syn::Lit::CStr(lit) => Ok(Self(lit.value().into_bytes())),
            syn::Lit::ByteStr(lit) => Ok(Self(lit.value())),
            syn::Lit::Byte(lit) => Ok(Self(vec![lit.value()])),
            syn::Lit::Char(lit) => {
                let mut vec = Vec::with_capacity(4);
                for &byte in lit.value().to_string().as_bytes().iter() {
                    vec.push(byte);
                }
                Ok(Self(vec))
            }
            syn::Lit::Int(lit) => {
                // Remove underscores from the literal
                let digits = lit.to_string().replace('_', "");

                // convert the digits to a sequence of bytes
                if digits.starts_with("0x") {
                    // hexadecimal string
                    Self::digits_to_bytes(literal.span(), &digits, 16, 2)
                } else if digits.starts_with("0b") {
                    // binary string
                    Self::digits_to_bytes(literal.span(), &digits, 2, 8)
                } else {
                    Err(syn::Error::new_spanned(
                        literal,
                        "Cannot convert number to bytes, only hexadecimal and binary literals are supported.",
                    ))
                }
            }
            _ => Err(syn::Error::new(
                span,
                "Unsupported literal for sequence of bytes",
            )),
        }
    }

    /// Convert a string of digits to bytes
    fn digits_to_bytes(
        span: Span,
        digits: &str,
        radix: u32,
        group_by: usize,
    ) -> Result<Self, syn::Error> {
        // remove the prefix and the underscores
        let trimmed = digits.replace('_', "").split_off(2);

        // compose a sequence of bytes
        let mut result = Vec::<u8>::with_capacity(trimmed.len() / group_by);

        // group the digits in chunks,
        // start from the end to avoid padding
        trimmed
            .chars()
            .collect::<Vec<_>>()
            .rchunks(group_by)
            .try_for_each(|chunk| {
                let byte = chunk.iter().collect::<String>();
                match u8::from_str_radix(&byte, radix) {
                    Ok(byte) => {
                        result.push(byte);
                        Ok(())
                    }
                    Err(e) => Err(syn::Error::new(
                        span,
                        format!("Failed to parse integer literal: {}", e),
                    )),
                }
            })?;

        // return the result in the correct order
        result.reverse();
        Ok(Self(result))
    }
}
