use crate::derive::ATTRIBUTE;
use core::fmt;
use proc_macro2::{Span, TokenStream};
use quote::{ToTokens, quote};
use syn::{
    Attribute, Expr, Lit, Meta, MetaNameValue, Path, Token, parse_quote, parse_str,
    punctuated::Punctuated, spanned::Spanned,
};

/// Error encountered when parsing attributes
#[derive(thiserror::Error, Debug, Clone)]
#[error("Attr: {error}")]
pub struct AttrError {
    /// Where the error occured
    pub span: Span,

    // THe type of error
    pub error: ErrorKind,
}

/// Error encountered when parsing attributes
#[derive(thiserror::Error, Debug, Clone)]
pub enum ErrorKind {
    /// Misuse of the derive macro
    #[error("Unrecognized attribute: {0}")]
    Invalid(String),

    /// TVF message is missing a field for storing the variant tag
    #[error("Missing tag field to identify enum variant")]
    MissingTag,

    /// Format error in the `tvf` attribute
    #[error("Wrong format, expect a list of tags separated by commas")]
    Format,

    /// Invalid path to a function
    #[error("Failed to parse a path")]
    Path,

    /// An enum type cannot have multiple default variants
    #[error("Multiple default variants")]
    MultiDefault,
}

impl AttrError {
    #[inline]
    pub fn new(span: Span, error: ErrorKind) -> Self {
        Self { span, error }
    }
}

// MARK: Enum attributes

/// Attributes defined on an enum type
#[derive(Clone)]
pub(crate) struct AttrEnum {
    /// Field identifier to identify the variant
    pub tag_id: Box<Expr>,

    /// Type of value used to discriminate the variants
    pub tag_type: TagType,
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

impl AttrEnum {
    pub(crate) fn identify(
        attrs: &[Attribute],
        workbuffer: &mut Vec<AttrNameValue>,
    ) -> Result<Self, AttrError> {
        let mut tag_id = None;
        let mut tag_type = check_repr(attrs);

        // Collect relevant tvf attributes
        find_attributes(attrs, workbuffer)?;
        //let overrides = AttrOverrides::identify(workbuffer)?;

        for attr in workbuffer.iter() {
            match (attr.name.as_str(), attr.value.as_ref()) {
                ("tag_type", Some(value)) => {
                    // The value is expected to be a literal
                    if let Expr::Lit(literal) = value.as_ref()
                        && let Lit::Str(string) = &literal.lit
                        && let Ok(tag) = TagType::parse(&string.value())
                    {
                        tag_type = Some(tag);
                    } else {
                        return Err(AttrError::new(
                            attr.span,
                            ErrorKind::Invalid(attr.to_string()),
                        ));
                    }
                }
                ("tag_id", Some(value)) => {
                    tag_id = Some(value.clone());
                }
                _ => {
                    return Err(AttrError::new(
                        attr.span,
                        ErrorKind::Invalid(attr.to_string()),
                    ));
                }
            }
        }

        // While the tag type will default to `String`, we must explicitely define a `tag_id`
        if let Some(tag_id) = tag_id {
            Ok(Self {
                tag_id,
                tag_type: tag_type.unwrap_or_default(),
            })
        } else {
            Err(AttrError::new(Span::call_site(), ErrorKind::MissingTag))
        }
    }
}

impl TagType {
    /// Identify a tag type from a string
    #[rustfmt::skip]
    pub(crate) fn parse(label: &str) -> Result<Self, ()> {
        match label {
            "u8"  | "byte"     => Ok(Self::Byte    ),
            "i64" | "signed"   => Ok(Self::Signed  ),
            "u64" | "unsigned" => Ok(Self::Unsigned),
            "str" | "string"   => Ok(Self::String  ),
            _ => Err(())
        }
    }

    /// Select the appropriate method to put the tag in the buffer
    #[rustfmt::skip]
    pub(crate) fn put_method(&self) -> TokenStream {
        let method = match self {
            TagType::Byte     => quote![ put_byte     ],
            TagType::Signed   => quote![ put_signed   ],
            TagType::Unsigned => quote![ put_unsigned ],
            TagType::String   => quote![ put_string   ],
        };
        quote![ <__TVF as __tvf::Tvf>::#method ]
    }

    /// Select the appropriate method to get the tag from the buffer
    #[rustfmt::skip]
    pub(crate) fn get_method(&self) -> TokenStream {
        let method = match self {
            TagType::Byte     => quote![ get_byte     ],
            TagType::Signed   => quote![ get_signed   ],
            TagType::Unsigned => quote![ get_unsigned ],
            TagType::String   => quote![ get_string   ],
        };
        quote![ <__TVF as __tvf::Tvf>::#method ]
    }
}

// MARK: Variant attributes

/// Attributes defined on an enum variant
#[derive(Default, Clone)]
pub(crate) struct AttrVariant {
    /// Is default variant type?
    pub default: bool,

    /// Variant tag value
    /// Default to variant name if tag type is set to string
    /// Default to variant discriminant otherwise
    pub tag: Option<Box<Expr>>,

    /// Override defaults
    pub overrides: AttrOverrides,
}

impl AttrVariant {
    pub(crate) fn identify(
        attrs: &[Attribute],
        workbuffer: &mut Vec<AttrNameValue>,
    ) -> Result<Self, AttrError> {
        let mut default = false;
        let mut tag = None;

        // Collect relevant tvf attributes
        find_attributes(attrs, workbuffer)?;
        let overrides = AttrOverrides::identify(workbuffer)?;

        for attr in workbuffer.iter() {
            match (attr.name.as_str(), attr.value.as_ref()) {
                ("tag", Some(value)) => {
                    tag = Some(value.clone());
                }
                ("default", None) => {
                    default = true;
                }
                _ => {
                    return Err(AttrError::new(
                        attr.span,
                        ErrorKind::Invalid(attr.to_string()),
                    ));
                }
            }
        }

        Ok(Self {
            default,
            tag,
            overrides,
        })
    }
}

// MARK: Field attributes

/// Attributes defined on a field
#[derive(Default, Clone)]
pub(crate) struct AttrField {
    /// Field index where to store and read the value
    /// We store it as an expression to allow using predefined constants
    pub field_id: Option<Box<Expr>>,

    /// Specify whenever the field should fallback to a default value when missing
    pub default: AttrDefault,

    /// Specify the fields of the field should be written
    /// in the parent TVF buffer insteand into a sub-buffer.
    pub flatten: bool,

    /// Override defaults
    pub overrides: AttrOverrides,
}

#[derive(Default, Clone, PartialEq, Eq)]
pub(crate) enum AttrDefault {
    /// No default value, if missing, throw error
    #[default]
    None,

    /// Use `Default` implementation for the type
    Default,

    /// Use custom function for obtaining the default value
    Custom(Box<Path>),
}

impl AttrField {
    pub(crate) fn identify(
        attrs: &[Attribute],
        workbuffer: &mut Vec<AttrNameValue>,
    ) -> Result<Self, AttrError> {
        let mut field_id = None;
        let mut default = AttrDefault::None;
        let mut flatten = false;

        // Collect relevant tvf attributes
        find_attributes(attrs, workbuffer)?;
        let overrides = AttrOverrides::identify(workbuffer)?;

        for attr in workbuffer.iter() {
            match (attr.name.as_str(), attr.value.as_ref()) {
                ("id", Some(value)) => {
                    field_id = Some(value.clone());
                }
                ("default", value) => {
                    if let Some(value) = value {
                        default = AttrDefault::Custom(Box::new(path_from_expr(value)?));
                    } else {
                        default = AttrDefault::Default;
                    }
                }
                ("flatten", None) => {
                    flatten = true;
                }
                _ => {
                    return Err(AttrError::new(
                        attr.span,
                        ErrorKind::Invalid(attr.to_string()),
                    ));
                }
            }
        }

        Ok(Self {
            field_id,
            default,
            flatten,
            overrides,
        })
    }
}

// MARK: Overrides attributes

/// Attributes for overriding defaults
#[derive(Default, Clone)]
pub(crate) struct AttrOverrides {
    /// Override default `to_tvf` implementation
    pub custom_to_tvf: Option<Box<Path>>,

    /// Override default `into_tvf` implementation
    pub custom_into_tvf: Option<Box<Path>>,

    /// Override default `from_tvf` implementation
    pub custom_from_tvf: Option<Box<Path>>,

    /// Flag the field as skipped when writing to TVF
    pub skip_to_tvf: bool,

    /// Flag the field as skipped when reading from TVF
    pub skip_from_tvf: bool,
}

impl AttrOverrides {
    /// Identify override attributes in the workbuffer.
    /// Returns identified overrides and removes them from the workbuffer.
    /// This allow to report errors on unknown attributes in the caller handler.
    pub(crate) fn identify(workbuffer: &mut Vec<AttrNameValue>) -> Result<Self, AttrError> {
        let mut custom_to_tvf = None;
        let mut custom_into_tvf = None;
        let mut custom_from_tvf = None;
        let mut skip_to_tvf = false;
        let mut skip_from_tvf = false;

        for index in (0..workbuffer.len()).rev() {
            let elem = &workbuffer[index];

            // Check if this attribute is recognized
            let identified = match (elem.name.as_str(), elem.value.as_deref()) {
                ("skip", None) => {
                    skip_to_tvf = true;
                    skip_from_tvf = true;
                    true
                }
                ("skip_to_tvf", None) => {
                    skip_to_tvf = true;
                    true
                }
                ("skip_from_tvf", None) => {
                    skip_from_tvf = true;
                    true
                }
                ("to_tvf", Some(value)) => {
                    custom_to_tvf = Some(Box::new(path_from_expr(value)?));
                    true
                }
                ("into_tvf", Some(value)) => {
                    custom_into_tvf = Some(Box::new(path_from_expr(value)?));
                    true
                }
                ("from_tvf", Some(value)) => {
                    custom_from_tvf = Some(Box::new(path_from_expr(value)?));
                    true
                }
                ("with", Some(value)) => {
                    let path = path_from_expr(value)?;
                    custom_to_tvf = Some(Box::new(parse_quote![#path::to_tvf]));
                    custom_into_tvf = Some(Box::new(parse_quote![#path::into_tvf]));
                    custom_from_tvf = Some(Box::new(parse_quote![#path::from_tvf]));
                    true
                }
                _ => false,
            };

            // When identified, remove from workbuffer
            if identified {
                workbuffer.swap_remove(index);
            }
        }

        Ok(Self {
            custom_to_tvf,
            custom_into_tvf,
            custom_from_tvf,
            skip_to_tvf,
            skip_from_tvf,
        })
    }
}

// MARK: Attribute helpers

/// Represents a name-value pair in an attribute.
#[derive(Debug, Clone)]
pub(crate) struct AttrNameValue {
    /// Span of the attribute
    span: Span,

    /// Name of the attribute
    name: String,

    /// Optional value of the attribute
    value: Option<Box<Expr>>,
}

impl fmt::Display for AttrNameValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(value) = &self.value {
            write!(f, "{}={}", self.name, value.to_token_stream())
        } else {
            write!(f, "{}", self.name)
        }
    }
}

impl AttrNameValue {
    #[inline]
    fn new_tag(path: Path) -> Result<Self, AttrError> {
        let span = path.span();
        if let Some(name) = path.get_ident() {
            Ok(Self {
                span,
                name: name.to_string(),
                value: None,
            })
        } else {
            Err(AttrError::new(span, ErrorKind::Format))
        }
    }

    #[inline]
    fn new_tag_value(name_value: MetaNameValue) -> Result<Self, AttrError> {
        let span = name_value.path.span();
        if let Some(name) = name_value.path.get_ident() {
            Ok(Self {
                span,
                name: name.to_string(),
                value: Some(Box::new(name_value.value)),
            })
        } else {
            Err(AttrError::new(span, ErrorKind::Format))
        }
    }
}

/// Iterate over the list of attributes to find the TVF attributes in the form `#[tvf(...)]`.
/// Collect iterators over TVF metas attributes and store them in `out`
/// The `out` buffer is reused over the macro processing to avoid re-allocations.
fn find_attributes(attrs: &[Attribute], out: &mut Vec<AttrNameValue>) -> Result<(), AttrError> {
    out.clear();

    // Iterate over all the attributes of the field and only
    // pick the attributes of the form `#[tvf(...)]`.
    for attr in attrs.iter() {
        if attr.path().is_ident(ATTRIBUTE)
            && let Meta::List(list) = &attr.meta
        {
            // Now look at the list of tags
            let metas = list
                .parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
                .map_err(|_| AttrError::new(list.span(), ErrorKind::Format))?;
            for meta in metas {
                match meta {
                    Meta::Path(name) => {
                        out.push(AttrNameValue::new_tag(name)?);
                    }
                    Meta::NameValue(name_value) => {
                        out.push(AttrNameValue::new_tag_value(name_value)?);
                    }
                    _ => {
                        return Err(AttrError::new(meta.span(), ErrorKind::Format));
                    }
                }
            }
        }
    }
    Ok(())
}

fn path_from_expr(expr: &Expr) -> Result<Path, AttrError> {
    if let Expr::Lit(literal) = expr
        && let Lit::Str(string) = &literal.lit
        && let Ok(path) = parse_str(&string.value())
    {
        Ok(path)
    } else {
        Err(AttrError::new(expr.span(), ErrorKind::Path))
    }
}

/// Checks the type of representation for enum variants:
/// - `#[repr(u8)]`
/// - `#[repr(u64)]`
/// - `#[repr(i64)]`
pub(crate) fn check_repr(attrs: &[Attribute]) -> Option<TagType> {
    // 1. Iterate over all attributes of the struct/enum
    for attr in attrs.iter() {
        if let Some(ident) = attr.path().get_ident() {
            // 2. Check if the attribute is `repr`
            if ident == "repr" {
                // 3. Parse the nested meta inside #[repr(...)]
                let mut tag_type = None;
                let _ = attr.parse_nested_meta(|meta| {
                    let int = meta.path.require_ident()?.to_string();
                    // 4. Check if one of the arguments is any of the primitive integer types
                    tag_type = match int.as_str() {
                        "u8" => Some(TagType::Byte),
                        "u16" | "u32" | "u64" | "usize" => Some(TagType::Unsigned),
                        "i16" | "i32" | "i64" | "isize" | "i8" => Some(TagType::Signed),
                        _ => None,
                    };
                    Ok(())
                });

                // Return successfully identified tag type as soon as it is found.
                if tag_type.is_some() {
                    return tag_type;
                }
            }
        }
    }
    None
}
