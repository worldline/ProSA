use syn::{Attribute, Error, Expr, ExprPath, LitStr, meta::ParseNestedMeta, parse_quote};

use crate::derive::{ATTRIBUTE, attribute::parse_id_attr};

/// parameter of the attribute to specify the id of the field
const PARAM_ID: &str = "id";

/// parameter to specify that we can use a default value if the field is missing in the deserialization
const PARAM_DEFAULT: &str = "default";

/// parameter used to skip the field
const PARAM_SKIP: &str = "skip";

/// parameter for skipping the serialization of a field
const PARAM_SKIP_SERIALIZING: &str = "skip_serializing";

/// parameter for skipping the deserialization of a field
const PARAM_SKIP_DESERIALIZING: &str = "skip_deserializing";

/// parameter for skipping the serialization of a field if a condition is met
const PARAM_SKIP_IF: &str = "skip_serializing_if";

/// parameter for overriding the serialization of a field
const PARAM_SERIALIZE_WITH: &str = "serialize_with";

/// parameter for overriding the deserialization of a field
const PARAM_DESERIALIZE_WITH: &str = "deserialize_with";

/// parameter for overriding the serialization and the deserialization of a field
const PARAM_WITH: &str = "with";

/// Option to specify if a field should be skipped on serialization
#[derive(Default)]
pub(crate) enum SkipOption {
    #[default]
    DontSkip,
    Skip,
    SkipIf(ExprPath),
}

/// Attribute parameters for a field
#[derive(Default)]
pub(crate) struct FieldAttribute {
    pub field_id: Option<Expr>,
    pub default: Option<ExprPath>,
    pub skip_serializing: SkipOption,
    pub skip_deserializing: bool,
    pub serialize_with: Option<ExprPath>,
    pub deserialize_with: Option<ExprPath>,
}

impl FieldAttribute {
    /// Create a new `FieldAttribute` from a list of attributes
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
        if param_meta.path.is_ident(PARAM_ID) {
            // expected usage (id = <expression>)
            self.field_id = Some(parse_id_attr(param_meta)?);
            Ok(())
        } else if param_meta.path.is_ident(PARAM_DEFAULT) {
            // expected usage (default) or (default = "path::to::function")
            if let Ok(parse_buffer) = param_meta.value() {
                // a function was provided to generate the default value
                let Result::<LitStr, Error>::Ok(value) = parse_buffer.parse() else {
                    return Err(
                        param_meta.error("expected a path to a function surrounded by \"...\"")
                    );
                };
                let Ok(expr) = syn::parse_str::<ExprPath>(&value.value()) else {
                    return Err(param_meta.error("string literal does not point to a function"));
                };
                self.default = Some(expr);
            } else {
                // by default use the Default trait value
                self.default = Some(parse_quote![::core::default::Default::default]);
            }
            Ok(())
        } else if param_meta.path.is_ident(PARAM_SKIP) {
            // expected usage (skip)
            self.skip_serializing = SkipOption::Skip;
            self.skip_deserializing = true;
            Ok(())
        } else if param_meta.path.is_ident(PARAM_SKIP_SERIALIZING) {
            // expected usage (skip_serializing)
            self.skip_serializing = SkipOption::Skip;
            Ok(())
        } else if param_meta.path.is_ident(PARAM_SKIP_DESERIALIZING) {
            // expected usage (skip_deserializing)
            self.skip_deserializing = true;
            Ok(())
        } else if param_meta.path.is_ident(PARAM_SKIP_IF) {
            // expected usage (skip_serializing_if = "path::to::function")
            let expr = parse_path_attr(param_meta, "function")?;
            self.skip_serializing = SkipOption::SkipIf(expr);
            Ok(())
        } else if param_meta.path.is_ident(PARAM_SERIALIZE_WITH) {
            // expected usage (serialize_with = "path::to::function")
            let expr = parse_path_attr(param_meta, "function")?;
            self.serialize_with = Some(expr);
            Ok(())
        } else if param_meta.path.is_ident(PARAM_DESERIALIZE_WITH) {
            // expected usage (deserialize_with = "path::to::function")
            let expr = parse_path_attr(param_meta, "function")?;
            self.deserialize_with = Some(expr);
            Ok(())
        } else if param_meta.path.is_ident(PARAM_WITH) {
            // expected usage (with = "path::to::module")
            let expr = parse_path_attr(param_meta, "module")?;
            self.serialize_with = Some(expr.clone());
            self.deserialize_with = Some(expr);
            Ok(())
        } else {
            Err(param_meta.error("unknown parameter"))
        }
    }
}

/// Parse an attribute of the form #[tvf(the_function = "path::to::function")]
fn parse_path_attr(param_meta: &ParseNestedMeta<'_>, designator: &str) -> Result<ExprPath, Error> {
    let Result::<LitStr, Error>::Ok(value) = param_meta.value()?.parse() else {
        return Err(param_meta.error(format![
            "expected a path to a {} surrounded by \"...\"",
            designator
        ]));
    };
    let Ok(expr) = syn::parse_str::<ExprPath>(&value.value()) else {
        return Err(param_meta.error(format!["string literal does not point to a {}", designator]));
    };
    Ok(expr)
}
