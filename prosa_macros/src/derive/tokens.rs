use crate::derive::ast::{TvfEnum, TvfField, TvfFields, TvfStruct, TvfVariant};
use proc_macro2::TokenStream;
use quote::{ToTokens as _, format_ident, quote};
use syn::{Generics, Ident, Index, parse_quote};

impl<'f> TvfStruct<'f> {
    fn impl_to_tvf(&self) -> TokenStream {
        // Prepare tokens
        let type_name = self.type_ident;

        // add `T: Tvf` bounds on generics
        let generics = add_tvf_bound(&self.generics, format_ident!("ToTvf"));
        let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

        // (de)structure the type
        let fields = self.fields.structuring();
        let tokens = self.fields.impl_to_tvf();

        quote![
            impl #impl_generics __tvf::ToTvf<__TVF> for #type_name #ty_generics #where_clause {
                fn to_tvf(&self, __msg: &mut __TVF) {
                    let Self #fields = self;
                    #to_tvf
                }
            }
        ]
    }

    fn impl_from_tvf(&self) -> TokenStream {
        // Prepare tokens
        let type_name = self.type_ident;

        // add `T: Tvf` bounds on generics
        let generics = add_tvf_bound(&self.generics, format_ident!("ToTvf"));
        let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

        // (de)structure the type
        let fields = self.fields.structuring();
        let tokens = self.fields.impl_to_tvf();

        quote![
            impl #impl_generics __tvf::ToTvf<__TVF> for #type_name #ty_generics #where_clause {
                fn from_tvf(__msg: &mut __TVF) -> ::std::result::Result<Self, __tvf::TvfError>
                where
                    Self: ::core::marker::Sized,
                {
                    #from_tvf
                    Ok(Self #fields)
                }
            }
        ]
    }
}

impl<'f> TvfEnum<'f> {
    fn impl_to_tvf(&self) -> TokenStream {
        // Prepare tokens
        let type_name = self.type_ident;
        let (impl_generics, ty_generics, where_clause) = self.generics.split_for_impl();

        // handle all cases of the enumeration
        let count = self.variants.len();
        let mut discri_decls = Vec::with_capacity(count);
        let mut cases = Vec::with_capacity(count);

        // Select appropriate methods for reading and storing the variant identifier
        let [put_variant, get_variant] = if self.is_repr_u8 {
            [format_ident!("put_u8"), format_ident!("try_get_u8")]
        } else {
            [format_ident!("put_u64"), format_ident!("try_get_u64")]
        };

        // Process all variants
        for variant in self.variants.iter() {
            // Declare the constant
            discri_decls.push(variant.discri_decl(self.is_repr_u8));

            // (de)structure the variant
            let var_ident = variant.variant_ident;
            let fields = variant.fields.structuring();
            let discri = variant.discri_ident();
            let tokens = variant.fields.impl_to_tvf();

            cases.push(quote! [
                Self::#var_ident #fields => {
                    __msg.#put_variant(#discri);
                    #tokens
                }
            ]);
        }

        // If one variant is the default one, for any unknown discriminant,
        // we try to construct the selected variant.
        if let Some(variant) = &self.default_variant {
            // Declare the constant
            discri_decls.push(variant.discri_decl(self.is_repr_u8));

            // (de)structure the variant
            let var_ident = variant.variant_ident;
            let fields = variant.fields.structuring();
            let discri = variant.discri_ident();
            let tokens = variant.fields.impl_to_tvf();

            // Add the variant as a regular entry in the store_on list
            cases.push(quote! [
                Self::#var_ident #fields => {
                    __msg.#put_variant(#discri);
                    #tokens
                }
            ]);
        }

        quote![
            impl #impl_generics __tvf::ToTvf<__TVF> for #type_name #ty_generics #where_clause {
                fn to_tvf(&self, __msg: &mut __TVF) {
                    #(#discri_decls)*
                    match self { #(#cases),* }
                }
            }
        ]
    }

    fn impl_from_tvf(&self) -> TokenStream {
        // Prepare tokens
        let type_name = self.type_ident;
        let (impl_generics, ty_generics, where_clause) = self.generics.split_for_impl();

        // handle all cases of the enumeration
        let count = self.variants.len();
        let mut discri_decls = Vec::with_capacity(count);
        let mut cases = Vec::with_capacity(count);

        // Select appropriate methods for reading and storing the variant identifier
        let [put_variant, get_variant] = if self.is_repr_u8 {
            [format_ident!("put_u8"), format_ident!("try_get_u8")]
        } else {
            [format_ident!("put_u64"), format_ident!("try_get_u64")]
        };

        // Process all variants
        for variant in self.variants.iter() {
            // Declare the constant
            discri_decls.push(variant.discri_decl(self.is_repr_u8));

            // (de)structure the variant
            let var_ident = variant.variant_ident;
            let fields = variant.fields.structuring();
            let discri = variant.discri_ident();
            let tokens = variant.fields.impl_to_tvf();

            cases.push(quote! [
                #discri => {
                    #tokens
                    ::core::result::Result::Ok(Self::#var_ident #fields)
                }
            ]);
        }

        // If one variant is the default one, for any unknown discriminant,
        // we try to construct the selected variant.
        let def_case = if let Some(variant) = &self.default_variant {
            // Declare the constant
            discri_decls.push(variant.discri_decl(self.is_repr_u8));

            // (de)structure the variant
            let var_ident = variant.variant_ident;
            let fields = variant.fields.structuring();
            let discri = variant.discri_ident();
            let tokens = variant.fields.impl_to_tvf();

            // Add the variant as a regular entry in the store_on list
            cases.push(quote! [
                Self::#var_ident #fields => {
                    __msg.#put_variant(#discri);
                    #tokens
                }
            ]);

            quote![
                #tokens
                ::core::result::Result::Ok(Self::#var_ident #fields)
            ]
        } else {
            quote![::core::result::Result::Err(
                __tvf::TvfError::InvalidVariant(__disc as u64)
            )]
        };

        quote![
            impl #impl_generics __tvf::FromTvf for #type_name #ty_generics #where_clause {
                fn from_tvf(__msg: &mut __TVF) -> ::std::result::Result<Self, __tvf::TvfError>
                where
                    Self: ::core::marker::Sized,
                {
                    #(#discri_decls)*

                    let __disc = __msg.#get_variant().map_err(|_| __tvf::TvfError::InvalidVariant(0u64))?;
                    match __disc {
                        #(#cases),*
                        _ => { #def_case }
                    }
                }
            }
        ]
    }
}

impl<'f> TvfVariant<'f> {
    /// Generate an identifier for the discriminant constant
    pub fn discri_ident(&self) -> Ident {
        format_ident!("__{}", self.variant_ident.to_string().to_uppercase())
    }

    /// Declare a discriminant constant
    pub fn discri_decl(&self, is_repr_u8: bool) -> TokenStream {
        let ident = self.discri_ident();

        // Either we can serialize the variant as a single byte or use a varint.
        let int_type = if is_repr_u8 {
            format_ident!("u8")
        } else {
            format_ident!("u64")
        };

        // Evaluate the numeric type identifying the variant.
        let value = if let Some(custom_id) = self.attr.custom_id {
            quote! [ #custom_id ]
        } else if let Some(expr) = self.discriminant {
            quote! [ #expr ]
        } else {
            let index = self.index;
            quote! [ #index ]
        };

        // Generate the constant
        quote! [ const #ident: #int_type = #value as #int_type; ]
    }
}

impl<'f> TvfFields<'f> {
    /// Generate tokens for (de)structuring a set of fields.
    /// For a struct Containing `a`, `b`, `c` this will generate:
    /// ```yml
    /// { a: __field_0, b: __field_1, c: __field_2 }
    /// ```
    ///
    /// For a tuple of three elements, this will generate:
    /// ```yml
    /// { 0: __field_0, 1: __field_1, 2: __field_2 }
    /// ```
    pub fn structuring(&self) -> TokenStream {
        // Store the generated tokens in this list
        let mut entries = Vec::with_capacity(self.fields.len());

        // for each field, write a `real_name: __field_x` mapping
        for field in self.fields.iter() {
            // prepare tokens
            let real_name = field.real_ident();
            let tmp_name = field.tmp_ident();

            // add a new mapping to the list
            entries.push(quote! [
                #real_name: #tmp_name
            ]);
        }

        quote![ { #(#entries),* } ]
    }

    /// Generate tokens for ToTvf implementation
    pub fn impl_to_tvf(&self) -> TokenStream {
        // Collect tokens
        let len = self.fields.len();
        let mut tokens = Vec::with_capacity(len);

        // iterate over each field
        for field in self.fields.iter() {
            let ty = &field.field.ty;
            let id = field.tmp_ident();

            // Select "to_tvf" function
            let to = if let Some(to) = &field.attr.custom_to_tvf {
                quote![ #to(#id, __msg); ]
            } else {
                quote![ <#ty as __tvf::ToTvf>::to_tvf(#id, __msg); ]
            };

            tokens.push(to);
        }

        // return tokens
        quote![ #(#tokens)* ]
    }

    /// Generate tokens for FromTvf implementation
    pub fn impl_from_tvf(&self) -> TokenStream {
        // Collect tokens
        let len = self.fields.len();
        let mut tokens = Vec::with_capacity(len);

        // iterate over each field
        for field in self.fields.iter() {
            let ty = &field.field.ty;
            let id = field.tmp_ident();

            // Select "tvf_from" function
            let from = if let Some(from) = &field.attr.custom_from_tvf {
                quote![ let #id = #from(__msg)?; ]
            } else {
                quote![ let #id = <#ty as __tvf::FromTvf>::tvf_from(__msg)?; ]
            };

            tokens.push(from);
        }

        // return tokens
        quote![ #(#tokens)* ]
    }
}

impl<'f> TvfField<'f> {
    /// Get the "real" name of the field
    pub fn real_ident(&self) -> TokenStream {
        if let Some(ident) = &self.field.ident {
            ident.to_token_stream()
        } else {
            let index = Index::from(self.index);
            quote! [#index]
        }
    }

    /// Create a temporary identifier for a field based on its position in the struct
    #[inline]
    pub fn tmp_ident(&self) -> Ident {
        format_ident!("__field_{}", self.index)
    }
}

/// Iterate over each generic of the type and append the necessary tvf trait bound
pub(crate) fn add_tvf_bound(generics: &Generics, tvf_trait: Ident) -> Generics {
    let mut new_gen = generics.clone();

    // Create a where clause and add tvf trait bounds
    let where_clause = new_gen.make_where_clause();
    for param in generics.type_params() {
        let param_name = &param.ident;
        where_clause
            .predicates
            .push(parse_quote!(#param_name : __tvf::#tvf_trait));
    }

    new_gen
}
