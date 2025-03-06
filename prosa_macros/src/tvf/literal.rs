use super::value::ValueType;
use chrono::Datelike;
use proc_macro2::{Literal, TokenStream};
use quote::{ToTokens, quote};
use std::num::ParseIntError;
use syn::{Error, Lit, parse_quote};

/// Given a literal, identify the corresponding TVF type
pub(crate) fn identify_literal(literal: &Literal) -> Result<ValueType, Error> {
    if let syn::Expr::Lit(literal) = parse_quote! [ #literal ] {
        match literal.lit {
            Lit::Bool(_) => Ok(ValueType::Byte),
            Lit::Byte(_) => Ok(ValueType::Byte),
            Lit::Char(_) => Ok(ValueType::Byte),
            Lit::Int(int) => {
                if int.suffix() == "u8" {
                    Ok(ValueType::Byte)
                } else if int.suffix().starts_with('u') {
                    Ok(ValueType::Unsigned)
                } else {
                    Ok(ValueType::Signed)
                }
            }
            Lit::Float(_) => Ok(ValueType::Float),
            Lit::Str(_) => Ok(ValueType::String),
            Lit::ByteStr(_) => Ok(ValueType::Bytes),
            _ => Err(Error::new_spanned(literal, "Invalid literal")),
        }
    } else {
        Err(Error::new_spanned(literal, "Invalid literal"))
    }
}

/// Convert a literal to the specified type
pub(crate) fn convert_literal(
    literal: &Literal,
    output_type: &ValueType,
) -> Result<TokenStream, Error> {
    // Check if the literal is already of the expected type
    // in that case we can return it as is
    let lit_type = identify_literal(literal)?;
    if lit_type == *output_type {
        return Ok(literal.to_token_stream());
    }

    let token_stream = match lit_type {
        ValueType::Byte | ValueType::Signed | ValueType::Unsigned | ValueType::Float => {
            match output_type {
                ValueType::Byte => quote! [ #literal as u8 ],
                ValueType::Signed => quote! [ #literal as i64 ],
                ValueType::Unsigned => quote! [ #literal as u64 ],
                ValueType::Float => quote! [ #literal as f64 ],
                ValueType::String => quote! [ #literal.to_string() ],
                ValueType::Bytes => {
                    // Enable special conversion for literals written in
                    // hexadecimal or in binary to be converted directly to bytes.

                    // Remove underscores from the literal
                    let digits = literal.to_string().replace('_', "");

                    // convert the digits to a sequence of bytes
                    let result = if digits.starts_with("0x") {
                        // hexadecimal string
                        digits_to_bytes(&digits, 16, 2)
                    } else if digits.starts_with("0b") {
                        // binary string
                        digits_to_bytes(&digits, 2, 8)
                    } else {
                        return Err(Error::new_spanned(
                            literal,
                            "Cannot convert number to bytes, only hexadecimal and binary literals are supported.",
                        ));
                    };

                    // write the bytes as a byte string
                    match result {
                        Ok(bytes) => quote! [ ::bytes::Bytes::from_static( &[ #(#bytes),* ] ) ],
                        Err(e) => {
                            return Err(Error::new_spanned(
                                literal,
                                format!("Cannot convert number to bytes: {}", e),
                            ));
                        }
                    }
                }
                _ => {
                    return Err(Error::new_spanned(
                        literal,
                        "Cannot convert number to the specified type.",
                    ));
                }
            }
        }
        ValueType::String => match output_type {
            ValueType::Bytes => quote! [ ::bytes::Bytes::from_static( #literal.as_bytes() ) ],
            ValueType::Date => {
                if let Ok(date) =
                    chrono::NaiveDate::parse_from_str(&literal.to_string(), "\"%Y-%m-%d\"")
                {
                    let year = date.year();
                    let month = date.month();
                    let day = date.day();
                    quote! [
                        ::chrono::NaiveDate::from_ymd_opt( #year, #month, #day ).unwrap()
                    ]
                } else {
                    return Err(Error::new_spanned(
                        literal,
                        "Invalid date or wrong format, expect \"YYYY-mm-dd\"",
                    ));
                }
            }
            ValueType::DateTime => {
                if let Ok(datetime) = chrono::NaiveDateTime::parse_from_str(
                    &literal.to_string(),
                    "\"%Y-%m-%d %H:%M:%S%.3f\"",
                ) {
                    let msecs = datetime.and_utc().timestamp_millis();
                    quote! [
                        ::chrono::DateTime::from_timestamp_millis(#msecs).unwrap().naive_utc()
                    ]
                } else {
                    return Err(Error::new_spanned(
                        literal,
                        "Invalid date-time or wrong format, expect \"YYYY-mm-dd HH:MM:SS.UUU\"",
                    ));
                }
            }
            _ => {
                return Err(Error::new_spanned(
                    literal,
                    "Cannot convert string to the specified type.",
                ));
            }
        },
        ValueType::Bytes => match output_type {
            ValueType::String => quote! [ ::std::str::from_utf8( #literal.as_ref() ).unwrap() ],
            _ => {
                return Err(Error::new_spanned(
                    literal,
                    "Cannot convert bytes to the specified type.",
                ));
            }
        },
        _ => {
            return Err(Error::new_spanned(
                literal,
                "Literal type not supported. Should not happen!",
            ));
        }
    };

    Ok(token_stream)
}

/// Convert a string of digits to bytes
fn digits_to_bytes(digits: &str, radix: u32, group_by: usize) -> Result<Vec<u8>, ParseIntError> {
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
                Err(e) => Err(e),
            }
        })?;

    // return the result in the correct order
    result.reverse();
    Ok(result)
}
