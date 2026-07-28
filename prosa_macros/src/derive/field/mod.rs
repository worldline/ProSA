mod attribute;
mod deserialize;
mod serialize;

use attribute::FieldAttribute;
use proc_macro2::{Ident, Span, TokenStream};
use quote::{ToTokens, quote};
use syn::{Error, Expr, Fields, Type, parse_quote, spanned::Spanned};

/// A field of a struct or enum variant
pub(crate) struct TvfFieldData {
    /// index of the field in the struct
    index: usize,

    /// name of the field (if named struct)
    name: Option<Ident>,

    /// attribute of the field
    attribute: FieldAttribute,

    /// type of the field
    field_type: Type,

    /// specify if the field is of type Option<T>
    is_option: bool,
}

impl TvfFieldData {
    /// Ignoring wherever the fields are named or not
    /// Find all of the fields in the struct or enum variant
    pub(crate) fn gather_fields(fields: &Fields) -> Result<Vec<Self>, Error> {
        // perform a first pass on all the fields of the struct
        let mut list = Vec::<Self>::with_capacity(fields.len());

        for field in fields.iter() {
            // check the "tvf" attribute of the field
            let Ok(attribute) = FieldAttribute::from_attributes(&field.attrs) else {
                return Err(Error::new(field.span(), "Invalid attribute"));
            };
            list.push(Self {
                index: list.len(),
                name: field.ident.clone(),
                attribute,
                field_type: field.ty.clone(),
                is_option: is_option_type(&field.ty),
            });
        }
        Ok(list)
    }

    /// Destructure the struct or enum variant into its fields
    pub(crate) fn destructure_fields(fields: &[Self]) -> TokenStream {
        let destructure_tokens = fields.iter().map(|f| f.destructure_field_tokens());
        quote! [ { #(#destructure_tokens)* } ]
    }

    /// Generate the code to destructure the field
    fn destructure_field_tokens(&self) -> TokenStream {
        let accessor = self.to_accessor();
        let variable = self.to_variable_name();
        quote! [ #accessor: #variable, ]
    }

    /// Generate a variable name for this field
    fn to_variable_name(&self) -> Ident {
        Ident::new(&format!("__field{}", self.index), Span::call_site())
    }

    /// Provide an accessor for this field
    fn to_accessor(&self) -> TokenStream {
        if let Some(name) = &self.name {
            name.to_token_stream()
        } else {
            syn::Index::from(self.index).to_token_stream()
        }
    }

    /// Find the field id to use for the corresponding TVF field
    fn to_field_id(&self) -> Expr {
        if let Some(field_id) = &self.attribute.field_id {
            field_id.clone()
        } else {
            let field_id = self.index + 1;
            parse_quote!(#field_id)
        }
    }
}

/// manually check a type to see if it is an Option
/// match with:
/// - ::core::option::Option
/// - ::std::option::Option
/// - core::option::Option
/// - std::option::Option
/// - option::Option
/// - Option
fn is_option_type(field_type: &Type) -> bool {
    if let Type::Path(type_path) = field_type {
        // check if the path contains the type 'Option'
        if let Some(index) = type_path
            .path
            .segments
            .iter()
            .position(|i| i.ident == "Option")
        {
            // if the path is longer than just 'Option',
            if index >= 2 {
                // check if the path starts with 'core' or 'std'
                let std_core = type_path.path.segments[index - 2].ident.to_string();
                if std_core != "core" && std_core != "std" {
                    return false;
                }
            }
            // check if the element just before 'Option' is 'option'
            if index >= 1 && type_path.path.segments[index - 1].ident != "option" {
                return false;
            }

            // the path has the expected format
            return true;
        }
    }
    false
}
