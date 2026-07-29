use super::attribute::SkipOption;
use crate::derive::{attribute::BaseAttribute, field::TvfFieldData};
use proc_macro2::TokenStream;
use quote::quote;

impl TvfFieldData {
    /// Serialize a list of fields (from a struct or enum variant)
    pub(crate) fn to_token_stream_for_serialize(
        fields: &[Self],
        attr: &BaseAttribute,
    ) -> TokenStream {
        // count the number of fields that will be serialized
        let len = fields.len();

        // serialize each field to produce the main token stream
        let serialize_tokens = fields
            .iter()
            .map(|f| f.serialize_field_tokens(attr.skip_serializing_none));

        // generate the code to serialize the fields
        quote! [
            {
                let mut __msg = __tvf::Tvf::with_capacity(#len);
                #(#serialize_tokens)*
                __msg
            }
        ]
    }

    /// Generate the code to serialize the field
    /// If the field is completely skipped, it will generate nothing.
    /// If the field has a skip condition, the serialization will be surounded
    /// by an if statement.
    /// This function expects that the struct or enum has been destructured beforehand.
    fn serialize_field_tokens(&self, skip_none: bool) -> TokenStream {
        // check if we should skip this field completely
        if let SkipOption::Skip = self.attribute.skip_serializing {
            return TokenStream::new();
        }

        let variable = self.to_variable_name();
        let field_id = self.to_field_id();

        // check if we should serialize this field with a custom function
        let serialize_tokens = if let Some(serialize_func) = &self.attribute.serialize_with {
            quote! [ #serialize_func(#variable) ]
        } else {
            quote! [ #variable.to_tlv_field() ]
        };

        // check if we should skip the serialization if the field is None
        let store_serialization = if skip_none && self.is_option {
            quote! [
                if let ::core::option::Option::Some(#variable) = #variable {
                    __msg.put_field(#field_id as usize, #serialize_tokens);
                }
            ]
        } else {
            quote! [ __msg.put_field(#field_id as usize, #serialize_tokens); ]
        };

        // if the skip is conditional, suround the serialization code with an if statement
        if let SkipOption::SkipIf(skip_if_func) = &self.attribute.skip_serializing {
            quote! [ if ! #skip_if_func(#variable) { #store_serialization } ]
        } else {
            store_serialization
        }
    }
}
