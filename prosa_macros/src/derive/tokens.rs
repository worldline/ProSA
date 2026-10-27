use crate::derive::{
    ast::{ImplTrait, TvfEnum, TvfField, TvfFields, TvfStruct, TvfVariant, extend_generics},
    attr::TagType,
};
use proc_macro2::TokenStream;
use quote::{ToTokens as _, format_ident, quote};
use syn::{Generics, Ident, Index};

// MARK: ToTvf / IntoTvf

impl<'f> TvfEnum<'f> {
    pub(crate) fn impl_to_tvf(&self, owned: bool) -> TokenStream {
        // Prepare tokens
        let type_name = self.type_ident;
        let [impl_generics, ty_generics, where_clause] = extend_generics(
            self.generics.clone(),
            if owned {
                ImplTrait::Into
            } else {
                ImplTrait::To
            },
        );

        // handle all cases of the enumeration
        let decl_discris = self.decl_discris(self.attr.tag_type, true);
        let mut cases = Vec::with_capacity(self.variants.len());
        let put_variant = self.attr.tag_type.put_method();
        let tag_id = &self.attr.tag_id;

        // Process all variants
        for variant in self.variants.iter() {
            // (de)structure the variant
            let var_ident = variant.variant_ident;
            let fields = variant.fields.structuring();
            let discri = variant.ident_discri();
            let tokens = variant.fields.impl_to_tvf(owned);

            cases.push(quote! [
                Self::#var_ident #fields => {
                    #put_variant(__msg, #tag_id, #discri);
                    #tokens
                }
            ]);
        }

        // If one variant is the default one, for any unknown discriminant,
        // we try to construct the selected variant.
        if let Some(variant) = &self.default_variant {
            // (de)structure the variant
            let var_ident = variant.variant_ident;
            let fields = variant.fields.structuring();
            let discri = variant.ident_discri();
            let tokens = variant.fields.impl_to_tvf(owned);

            // Add the variant as a regular entry in the store_on list
            cases.push(quote! [
                Self::#var_ident #fields => {
                    #put_variant(__msg, #tag_id, #discri);
                    #tokens
                }
            ]);
        }

        let main_trait = if owned {
            quote! {
                impl #impl_generics __tvf::IntoTvf<__TVF> for #type_name #ty_generics #where_clause {
                    fn into_tvf(self, __msg: &mut __TVF) {
                        #decl_discris
                        match self { #(#cases),* }
                    }
                }
            }
        } else {
            quote! {
                impl #impl_generics __tvf::ToTvf<__TVF> for #type_name #ty_generics #where_clause {
                    fn to_tvf(&self, __msg: &mut __TVF) {
                        #decl_discris
                        match self { #(#cases),* }
                    }
                }
            }
        };

        let field_trait = impl_to_field(type_name, self.generics.clone(), owned);
        quote! { #main_trait #field_trait }
    }
}

impl<'f> TvfStruct<'f> {
    pub(crate) fn impl_to_tvf(&self, owned: bool) -> TokenStream {
        // Prepare tokens
        let type_name = self.type_ident;

        // add `T: Tvf` bounds on generics
        let [impl_generics, ty_generics, where_clause] = extend_generics(
            self.generics.clone(),
            if owned {
                ImplTrait::Into
            } else {
                ImplTrait::To
            },
        );

        // (de)structure the type
        let fields = self.fields.structuring();
        let tokens = self.fields.impl_to_tvf(owned);

        let main_trait = if owned {
            quote! {
                impl #impl_generics __tvf::IntoTvf<__TVF> for #type_name #ty_generics #where_clause {
                    fn into_tvf(self, __msg: &mut __TVF) {
                        let Self #fields = self;
                        #tokens
                    }
                }
            }
        } else {
            quote! {
                impl #impl_generics __tvf::ToTvf<__TVF> for #type_name #ty_generics #where_clause {
                    fn to_tvf(&self, __msg: &mut __TVF) {
                        let Self #fields = self;
                        #tokens
                    }
                }
            }
        };
        let field_trait = impl_to_field(type_name, self.generics.clone(), owned);
        quote! { #main_trait #field_trait }
    }
}

impl<'f> TvfFields<'f> {
    /// Generate tokens for ToTvf implementation
    pub(crate) fn impl_to_tvf(&self, owned: bool) -> TokenStream {
        // Collect tokens
        let len = self.fields.len();
        let decls = self.decl_field_ids();
        let mut tokens = Vec::with_capacity(len);

        // iterate over each field
        for field in self.fields.iter() {
            let ty = &field.field.ty;
            let ident = field.tmp_ident();
            let id = field.ident_field_id();

            let f = if owned {
                // Select "into_tvf" function
                if let Some(func) = &field.attr.custom_into_tvf {
                    quote![ #func(#ident, #id, __msg); ]
                } else {
                    quote![ <#ty as __tvf::IntoField<__TVF>>::into_field(#ident, #id, __msg); ]
                }
            } else {
                // Select "to_tvf" function
                if let Some(func) = &field.attr.custom_to_tvf {
                    quote![ #func(&#ident, #id, __msg); ]
                } else {
                    quote![ <#ty as __tvf::ToField<__TVF>>::to_field(&#ident, #id, __msg); ]
                }
            };
            tokens.push(f);
        }

        // return tokens
        quote![ #decls #(#tokens)* ]
    }
}

fn impl_to_field(type_name: &Ident, generics: Generics, owned: bool) -> TokenStream {
    if owned {
        let [impl_generics, ty_generics, where_clause] = extend_generics(generics, ImplTrait::Into);

        quote! {
            impl #impl_generics __tvf::IntoField<__TVF> for #type_name #ty_generics #where_clause {
                fn into_field(self, __id: usize, __msg: &mut __TVF) {
                    let mut __buffer = __TVF::default();
                    self.into_tvf(&mut __buffer);
                    __msg.put_buffer(__id, __buffer);
                }
            }
        }
    } else {
        let [impl_generics, ty_generics, where_clause] = extend_generics(generics, ImplTrait::To);

        quote! {
            impl #impl_generics __tvf::ToField<__TVF> for #type_name #ty_generics #where_clause {
                fn to_field(&self, __id: usize, __msg: &mut __TVF) {
                    let mut __buffer = __TVF::default();
                    self.to_tvf(&mut __buffer);
                    __msg.put_buffer(__id, __buffer);
                }
            }
        }
    }
}

// MARK: FromTvf

impl<'f> TvfEnum<'f> {
    pub(crate) fn impl_from_tvf(&self) -> TokenStream {
        // Prepare tokens
        let type_name = self.type_ident;
        let [impl_generics, ty_generics, where_clause] =
            extend_generics(self.generics.clone(), ImplTrait::From);

        // handle all cases of the enumeration
        let decl_discris = self.decl_discris(self.attr.tag_type, false);
        let mut cases = Vec::with_capacity(self.variants.len());
        let get_variant = self.attr.tag_type.get_method();
        let tag_id = &self.attr.tag_id;

        // Process all variants
        for variant in self.variants.iter() {
            // (de)structure the variant
            let var_ident = variant.variant_ident;
            let fields = variant.fields.structuring();
            let discri = variant.ident_discri();
            let tokens = variant.fields.impl_from_tvf();

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
            // (de)structure the variant
            let var_ident = variant.variant_ident;
            let fields = variant.fields.structuring();
            let discri = variant.ident_discri();
            let tokens = variant.fields.impl_from_tvf();

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
                __tvf::TvfError::SerializationError(format!["Unknown tag \"{}\"", __disc])
            )]
        };

        // When reading a string from the TVf message, we need to
        // cast it to a `&str` to use it in the match statement.
        let as_str = if self.attr.tag_type == TagType::String {
            quote![ .as_str() ]
        } else {
            TokenStream::new()
        };

        let main_trait = quote! {
            impl #impl_generics __tvf::FromTvf<__TVF> for #type_name #ty_generics #where_clause {
                fn from_tvf(__msg: &__TVF) -> ::core::result::Result<Self, __tvf::TvfError>
                where
                    Self: ::core::marker::Sized,
                {
                    #decl_discris

                    let __disc = #get_variant(__msg, #tag_id)?;
                    match __disc #as_str {
                        #(#cases),*
                        _ => { #def_case }
                    }
                }
            }
        };
        let field_trait = impl_from_field(type_name, self.generics.clone());
        quote! { #main_trait #field_trait }
    }
}

impl<'f> TvfStruct<'f> {
    pub(crate) fn impl_from_tvf(&self) -> TokenStream {
        // Prepare tokens
        let type_name = self.type_ident;

        // add `T: Tvf` bounds on generics
        let [impl_generics, ty_generics, where_clause] =
            extend_generics(self.generics.clone(), ImplTrait::From);

        // (de)structure the type
        let fields = self.fields.structuring();
        let tokens = self.fields.impl_from_tvf();

        let main_trait = quote! {
            impl #impl_generics __tvf::FromTvf<__TVF> for #type_name #ty_generics #where_clause {
                fn from_tvf(__msg: &__TVF) -> ::core::result::Result<Self, __tvf::TvfError>
                where
                    Self: ::core::marker::Sized,
                {
                    #tokens
                    Ok(Self #fields)
                }
            }
        };
        let field_trait = impl_from_field(type_name, self.generics.clone());
        quote! { #main_trait #field_trait }
    }
}

impl<'f> TvfFields<'f> {
    /// Generate tokens for FromTvf implementation
    pub(crate) fn impl_from_tvf(&self) -> TokenStream {
        // Collect tokens
        let len = self.fields.len();
        let decls = self.decl_field_ids();
        let mut tokens = Vec::with_capacity(len);

        // iterate over each field
        for field in self.fields.iter() {
            let ty = &field.field.ty;
            let ident = field.tmp_ident();
            let id = field.ident_field_id();

            // Select "tvf_from" function
            let f = if let Some(func) = &field.attr.custom_from_tvf {
                quote![ let #ident = #func(__msg, #id)?; ]
            } else {
                quote![ let #ident = <#ty as __tvf::FromField<__TVF>>::from_field(__msg, #id)?; ]
            };
            tokens.push(f);
        }

        // return tokens
        quote![ #decls #(#tokens)* ]
    }
}

fn impl_from_field(type_name: &Ident, generics: Generics) -> TokenStream {
    let [impl_generics, ty_generics, where_clause] = extend_generics(generics, ImplTrait::From);

    quote! {
        impl #impl_generics __tvf::FromField<__TVF> for #type_name #ty_generics #where_clause {
            fn from_field(__msg: &__TVF, __id: usize) -> Result<Self, __tvf::TvfError> {
                let __buffer = __msg.get_buffer(__id)?;
                <Self as __tvf::FromTvf<__TVF>>::from_tvf(__buffer.as_ref())
            }
        }
    }
}

// MARK: Common

impl<'f> TvfEnum<'f> {
    /// Declare the variant discriminants
    pub(crate) fn decl_discris(&self, tag_type: TagType, include_default: bool) -> TokenStream {
        // Decalre constants for each variant
        let mut decls = self
            .variants
            .iter()
            .map(|v| v.decl_discri(tag_type))
            .collect::<Vec<_>>();

        // If a default variant was defined, include it too
        if include_default && let Some(default) = &self.default_variant {
            decls.push(default.decl_discri(tag_type));
        }

        // Output the tokens
        decls.into_iter().collect()
    }
}

impl<'f> TvfVariant<'f> {
    /// Generate an identifier for the discriminant constant
    pub(crate) fn ident_discri(&self) -> Ident {
        format_ident!("__{}", self.variant_ident.to_string().to_uppercase())
    }

    /// Declare a discriminant constant
    pub(crate) fn decl_discri(&self, tag_type: TagType) -> TokenStream {
        let ident = self.ident_discri();

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
            #[rustfmt::skip]
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

    /// Declare constants for the field numeric identifiers
    pub(crate) fn decl_field_ids(&self) -> TokenStream {
        self.fields.iter().map(|f| f.decl_field_id()).collect()
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

    /// Create a identifier for the field ID
    #[inline]
    pub(crate) fn ident_field_id(&self) -> Ident {
        format_ident!("__ID_{}", self.index)
    }

    /// Declare the field ID
    pub(crate) fn decl_field_id(&self) -> TokenStream {
        let ident = self.ident_field_id();
        let value = if let Some(id) = &self.attr.field_id {
            quote![ (#id) as usize ]
        } else {
            let index = self.index;
            quote![ #index ]
        };
        quote![ const #ident: usize = #value; ]
    }
}
