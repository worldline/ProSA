//! Module for ProSA TVF dictionary

pub mod ops;

#[cfg(all(feature = "dict", feature = "serde"))]
pub mod serialize;

#[cfg(all(feature = "dict", feature = "serde"))]
pub mod deserialize;

use crate::msg::{
    tvf::Tvf,
    value::{TvfType, TvfValue},
};
use ops::PathError;
use std::{borrow::Cow, collections::HashMap, fmt::Debug, sync::Arc};

/// A dictionary mapping TVF field id with labels
#[derive(Default, Clone)]
pub struct Dictionary<P> {
    id_to_label: HashMap<usize, (Arc<str>, Arc<Entry<P>>)>,
    label_to_id: HashMap<Arc<str>, (usize, Arc<Entry<P>>)>,
}

/// Definition associated with a field
#[derive(Clone)]
pub enum Entry<P> {
    /// Field is a leaf of the dictionary
    Leaf {
        /// Specify the expected type of the field
        expected_type: TvfType,

        /// Specify an arbitrary payload for this field
        payload: P,
    },

    /// Field contains a sub buffer with the corresponding dictionary
    Node(Arc<Dictionary<P>>),

    /// Field contains a list of sub fields matching with the definition rule
    List(Box<Entry<P>>),
}

/// Provide labels to a message
pub struct LabeledTvf<'dict, 't, P, T>
where
    P: Clone,
    T: Clone,
{
    dictionary: Cow<'dict, Dictionary<P>>,
    message: Cow<'t, T>,
}

impl<P> Dictionary<P> {
    /// Create a new dictionary
    #[inline]
    pub fn new() -> Self {
        Self {
            id_to_label: HashMap::new(),
            label_to_id: HashMap::new(),
        }
    }

    /// Create a new dictionary with a capacity
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            id_to_label: HashMap::with_capacity(capacity),
            label_to_id: HashMap::with_capacity(capacity),
        }
    }

    /// Get the label of a field
    #[inline]
    pub fn get_label(&self, id: usize) -> Option<&str> {
        self.id_to_label.get(&id).map(|(label, _)| label.as_ref())
    }

    /// Get the id of a field
    #[inline]
    pub fn get_id(&self, label: &str) -> Option<usize> {
        self.label_to_id.get(label).map(|(id, _)| *id)
    }

    /// Get the label of a field and its corresponding entry
    #[inline]
    pub fn get_label_and_entry(&self, id: usize) -> Option<(&str, &Entry<P>)> {
        self.id_to_label
            .get(&id)
            .map(|(label, entry)| (label.as_ref(), entry.as_ref()))
    }

    /// Get the id of a field and its corresponding entry
    #[inline]
    pub fn get_id_and_entry(&self, label: &str) -> Option<(usize, &Entry<P>)> {
        self.label_to_id
            .get(label)
            .map(|(id, entry)| (*id, entry.as_ref()))
    }

    /// Add a new field definition to the dictionary
    pub fn add_entry(&mut self, id: usize, label: &str, entry: Entry<P>) {
        let label: Arc<str> = label.into();
        let def = Arc::new(entry);
        self.id_to_label.insert(id, (label.clone(), def.clone()));
        self.label_to_id.insert(label, (id, def));
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
    pub fn new_dictionary(dictionary: Arc<Dictionary<P>>) -> Self {
        Self::Node(dictionary)
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
    pub fn new_repeatable_dictionary(dictionary: Arc<Dictionary<P>>) -> Self {
        Self::List(Box::new(Self::Node(dictionary)))
    }
}

impl<'dict, 't, P, T> LabeledTvf<'dict, 't, P, T>
where
    P: Clone,
    T: Clone,
{
    /// Create a new labeled message
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
    pub fn get_dictionary(&self) -> Cow<'_, Dictionary<P>> {
        self.dictionary.clone()
    }

    /// Provide access to the internal message
    pub fn get_message(&self) -> Cow<'_, T> {
        self.message.clone()
    }
}

impl<'t, P, T> LabeledTvf<'_, 't, P, T>
where
    P: Clone,
    T: Clone + Tvf + Default + Debug,
{
    /// Return the field for the given label
    pub fn get_field(&'t self, label: &str) -> Result<TvfValue<'t, T>, PathError> {
        if let Some((id, entry)) = self.dictionary.get_id_and_entry(label) {
            if let Entry::Leaf {
                expected_type,
                payload: _,
            } = entry
            {
                Ok(TvfValue::from_buffer(
                    self.message.as_ref(),
                    id,
                    *expected_type,
                )?)
            } else {
                Ok(TvfValue::Buffer(self.message.get_buffer(id)?))
            }
        } else {
            // TODO: return a more specific error
            Err(PathError::UnknownLabel(label.to_string()))
        }
    }
}
