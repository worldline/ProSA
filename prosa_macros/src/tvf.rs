mod buffer;
mod literal;
mod value;

use buffer::{generate_list, generate_map};
use proc_macro2::{Delimiter, TokenStream, TokenTree};
use syn::Error;

pub(crate) fn gen_tvf_impl(input: TokenStream) -> Result<TokenStream, Error> {
    // Process the token tree
    let mut tokens = input.into_iter();

    // The first token must be the buffer type used
    let buffer_type = if let Some(TokenTree::Ident(buffer_type)) = tokens.next() {
        buffer_type
    } else {
        return Err(Error::new_spanned(
            TokenStream::new(),
            "First argument must be a type identifier",
        ));
    };

    // The second token must be the content enclosed in {} or []
    let result = if let Some(TokenTree::Group(content)) = tokens.next() {
        match content.delimiter() {
            Delimiter::Brace => generate_map(&buffer_type, &content),
            Delimiter::Bracket => generate_list(&buffer_type, &content),
            _ => Err(Error::new_spanned(
                content,
                "Invalid delimiter, expected {} or []",
            )),
        }
    } else {
        Err(Error::new_spanned(
            TokenStream::new(),
            "Second argument must be the content enclosed in {} or []",
        ))
    };

    // Raise an error on any extra tokens
    if let Some(token) = tokens.next() {
        return Err(Error::new_spanned(token, "Unexpected token"));
    }

    result
}
