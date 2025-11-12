use super::{Dictionary, EntryType};
use crate::msg::tvf::TvfError;
use std::fmt;

/// A path of unprocessed labels
#[derive(Debug, Clone)]
pub struct Path<const S: char = '.'>(pub Vec<PathElement>);

/// An element in a path, can be either a label or a tag
#[derive(Debug, Clone)]
pub enum PathElement {
    /// Numeric Tag
    Tag(usize),

    /// Label to resolve into a tag using a dictionary
    Label(String),
}

/// Error that can be encountered when resolving a path
#[derive(Debug, thiserror::Error)]
pub enum PathError {
    /// Encountered an unknown label in the provided path
    #[error("Encountered an invalid label \"{0}\" in the provided path.")]
    UnknownLabel(String),

    /// Invalid number in path
    #[error("Could not parse \"{0}\" as a number.")]
    ParseNumber(String),

    /// Reached the end of dictionary before the end of the path
    #[error("Reached the end of the dictionary before the end of the path \"{0}\".")]
    EndOfDict(String),

    /// Reached the end of the path before the end of the dictionary
    #[error("Reached the end of the path before the end of the dictionary.")]
    EndOfPath,

    /// TVF error encountered when accessing the message
    #[error("TVF error ({0}).")]
    Tvf(#[from] TvfError),
}

/// Parse the string into a path
impl<const S: char> From<&str> for Path<S> {
    fn from(path: &str) -> Self {
        let path = path.split(S).collect::<Vec<_>>();
        Self(path.iter().map(|&s| PathElement::from(s)).collect())
    }
}

/// Parse the string into a path element
impl From<&str> for PathElement {
    fn from(value: &str) -> Self {
        if let Ok(tag) = value.parse::<usize>() {
            Self::Tag(tag)
        } else {
            Self::Label(value.to_string())
        }
    }
}

/// Define how the path should be displayed
impl<const S: char> fmt::Display for Path<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.0.is_empty() {
            write!(f, "{}", self.0[0])?;
            for elem in &self.0[1..] {
                write!(f, "{S}{elem}")?;
            }
        }
        Ok(())
    }
}

/// Print the path element, either a number or a text
impl fmt::Display for PathElement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Tag(tag) => write!(f, "{tag}"),
            Self::Label(label) => write!(f, "{label}"),
        }
    }
}

impl<P> Dictionary<P> {
    /// Convert a path of labels into the corresponding path of tags
    #[inline]
    pub fn resolve_path_str<const S: char>(&self, path: &str) -> Result<Vec<usize>, PathError> {
        let path = Path::<S>::from(path);
        self.resolve_path(&path)
    }

    /// Convert a path of labels into the corresponding path of tags
    pub fn resolve_path<const S: char>(&self, path: &Path<S>) -> Result<Vec<usize>, PathError> {
        // List of tags defining a path in a TVF message
        let mut output = Vec::with_capacity(path.0.len());

        // Current state of the dictionary
        let mut current = Some(self);
        let mut repeatable = false;

        // iterate over each element of the path
        for element in path.0.iter() {
            if repeatable {
                // Currently processing a repeatable entry, we only expect numeric tags here
                match element {
                    PathElement::Tag(tag) => {
                        output.push(*tag);
                    }
                    PathElement::Label(label) => {
                        // The label is unknown in the dictionary
                        return Err(PathError::UnknownLabel(label.clone()));
                    }
                }
                repeatable = false;
            } else if let Some(dict) = current {
                // We currently have a reference to a dictionary
                match element {
                    // If it is a numeric tag, the path element is already resolved.
                    // Just pick the corresponding dictionary entry and keep going.
                    PathElement::Tag(tag) => {
                        output.push(*tag);
                        if let Some((_, entry)) = dict.get_label_and_entry(*tag)
                            && let EntryType::Node(sub_dict) = &entry.entry_type
                        {
                            current = Some(sub_dict.as_ref());
                            repeatable = entry.is_repeatable;
                        }
                    }
                    // If it is a text label, we need to find the corresponding numeric tag using the dictionary.
                    PathElement::Label(label) => {
                        if let Some((tag, entry)) = dict.get_id_and_entry(label) {
                            output.push(tag);
                            if let EntryType::Node(sub_dict) = &entry.entry_type {
                                current = Some(sub_dict.as_ref());
                                repeatable = entry.is_repeatable;
                            }
                        } else {
                            return Err(PathError::UnknownLabel(label.clone()));
                        }
                    }
                }
            } else {
                // We no longer have a dictionary, we can only process numeric tags now
                match element {
                    PathElement::Tag(tag) => {
                        output.push(*tag);
                    }
                    PathElement::Label(label) => {
                        return Err(PathError::EndOfDict(label.clone()));
                    }
                }
            }
        }
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use crate::dict::*;
    use std::sync::LazyLock;

    /// A sample dictionary for testing
    static DICT: LazyLock<Dictionary<()>> = LazyLock::new(|| {
        // Create a sub-directory to insert into the main one
        let mut sub_dict = Dictionary::with_capacity(3);
        sub_dict.add_entry(10, "name", Entry::new(TvfType::String, ()));
        sub_dict.add_entry(20, "birth", Entry::new(TvfType::Date, ()));
        sub_dict.add_entry(30, "address", Entry::new(TvfType::String, ()));
        let sub_dict = Arc::new(sub_dict);

        // Create the main dictionary
        let mut dict = Dictionary::with_capacity(6);
        dict.add_entry(1, "first", Entry::new(TvfType::Signed, ()));
        dict.add_entry(2, "second", Entry::new(TvfType::Float, ()));
        dict.add_entry(3, "third", Entry::new(TvfType::Bytes, ()));
        dict.add_entry(4, "fourth", Entry::new_sub_dict(sub_dict.clone(), ()));
        dict.add_entry(5, "fifth", Entry::new_list(TvfType::DateTime, ()));
        dict.add_entry(6, "sixth", Entry::new_list_sub_dict(sub_dict.clone(), ()));

        dict
    });

    #[test]
    fn dict_resolve_path() {
        assert_eq!(vec![1], DICT.resolve_path_str::<'.'>("first").unwrap());
        assert_eq!(vec![2], DICT.resolve_path_str::<'.'>("2").unwrap());
        assert_eq!(
            vec![4, 10],
            DICT.resolve_path_str::<'.'>("fourth.name").unwrap()
        );
        assert_eq!(vec![5, 1], DICT.resolve_path_str::<'.'>("fifth.1").unwrap());
        assert_eq!(
            vec![6, 2, 10],
            DICT.resolve_path_str::<'.'>("sixth.2.name").unwrap()
        );
    }
}
