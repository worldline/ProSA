use crate::tvf::expr::*;
use chrono::{Datelike, NaiveDate, NaiveDateTime};
use proc_macro2::TokenStream;
use quote::quote;

impl TvfExpr {
    fn to_tokens(&self, buffer: &syn::Ident) -> TokenStream {
        let ident = self.id.to_tokens();
        let value = self.value.to_tokens();
        let put_method = self.out_type.put_method();

        quote![ #buffer.#put_method(#ident, #value) ]
    }
}

impl TvfId {
    /// Convert the value into tokens
    #[rustfmt::skip]
    fn to_tokens(&self) -> TokenStream {
        match self {
            Self::Int  (int  ) => quote![   #int     ],
            Self::Ident(ident) => quote![   #ident   ],
            Self::Expr (expr ) => quote![ ( #expr  ) ],
        }
    }
}

impl TvfValue {
    /// Convert the value into tokens
    #[rustfmt::skip]
    fn to_tokens(&self) -> TokenStream {
        match self {
            Self::Lit   (lit   ) => quote![   #lit     ],
            Self::Ident (ident ) => quote![   #ident   ],
            Self::Expr  (expr  ) => quote![ ( #expr  ) ],
            Self::Buffer(buffer) => todo![],
        }
    }
}

impl TvfType {
    /// Rust type corresponding to the TVF type
    #[rustfmt::skip]
    fn cast_type(self) -> Option<TokenStream> {
        match self {
            Self::Byte     => Some(quote![ u8  ]),
            Self::Signed   => Some(quote![ i64 ]),
            Self::Unsigned => Some(quote![ u64 ]),
            Self::Float    => Some(quote![ f64 ]),
            _              => None,
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

/// Write tokens to generate the given date
fn date_to_tokens(date: NaiveDate) -> TokenStream {
    let year = date.year();
    let month = date.month();
    let day = date.day();
    quote! [
        ::chrono::NaiveDate::from_ymd_opt( #year, #month, #day ).unwrap()
    ]
}

/// Write tokens to generate the given datetime
fn datetime_to_tokens(datetime: NaiveDateTime) -> TokenStream {
    let msecs = datetime.and_utc().timestamp_millis();
    quote! [
        ::chrono::DateTime::from_timestamp_millis(#msecs).unwrap().naive_utc()
    ]
}

impl Bytes {
    /// Generate tokens to rebuild the sequence of bytes
    #[inline]
    fn to_token(&self) -> TokenStream {
        let bytes = self.0.as_slice();
        quote! [ ::bytes::Bytes::from_static( &[ #(#bytes),* ] ) ]
    }
}
