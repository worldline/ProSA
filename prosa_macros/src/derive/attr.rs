use crate::derive::ATTRIBUTE;
use syn::{
    Attribute, DeriveInput, Expr, Lit, LitInt, Meta, MetaNameValue, Path, Token, parse_str,
    punctuated::Punctuated, spanned::Spanned,
};

/// Error encountered when parsing attributes
#[derive(thiserror::Error, Debug, Clone, Copy)]
pub enum AttrError {
    #[error("Wrong format, expect a list of tags separated by commas")]
    Format,

    #[error("Failed to parse a path")]
    Path,

    #[error("Multiple default variants")]
    MultiDefault,

    #[error("Variant value is too large for being encoded into a byte")]
    TooBigVariant,
}

/// Attributes defined on an enum variant
#[derive(Default, Clone)]
pub struct AttrVariant {
    /// Is default variant type?
    pub default: bool,

    /// Override numeric representation
    pub custom_id: Option<usize>,
}

impl AttrVariant {
    pub fn identify(attrs: &[Attribute]) -> Result<Self, AttrError> {
        let mut default = false;
        let mut custom_id = None;

        // Iterate over all the attributes of the field and only
        // pick the attributes of the form `#[tvf(...)]`.
        for attr in attrs.iter() {
            if attr.path().is_ident(ATTRIBUTE)
                && let Meta::List(list) = &attr.meta
            {
                let _ = list.parse_nested_meta(|meta| {
                    if meta.path.is_ident("default") {
                        default = true;
                        Ok(())
                    } else if meta.path.is_ident("id") {
                        meta.input.parse::<Token![=]>()?;
                        let val: LitInt = meta.input.parse()?;
                        custom_id = Some(val.base10_parse()?);
                        Ok(())
                    } else {
                        Err(syn::Error::new(attr.span(), "Unsupported attribute value"))
                    }
                });
            }
        }

        Ok(Self { default, custom_id })
    }
}

/// Attributes defined on a field
#[derive(Default, Clone)]
pub struct AttrField {
    /// Override default to_tvf implementation
    pub custom_to_tvf: Option<Path>,

    /// Override default from_tvf implementation
    pub custom_from_tvf: Option<Path>,
}

impl AttrField {
    pub fn identify(attrs: &[Attribute]) -> Result<Self, AttrError> {
        let mut custom_to_tvf = None;
        let mut custom_from_tvf = None;

        // Iterate over all the attributes of the field and only
        // pick the attributes of the form `#[tvf(...)]`.
        for attr in attrs.iter() {
            if attr.path().is_ident(ATTRIBUTE)
                && let Meta::List(list) = &attr.meta
            {
                // Now look at the list of tags
                let name_values = list
                    .parse_args_with(Punctuated::<MetaNameValue, Token![,]>::parse_terminated)
                    .map_err(|_| AttrError::Format)?;
                for name_value in name_values {
                    let name = name_value.path;
                    if name.is_ident("to_tvf") {
                        custom_to_tvf = Some(path_from_expr(&name_value.value)?)
                    } else if name.is_ident("from_tvf") {
                        custom_from_tvf = Some(path_from_expr(&name_value.value)?)
                    }
                }
            }
        }

        Ok(Self {
            custom_to_tvf: custom_to_tvf,
            custom_from_tvf: custom_from_tvf,
        })
    }
}

fn path_from_expr(expr: &Expr) -> Result<Path, AttrError> {
    if let Expr::Lit(literal) = expr
        && let Lit::Str(string) = &literal.lit
        && let Ok(path) = parse_str(&string.value())
    {
        Ok(path)
    } else {
        Err(AttrError::Path)
    }
}

/// Checks if the input has `#[repr(u8)]`
pub(crate) fn has_repr_u8(input: &DeriveInput) -> bool {
    // 1. Iterate over all attributes of the struct/enum
    for attr in &input.attrs {
        // 2. Check if the attribute is `repr`
        if attr.path().is_ident("repr") {
            // 3. Parse the nested meta inside #[repr(...)]
            let mut found_u8 = false;
            let _ = attr.parse_nested_meta(|meta| {
                // 4. Check if one of the arguments is `u8`
                if meta.path.is_ident("u8") {
                    found_u8 = true;
                }
                Ok(())
            });

            if found_u8 {
                return true;
            }
        }
    }
    false
}
