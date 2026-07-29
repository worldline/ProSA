use crate::derive::{
    ATTRIBUTE,
    attribute::{BaseAttribute, parse_id_attr},
};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Attribute, Error, Expr, LitStr, meta::ParseNestedMeta, parse_quote};

/* SPECIFIC TO ENUM */

/// parameter used specify the field id where to store the tag
const PARAM_TAG_ID: &str = "tag";

/// parameter used specify the field id where to store the payload of the enum
const PARAM_PAYLOAD_ID: &str = "content";

/// parameter used specify a tag type
const PARAM_TAG_TYPE: &str = "tag_type";

/// Global parameters for an enum
#[derive(Default)]
pub(crate) struct EnumAttribute {
    pub tag_id: Option<Expr>,
    pub payload_id: Option<Expr>,
    pub tag_type: TagType,
    pub base_attr: BaseAttribute,
}

impl EnumAttribute {
    /// Create a new `EnumAttribute` from a list of attributes
    pub(crate) fn from_attributes(attributes: &[Attribute]) -> Result<Self, Error> {
        let mut attr = Self::default();
        attr.read_field_attributes(attributes)?;
        Ok(attr)
    }

    /// Read the tvf attributes of a field
    /// collect all of the parameters and store the result in Self
    fn read_field_attributes(&mut self, attributes: &[Attribute]) -> Result<(), Error> {
        // iterate over all the attributes of the field
        // to find the ones that use the "tvf" label
        for attribute in attributes {
            if attribute.path().is_ident(&ATTRIBUTE) {
                // only attributes of the form #[tvf(...)] are allowed
                attribute.parse_nested_meta(|meta| self.parse_parameter(&meta))?;
            }
        }
        Ok(())
    }

    /// Parse a single parameter of the tvf attribute
    fn parse_parameter(&mut self, param_meta: &ParseNestedMeta<'_>) -> Result<(), Error> {
        if param_meta.path.is_ident(PARAM_TAG_ID) {
            // expected usage (tag = <expression>)
            self.tag_id = Some(parse_id_attr(param_meta)?);
        } else if param_meta.path.is_ident(PARAM_PAYLOAD_ID) {
            // expected usage (content = <expression>)
            self.payload_id = Some(parse_id_attr(param_meta)?);
        } else if param_meta.path.is_ident(PARAM_TAG_TYPE) {
            // expected usage (tag_type = "type")
            let tag_type: LitStr = param_meta.value()?.parse()?;
            self.tag_type = match tag_type.value().to_ascii_lowercase().as_str() {
                "byte" => TagType::Byte,
                "signed" => TagType::Signed,
                "unsigned" => TagType::Unsigned,
                "string" => TagType::String,
                _ => return Err(param_meta.error("unknown tag type")),
            };
        } else {
            return self.base_attr.parse_parameter(param_meta);
        }
        Ok(())
    }

    pub(crate) fn evaluate_tags(&self) -> (Expr, Expr) {
        let id_tag: Expr = if let Some(expr) = &self.tag_id {
            expr.clone()
        } else {
            parse_quote![1]
        };
        let id_content: Expr = if let Some(expr) = &self.payload_id {
            expr.clone()
        } else {
            parse_quote![ #id_tag + 1 ]
        };
        (id_tag, id_content)
    }
}

/// Specify the type of the field which encode the variant tag
#[derive(Default, PartialEq, Clone, Copy)]
pub(crate) enum TagType {
    Byte,
    Signed,
    Unsigned,

    #[default]
    String,
}

impl TagType {
    /// Select the appropriate method to put the tag in the buffer
    pub(crate) fn put_method(&self) -> TokenStream {
        let method = match self {
            TagType::Byte => quote![put_byte],
            TagType::Signed => quote![put_signed],
            TagType::Unsigned => quote![put_unsigned],
            TagType::String => quote![put_string],
        };
        quote! [ <__TVF as __tvf::Tvf>::#method ]
    }

    /// Select the appropriate method to get the tag from the buffer
    pub(crate) fn get_method(&self) -> TokenStream {
        let method = match self {
            TagType::Byte => quote![get_byte],
            TagType::Signed => quote![get_signed],
            TagType::Unsigned => quote![get_unsigned],
            TagType::String => quote![get_string],
        };
        quote! [ <__TVF as __tvf::Tvf>::#method ]
    }
}
