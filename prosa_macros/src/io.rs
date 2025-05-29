use quote::quote;
use syn::parse::Parser;
use syn::spanned::Spanned;

use crate::add_angle_bracketed;

/// Add the Generic type IO to specify the net object
fn add_io_generic(generics: &mut syn::Generics) -> syn::parse::Result<()> {
    // Check if a generic IO is already present
    let io_predicate = generics.params.iter().position(|p| {
        if let syn::GenericParam::Type(tp) = p {
            tp.ident == "IO"
        } else {
            false
        }
    });

    // Add the IO generic if it's not present
    if io_predicate.is_none() {
        let ident = syn::Ident::new("IO", proc_macro2::Span::call_site());
        generics
            .params
            .push(syn::GenericParam::Type(syn::TypeParam::from(ident)));
    }

    // Add a where clause to specify IO type
    generics.make_where_clause();
    if let Some(ref mut where_clause) = generics.where_clause {
        let io_where: syn::WherePredicate = syn::parse2(
            quote! { IO: 'static + tokio::io::AsyncReadExt + tokio::io::AsyncWriteExt + std::marker::Unpin + std::marker::Send },
        )?;
        where_clause.predicates.push(io_where);
    } else {
        return Err(syn::Error::new(
            generics.span(),
            "wrong where clause on the struct",
        ));
    }

    Ok(())
}

fn generate_struct(mut item_struct: syn::ItemStruct) -> syn::parse::Result<syn::ItemStruct> {
    // Add mandatory fields
    if let syn::Fields::Named(ref mut fields) = item_struct.fields {
        // Add the stream field to handle a net object
        fields.named.push(
            syn::Field::parse_named
                .parse2(quote! { stream: IO })
                .unwrap(),
        );
        // Add the Address field that is the remote address
        fields.named.push(
            syn::Field::parse_named
                .parse2(quote! { addr: std::option::Option<std::net::SocketAddr> })
                .unwrap(),
        );
        // Add the buffer object to read from the net object
        fields.named.push(
            syn::Field::parse_named
                .parse2(quote! { buffer: bytes::BytesMut })
                .unwrap(),
        );

        // Add the socket id information
        fields.named.push(
            syn::Field::parse_named
                .parse2(quote! { socket_id: u32 })
                .unwrap(),
        );
    }

    // Add the Generic type IO to specify the net object
    add_io_generic(&mut item_struct.generics)?;

    Ok(item_struct)
}

fn generate_struct_impl(
    item_struct: &syn::ItemStruct,
) -> syn::parse::Result<proc_macro2::TokenStream> {
    let item_ident = &item_struct.ident;
    let item_generics = &item_struct.generics;
    let item_other_fields =
        if let syn::Fields::Named(syn::FieldsNamed { named, .. }) = &item_struct.fields {
            let token_other_fields =
                named.iter().filter_map(|f| {
                    if let Some(ident) = f.ident.as_ref().filter(|&i| {
                        i != "stream" && i != "addr" && i != "buffer" && i != "socket_id"
                    }) {
                        return Some(quote! { #ident: std::default::Default::default() });
                    }

                    None
                });

            quote! { #(#token_other_fields,)* }
        } else {
            proc_macro2::TokenStream::new()
        };

    Ok(quote! {
        impl #item_generics std::convert::From<IO> for #item_ident #item_generics
        where
            IO: 'static + tokio::io::AsyncReadExt + tokio::io::AsyncWriteExt + std::os::fd::AsRawFd + std::marker::Unpin + std::marker::Send
        {
            fn from(stream: IO) -> Self {
                let socket_id = stream.as_raw_fd() as u32;
                #item_ident {
                    stream,
                    addr: None,
                    buffer: bytes::BytesMut::with_capacity(16384),
                    socket_id,
                    #item_other_fields
                }
            }
        }
        impl #item_generics std::convert::From<(IO, std::net::SocketAddr)> for #item_ident #item_generics
        where
            IO: 'static + tokio::io::AsyncReadExt + tokio::io::AsyncWriteExt + std::os::fd::AsRawFd + std::marker::Unpin + std::marker::Send
        {
            fn from(socket: (IO, std::net::SocketAddr)) -> Self {
                let (stream, addr) = socket;
                let socket_id = stream.as_raw_fd() as u32;
                #item_ident {
                    stream,
                    addr: Some(addr),
                    buffer: bytes::BytesMut::with_capacity(16384),
                    socket_id,
                    #item_other_fields
                }
            }
        }
    })
}

fn add_struct_impl(mut item_impl: syn::ItemImpl) -> syn::parse::Result<syn::ItemImpl> {
    add_io_generic(&mut item_impl.generics)?;

    // Add IO template if missing
    if let syn::Type::Path(syn::TypePath {
        path: syn::Path {
            ref mut segments, ..
        },
        ..
    }) = *item_impl.self_ty
    {
        if let Some(segment) = segments.first_mut() {
            if let syn::PathArguments::AngleBracketed(ref mut ab) = segment.arguments {
                add_angle_bracketed("IO", ab)?;
            } else {
                let io_arg: syn::AngleBracketedGenericArguments = syn::parse2(quote! { <IO> })?;
                segment.arguments = syn::PathArguments::AngleBracketed(io_arg);
            }
        } else {
            return Err(syn::Error::new(
                segments.span(),
                "no path segment for the impl",
            ));
        }
    } else {
        return Err(syn::Error::new(
            item_impl.self_ty.span(),
            "wrong path for impl",
        ));
    }

    Ok(item_impl)
}

/// Implementation of the procedural prosa_io macro
pub(crate) fn io_impl(item: syn::Item) -> syn::parse::Result<proc_macro2::TokenStream> {
    match item {
        syn::Item::Struct(item_struct) => {
            let struct_output = generate_struct(item_struct)?;
            let struct_impl = generate_struct_impl(&struct_output)?;
            Ok(quote! {
                #struct_output
                #struct_impl
            })
        }
        syn::Item::Impl(item_impl) => {
            let impl_output = add_struct_impl(item_impl)?;
            Ok(quote! {
                #impl_output
            })
        }
        _ => Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "expected struct or impl expression",
        )),
    }
}
