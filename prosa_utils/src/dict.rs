pub mod ops;

use std::{collections::HashMap, rc::Rc, sync::Arc};

/// A dictionary mapping TVF field id with labels
#[derive(Default, Clone)]
pub struct Dictionary<P> {
    id_to_label: HashMap<usize, (Rc<str>, Rc<Entry<P>>)>,
    label_to_id: HashMap<Rc<str>, (usize, Rc<Entry<P>>)>,
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
