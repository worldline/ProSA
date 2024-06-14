use quote::quote;
use syn::parse::Parser;

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
        }
    })
}

/// Implementation of the procedural proc_settings macro
pub(crate) fn proc_settings_impl(item: syn::Item) -> syn::parse::Result<proc_macro2::TokenStream> {
    if let syn::Item::Struct(item_struct) = item {
        let struct_output = generate_proc_settings_struct(item_struct)?;
        let struct_impl_proc_settings = generate_struct_impl_proc_settings(&struct_output)?;
        Ok(quote! {
            #struct_output
            #struct_impl_proc_settings
        })
    } else {
        Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "expected struct expression",
        ))
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
    if let syn::Item::Struct(item_struct) = item {
        let struct_output = generate_settings_struct(item_struct)?;
        let struct_impl_settings = generate_struct_impl_settings(&struct_output)?;
        Ok(quote! {
            #struct_output
            #struct_impl_settings
        })
    } else {
        Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "expected struct expression",
        ))
    }
}
