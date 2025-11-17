use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::Token;
use syn::spanned::Spanned;
use syn::{parse::Parser, punctuated::Punctuated};

use crate::add_angle_bracketed;

struct ProcParams {
    settings: Option<syn::Path>,
    queue_size: syn::LitInt,
}

impl ProcParams {
    fn parse_attr_args(
        &mut self,
        args: &Punctuated<syn::Meta, Token![,]>,
    ) -> syn::parse::Result<()> {
        for meta in args {
            if let syn::Meta::NameValue(v) = meta {
                if !v.path.segments.is_empty() {
                    let name = &v.path.segments.first().unwrap().ident;
                    if name == "settings" {
                        if let syn::Expr::Path(syn::ExprPath { path, .. }) = &v.value {
                            self.settings = Some(path.clone());
                        } else {
                            return Err(syn::Error::new(
                                v.value.span(),
                                "expected string value for proc args name",
                            ));
                        }
                    } else if name == "queue_size" {
                        if let syn::Expr::Lit(syn::ExprLit {
                            lit: syn::Lit::Int(i),
                            ..
                        }) = &v.value
                        {
                            self.queue_size = i.clone();
                        } else {
                            return Err(syn::Error::new(
                                v.value.span(),
                                "expected int value for task args queue_size (2048 by default)",
                            ));
                        }
                    } else {
                        return Err(syn::Error::new(
                            name.span(),
                            format!("unknown proc args value {name}"),
                        ));
                    }
                } else {
                    return Err(syn::Error::new(v.span(), "no key define for the value"));
                }
            } else {
                return Err(syn::Error::new(meta.span(), "unexpected task expression"));
            }
        }

        Ok(())
    }
}

impl std::default::Default for ProcParams {
    fn default() -> Self {
        Self {
            settings: None,
            queue_size: syn::LitInt::new("2048", Span::call_site()),
        }
    }
}

/// Add the Generic type M to specify the internal message object
fn add_message_generic(generics: &mut syn::Generics) -> syn::parse::Result<()> {
    // Check if a generic M is already present
    let message_predicate = generics.params.iter().position(|p| {
        if let syn::GenericParam::Type(tp) = p {
            tp.ident == "M"
        } else {
            false
        }
    });

    // Add the M generic if it's not present
    if message_predicate.is_none() {
        let ident = syn::Ident::new("M", proc_macro2::Span::call_site());
        generics
            .params
            .push(syn::GenericParam::Type(syn::TypeParam::from(ident)));
    }

    // Add a where clause to specify IO type
    generics.make_where_clause();
    if let Some(ref mut where_clause) = generics.where_clause {
        let message_where: syn::WherePredicate = syn::parse2(
            quote! { M: 'static + std::marker::Send + std::marker::Sync + std::marker::Sized + std::clone::Clone + std::fmt::Debug + prosa_utils::msg::tvf::Tvf + std::default::Default },
        )?;
        where_clause.predicates.push(message_where);
    } else {
        return Err(syn::Error::new(
            generics.span(),
            "wrong where clause on the struct",
        ));
    }

    Ok(())
}

fn generate_struct(
    mut item_struct: syn::ItemStruct,
    args: &ProcParams,
) -> syn::parse::Result<syn::ItemStruct> {
    // Add mandatory fields
    if let syn::Fields::Named(ref mut fields) = item_struct.fields {
        // Parameter of the processor
        fields.named.push(
            syn::Field::parse_named
                .parse2(quote! { proc: std::sync::Arc<prosa::core::proc::ProcParam<M>> })
                .unwrap(),
        );

        // Service table
        fields.named.push(
            syn::Field::parse_named
                .parse2(quote! { service: std::sync::Arc<prosa::core::service::ServiceTable<M>> })
                .unwrap(),
        );

        // Add the receiver queue for processor messaging
        fields.named.push(
            syn::Field::parse_named
                .parse2(quote! { internal_rx_queue: tokio::sync::mpsc::Receiver<prosa::core::msg::InternalMsg<M>> })
                .unwrap(),
        );

        // Add the processor settings if needed
        if let Some(settings) = &args.settings {
            fields.named.push(
                syn::Field::parse_named
                    .parse2(quote! {
                        /// Settings of the processor
                        pub settings: #settings
                    })
                    .unwrap(),
            );
        }
    }

    // Add the Generic type M to specify the internal message object
    add_message_generic(&mut item_struct.generics)?;

    Ok(item_struct)
}

fn generate_struct_impl_bus_param(
    item_struct: &syn::ItemStruct,
) -> syn::parse::Result<proc_macro2::TokenStream> {
    let item_ident = &item_struct.ident;
    let item_generics = &item_struct.generics;

    Ok(quote! {
        impl #item_generics prosa::core::proc::ProcBusParam for #item_ident #item_generics
        where
            M: 'static + std::marker::Send + std::marker::Sync + std::marker::Sized + std::clone::Clone + std::fmt::Debug + prosa_utils::msg::tvf::Tvf + std::default::Default,
        {
            #[doc=concat!(" Getter of the ", stringify!(#item_ident), " processor ID")]
            fn get_proc_id(&self) -> u32 {
                self.proc.get_proc_id()
            }

            #[doc=concat!(" Getter of the ", stringify!(#item_ident), " processor Name")]
            fn name(&self) -> &std::primitive::str {
                self.proc.name()
            }
        }
    })
}

fn generate_struct_impl_config(
    item_struct: &syn::ItemStruct,
    args: &ProcParams,
) -> syn::parse::Result<proc_macro2::TokenStream> {
    let item_ident = &item_struct.ident;
    let item_generics = &item_struct.generics;
    let queue_size = &args.queue_size;

    let (settings, settings_quote) = if let Some(settings) = &args.settings {
        (settings.clone(), quote! { settings, })
    } else {
        let setting_string_path: syn::Path = syn::parse2(quote! { std::string::String })?;

        (setting_string_path, TokenStream::new())
    };

    Ok(quote! {
        // The definition must be done for the protocol
        impl #item_generics prosa::core::proc::ProcConfig<M> for #item_ident #item_generics
        where
            M: 'static + std::marker::Send + std::marker::Sync + std::marker::Sized + std::clone::Clone + std::fmt::Debug + prosa_utils::msg::tvf::Tvf + std::default::Default,
        {
            type Settings = #settings;

            fn create(proc_id: u32, proc_name: String, main: prosa::core::main::Main<M>, settings: Self::Settings) -> Self {
                let (internal_tx_queue, internal_rx_queue) = tokio::sync::mpsc::channel(#queue_size);
                let proc = std::sync::Arc::new(prosa::core::proc::ProcParam::new(proc_id, proc_name, internal_tx_queue, main));
                #item_ident {
                    proc,
                    service: std::default::Default::default(),
                    internal_rx_queue,
                    #settings_quote
                }
            }

            fn get_proc_param(&self) -> &prosa::core::proc::ProcParam<M> {
                &self.proc
            }
        }
    })
}

fn generate_struct_impl_epilogue(
    item_struct: &syn::ItemStruct,
    args: &ProcParams,
) -> syn::parse::Result<proc_macro2::TokenStream> {
    let item_ident = &item_struct.ident;
    let item_generics = &item_struct.generics;

    let restart_settings_quote = if args.settings.is_some() {
        quote! { prosa::core::proc::ProcSettings::get_proc_restart_delay(&self.settings) }
    } else {
        quote! { (std::time::Duration::from_millis(50), 300) }
    };

    Ok(quote! {
        // The definition must be done for the protocol
        impl #item_generics prosa::core::proc::ProcEpilogue for #item_ident #item_generics
        where
            M: 'static + std::marker::Send + std::marker::Sync + std::marker::Sized + std::clone::Clone + std::fmt::Debug + prosa_utils::msg::tvf::Tvf + std::default::Default,
        {
            fn get_proc_restart_delay(&self) -> (std::time::Duration, u32) {
                #restart_settings_quote
            }

            async fn remove_proc(&self, err: std::option::Option<Box<dyn prosa::core::error::ProcError + std::marker::Send + std::marker::Sync>>) -> std::result::Result<(), prosa::core::error::BusError> {
                self.proc.remove_proc(err).await
            }

            fn is_stopping(&self) -> bool {
                self.proc.is_stopping()
            }
        }
    })
}

fn add_struct_impl(mut item_impl: syn::ItemImpl) -> syn::parse::Result<syn::ItemImpl> {
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
                //  Test if Generic are defined for the struct ex. `impl<M, T> Struct<M, T> {}`
                if item_impl.generics.params.len() > 1 {
                    add_message_generic(&mut item_impl.generics)?;
                    add_angle_bracketed("M", ab)?;
                } else {
                    // The impl is for custom type `impl Struct<StructM> {}` so return without adding generic
                    return Ok(item_impl);
                }
            } else {
                add_message_generic(&mut item_impl.generics)?;
                let m_arg: syn::AngleBracketedGenericArguments = syn::parse2(quote! { <M> })?;
                segment.arguments = syn::PathArguments::AngleBracketed(m_arg);
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

/// Implementation of the procedural prosa_proc macro
pub(crate) fn proc_impl(
    args: &Punctuated<syn::Meta, Token![,]>,
    item: syn::Item,
) -> syn::parse::Result<proc_macro2::TokenStream> {
    let mut proc_args: ProcParams = std::default::Default::default();
    proc_args.parse_attr_args(args)?;

    match item {
        syn::Item::Struct(item_struct) => {
            let struct_output = generate_struct(item_struct, &proc_args)?;
            let struct_impl_bus_param = generate_struct_impl_bus_param(&struct_output)?;
            let struct_impl_config = generate_struct_impl_config(&struct_output, &proc_args)?;
            let struct_impl_epilogue = generate_struct_impl_epilogue(&struct_output, &proc_args)?;
            Ok(quote! {
                #struct_output
                #struct_impl_bus_param
                #struct_impl_config
                #struct_impl_epilogue
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
