/// Structures to properly identify the TVF fields to generate
pub(crate) mod expr;

/// Consume tvf! macro tokens and generate corresponding tree structure of expressions
pub(crate) mod parser;

/// Convert the expressions into tokens to be passed to the compiler
pub(crate) mod tokens;

use crate::tvf::{parser::TvfParser, tokens::buffer_to_tokens};
use proc_macro2::{Delimiter, TokenStream, TokenTree};
use quote::quote;
use syn::spanned::Spanned;

pub(crate) fn gen_tvf_impl(input: TokenStream) -> Result<TokenStream, syn::Error> {
    // Process the token tree
    let span = input.span();
    let mut tokens = input.into_iter();

    // The first token must be the buffer type used
    let buffer_type = if let Some(TokenTree::Ident(buffer_type)) = tokens.next() {
        buffer_type
    } else {
        return Err(syn::Error::new(
            span,
            "First argument must be a type identifier",
        ));
    };

    // The second token must be the content enclosed in {} or []
    let output = if let Some(TokenTree::Group(group)) = tokens.next() {
        match group.delimiter() {
            Delimiter::Brace => {
                let mut parser = TvfParser::new(group.stream(), true);
                let exprs = parser.collect_expr()?;
                buffer_to_tokens(&buffer_type, &exprs)?
            }
            Delimiter::Bracket => {
                let mut parser = TvfParser::new(group.stream(), false);
                let exprs = parser.collect_expr()?;
                buffer_to_tokens(&buffer_type, &exprs)?
            }
            _ => {
                return Err(syn::Error::new(
                    span,
                    "Invalid delimiter, expected {} or []",
                ));
            }
        }
    } else {
        return Err(syn::Error::new(
            span,
            "Second argument must be the content enclosed in {} or []",
        ));
    };

    // Raise an error on any extra tokens
    if let Some(token) = tokens.next() {
        return Err(syn::Error::new_spanned(token, "Unexpected token"));
    }

    Ok(quote![
        {
            use ::prosa_utils::msg::tvf as __tvf;
            use ::prosa_utils::msg::bytes as __bytes;
            use ::prosa_utils::msg::chrono as __chrono;
            #output
        }
    ])
}
