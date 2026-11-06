mod attribute;
mod variant;

use attribute::{EnumAttribute, TagType};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Attribute, DataEnum, Error, Ident};
use variant::TvfVariantData;

/// Serialize a list of fields from an enum
pub(crate) fn to_token_stream_for_serialize(
    type_name: &Ident,
    an_enum: &DataEnum,
    attributes: &[Attribute],
) -> Result<TokenStream, Error> {
    // parse the attributes of the enum
    let attr = EnumAttribute::from_attributes(attributes)?;

    // figure out which ids to use to store the tag and the content of the enum
    let (id_tag, id_content) = attr.evaluate_tags();

    // generate the token stream for each variant
    let variants = TvfVariantData::gather_variants(&an_enum.variants)?;
    let tag_const_tokens = variants.iter().map(|v| v.declare_tag_const(attr.tag_type));
    let serialize_tokens = variants
        .iter()
        .map(|v: &TvfVariantData| v.serialize_variant_tokens(type_name, &attr));
    let put_method = attr.tag_type.put_method();

    // generate the code to serialize the fields
    Ok(quote! [
        impl __tvf::ToTvf for #type_name {
            fn to_tvf_buffer(&self) -> __tvf::Tvf {
                #(#tag_const_tokens)*
                let (tag, content) = match self { #(#serialize_tokens)* };
                let mut enum_buffer = __tvf::Tvf::with_capacity(2);
                #put_method(&mut enum_buffer, #id_tag, tag);
                enum_buffer.put_buffer(#id_content, content);
                enum_buffer
            }
        }
    ])
}

/// Deserialize a list of fields from an enum
pub(crate) fn to_token_stream_for_deserialize(
    type_name: &Ident,
    an_enum: &DataEnum,
    attributes: &[Attribute],
) -> Result<TokenStream, Error> {
    // parse the attributes of the enum
    let attr = EnumAttribute::from_attributes(attributes)?;

    // figure out which ids to use to store the tag and the content of the enum
    let (id_tag, id_content) = attr.evaluate_tags();

    // generate the token stream for each variant
    let variants = TvfVariantData::gather_variants(&an_enum.variants)?;
    let tag_const_tokens = variants.iter().map(|v| v.declare_tag_const(attr.tag_type));
    let deserialize_tokens = variants.iter().map(|v| v.deserialize_variant_tokens(&attr));
    let get_method = attr.tag_type.get_method();

    // Because get_string return a Cow<'_, String>,
    // we need to handle the match statement differently.
    let match_tag = if attr.tag_type == TagType::String {
        quote![tag.as_str()]
    } else {
        quote![tag]
    };

    // error message if the tag value is not recognized
    let error_msg = format!("Invalid tag value for enum `{}`", type_name);

    // generate the code to deserialize the fields
    Ok(quote! [
        impl __tvf::FromTvf<__TVF> for #type_name {
            fn from_tvf(enum_buffer: &__TVF) ->
                ::core::result::Result<Self, __tvf::TvfError>
            {
                #(#tag_const_tokens)*
                let tag = #get_method(enum_buffer, #id_tag)?;
                let __msg = <__TVF as __tvf::Tvf>::
                    get_buffer(enum_buffer, #id_content)?;
                Ok(match #match_tag {
                    #(#deserialize_tokens)*
                    _ => return ::core::result::Result::Err(
                        __tvf::TvfError::SerializationError(#error_msg.to_string())
                    ),
                })
            }
        }
    ])
}
