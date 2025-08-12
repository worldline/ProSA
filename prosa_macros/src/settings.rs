use quote::{ToTokens, quote};
use syn::{
    ItemImpl,
    parse::{Parse, Parser},
};

/// Function to add default member to Default trait impl
fn add_default_member<F>(mut item_impl: ItemImpl, func: F) -> syn::parse::Result<ItemImpl>
where
    F: Fn(&mut syn::ExprStruct),
{
    if let (Some((_, trait_path, _)), syn::Type::Path(self_path)) =
        (&item_impl.trait_, item_impl.self_ty.as_ref())
    {
        // Only consider Default trait impl
        if trait_path.get_ident().is_some_and(|i| i == "Default") {
            // Get self object ident and `fn default() -> Self` method
            if let (Some(self_ident), Some(syn::ImplItem::Fn(item_fn))) =
                (self_path.path.get_ident(), item_impl.items.first_mut())
            {
                // Iterate over statements (code)
                for stmt in &mut item_fn.block.stmts {
                    match stmt {
                        // Local statement (let = ...)
                        syn::Stmt::Local(syn::Local {
                            init: Some(syn::LocalInit { expr, .. }),
                            ..
                        }) => {
                            if let syn::Expr::Struct(expr) = expr.as_mut()
                                && expr.path.is_ident(self_ident)
                            {
                                if !expr.fields.trailing_punct() {
                                    expr.fields.push_punct(syn::token::Comma::default());
                                }

                                func(expr);
                            }
                        }
                        // Direct Expr return (Self {..})
                        syn::Stmt::Expr(syn::Expr::Struct(expr), _) => {
                            if expr.path.is_ident(self_ident) {
                                if !expr.fields.trailing_punct() {
                                    expr.fields.push_punct(syn::token::Comma::default());
                                }

                                func(expr);
                            }
                        }
                        _ => {}
                    }
                }

                Ok(item_impl)
            } else {
                Err(syn::Error::new(
                    proc_macro2::Span::call_site(),
                    "Wrong Default impl, missing `fn default() -> Self`",
                ))
            }
        } else if let Some(ident) = trait_path.get_ident() {
            Err(syn::Error::new(
                ident.span(),
                format!("expected Default impl expression instead of {ident}"),
            ))
        } else {
            Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                "expected Default impl expression",
            ))
        }
    } else {
        Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "expected Default impl expression",
        ))
    }
}

fn generate_proc_settings_struct(
    mut item_struct: syn::ItemStruct,
) -> syn::parse::Result<syn::ItemStruct> {
    // Add mandatory fields
    if let syn::Fields::Named(ref mut fields) = item_struct.fields {
        // Adaptor config path
        fields.named.push(
            syn::Field::parse_named
                .parse2(quote! { adaptor_config_path: std::option::Option<std::string::String> })
                .unwrap(),
        );

        // Restart duration period
        fields.named.push(
            syn::Field::parse_named
                .parse2(quote! {
                    #[serde(skip_serializing)]
                    proc_restart_duration_period: std::option::Option<std::time::Duration>
                })
                .unwrap(),
        );

        // Max restart period
        fields.named.push(
            syn::Field::parse_named
                .parse2(quote! {
                    #[serde(skip_serializing)]
                    proc_max_restart_period: std::option::Option<std::primitive::u32>
                })
                .unwrap(),
        );
    }

    Ok(item_struct)
}

fn generate_struct_impl_proc_settings(
    item_struct: &syn::ItemStruct,
) -> syn::parse::Result<proc_macro2::TokenStream> {
    let item_ident = &item_struct.ident;

    Ok(quote! {
        impl prosa::core::proc::ProcSettings for #item_ident {
            fn get_adaptor_config_path(&self) -> std::option::Option<&std::string::String> {
                self.adaptor_config_path.as_ref()
            }

            fn get_proc_restart_delay(&self) -> (std::time::Duration, u32) {
                (self.proc_restart_duration_period.unwrap_or(std::time::Duration::from_millis(50)), self.proc_max_restart_period.unwrap_or(300))
            }
        }
    })
}

/// Implementation of the procedural proc_settings macro
pub(crate) fn proc_settings_impl(item: syn::Item) -> syn::parse::Result<proc_macro2::TokenStream> {
    match item {
        syn::Item::Struct(item_struct) => {
            let struct_output = generate_proc_settings_struct(item_struct)?;
            let struct_impl_proc_settings = generate_struct_impl_proc_settings(&struct_output)?;
            Ok(quote! {
                #struct_output
                #struct_impl_proc_settings
            })
        }
        syn::Item::Impl(item_impl) => Ok(add_default_member(item_impl, |x| {
            x.fields.push_value(
                syn::FieldValue::parse
                    .parse2(quote! { adaptor_config_path: None })
                    .unwrap(),
            );
            x.fields.push_punct(syn::token::Comma::default());
            x.fields.push_value(
                syn::FieldValue::parse
                    .parse2(quote! { proc_restart_duration_period: None })
                    .unwrap(),
            );
            x.fields.push_punct(syn::token::Comma::default());
            x.fields.push_value(
                syn::FieldValue::parse
                    .parse2(quote! { proc_max_restart_period: None })
                    .unwrap(),
            );
            x.fields.push_punct(syn::token::Comma::default());
        })?
        .into_token_stream()),
        _ => Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "expected struct expression",
        )),
    }
}

fn generate_settings_struct(
    mut item_struct: syn::ItemStruct,
) -> syn::parse::Result<syn::ItemStruct> {
    // Add mandatory fields
    if let syn::Fields::Named(ref mut fields) = item_struct.fields {
        // ProSA name setting
        fields.named.push(
            syn::Field::parse_named
                .parse2(quote! { name: std::option::Option<std::string::String> })
                .unwrap(),
        );

        // ProSA observability setting
        fields.named.push(
            syn::Field::parse_named
                .parse2(quote! {
                #[serde(default)]
                observability: prosa_utils::config::observability::Observability })
                .unwrap(),
        );
    }

    Ok(item_struct)
}

fn generate_struct_impl_settings(
    item_struct: &syn::ItemStruct,
) -> syn::parse::Result<proc_macro2::TokenStream> {
    let item_ident = &item_struct.ident;

    Ok(quote! {
        impl prosa::core::settings::Settings for #item_ident {
            fn get_prosa_name(&self) -> String {
                if let Some(name) = &self.name {
                    name.clone()
                } else if let Ok(hostname) = std::env::var("HOSTNAME") {
                    format!("prosa-{}", hostname)
                } else {
                    std::string::String::from("prosa")
                }
            }

            fn set_prosa_name(&mut self, name: std::string::String) {
                self.name = Some(name);
            }

            fn get_observability(&self) -> &prosa_utils::config::observability::Observability {
                &self.observability
            }
        }
    })
}

/// Implementation of the procedural proc macro
pub(crate) fn settings_impl(item: syn::Item) -> syn::parse::Result<proc_macro2::TokenStream> {
    match item {
        syn::Item::Struct(item_struct) => {
            let struct_output = generate_settings_struct(item_struct)?;
            let struct_impl_settings = generate_struct_impl_settings(&struct_output)?;
            Ok(quote! {
                #struct_output
                #struct_impl_settings
            })
        }
        syn::Item::Impl(item_impl) => Ok(add_default_member(item_impl, |x| {
            x.fields.push_value(
                syn::FieldValue::parse
                    .parse2(quote! { name: None })
                    .unwrap(),
            );
            x.fields.push_punct(syn::token::Comma::default());

            x.fields.push_value(
                syn::FieldValue::parse
                    .parse2(quote! { observability: prosa_utils::config::observability::Observability::default() })
                    .unwrap(),
            );
            x.fields.push_punct(syn::token::Comma::default());
        })?
        .into_token_stream()),
        _ => Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "expected struct/Default impl expression",
        )),
    }
}
