use super::*;

impl<P> Dictionary<P> {
    /// Create a new empty dictionary
    #[inline]
    pub fn new() -> Self {
        Self {
            tag_to_label: HashMap::new(),
            label_to_tag: HashMap::new(),
        }
    }

    /// Create a new dictionary with a capacity
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            tag_to_label: HashMap::with_capacity(capacity),
            label_to_tag: HashMap::with_capacity(capacity),
        }
    }

    /// Get the label of a field
    #[inline]
    pub fn get_label(&self, id: usize) -> Option<&str> {
        self.tag_to_label.get(&id).map(|(label, _)| label.as_ref())
    }

    /// Get the id of a field
    #[inline]
    pub fn get_id(&self, label: &str) -> Option<usize> {
        self.label_to_tag.get(label).map(|(id, _)| *id)
    }

    /// Get the label of a field and its corresponding entry
    #[inline]
    pub fn get_label_and_entry(&self, id: usize) -> Option<(&str, &Entry<P>)> {
        self.tag_to_label
            .get(&id)
            .map(|(label, entry)| (label.as_ref(), entry.as_ref()))
    }

    /// Get the id of a field and its corresponding entry
    #[inline]
    pub fn get_id_and_entry(&self, label: &str) -> Option<(usize, &Entry<P>)> {
        self.label_to_tag
            .get(label)
            .map(|(id, entry)| (*id, entry.as_ref()))
    }

    /// Add a new field definition to the dictionary
    pub fn add_entry(&mut self, id: usize, label: &str, entry: Entry<P>) {
        let label: Arc<str> = label.into();
        let def = Arc::new(entry);
        self.tag_to_label.insert(id, (label.clone(), def.clone()));
        self.label_to_tag.insert(label, (id, def));
    }
}

impl<P> Entry<P> {
    /// Helper function to extract a sub dictionary from this entry.
    /// This facilitate some recursive algorithms where we need to
    /// iterate over a dictionary while following another data structure.
    /// The additional integer returned correspond to the depth of the
    /// sub dictionary when extracted from an `Entry::List` variant.
    pub(crate) fn get_sub_dict(&self) -> Option<&Dictionary<P>> {
        match &self.entry_type {
            EntryType::Node(sub_dict) => Some(sub_dict.as_ref()),
            _ => None,
        }
    }

    /// Get the payload of the entry
    #[inline]
    pub fn get_payload(&self) -> &P {
        &self.payload
    }
}
