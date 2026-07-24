//! Dictionary for TVF message.
//! Define text mnemonic for fields and identify the type of data stored in a field.

pub mod deserialize;
pub mod serialize;
pub mod value;

use crate::msg::tvf::TvfType;

/// Trait to define a TVF[^tvfnote] dictionary.
/// Useful to map numerical tags with text-based field identifiers.
///
/// [^tvfnote]: **T**ag **V**alue **F**ormat
pub trait TvfDict {
    /// Given a numerical tag, return the corresponding text mnemonic
    fn from_id(&self, id: usize) -> Option<&str>;

    /// Given a text mnemonic, return the corresponding numerical tag
    fn from_mnemo(&self, mnemo: &str) -> Option<usize>;

    /// Get the type hint for a field
    fn type_hint(&self, id: usize) -> Option<TvfType>;

    /// Get a sub-dictionary for a field
    fn sub_dict(&self, _id: usize) -> Option<&dyn TvfDict> {
        None
    }
}

// re-export serializer types
pub use deserialize::TvfDeserializer;
pub use serialize::TvfSerializer;

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Bidirectional dictionary
    #[derive(Debug, Clone)]
    struct Dict {
        id_to_mnemo: HashMap<usize, String>,
        mnemo_to_id: HashMap<String, usize>,
    }

    impl Dict {
        fn new() -> Self {
            Self {
                id_to_mnemo: HashMap::new(),
                mnemo_to_id: HashMap::new(),
            }
        }

        fn add(&mut self, id: usize, mnemo: &str) {
            self.id_to_mnemo.insert(id, mnemo.to_string());
            self.mnemo_to_id.insert(mnemo.to_string(), id);
        }
    }

    /// Implementation of the trait allowing for tree-like dictionaries
    impl TvfDict for Dict {
        fn from_id(&self, id: usize) -> Option<&str> {
            self.id_to_mnemo.get(&id).map(|e| e.as_str())
        }

        fn from_mnemo(&self, mnemo: &str) -> Option<usize> {
            self.mnemo_to_id.get(mnemo).map(|e| *e)
        }

        fn type_hint(&self, _id: usize) -> Option<TvfType> {
            Some(TvfType::String)
        }
    }

    #[test]
    fn test_dict() {
        let mut dict = Dict::new();
        dict.add(1, "A");
        dict.add(2, "B");
        dict.add(3, "C");

        assert_eq!("A", dict.from_id(1).unwrap());
        assert_eq!(2, dict.from_mnemo("B").unwrap());
    }
}
