use super::Dictionary;
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
    EndOfDict(Path),

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
            // An element can be either a numeric tag or a text label
            match element {
                // If it is a numeric tag, the path element is already resolved.
                // Just pick the corresponding dictionary entry and keep going.
                PathElement::Tag(tag) => {
                    if let Some(dict) = current
                        && !repeatable
                    {
                        // Access the entry definition corresponding to this tag.
                        if let Some((_, entry)) = dict.tag_to_label.get(tag) {
                            current = entry.get_sub_dict();
                        } else {
                            // If there is no further sub dictionary, the remaining path can only be composed of numeric tags.
                            current = None;
                        }
                    }
                    repeatable = false;
                    output.push(*tag);
                }
                // If it is a text label, we need to find the corresponding numeric tag using the dictionary.
                PathElement::Label(label) => {
                    if let Some(dict) = current
                        && !repeatable
                    {
                        // Access the entry definition corresponding to this label.
                        if let Some((tag, entry)) = dict.label_to_tag.get(label.as_str()) {
                            current = entry.get_sub_dict();
                            output.push(*tag);
                        } else {
                            // The label is unknown in the dictionary
                            return Err(PathError::UnknownLabel(label.clone()));
                        }
                    } else {
                        // We no longer have a dictionary, we cannot evaluate labels anymore.
                        return Err(PathError::EndOfDict(Path::<'.'>(path.0.clone())));
                    }
                }
            }
        }
        Ok(output)
    }
}
