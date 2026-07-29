use crate::derive::{attribute::BaseAttribute, field::TvfFieldData};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Attribute, DataStruct, Error, Ident};

/// Serialize a list of fields from a struct
pub(crate) fn to_token_stream_for_serialize(
    type_name: &Ident,
    structure: &DataStruct,
    attributes: &[Attribute],
) -> Result<TokenStream, Error> {
    // parse the attributes of the struct
    let attr = BaseAttribute::from_attributes(attributes)?;

    // parse the fields of the struct
    let fields = TvfFieldData::gather_fields(&structure.fields)?;
    let destructure_tokens = TvfFieldData::destructure_fields(&fields);
    let serialize_tokens = TvfFieldData::to_token_stream_for_serialize(&fields, &attr);

    // generate the code to serialize the fields
    Ok(quote! [
        impl __tvf::ToTvf for #type_name {
            fn to_tvf_buffer(&self) -> __tvf::Tvf {
                let Self #destructure_tokens = self;
                #serialize_tokens
            }
        }
    ])
}

/// Deserialize a list of fields from a struct
pub(crate) fn to_token_stream_for_deserialize(
    type_name: &Ident,
    structure: &DataStruct,
    attributes: &[Attribute],
) -> Result<TokenStream, Error> {
    // parse the attributes of the struct
    let attr = BaseAttribute::from_attributes(attributes)?;

    // parse the fields of the struct
    let fields = TvfFieldData::gather_fields(&structure.fields)?;
    let tokens = TvfFieldData::to_token_stream_for_deserialize(&fields, &attr);

    // if we must deny unknown fields, we count the number of fields encountered
    let deny_unknown_counter = attr.deny_unkown_counter();
    let deny_unkown_check = attr.deny_unknown_check();

    // generate the code to deserialize the fields
    Ok(quote! [
        impl __tvf::FromTvf<__TVF> for #type_name {
            fn from_tvf(__msg: &__TVF) ->
                ::core::result::Result<Self, __tvf::TvfError>
            {
                #deny_unknown_counter
                let result = Self #tokens;
                #deny_unkown_check
                Ok(result)
            }
        }
    ])
}
