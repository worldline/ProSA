#![doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/COPYRIGHT"))]
//!
//! [![github]](https://github.com/worldline/prosa)&ensp;[![crates-io]](https://crates.io/crates/prosa-macros)&ensp;[![docs-rs]](crate)
//!
#![doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/doc_assets/header_badges.md"))]
//!
//! Base library to defines procedural macros

#![warn(missing_docs)]
#![deny(unreachable_pub)]

use proc_macro::TokenStream;
use quote::quote;
use syn::{Token, parse::Parser, parse_macro_input, punctuated::Punctuated};

mod adaptor;
mod io;
mod proc;
mod settings;
mod tvf;

fn add_angle_bracketed(
    name: &str,
    argument: &mut syn::AngleBracketedGenericArguments,
) -> syn::parse::Result<()> {
    for t in argument.args.iter() {
        if let syn::GenericArgument::Type(syn::Type::Path(syn::TypePath {
            path: syn::Path { segments, .. },
            ..
        })) = t
        {
            for ps in segments.iter() {
                if ps.ident == name {
                    return Ok(());
                }
            }
        }
    }

    // add the <name> parameter if it's not present
    let tp: syn::TypePath = syn::parse2(quote! { #name })?;
    argument
        .args
        .push(syn::GenericArgument::Type(syn::Type::Path(tp)));

    Ok(())
}

/// Derive macro to define a generic ProSA Adaptor.
#[proc_macro_derive(Adaptor)]
pub fn adaptor(input: TokenStream) -> TokenStream {
    adaptor::adaptor_impl(parse_macro_input!(input as syn::DeriveInput))
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// Procedural macro to help building an ProSA Processor
#[proc_macro_attribute]
pub fn proc(args: TokenStream, input: TokenStream) -> TokenStream {
    let args = match Punctuated::<syn::Meta, Token![,]>::parse_terminated.parse2(args.into()) {
        Ok(args) => args,
        Err(e) => return e.to_compile_error().into(),
    };

    proc::proc_impl(&args, parse_macro_input!(input as syn::Item))
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// Procedural macro to help building an ProSA Settings
#[proc_macro_attribute]
pub fn settings(args: TokenStream, input: TokenStream) -> TokenStream {
    assert!(args.is_empty());
    settings::settings_impl(parse_macro_input!(input as syn::Item))
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// Procedural macro to help building an ProSA Processor Settings
#[proc_macro_attribute]
pub fn proc_settings(args: TokenStream, input: TokenStream) -> TokenStream {
    assert!(args.is_empty());
    settings::proc_settings_impl(parse_macro_input!(input as syn::Item))
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// Procedural macro to help defining an ProSA IO (use for socket,...)
///
/// ```
/// struct MyIoClass<IO> {
///     stream: IO,
///     buffer: bytes::BytesMut,
/// }
/// ```
#[proc_macro_attribute]
pub fn io(args: TokenStream, input: TokenStream) -> TokenStream {
    assert!(args.is_empty());
    io::io_impl(parse_macro_input!(input as syn::Item))
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// Macro to generate a TVF message from a JSON-like syntax
///
/// ```
/// use prosa_utils::msg::simple_string_tvf::SimpleStringTvf;
/// use prosa_macros::tvf;
///
/// let message = tvf![ SimpleStringTvf {
///     1 => 2,
///     3 => 4usize,
///     5 => [
///         "a list starts at index 1",
///         "support trailing comma",
///     ],
///     6 => "2024-01-01" as Date,
///     7 => "2024-01-01 12:00:00.000" as DateTime,
///     9 => 0x01020304 as Bytes,
/// } ];
/// ```
#[proc_macro]
pub fn tvf(input: TokenStream) -> TokenStream {
    tvf::gen_tvf_impl(input.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}
