//! Module for defining path of labels and numerical identifiers.
//! This is usefull for accessing a specific field in a labeled TVF message.

use super::{Dictionary, Entry};
use crate::msg::tvf::TvfError;
use std::fmt;

/// Error that can be encountered when resolving a path
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum PathError {
    /// Encountered an unknown label in the provided path
    #[error("Encountered an invalid label \"{0}\" in the provided path.")]
    UnknownLabel(String),

    /// Invalid number in path
    #[error("Could not parse \"{0}\" as a number.")]
    ParseNumber(String),

    /// Reached the end of dictionary before the end of the path
    #[error("Reached the end of the dictionary before the end of the path [{0}].")]
    EndOfDict(Path),

    /// Reached the end of the path before the end of the dictionary
    #[error("Reached the end of the path before the end of the dictionary.")]
    EndOfPath,

    /// TVF error encountered when accessing the message
    #[error("TVF error ({0}).")]
    Tvf(#[from] TvfError),
}

/// Path to a field in a TVF message
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Path {
    /// Elements of the path
    elements: Vec<PathElement>,

    /// Separator used between the elements
    separator: char,
}

/// Path element which is either a numerical identifier or a label.
#[derive(Debug, Clone, PartialEq, Eq)]
enum PathElement {
    /// Numerical identifier to directly access a field
    Id(usize),

    /// Label alias to a field
    Label(String),
}

/// Allow to display a path as "label1.20.label2" or "label1/20/label2"
impl fmt::Display for Path {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some((head, tail)) = self.elements.split_first() {
            write!(f, "{}", head)?;
            for element in tail {
                write!(f, "{}{}", self.separator, element)?;
            }
        }
        Ok(())
    }
}

/// Allow to display a single path element
impl fmt::Display for PathElement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Id(id) => write!(f, "{}", id),
            Self::Label(label) => write!(f, "{}", label),
        }
    }
}

/// Convert a simple path into an struct that can be reported in an error
impl Path {
    /// Create a path from a raw string
    pub fn from_raw(path: &str, separator: char) -> Self {
        let elements = path.split(separator).map(PathElement::new).collect();
        Self {
            elements,
            separator,
        }
    }

    /// Create a path from a slice
    #[inline]
    fn from_slice(elements: &[PathElement], separator: char) -> Self {
        Self {
            elements: elements.to_vec(),
            separator,
        }
    }
}

impl PathElement {
    /// Create a path element from a string.
    fn new(text: &str) -> Self {
        if let Ok(id) = text.parse::<usize>() {
            Self::Id(id)
        } else {
            Self::Label(text.to_string())
        }
    }
}

impl<P> Dictionary<P> {
    /// Convert a path of labels into the corresponding path of ids
    #[inline]
    pub fn resolve_raw_path(&self, path: &str, separator: char) -> Result<Vec<usize>, PathError> {
        let path = Path::from_raw(path, separator);
        self.resolve_path(&path)
    }

    /// Convert a path of labels into the corresponding path of ids
    pub fn resolve_path(&self, path: &Path) -> Result<Vec<usize>, PathError> {
        let mut output = Vec::with_capacity(path.elements.len());

        // Check the root of the buffer
        if let Some((element, tail)) = path.elements.split_first() {
            // Given either an identifier or a label,
            // we can try to find a corresponding sub-dictionary.
            match element {
                PathElement::Id(id) => {
                    output.push(*id);
                    if let Some((_, node)) = self.id_to_label.get(id) {
                        node.aggregate_path(tail, path.separator, &mut output)?;
                    } else {
                        aggregate_path_of_numbers(tail, path.separator, &mut output)?;
                    }
                }
                PathElement::Label(label) => {
                    if let Some((id, node)) = self.label_to_id.get(label.as_str()) {
                        output.push(*id);
                        node.aggregate_path(tail, path.separator, &mut output)?;
                    } else {
                        return Err(PathError::UnknownLabel(label.to_string()));
                    }
                }
            }
        }
        Ok(output)
    }
}

impl<P> Entry<P> {
    /// Aggregate a path of labels into the corresponding path of ids
    fn aggregate_path(
        &self,
        path: &[PathElement],
        separator: char,
        output: &mut Vec<usize>,
    ) -> Result<(), PathError> {
        match self {
            // End of dictionary.
            // The path can still be evaluated if all remaining elements are numerical.
            Entry::Leaf {
                expected_type: _,
                payload: _,
            } => aggregate_path_of_numbers(path, separator, output),

            // Sub dictionary
            Entry::Node(sub_dict) => {
                if let Some((element, tail)) = path.split_first() {
                    // Given either an identifier or a label,
                    // we can try to find a corresponding sub-dictionary.
                    match element {
                        PathElement::Id(id) => {
                            output.push(*id);
                            if let Some((_, node)) = sub_dict.id_to_label.get(id) {
                                node.aggregate_path(tail, separator, output)
                            } else {
                                aggregate_path_of_numbers(tail, separator, output)
                            }
                        }
                        PathElement::Label(label) => {
                            if let Some((id, node)) = sub_dict.label_to_id.get(label.as_str()) {
                                output.push(*id);
                                node.aggregate_path(tail, separator, output)
                            } else {
                                Err(PathError::UnknownLabel(label.to_string()))
                            }
                        }
                    }
                } else {
                    Err(PathError::EndOfPath)
                }
            }

            // Expecting a list of sub entries.
            Entry::List(sub_def) => {
                if let Some((element, tail)) = path.split_first() {
                    match element {
                        // Corresponding path should contain a numerical element at that location.
                        PathElement::Id(id) => {
                            output.push(*id);
                            sub_def.aggregate_path(tail, separator, output)
                        }

                        // Label cannot be evaluated.
                        PathElement::Label(label) => Err(PathError::UnknownLabel(label.clone())),
                    }
                } else {
                    Err(PathError::EndOfPath)
                }
            }
        }
    }
}

/// Expecting a sequence of numbers, populate the output vector
fn aggregate_path_of_numbers(
    path: &[PathElement],
    separator: char,
    output: &mut Vec<usize>,
) -> Result<(), PathError> {
    // iterate over the elements of the path
    for element in path {
        // We expect only numerical identifiers
        if let PathElement::Id(id) = element {
            output.push(*id);
        } else {
            // We encountered a text label.
            // The path cannot be evaluated without a dictionary definition.
            return Err(PathError::EndOfDict(Path::from_slice(path, separator)));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::OnceLock;

    fn get_dictionnary() -> &'static Dictionary<()> {
        static DICT: OnceLock<Dictionary<()>> = OnceLock::new();
        DICT.get_or_init(|| {
            let mut dict = Dictionary::<()>::new();
            dict.add_entry(
                1,
                "label1",
                Entry::List({
                    let mut sub_dict = Dictionary::<()>::new();
                    sub_dict.add_entry(30, "label3", Entry::default());
                    Box::new(sub_dict.into())
                }),
            );
            dict.add_entry(
                2,
                "LABEL2",
                Entry::List({
                    let mut sub_dict = Dictionary::<()>::new();
                    sub_dict.add_entry(40, "Lab4", Entry::default());
                    Box::new(sub_dict.into())
                }),
            );
            dict.add_entry(10, "MY_LABEL", Entry::default());
            dict
        })
    }

    #[test]
    fn test_path() {
        type E = PathElement;
        let path1 = Path {
            elements: vec![
                E::Label("label1".to_string()),
                E::Id(20),
                E::Label("label3".to_string()),
                E::Id(4),
                E::Id(5),
            ],
            separator: '/',
        };
        let path2 = Path::from_raw("label1/20/label3/4/5", '/');

        assert_eq!(path1, path2);
    }

    #[test]
    fn test_resolve_path() {
        let dict = get_dictionnary();
        let path = Path::from_raw("label1.3.label3.4.5", '.');
        let path = dict.resolve_path(&path).unwrap();
        assert_eq!(vec![1, 3, 30, 4, 5], path);
    }

    #[test]
    fn test_resolve_raw_path() {
        let dict = get_dictionnary();
        let path = dict.resolve_raw_path("LABEL2-12-Lab4-4-5", '-').unwrap();
        assert_eq!(vec![2, 12, 40, 4, 5], path);
    }

    #[test]
    fn test_start_with_numeric() {
        let dict = get_dictionnary();
        let path = dict.resolve_raw_path("1=2=label3=6=7", '=').unwrap();
        assert_eq!(vec![1, 2, 30, 6, 7], path);
    }

    #[test]
    fn test_unknown_label() {
        let dict = get_dictionnary();
        let unknown = dict.resolve_raw_path("toto+10", '+').unwrap_err();
        assert_eq!(PathError::UnknownLabel("toto".to_string()), unknown);
    }

    #[test]
    fn test_edge_case() {
        let dict = get_dictionnary();

        // if the label contains a character used as a separator,
        // we cannot do much except return an error.
        let unknown = dict.resolve_raw_path("MY_LABEL_11", '_').unwrap_err();
        assert_eq!(PathError::UnknownLabel("MY".to_string()), unknown);
    }
}
