//! Dictionary module for TVF messages.
//! This allow associating labels to numeric identifier in TVF messages.

pub mod path;

use crate::msg::{
    tvf::{Tvf, TvfError},
    value::{TvfType, TvfValue},
};
use std::{borrow::Cow, collections::HashMap, ptr, sync::Arc};

/// Dictionary for associating labels with numeric identifiers.
/// Each entry can be further associated with an arbitrary definition.
/// Said definition can also be a sub-dictionary to support messages with a tree structures.
#[derive(Debug, Default, Clone)]
pub struct Dictionary<P> {
    id_to_label: HashMap<usize, (Arc<str>, Arc<Entry<P>>)>,
    label_to_id: HashMap<Arc<str>, (usize, Arc<Entry<P>>)>,
}

/// Definition associated with a field in a TVF message.
#[derive(Debug, Clone)]
pub enum Entry<P> {
    /// Entry with a custom definition.
    Leaf {
        /// The expected type of the field
        expected_type: TvfType,

        /// The arbitrary payload
        payload: P,
    },

    /// Entry expect a sub-buffer mapped to the provided sub-dictionary.
    Node(Box<Dictionary<P>>),

    /// Entry expect a sub-buffer that acts as a list.
    List(Box<Entry<P>>),
}

/// Binds a TVF message to a dictionary.
/// Provide methods to access fields using labels instead of numeric identifiers.
pub struct LabeledTvf<'dict, 't, P, T>
where
    P: Clone,
    T: Clone,
{
    dictionary: Cow<'dict, Dictionary<P>>,
    message: Cow<'t, T>,
}

impl<P> Dictionary<P> {
    /// Create a new empty dictionary
    #[inline]
    pub fn new() -> Self {
        Self {
            id_to_label: HashMap::new(),
            label_to_id: HashMap::new(),
        }
    }

    /// Create a new empty dictionary with the given capacity
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            id_to_label: HashMap::with_capacity(capacity),
            label_to_id: HashMap::with_capacity(capacity),
        }
    }

    /// Get the label corresponding to an identifier.
    #[inline]
    pub fn get_label(&self, id: usize) -> Option<&str> {
        self.id_to_label.get(&id).map(|(label, _)| label.as_ref())
    }

    /// Get the identifier corresponding to a label.
    #[inline]
    pub fn get_id(&self, label: &str) -> Option<usize> {
        self.label_to_id.get(label).map(|(id, _)| *id)
    }

    /// Add a new field definition to the dictionary.
    pub fn add_entry(&mut self, id: usize, label: &str, entry: Entry<P>) {
        let label: Arc<str> = label.into();
        let def = Arc::new(entry);
        self.id_to_label.insert(id, (label.clone(), def.clone()));
        self.label_to_id.insert(label, (id, def));
    }

    /// Get the label and the entry definition given an identifier.
    #[inline]
    pub fn get_entry_with_id(&self, id: usize) -> Option<(&str, &Entry<P>)> {
        self.id_to_label
            .get(&id)
            .map(|(label, entry)| (label.as_ref(), entry.as_ref()))
    }

    /// Get the identifier and the entry definition given a label.
    #[inline]
    pub fn get_entry_with_label(&self, label: &str) -> Option<(usize, &Entry<P>)> {
        self.label_to_id
            .get(label)
            .map(|(id, entry)| (*id, entry.as_ref()))
    }
}

impl<P> Entry<P> {
    /// Create a new entry
    #[inline]
    pub fn new(expected_type: TvfType, payload: P) -> Self {
        Self::Leaf {
            expected_type,
            payload,
        }
    }

    /// Create a new sub dictionary
    #[inline]
    pub fn new_dictionary(dictionary: Dictionary<P>) -> Self {
        Self::Node(Box::new(dictionary))
    }

    /// Create a new repeatable entry
    #[inline]
    pub fn new_repeatable(expected_type: TvfType, payload: P) -> Self {
        Self::List(Box::new(Self::Leaf {
            expected_type,
            payload,
        }))
    }

    /// Create a list of repeatable entries with common dictionary
    #[inline]
    pub fn new_repeatable_dictionary(dictionary: Dictionary<P>) -> Self {
        Self::List(Box::new(Self::Node(Box::new(dictionary))))
    }
}

/// Implement the `Default` trait for entries if the provided payload type implement `Default` too.
impl<P> Default for Entry<P>
where
    P: Default,
{
    #[inline]
    fn default() -> Self {
        Self::Leaf {
            expected_type: TvfType::Buffer,
            payload: P::default(),
        }
    }
}

/// Trivially convert a dictionary into an entry for another dictionary
impl<P> From<Dictionary<P>> for Entry<P> {
    #[inline]
    fn from(value: Dictionary<P>) -> Self {
        Entry::Node(Box::new(value))
    }
}

impl<P> PartialEq for Entry<P>
where
    P: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::Leaf {
                    expected_type: atype,
                    payload: apl,
                },
                Self::Leaf {
                    expected_type: btype,
                    payload: bpl,
                },
            ) => atype == btype && apl == bpl,
            (Self::Node(a), Self::Node(b)) => ptr::eq(a, b),
            (Self::List(a), Self::List(b)) => a == b,
            _ => false,
        }
    }
}

impl<'dict, 't, P, T> LabeledTvf<'dict, 't, P, T>
where
    P: Clone,
    T: Clone,
{
    /// Bind a dictionary to a TVF message to make a labeled message.
    #[inline]
    pub fn new(dictionary: Cow<'dict, Dictionary<P>>, message: Cow<'t, T>) -> Self {
        Self {
            dictionary,
            message,
        }
    }
}

impl<P, T> LabeledTvf<'_, '_, P, T>
where
    P: Clone,
    T: Clone,
{
    /// Provide access to the internal dictionary
    #[inline]
    pub fn dictionary(&self) -> &Dictionary<P> {
        self.dictionary.as_ref()
    }

    /// Provide access to the internal message
    #[inline]
    pub fn message(&self) -> &T {
        self.message.as_ref()
    }
}

impl<'t, P, T> LabeledTvf<'_, 't, P, T>
where
    P: Clone,
    T: Clone + Tvf,
{
    /// Access the field of a TVF message using a label instead of the numeric identifier.
    pub fn get_field(
        &'t self,
        label: &str,
        expected: TvfType,
    ) -> Result<TvfValue<'t, T>, TvfError> {
        if let Some(id) = self.dictionary.get_id(label) {
            Ok(self.message.get(id, expected)?)
        } else {
            Err(TvfError::FieldNotFound(0))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::msg::simple_string_tvf::SimpleStringTvf;
    use prosa_macros::tvf;
    use std::sync::OnceLock;
    extern crate self as prosa_utils;

    fn get_dictionnary() -> &'static Dictionary<i32> {
        static DICT: OnceLock<Dictionary<i32>> = OnceLock::new();
        DICT.get_or_init(|| {
            let mut sub_dict = Dictionary::default();
            sub_dict.add_entry(10, "first", Entry::new(TvfType::String, -1));
            sub_dict.add_entry(20, "second", Entry::new(TvfType::String, -22));
            sub_dict.add_entry(30, "third", Entry::new(TvfType::Unsigned, 333));

            let mut dict = Dictionary::default();
            dict.add_entry(2, "label2", Entry::new(TvfType::Unsigned, 24));
            dict.add_entry(4, "label4", Entry::new_dictionary(sub_dict.clone()));
            dict.add_entry(
                5,
                "label5",
                Entry::new_repeatable_dictionary(sub_dict.clone()),
            );

            dict
        })
    }

    #[test]
    fn test_dict() {
        let dict = get_dictionnary();
        assert_eq!(5, dict.get_id("label5").unwrap());
        assert_eq!("label5", dict.get_label(5).unwrap());

        let (id1, entry1) = dict.get_entry_with_label("label2").unwrap();
        assert_eq!(2, id1);
        let (label2, entry2) = dict.get_entry_with_id(2).unwrap();
        assert_eq!("label2", label2);
        assert_eq!(entry1, entry2);
    }

    #[test]
    fn test_labeled_tvf() {
        type V<'t> = TvfValue<'t, SimpleStringTvf>;

        let dict = get_dictionnary();
        let message = tvf!(SimpleStringTvf {
            2 => 7,
            4 => {
                10 => "hello",
                20 => "world",
                30 => 11,
                40 => 4.5,
                50 => "2025-01-01" as Date,
                60 => "2023-06-05 15:02:00.000" as DateTime,
                70 => 0x00010203_04050607_08090A0B_0C0D0E0F as Bytes,
            },
            5 => [
                {
                    10 => "hola",
                    20 => "mondo",
                    30 => 21
                },
                {
                    10 => "bonjour",
                    20 => "monde",
                    30 => 31
                }
            ],
            6 => "a line",
            7 => {
                1 => 100,
                2 => "text"
            }
        });
        let labeled = LabeledTvf::new(Cow::Borrowed(dict), Cow::Owned(message));

        assert_eq!(
            V::Unsigned(7),
            labeled.get_field("label2", TvfType::Unsigned).unwrap()
        );
    }
}
