//! Common operations on dictionaries

use super::{Dictionary, Entry};
use crate::msg::tvf::TvfError;
use std::fmt;

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
    #[error("Reached the end of the dictionary before the end of the path [{0}].")]
    EndOfDict(PathOfLabels<'/'>),

    /// Reached the end of the path before the end of the dictionary
    #[error("Reached the end of the path before the end of the dictionary.")]
    EndOfPath,

    /// TVF error encountered when accessing the message
    #[error("TVF error ({0}).")]
    Tvf(#[from] TvfError),
}

/// A path of unprocessed labels
#[derive(Debug, Clone)]
pub struct PathOfLabels<const S: char = '.'>(Vec<String>);

/// Define how the path should be displayed
impl<const S: char> fmt::Display for PathOfLabels<S> {
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

/// Convert a simple path into an struct that can be reported in an error
impl<const S: char> From<&[&str]> for PathOfLabels<S> {
    fn from(path: &[&str]) -> Self {
        Self(path.iter().map(|s| s.to_string()).collect())
    }
}

impl<P> Dictionary<P> {
    /// Convert a path of labels into the corresponding path of ids
    #[inline]
    pub fn resolve_path(&self, path: &str, separator: char) -> Result<Vec<usize>, PathError> {
        self.resolve_splitted_path(&path.split(separator).collect::<Vec<_>>())
    }

    /// Convert a path of labels into the corresponding path of ids
    pub fn resolve_splitted_path(&self, path: &[&str]) -> Result<Vec<usize>, PathError> {
        let mut output = Vec::with_capacity(path.len());
        if let Some(&label) = path.first() {
            if let Some((id, node)) = self.label_to_id.get(label) {
                output.push(*id);
                node.aggregate_path(&path[1..], &mut output)?;
            } else if let Ok(number) = label.parse::<usize>() {
                output.push(number);
                if let Some((_, sub_def)) = self.id_to_label.get(&number) {
                    sub_def.aggregate_path(&path[1..], &mut output)?;
                } else {
                    aggregate_path_of_numbers(&path[1..], &mut output)?;
                }
            } else {
                return Err(PathError::UnknownLabel(label.to_string()));
            }
        }
        Ok(output)
    }
}

impl<P> Entry<P> {
    /// Aggregate a path of labels into the corresponding path of ids
    fn aggregate_path(&self, path: &[&str], output: &mut Vec<usize>) -> Result<(), PathError> {
        match self {
            Entry::Leaf(_) => aggregate_path_of_numbers(path, output),
            Entry::Node(sub_dict) => {
                if let Some(&label) = path.first() {
                    if let Some((id, node)) = sub_dict.label_to_id.get(label) {
                        output.push(*id);
                        node.aggregate_path(&path[1..], output)
                    } else if let Ok(number) = label.parse::<usize>() {
                        output.push(number);
                        if let Some((_, sub_def)) = sub_dict.id_to_label.get(&number) {
                            sub_def.aggregate_path(&path[1..], output)
                        } else {
                            aggregate_path_of_numbers(&path[1..], output)
                        }
                    } else {
                        Err(PathError::UnknownLabel(label.to_string()))
                    }
                } else {
                    Err(PathError::EndOfPath)
                }
            }
            Entry::List(sub_def) => {
                if let Some(&number) = path.first() {
                    if let Ok(id) = number.parse::<usize>() {
                        output.push(id);
                        sub_def.aggregate_path(&path[1..], output)
                    } else {
                        Err(PathError::ParseNumber(number.to_string()))
                    }
                } else {
                    Err(PathError::EndOfDict(PathOfLabels::from(path)))
                }
            }
        }
    }
}

/// Expecting a sequence of numbers, populate the output vector
fn aggregate_path_of_numbers(path: &[&str], output: &mut Vec<usize>) -> Result<(), PathError> {
    for &number in path {
        if let Ok(id) = number.parse::<usize>() {
            output.push(id);
        } else {
            return Err(PathError::ParseNumber(number.to_string()));
        }
    }
    Ok(())
}
