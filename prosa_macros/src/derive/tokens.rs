use crate::derive::{
    ast::{TvfEnum, TvfField, TvfFields, TvfStruct, TvfVariant},
    attr::TagType,
};
use proc_macro2::TokenStream;
use quote::{ToTokens as _, format_ident, quote};
use syn::{Generics, Ident, Index, parse_quote};

// MARK: ToTvf

impl<'f> TvfEnum<'f> {
    pub(crate) fn impl_to_tvf(&self) -> TokenStream {
        // Prepare tokens
        let type_name = self.type_ident;
        let (impl_generics, ty_generics, where_clause) = self.generics.split_for_impl();

        // handle all cases of the enumeration
        let count = self.variants.len();
        let mut discri_decls = Vec::with_capacity(count);
        let mut cases = Vec::with_capacity(count);

        let tag_type = self.attr.tag_type;
        let put_variant = tag_type.put_method();

        // Process all variants
        for variant in self.variants.iter() {
            // Declare the constant
            discri_decls.push(variant.discri_decl(tag_type));

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
            discri_decls.push(variant.discri_decl(self.attr.tag_type));

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
}

impl<'f> TvfStruct<'f> {
    pub(crate) fn impl_to_tvf(&self) -> TokenStream {
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
                    #tokens
                }
            }
        ]
    }
}

impl<'f> TvfFields<'f> {
    /// Generate tokens for ToTvf implementation
    pub(crate) fn impl_to_tvf(&self) -> TokenStream {
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
}

// MARK: FromTvf

impl<'f> TvfEnum<'f> {
    pub(crate) fn impl_from_tvf(&self) -> TokenStream {
        // Prepare tokens
        let type_name = self.type_ident;
        let (impl_generics, ty_generics, where_clause) = self.generics.split_for_impl();

        // handle all cases of the enumeration
        let count = self.variants.len();
        let mut discri_decls = Vec::with_capacity(count);
        let mut cases = Vec::with_capacity(count);

        let tag_type = self.attr.tag_type;
        let get_variant = tag_type.get_method();

        // Process all variants
        for variant in self.variants.iter() {
            // Declare the constant
            discri_decls.push(variant.discri_decl(tag_type));

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
            discri_decls.push(variant.discri_decl(self.attr.tag_type));

            // (de)structure the variant
            let var_ident = variant.variant_ident;
            let fields = variant.fields.structuring();
            let discri = variant.discri_ident();
            let tokens = variant.fields.impl_to_tvf();

            // Add the variant as a regular entry in the store_on list
            cases.push(quote! [
                #discri => {
                    #tokens
                    ::core::result::Result::Ok(Self::#var_ident #fields)
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

impl<'f> TvfStruct<'f> {
    pub(crate) fn impl_from_tvf(&self) -> TokenStream {
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
                    #tokens
                    Ok(Self #fields)
                }
            }
        ]
    }
}

impl<'f> TvfFields<'f> {
    /// Generate tokens for FromTvf implementation
    pub(crate) fn impl_from_tvf(&self) -> TokenStream {
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

// MARK: Common

impl<'f> TvfVariant<'f> {
    /// Generate an identifier for the discriminant constant
    pub(crate) fn discri_ident(&self) -> Ident {
        format_ident!("__{}", self.variant_ident.to_string().to_uppercase())
    }

    /// Declare a discriminant constant
    pub(crate) fn discri_decl(&self, tag_type: TagType) -> TokenStream {
        let ident = self.discri_ident();

        // we have either a text label as a tag or numeric values
        if tag_type == TagType::String {
            let value = if let Some(tag) = &self.attr.tag {
                quote![ #tag ]
            } else {
                let variant = self.variant_ident.to_string();
                quote![ #variant ]
            };
            quote! [ const #ident: &'static str = #value; ]
        } else {
            let value = if let Some(tag) = &self.attr.tag {
                quote![ #tag ]
            } else if let Some(expr) = self.discriminant {
                quote![ #expr ]
            } else {
                let index = self.index;
                quote![ #index ]
            };

            // Either we can serialize the variant as a single byte or use a varint.
            #[cfg_attr(rustfmt, rustfmt_skip)]
            let int_type = match tag_type {
                TagType::Byte     => format_ident!("u8" ),
                TagType::Signed   => format_ident!("i64"),
                TagType::Unsigned => format_ident!("u64"),
                _ => panic!("TagType::String should have already been filtered beforehand"),
            };
            quote! [ const #ident: #int_type = #value as #int_type; ]
        }
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
    pub(crate) fn structuring(&self) -> TokenStream {
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
}

impl<'f> TvfField<'f> {
    /// Get the "real" name of the field
    pub(crate) fn real_ident(&self) -> TokenStream {
        if let Some(ident) = &self.field.ident {
            ident.to_token_stream()
        } else {
            let index = Index::from(self.index);
            quote! [#index]
        }
    }

    /// Create a temporary identifier for a field based on its position in the struct
    #[inline]
    pub(crate) fn tmp_ident(&self) -> Ident {
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
