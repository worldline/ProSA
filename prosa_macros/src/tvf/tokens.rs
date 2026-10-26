use crate::tvf::{
    error,
    expr::*,
    parse_value::{convert_value_to_string, parse_date, parse_datetime, parse_float, parse_int},
};
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
        let mut modifier = self.modifier;

        // Process the value
        let out_type = self.get_type()?;
        let value = match (&self.value, out_type) {
            (TvfValue::Lit(lit), TvfType::Byte) => {
                let int = parse_int::<u8>(lit)?;
                quote! { #int }
            }
            (TvfValue::Lit(lit), TvfType::Signed) => {
                let int = parse_int::<i128>(lit)?;
                quote! { #int }
            }
            (TvfValue::Lit(lit), TvfType::Unsigned) => {
                let int = parse_int::<u128>(lit)?;
                quote! { #int }
            }
            (TvfValue::Lit(lit), TvfType::Float) => {
                let float = parse_float(lit)?;
                quote! { #float }
            }
            (_, TvfType::String) => {
                modifier = self.modifier.filter_out_numeric();
                convert_value_to_string(&self.value, self.modifier)?
            }
            (TvfValue::Lit(lit), TvfType::Bytes) => Bytes::from_literal(lit)?.to_token(),
            (TvfValue::Lit(lit), TvfType::Date) => date_to_tokens(parse_date(lit)?),
            (TvfValue::Lit(lit), TvfType::DateTime) => datetime_to_tokens(parse_datetime(lit)?),
            (TvfValue::Buffer(_, span), _) if out_type != TvfType::Buffer => {
                return Err(error(
                    *span,
                    format!("Cannot apply type {} to sub-buffer declaration", out_type),
                ));
            }
            _ => self.value.to_tokens(buffer_type)?,
        };

        // Check that we do not use a modifier that is not suitable for the expected output type
        if modifier.is_numeric()
            && matches!(
                out_type,
                TvfType::String
                    | TvfType::Bytes
                    | TvfType::Date
                    | TvfType::DateTime
                    | TvfType::Buffer
            )
        {
            return Err(error(
                self.value.span(),
                format!("Cannot apply {} to field of type {}", modifier, out_type),
            ));
        } else if modifier == Modifier::Negative
            && matches!(out_type, TvfType::Byte | TvfType::Unsigned)
        {
            return Err(error(
                self.value.span(),
                "Cannot apply negative operator to an unsigned integer type".to_string(),
            ));
        }

        let put_method = out_type.put_method();
        let value_cast = out_type.cast_type(modifier, value);
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
            Self::Int (int ) => quote![   #int    as usize ],
            Self::Expr(expr) => quote![ ( #expr ) as usize ],
        }
    }
}

impl TvfValue {
    /// Convert the value into tokens
    #[rustfmt::skip]
    fn to_tokens(&self, buffer_type: &syn::Ident) -> Result<TokenStream, syn::Error> {
        match self {
            TvfValue::Lit   (lit)    => Ok(lit  .to_token_stream()),
            TvfValue::Ident (ident)  => Ok(ident.to_token_stream()),
            TvfValue::Format(tokens) => Ok(tokens.clone()),
            TvfValue::Expr  (tokens) => Ok(tokens.clone()),
            TvfValue::Buffer(exprs, _) => buffer_to_tokens(buffer_type, exprs),
        }
    }
}

impl TvfType {
    /// Rust type corresponding to the TVF type
    #[rustfmt::skip]
    fn cast_type(self, modifier: Modifier, value: TokenStream) -> TokenStream {
        let (modifier, cast) = match self {
            TvfType::Byte     => (modifier, quote![ as u8  ]),
            TvfType::Signed   => (modifier, quote![ as i64 ]),
            TvfType::Unsigned => (modifier, quote![ as u64 ]),
            TvfType::Float    => (modifier, quote![ as f64 ]),
            _ => (modifier.filter_out_numeric(), TokenStream::new()),
        };
        let modifier = modifier.to_token();
        quote![ (#modifier (#value)) #cast ]
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
    pub(crate) fn to_token(self) -> TokenStream {
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
