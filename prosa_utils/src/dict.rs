pub mod ops;

#[cfg(all(feature = "dict", feature = "serde"))]
pub mod serialize;

//#[cfg(all(feature = "dict", feature = "serde"))]
//pub mod deserialize;

use crate::msg::{tvf::Tvf, value::TvfValue};
use ops::PathError;
use std::{borrow::Cow, collections::HashMap, sync::Arc};

/// A dictionary mapping TVF field id with labels
#[derive(Default, Clone)]
pub struct Dictionary<P> {
    id_to_label: HashMap<usize, (Arc<str>, Arc<Entry<P>>)>,
    label_to_id: HashMap<Arc<str>, (usize, Arc<Entry<P>>)>,
}

/// Definition associated with a field
#[derive(Clone)]
enum Entry<P> {
    /// Field is a leaf of the dictionary
    Leaf(Arc<P>),

    /// Field contains a sub buffer with the corresponding dictionary
    SubDict(Dictionary<P>),

    /// Field contains a list of sub fields matching with the definition rule
    Repeatable(Box<Entry<P>>),
}

/// Provide labels to a message
pub struct LabeledTvf<'d, 't, P, T>
where
    P: Clone,
    T: Clone,
{
    dictionary: Cow<'d, Dictionary<P>>,
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
}

impl<'d, 't, P, T> LabeledTvf<'d, 't, P, T>
where
    P: Clone,
    T: Clone,
{
    /// Create a new labeled message
    pub fn new(dictionary: Cow<'d, Dictionary<P>>, message: Cow<'t, T>) -> Self {
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
    T: Clone + Tvf,
{
    /// Return the field for the given label
    pub fn get_field(&'t self, label: &str) -> Result<TvfValue<'t, T>, PathError> {
        if let Some(id) = self.dictionary.get_id(label) {
            Ok(self.message.get(id)?)
        } else {
            // TODO: return a more specific error
            Err(PathError::UnknownLabel(label.to_string()))
        }
    }
}
