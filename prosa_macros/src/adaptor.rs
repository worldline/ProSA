use quote::quote;

/// Implementation of the ProSA Adaptor Derive macro
pub(crate) fn adaptor_impl(ast: syn::DeriveInput) -> syn::parse::Result<proc_macro2::TokenStream> {
    let name = &ast.ident;
    let (impl_generics, type_generics, where_clause) = &ast.generics.split_for_impl();
    Ok(quote! {
        impl #impl_generics prosa::core::adaptor::Adaptor for #name #type_generics #where_clause {
            fn terminate(&self) {}
        }
    })
}
