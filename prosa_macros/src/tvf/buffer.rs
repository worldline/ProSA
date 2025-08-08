use super::value::{ValueType, generate_value};
use proc_macro2::{Group, Spacing, TokenStream, TokenTree};
use quote::{ToTokens, quote};
use syn::{Error, Ident};

/// From `[ a, b, c, ... ]` Generate a list buffer
pub(crate) fn generate_list(buffer_type: &Ident, content: &Group) -> Result<TokenStream, Error> {
    // group the values separated by commas into entries
    let mut entries = Vec::<(TokenStream, ValueType)>::new();

    // allocate a buffer to append tokens until a comma is found
    let mut token_buffer = TokenStream::new();
    for token_tree in content.stream() {
        // if the token is a comma, we have a complete entry
        if let TokenTree::Punct(punct) = &token_tree
            && punct.as_char() == ','
            && !token_buffer.is_empty()
        {
            // generate the sequence of tokens to build the value
            entries.push(generate_value(buffer_type, &token_buffer)?);

            // reset the buffer
            token_buffer = TokenStream::new();
            continue;
        }
        // otherwise append to the buffer
        token_buffer.extend(token_tree.to_token_stream());
    }

    // handle the last entry
    if !token_buffer.is_empty() {
        entries.push(generate_value(buffer_type, &token_buffer)?);
    }

    let token_stream = entries
        .iter()
        .enumerate()
        .map(|(index, (tokens, value_type))| {
            let key = index + 1;
            let put_method = TokenStream::from(value_type);
            quote! [
                <#buffer_type as ::prosa_utils::msg::tvf::Tvf>::#put_method(&mut __list_buffer, #key, #tokens);
            ]
        });

    Ok(quote! [
        {
            let mut __list_buffer = <#buffer_type>::default();
            #(#token_stream)*
            __list_buffer
        }
    ])
}

/// From `{ 1 => a, 2 => b, 3 => c, ... }` Generate a map buffer
pub(crate) fn generate_map(buffer_type: &Ident, content: &Group) -> Result<TokenStream, Error> {
    // group the values separated by commas into entries
    let mut entries = Vec::<(TokenStream, TokenStream, ValueType)>::new();

    // allocate a buffer to append tokens until a `=>` and a comma is found
    let mut read_state = ReadState::Key;
    let mut token_buffer_key = TokenStream::new();
    let mut token_buffer_value = TokenStream::new();
    for token_tree in content.stream() {
        match read_state {
            ReadState::Key => {
                // if the token is a `=`, we check if the next token is a `>`
                if let TokenTree::Punct(punct) = &token_tree
                    && punct.as_char() == '='
                    && punct.spacing() == Spacing::Joint
                {
                    read_state = ReadState::Arrow;
                    continue;
                }
                token_buffer_key.extend(token_tree.to_token_stream());
            }
            ReadState::Arrow => {
                // verify that the next token is a `>`
                if let TokenTree::Punct(punct) = &token_tree {
                    if punct.as_char() == '>' {
                        read_state = ReadState::Value;
                    }
                } else {
                    return Err(Error::new_spanned(token_tree, "Expected `=>`"));
                }
            }
            ReadState::Value => {
                // if the token is a comma, we have a complete entry
                if let TokenTree::Punct(punct) = &token_tree
                    && punct.as_char() == ','
                    && !token_buffer_value.is_empty()
                {
                    // generate the sequence of tokens to build the value
                    let (tokens, value_type) = generate_value(buffer_type, &token_buffer_value)?;
                    entries.push((token_buffer_key.clone(), tokens, value_type));

                    // reset the buffers
                    read_state = ReadState::Key;
                    token_buffer_key = TokenStream::new();
                    token_buffer_value = TokenStream::new();
                    continue;
                }
                token_buffer_value.extend(token_tree.to_token_stream());
            }
        }
    }

    // handle the last entry
    if !token_buffer_key.is_empty() && !token_buffer_value.is_empty() {
        let (tokens, value_type) = generate_value(buffer_type, &token_buffer_value)?;
        entries.push((token_buffer_key, tokens, value_type));
    }

    let token_stream = entries.iter().map(|(key, tokens, value_type)| {
        let put_method = TokenStream::from(value_type);
        quote! [
            <#buffer_type as ::prosa_utils::msg::tvf::Tvf>::#put_method(&mut __map_buffer, (#key) as usize, #tokens);
        ]
    });

    Ok(quote! [
        {
            let mut __map_buffer = <#buffer_type>::default();
            #(#token_stream)*
            __map_buffer
        }
    ])
}

/// Represent the reading state when parsing a map buffer
enum ReadState {
    Key,
    Arrow,
    Value,
}
