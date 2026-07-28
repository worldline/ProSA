use crate::derive::{ATTRIBUTE, attribute::BaseAttribute};
use syn::{Attribute, Error, Expr, meta::ParseNestedMeta};

/* SPECIFIC TO ENUM VARIANT */

/// parameter used specify a tag type
const PARAM_TAG_VALUE: &str = "tag_value";

/// Global parameters for a struct (or enum variant)
#[derive(Default)]
pub(crate) struct VariantAttribute {
    pub tag_value: Option<Expr>,
    pub base_attr: BaseAttribute,
}

impl VariantAttribute {
    /// Create a new `VariantAttribute` from a list of attributes
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
        if param_meta.path.is_ident(PARAM_TAG_VALUE) {
            // expected usage (tag_value = <expression>)
            self.tag_value = Some(param_meta.value()?.parse()?);
        } else {
            return self.base_attr.parse_parameter(param_meta);
        }
        Ok(())
    }
}
