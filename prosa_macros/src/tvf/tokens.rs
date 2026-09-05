use crate::tvf::expr::*;
use chrono::{Datelike, NaiveDate, NaiveDateTime};
use proc_macro2::TokenStream;
use quote::{ToTokens, quote};

/// Generate the tokens to build a TVF buffer from a list of expressions
pub(crate) fn buffer_to_tokens(
    buffer_type: &syn::Ident,
    expressions: &[TvfExpr],
) -> Result<TokenStream, syn::Error> {
    // collect the expressions to build the buffer
    let mut lines = Vec::with_capacity(expressions.len());
    for expr in expressions.iter() {
        lines.push(expr.to_tokens(buffer_type)?);
    }

    Ok(quote![
        {
            let mut __buffer = <#buffer_type as Default>::default();
             #(#lines)*
            __buffer
        }
    ])
}

impl TvfExpr {
    /// Convert the expression into tokens
    fn to_tokens(&self, buffer_type: &syn::Ident) -> Result<TokenStream, syn::Error> {
        let id = self.id.to_tokens();

        // Process the value
        let value_span = self.value.span();
        let (out_type, value) = match &self.value {
            TvfValue::Lit(lit) => {
                if let Some(explicit) = self.explicit_type {
                    let value = Value::from_literal_with_type(lit, explicit)?;
                    (explicit, value.to_tokens())
                } else {
                    let value = Value::from_literal(lit)?;
                    (value.identify(), value.to_tokens())
                }
            }
            TvfValue::Ident(ident) => {
                if ident == "true" || ident == "false" {
                    (
                        self.explicit_type.unwrap_or(TvfType::Byte),
                        ident.to_token_stream(),
                    )
                } else if let Some(explicit) = self.explicit_type {
                    (explicit, ident.to_token_stream())
                } else {
                    return Err(syn::Error::new(
                        value_span,
                        "Missing explicit type for variable",
                    ));
                }
            }
            TvfValue::Expr(stream) => {
                if let Some(explicit) = self.explicit_type {
                    (explicit, stream.clone())
                } else {
                    return Err(syn::Error::new(
                        value_span,
                        "Missing explicit type for expression",
                    ));
                }
            }
            TvfValue::Buffer(sub, _) => (TvfType::Buffer, buffer_to_tokens(buffer_type, sub)?),
        };

        let put_method = out_type.put_method();
        let value_cast = out_type.cast_type(self.modifier, value);
        Ok(quote![
            <#buffer_type as __tvf::Tvf>::#put_method(&mut __buffer, #id, #value_cast);
        ])
    }
}

impl TvfId {
    /// Convert the value into tokens
    #[rustfmt::skip]
    fn to_tokens(&self) -> TokenStream {
        match self {
            Self::Int  (int  ) => quote![   #int     as usize ],
            Self::Ident(ident) => quote![   #ident   as usize ],
            Self::Expr (expr ) => quote![ ( #expr  ) as usize ],
        }
    }
}

impl TvfType {
    /// Rust type corresponding to the TVF type
    #[rustfmt::skip]
    fn cast_type(self, modifier: Modifier, value: TokenStream) -> TokenStream {
        let md = modifier.to_token();
        match self {
            Self::Byte     => quote![ (#md #value) as u8  ],
            Self::Signed   => quote![ (#md #value) as i64 ],
            Self::Unsigned => quote![ (#md #value) as u64 ],
            Self::Float    => quote![ (#md #value) as f64 ],
            Self::String   => quote![ (#md #value).to_string() ],
            Self::Bytes    => value,
            Self::Date     => value,
            Self::DateTime => value,
            Self::Buffer   => value,
        }
    }

    /// Name of the put method expected given the type
    #[rustfmt::skip]
    fn put_method(self) -> TokenStream {
        match self {
            Self::Byte     => quote![ put_byte     ],
            Self::Signed   => quote![ put_signed   ],
            Self::Unsigned => quote![ put_unsigned ],
            Self::Float    => quote![ put_float    ],
            Self::String   => quote![ put_string   ],
            Self::Bytes    => quote![ put_bytes    ],
            Self::Date     => quote![ put_date     ],
            Self::DateTime => quote![ put_datetime ],
            Self::Buffer   => quote![ put_buffer   ],
        }
    }
}

impl Modifier {
    /// Generate token for current unary operator
    #[rustfmt::skip]
    fn to_token(self) -> TokenStream {
        match self {
            Self::None        => TokenStream::new(),
            Self::Positive    => TokenStream::new(),
            Self::Negative    => quote![ - ],
            Self::LogicalNot  => quote![ ! ],
            Self::Dereference => quote![ * ],
            Self::Borrow      => quote![ & ],
        }
    }
}

impl<'e> Value<'e> {
    /// Generate token for a value
    #[rustfmt::skip]
    fn to_tokens(&'e self) -> TokenStream {
        match self {
            Value::Bool     (val) => quote![ #val ],
            Value::Byte     (val) => quote![ #val ],
            Value::Signed   (val) => quote![ #val ],
            Value::Unsigned (val) => quote![ #val ],
            Value::Float    (val) => quote![ #val ],
            Value::String   (val) => quote![ #val ],
            Value::Bytes    (val) => val.to_token(),
            Value::Date     (val) => date_to_tokens    (*val),
            Value::DateTime (val) => datetime_to_tokens(*val),
            Value::Buffer   ( _ ) => panic!("Should not be called here"),
        }
    }
}

/// Write tokens to generate the given date
fn date_to_tokens(date: NaiveDate) -> TokenStream {
    let year = date.year();
    let month = date.month();
    let day = date.day();
    quote! [
        __chrono::NaiveDate::from_ymd_opt( #year, #month, #day ).unwrap()
    ]
}

/// Write tokens to generate the given datetime
fn datetime_to_tokens(datetime: NaiveDateTime) -> TokenStream {
    let msecs = datetime.and_utc().timestamp_millis();
    quote! [
        __chrono::DateTime::from_timestamp_millis(#msecs).unwrap().naive_utc()
    ]
}

impl Bytes {
    /// Generate tokens to rebuild the sequence of bytes
    #[inline]
    fn to_token(&self) -> TokenStream {
        let bytes = self.0.as_slice();
        quote! [ __bytes::Bytes::from_static( &[ #(#bytes),* ] ) ]
    }
}
