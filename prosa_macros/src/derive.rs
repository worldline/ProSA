/// Abstract Syntatic Tree components
mod ast;

/// Attributes
mod attr;

/// Token generation
mod tokens;

use crate::derive::{
    ast::{TvfEnum, TvfStruct},
    attr::AttrError,
};
use proc_macro2::TokenStream;
use syn::{Data, DeriveInput};

/// name of the attribute to find in the list of attributes
const ATTRIBUTE: &str = "tvf";

/// Error when implementing the tvf traits
#[derive(thiserror::Error, Debug, Clone)]
pub enum TvfError {
    #[error("Union types are not supported")]
    Union,

    #[error("Attribute errors: {0}")]
    Attr(#[from] AttrError),
}

/// Convert the error into a syn::Error
impl From<TvfError> for syn::Error {
    fn from(value: TvfError) -> Self {
        syn::Error::new_spanned(TokenStream::new(), value.to_string())
    }
}

pub(crate) fn impl_derive_to_tvf(input: &DeriveInput) -> Result<TokenStream, TvfError> {
    match &input.data {
        // Implement for struct
        Data::Struct(data) => {
            let tokens =
                TvfStruct::new(&input.ident, &input.attrs, &input.generics, data)?.impl_to_tvf();
            Ok(encapsulate(&tokens))
        }

        // Implement for enum without payload
        Data::Enum(data) => {
            let tokens =
                TvfEnum::new(&input.ident, &input.attrs, &input.generics, data)?.impl_to_tvf();
            Ok(encapsulate(&tokens))
        }
        _ => Err(TvfError::Union),
    }
}

pub(crate) fn impl_derive_from_tvf(input: &DeriveInput) -> Result<TokenStream, TvfError> {
    match &input.data {
        // Implement for struct
        Data::Struct(data) => {
            let tokens =
                TvfStruct::new(&input.ident, &input.attrs, &input.generics, data)?.impl_from_tvf();
            Ok(encapsulate(&tokens))
        }

        // Implement for enum without payload
        Data::Enum(data) => {
            let tokens =
                TvfEnum::new(&input.ident, &input.attrs, &input.generics, data)?.impl_from_tvf();
            Ok(encapsulate(&tokens))
        }
        _ => Err(TvfError::Union),
    }
}

/// Encapsulate the token stream into a scope with the necessary modules
fn encapsulate(generated: &TokenStream) -> TokenStream {
    quote::quote![
        const _: () = {
            #[allow(unused_imports)]
            use prosa_utils::msg::tvf as __tvf;

            #generated
        };
    ]
}
