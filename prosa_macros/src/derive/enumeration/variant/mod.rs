mod attribute;

use super::attribute::{EnumAttribute, TagType};
use crate::derive::field::TvfFieldData;
use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{Error, Expr, Ident, Variant, parse_quote, punctuated::Punctuated, token::Comma};

pub(crate) use attribute::VariantAttribute;

/// Data of a variant of an enum
pub(crate) struct TvfVariantData {
    /// index of the variant in the enum
    index: usize,

    /// name of the variant
    name: Ident,

    /// discriminant associated with the variant (must evaluate to an integer)
    discriminant: Option<Expr>,

    /// fields of the variant
    fields: Vec<TvfFieldData>,

    /// attributes of the variant
    attr: VariantAttribute,
}

impl TvfVariantData {
    /// Preprocess the variants of the enum
    pub(crate) fn gather_variants(
        variants: &Punctuated<Variant, Comma>,
    ) -> Result<Vec<Self>, Error> {
        // perform a first pass on all the variants of the enum
        let mut list = Vec::<Self>::with_capacity(variants.len());

        // assemble the list of variants with their corresponding data
        for (index, variant) in variants.iter().enumerate() {
            let attr = VariantAttribute::from_attributes(&variant.attrs)?;

            let fields = TvfFieldData::gather_fields(&variant.fields)?;
            list.push(Self {
                index,
                name: variant.ident.clone(),
                discriminant: if let Some((_, expr)) = &variant.discriminant {
                    Some(expr.clone())
                } else {
                    None
                },
                fields,
                attr,
            });
        }
        Ok(list)
    }

    /// Serialize a variant of an enum
    pub(crate) fn serialize_variant_tokens(
        &self,
        enum_name: &Ident,
        attr: &EnumAttribute,
    ) -> TokenStream {
        let variant_name = self.name.clone();
        let tag_value = self.to_tag_const_name();
        let destructure_tokens = TvfFieldData::destructure_fields(&self.fields);

        // merge the attributes of the enum with the attributes of the variant
        let base_attr = self.attr.base_attr.merge(&attr.base_attr);
        let serialize_tokens =
            TvfFieldData::to_token_stream_for_serialize(&self.fields, &base_attr);
        quote! [
            #enum_name::#variant_name #destructure_tokens =>
                (#tag_value, #serialize_tokens),
        ]
    }

    /// Serialize a variant of an enum
    pub(crate) fn deserialize_variant_tokens(&self, attr: &EnumAttribute) -> TokenStream {
        let variant_name = self.name.clone();
        let tag_value = self.to_tag_const_name();

        // merge the attributes of the enum with the attributes of the variant
        let base_attr = self.attr.base_attr.merge(&attr.base_attr);
        let deserialize_tokens =
            TvfFieldData::to_token_stream_for_deserialize(&self.fields, &base_attr);

        // if we must deny unknown fields, we count the number of fields encountered
        let deny_unknown_counter = base_attr.deny_unkown_counter();
        let deny_unkown_check = base_attr.deny_unknown_check();

        quote! [
            #tag_value => {
                #deny_unknown_counter
                let result = Self::#variant_name #deserialize_tokens;
                #deny_unkown_check
                result
            },
        ]
    }

    /// Generate a constat name for the tag value of the variant
    pub(crate) fn to_tag_const_name(&self) -> Ident {
        Ident::new(&format!("__TAG_TYPE{}", self.index), Span::call_site())
    }

    /// Declare a constant for the tag value of the variant
    pub(crate) fn declare_tag_const(&self, tag_type: TagType) -> TokenStream {
        let tag_const_name = self.to_tag_const_name();

        // handle strings and numeric types differently
        if tag_type == TagType::String {
            // get the string identifier for the tag value
            let tag_value = if let Some(value) = &self.attr.tag_value {
                value.clone()
            } else {
                let name = &self.name.to_string();
                Expr::Lit(parse_quote! [ #name ])
            };

            quote! [ const #tag_const_name: &str = #tag_value; ]
        } else {
            // get an integer identifier for the tag value
            let tag_value = if let Some(value) = &self.attr.tag_value {
                value.clone()
            } else if let Some(value) = &self.discriminant {
                value.clone()
            } else {
                let index = self.index;
                Expr::Lit(parse_quote! [ #index ])
            };

            // select the basic type to use
            let basic_type = match tag_type {
                TagType::Byte => quote![u8],
                TagType::Signed => quote![i64],
                TagType::Unsigned => quote![u64],
                _ => panic!("Unknown tag type"),
            };

            quote! [ const #tag_const_name: #basic_type = (#tag_value) as #basic_type; ]
        }
    }
}
