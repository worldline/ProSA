//! Module for ProSA TVF dictionary

/// Base operations for the Dictionary type
pub mod base;

/// Define the type of a field in a dictionary
pub mod field_type;

/// Allow access to a field in a TVF message using a path
pub mod path;

/// Serialize a TVF message binded to a dictionary
#[cfg(all(feature = "dict", feature = "serde"))]
pub mod serialize;

/// Deserialize a TVF message using a dictionary
#[cfg(all(feature = "dict", feature = "serde"))]
pub mod deserialize;

use crate::dict::field_type::TvfType;
use std::{borrow::Cow, collections::HashMap, sync::Arc};

/// Binding between a TVF message and a dictionary.
pub struct LabeledTvf<'dict, 'tvf, P, T>
where
    P: Clone,
    T: Clone,
{
    /// Ignore unknown fields when serializing
    pub ignore_unknown: bool,

    /// Dictionary used to map TVF tags with labels
    pub dictionary: Cow<'dict, Dictionary<P>>,

    /// TVF message to analyze
    pub message: Cow<'tvf, T>,
}

/// A dictionary mapping TVF field tags with labels
#[derive(Default, Clone)]
pub struct Dictionary<P> {
    /// Given a numeric tag, find the associated label and access the extra data
    tag_to_label: HashMap<usize, (Arc<str>, Arc<Entry<P>>)>,

    /// Given a text label, find the associated tag and access the extra data
    label_to_tag: HashMap<Arc<str>, (usize, Arc<Entry<P>>)>,
}

/// Definition entry associated with a field
#[derive(Clone)]
pub struct Entry<P> {
    /// Specify if this entry is a repeatable field
    is_repeatable: bool,

    /// The type of data expected in this dictionary
    entry_type: EntryType<P>,

    /// Specify an arbitrary payload for this field
    payload: P,
}

/// Specify if the entry contains a sub dictionary or not
#[derive(Clone)]
pub enum EntryType<P> {
    /// There are sub nodes in the dictionary
    Node(Arc<Dictionary<P>>),

    /// End of the dictionary following this branch
    Leaf(TvfType),
}
