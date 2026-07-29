use crate::derive::{attribute::BaseAttribute, field::TvfFieldData};
use proc_macro2::TokenStream;
use quote::quote;

impl TvfFieldData {
    /// Deserialize a list of fields (from a struct or enum variant)
    pub(crate) fn to_token_stream_for_deserialize(
        fields: &[Self],
        attr: &BaseAttribute,
    ) -> TokenStream {
        // deserialize each field to produce the main token stream
        let deserialize_tokens = fields
            .iter()
            .map(|f| f.deserialize_field_tokens(attr.deny_unkown_fields));

        // generate the code to deserialize the fields
        quote! [ { #(#deserialize_tokens)* } ]
    }

    /// Generate the code to deserialize the field
    /// If the field is completely skipped, it will generate nothing.
    /// If the field is not found, it will either raise an error or use the default.
    pub(crate) fn deserialize_field_tokens(&self, deny_unknown: bool) -> TokenStream {
        let accessor = self.to_accessor();

        // check if we should skip this field completely
        if self.attribute.skip_deserializing {
            // if the field is skipped, we call the default function
            return if let Some(default) = &self.attribute.default {
                quote! [ #accessor: #default(), ]
            } else {
                quote! [ #accessor: ::core::default::Default::default(), ]
            };
        }

        let field_id = self.to_field_id();
        let type_name = &self.field_type;

        // check if we should serialize this field with a custom function
        let deserialize_tokens = if let Some(deserialize_func) = &self.attribute.deserialize_with {
            quote! [ #deserialize_func(value)? ]
        } else {
            quote! [ <#type_name>::from_tlv_field(value)? ]
        };

        // check if the field support default when it is missing
        let missing_tokens = if let Some(default) = &self.attribute.default {
            quote! [ #default() ]
        } else {
            quote! [ return ::core::result::Result::Err(__tvf::TvfError::FieldNotFound(#field_id as usize)); ]
        };

        // a known field has been encountered, increment the counter
        let increment_counter = if deny_unknown {
            quote! [ fields_count += 1; ]
        } else {
            TokenStream::new()
        };

        // check if the field was found
        quote! [
            #accessor: {
                if let ::core::result::Result::Ok(value) = __msg.get_field(#field_id as usize) {
                    #increment_counter
                    #deserialize_tokens
                } else {
                    #missing_tokens
                }
            },
        ]
    }
}
