use crate::derive::ATTRIBUTE;
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Attribute, Error, Expr, meta::ParseNestedMeta};

/// parameter used to skip the field
const PARAM_DENY_UNKNOWN: &str = "deny_unknown_fields";

/// parameter to specify that we can use a default value if the field is missing in the deserialization
const PARAM_SKIP_NONE: &str = "skip_serializing_none";

/*
    TODO:
    - define a AttributeParser trait for parsing the attributes
*/

/// Global parameters for a struct (or enum variant)
#[derive(Default)]
pub(crate) struct BaseAttribute {
    pub deny_unkown_fields: bool,
    pub skip_serializing_none: bool,
}

impl BaseAttribute {
    /// Create a new `BaseAttribute` from a list of attributes
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
    pub(crate) fn parse_parameter(
        &mut self,
        param_meta: &ParseNestedMeta<'_>,
    ) -> Result<(), Error> {
        if param_meta.path.is_ident(PARAM_SKIP_NONE) {
            // expected usage (skip_serializing_none)
            self.skip_serializing_none = true;
        } else if param_meta.path.is_ident(PARAM_DENY_UNKNOWN) {
            // expected usage (deny_unknown_fields)
            self.deny_unkown_fields = true;
        } else {
            return Err(param_meta.error("unknown parameter"));
        }
        Ok(())
    }

    /// Generate a counter to count the number of fields encountered
    pub(crate) fn deny_unkown_counter(&self) -> TokenStream {
        if self.deny_unkown_fields {
            quote! [ let mut fields_count = 0usize; ]
        } else {
            TokenStream::new()
        }
    }

    /// Generate a check to deny unknown fields
    pub(crate) fn deny_unknown_check(&self) -> TokenStream {
        if self.deny_unkown_fields {
            quote! [
                let buffer_size = <__TVF as __tvf::Tvf>::len(&__msg);
                if fields_count < buffer_size {
                    return ::core::result::Result::Err(__tvf::TvfError::SerializationError(
                        format! [
                            "Buffer contains {} unknown additional fields.",
                            buffer_size - fields_count
                        ]
                    ));
                }
            ]
        } else {
            TokenStream::new()
        }
    }

    /// Merge two attributes into one
    pub(crate) fn merge(&self, other: &Self) -> Self {
        Self {
            deny_unkown_fields: self.deny_unkown_fields || other.deny_unkown_fields,
            skip_serializing_none: self.skip_serializing_none || other.skip_serializing_none,
        }
    }
}

/// Parse an attribute where the provided value must evaluate to an unsigned integer (field id)
pub(crate) fn parse_id_attr(param_meta: &ParseNestedMeta<'_>) -> Result<Expr, Error> {
    let value: Expr = param_meta.value()?.parse()?;
    // accept only expressions that may be able to evaluate to an integer
    match value {
        Expr::Binary(_)
        | Expr::Block(_)
        | Expr::Call(_)
        | Expr::Cast(_)
        | Expr::Closure(_)
        | Expr::Const(_)
        | Expr::Field(_)
        | Expr::Index(_)
        | Expr::Lit(_)
        | Expr::Macro(_)
        | Expr::Paren(_)
        | Expr::Path(_)
        | Expr::Unary(_) => Ok(value),
        _ => Err(param_meta.error("expected an expression that evaluates to an integer")),
    }
}
