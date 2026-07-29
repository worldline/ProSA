use crate::derive::attr::{AttrError, AttrField, AttrVariant};
use syn::{DataEnum, DataStruct, Expr, Fields, Generics, Ident};

/// Container for an enumeration
#[derive(Clone)]
pub(crate) struct TvfEnum<'e> {
    /// Identifier of the enum type
    pub type_ident: &'e Ident,

    /// Generics associated with the type
    pub generics: &'e Generics,

    /// Variants of the enumeration
    pub variants: Vec<TvfVariant<'e>>,

    /// Has default variant
    pub default_variant: Option<TvfVariant<'e>>,

    /// Check if the enum has a `#[repr(u8)]` attribute
    pub is_repr_u8: bool,
}

/// Container for a variant of an enumeration
#[derive(Clone)]
pub(crate) struct TvfVariant<'v> {
    /// The attributes that have been identified for this variant.
    pub attr: AttrVariant,

    /// Index of the variant
    pub index: usize,

    /// Discriminant of the variant
    pub discriminant: Option<&'v Expr>,

    /// Identifier of the enum variant
    pub variant_ident: &'v Ident,

    /// Fields of the variant
    pub fields: TvfFields<'v>,
}

/// Container for an structure
#[derive(Clone)]
pub(crate) struct TvfStruct<'s> {
    /// Identifier of the struct type
    pub type_ident: &'s Ident,

    /// Generics associated with the type
    pub generics: &'s Generics,

    /// Fields of the variant
    pub fields: TvfFields<'s>,
}

/// Container for the list of fields as well as the number of const-sized fields.
#[derive(Clone)]
pub(crate) struct TvfFields<'f> {
    /// List of fields ordered with constant sized fields first
    pub fields: Vec<TvfField<'f>>,
}

/// A field with its attributes.
#[derive(Clone)]
pub(crate) struct TvfField<'f> {
    /// The attributes that have been identified for this field.
    pub attr: AttrField,

    /// The original index of the field before reordering
    pub index: usize,

    /// The field itself.
    pub field: &'f syn::Field,
}

impl<'f> TvfEnum<'f> {
    /// Analyze the enum
    pub fn new(
        type_ident: &'f Ident,
        generics: &'f Generics,
        enum_data: &'f DataEnum,
        is_repr_u8: bool,
    ) -> Result<Self, AttrError> {
        // We will collect metadata about the variants as we iterate over them.
        let var_count = enum_data.variants.len();
        let mut out_vars = Vec::with_capacity(var_count);
        let mut count_def = 0;

        // Look for a default variant if any
        let mut def_var = None;

        // Iterate over all the fields and identify their attributes.
        for (index, variant) in enum_data.variants.iter().enumerate() {
            // Parse the attributes of the field and identify them.
            let var_attr = AttrVariant::identify(&variant.attrs)?;
            let is_def = var_attr.default;

            // Recover the fields of the variant
            let fields = TvfFields::new(&variant.fields)?;

            // Count the number of variants tagged as default
            let var = TvfVariant {
                attr: var_attr,
                index,
                discriminant: variant.discriminant.as_ref().map(|d| &d.1),
                variant_ident: &variant.ident,
                fields,
            };

            // Verify the variant is valid
            if !var.check_repr(is_repr_u8) {
                return Err(AttrError::TooBigVariant);
            }

            // A variant was tagged as default
            if is_def {
                count_def += 1;
                def_var = Some(var);
            } else {
                // Add the variant to the list
                out_vars.push(var);
            }
        }

        if count_def > 1 {
            Err(AttrError::MultiDefault)
        } else {
            // Return the ordered list of fields
            Ok(Self {
                type_ident,
                generics,
                variants: out_vars,
                default_variant: def_var,
                is_repr_u8,
            })
        }
    }
}

impl<'f> TvfVariant<'f> {
    /// Return true if the variant value is compatible with the target representation
    pub fn check_repr(&self, is_repr_u8: bool) -> bool {
        // Max value for a byte
        const MAX: usize = u8::MAX as usize;

        // We only need to verify when the target encoding is u8 instead of varint
        if is_repr_u8 {
            // Evaluate the numeric type identifying the variant.
            if let Some(custom_id) = self.attr.custom_id {
                custom_id <= MAX
            } else if let Some(_) = self.discriminant {
                true
            } else {
                self.index <= MAX
            }
        } else {
            true
        }
    }
}

impl<'f> TvfStruct<'f> {
    /// Analyze the struct
    pub fn new(
        type_ident: &'f Ident,
        generics: &'f Generics,
        struct_data: &'f DataStruct,
    ) -> Result<Self, AttrError> {
        let fields = TvfFields::new(&struct_data.fields)?;

        // Return the ordered list of fields
        Ok(Self {
            type_ident,
            generics,
            fields,
        })
    }
}

impl<'f> TvfFields<'f> {
    /// Analyze the fields
    pub fn new(fields: &'f Fields) -> Result<Self, AttrError> {
        // We will collect metadata about the fields as we iterate over them.
        let mut out_fields = Vec::with_capacity(fields.len());

        // Iterate over all the fields and identify their attributes.
        for (index, field) in fields.iter().enumerate() {
            // Parse the attributes of the field and identify them.
            let attr = AttrField::identify(&field.attrs)?;

            // Add the new entry to the list
            out_fields.push(TvfField { attr, index, field });
        }

        // Return the ordered list of fields
        Ok(Self { fields: out_fields })
    }
}
