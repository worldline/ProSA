use quote::quote;

/// Implementation of the ProSA Adaptor Derive macro
pub(crate) fn adaptor_impl(ast: syn::DeriveInput) -> syn::parse::Result<proc_macro2::TokenStream> {
    let name = &ast.ident;
    let generics = &ast.generics;
    Ok(quote! {
        impl #generics prosa::core::adaptor::Adaptor for #name #generics {
            fn terminate(&mut self) {}
        }
    })
}
