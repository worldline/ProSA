pub(crate) mod attribute;

pub(crate) mod field;

pub(crate) mod structure;

pub(crate) mod enumeration;

use proc_macro::TokenStream;
use quote::quote;

/// name of the attribute to find in the list of attributes
const ATTRIBUTE: &str = "tvf";

/// Surround the generated code with module and imports
pub(crate) fn module_setup(generated: proc_macro2::TokenStream) -> TokenStream {
    quote! [
        const _: () = {
            #[allow(unused_imports)]
            use prosa_utils::msg::tvf as __tvf;

            #generated
        };
    ]
    .into()
}
