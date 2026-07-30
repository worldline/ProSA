use crate::derive::ATTRIBUTE;
use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    Attribute, DeriveInput, Expr, Lit, LitInt, LitStr, Meta, MetaNameValue, Path, Token, parse_str,
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
}

/// Attributes defined on an enum type
#[derive(Clone)]
pub(crate) struct AttrEnum {
    /// Field identifier to identify the variant
    pub tag_id: usize,

    /// Type of value used to discriminate the variants
    pub tag_type: TagType,
}

impl AttrEnum {
    pub(crate) fn identify(attrs: &[Attribute]) -> Result<Self, AttrError> {
        let mut tag_id = None;
        let mut tag_type = None;

        // Iterate over all the attributes of the field and only
        // pick the attributes of the form `#[tvf(...)]`.
        for attr in attrs.iter() {
            if attr.path().is_ident(ATTRIBUTE)
                && let Meta::List(list) = &attr.meta
            {
                let _ = list.parse_nested_meta(|meta| {
                    if meta.path.is_ident("tag_type") {
                        meta.input.parse::<Token![=]>()?;
                        let val: LitStr = meta.input.parse()?;
                        tag_type = TagType::parse(&val.value()).ok();
                        Ok(())
                    } else if meta.path.is_ident("tag_id") {
                        meta.input.parse::<Token![=]>()?;
                        let val: LitInt = meta.input.parse()?;
                        tag_id = Some(val.base10_parse()?);
                        Ok(())
                    } else {
                        Err(syn::Error::new(attr.span(), "Unsupported attribute value"))
                    }
                });
            }
        }

        Ok(Self {
            tag_id: tag_id.expect("Tag field identifier is not set"),
            tag_type: tag_type.expect("Tag value type is not set"),
        })
    }
}

/// Attributes defined on an enum variant
#[derive(Default, Clone)]
pub(crate) struct AttrVariant {
    /// Is default variant type?
    pub default: bool,

    /// Variant tag value
    /// Default to variant name if tag type is set to string
    /// Default to variant discriminant otherwise
    pub tag: Option<Lit>,
}

impl AttrVariant {
    pub(crate) fn identify(attrs: &[Attribute]) -> Result<Self, AttrError> {
        let mut default = false;
        let mut tag = None;

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
                    } else if meta.path.is_ident("tag") {
                        meta.input.parse::<Token![=]>()?;
                        tag = Some(meta.input.parse()?);
                        Ok(())
                    } else {
                        Err(syn::Error::new(attr.span(), "Unsupported attribute value"))
                    }
                });
            }
        }

        Ok(Self { default, tag })
    }
}

/// Attributes defined on a field
#[derive(Default, Clone)]
pub(crate) struct AttrField {
    /// Override default to_tvf implementation
    pub custom_to_tvf: Option<Path>,

    /// Override default from_tvf implementation
    pub custom_from_tvf: Option<Path>,
}

impl AttrField {
    pub(crate) fn identify(attrs: &[Attribute]) -> Result<Self, AttrError> {
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

/// Specify the type of the field which encode the variant tag
#[derive(Default, PartialEq, Clone, Copy)]
pub(crate) enum TagType {
    /// Simple byte
    Byte,

    /// Signed integer
    Signed,

    /// Unsigned integer
    Unsigned,

    /// Text label
    #[default]
    String,
}

impl TagType {
    /// Identify a tag type from a string
    pub(crate) fn parse(label: &str) -> Result<Self, ()> {
        #[cfg_attr(rustfmt, rustfmt_skip)]
        match label {
            "u8"  | "byte"     => Ok(Self::Byte    ),
            "i64" | "signed"   => Ok(Self::Signed  ),
            "u64" | "unsigned" => Ok(Self::Unsigned),
            "str" | "string"   => Ok(Self::String  ),
            _ => Err(())
        }
    }

    /// Select the appropriate method to put the tag in the buffer
    pub(crate) fn put_method(&self) -> TokenStream {
        #[cfg_attr(rustfmt, rustfmt_skip)]
        let method = match self {
            TagType::Byte     => quote![ put_byte     ],
            TagType::Signed   => quote![ put_signed   ],
            TagType::Unsigned => quote![ put_unsigned ],
            TagType::String   => quote![ put_string   ],
        };
        quote![ <__TVF as __tvf::Tvf>::#method ]
    }

    /// Select the appropriate method to get the tag from the buffer
    pub(crate) fn get_method(&self) -> TokenStream {
        #[cfg_attr(rustfmt, rustfmt_skip)]
        let method = match self {
            TagType::Byte     => quote![ get_byte     ],
            TagType::Signed   => quote![ get_signed   ],
            TagType::Unsigned => quote![ get_unsigned ],
            TagType::String   => quote![ get_string   ],
        };
        quote![ <__TVF as __tvf::Tvf>::#method ]
    }
}
