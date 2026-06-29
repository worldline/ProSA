/// Rules for generating numeric values
pub mod numeric;

/// Utility functions for parsing ranges
pub mod util;

use std::{
    collections::HashMap,
    num::{ParseFloatError, ParseIntError},
};

/// Generic scenario with an introduction, a middle looping part and a ending.
pub struct Scenario<T> {
    /// List of messages to send
    messages: Vec<Message<T>>,

    /// How many messages are part of the introduction
    /// to be played only once at the beginning of the scenario.
    index_intro: usize,

    /// At which index do we reach the end of the scenario
    /// to be played only once when we finish the benchmark.
    index_ending: usize,
}

/// Generic message
pub struct Message<T> {
    /// Template buffer with predefined fields
    template: T,

    /// Dynamic fields to add when submitting the message
    dynamics: HashMap<usize, Box<dyn Rule<T>>>,
}

/// Generic rule to insert a TVF field in a buffer
pub trait Rule<T> {
    /// Insert a dynamically generated value into the provided TVF buffer
    fn insert(&mut self, tag: usize, buffer: &mut T);
}

/// Parse a string to generate a rule
pub trait RuleParse: Sized {
    /// Rule label for identifying the type of rule to parse
    const LABEL: &'static str;

    /// Parse a string to deduce a rule
    fn parse(expr: &str) -> Result<Self, ParseError>;
}

/// Errors encountered when trying to parse a rule
#[derive(thiserror::Error, Debug, Clone)]
pub enum ParseError {
    /// Could not identify a rule for a label of the form "!my_label"
    #[error("Could not identify rule for provided label \"{0}\"")]
    UnknownType(String),

    /// Could not parse a value following a label
    #[error("Could not parse value \"{0}\"")]
    InvalidValue(String),

    /// Error when parsing an integer
    #[error("Error parsing an integer")]
    ParseInt(#[from] ParseIntError),

    /// Error when parsing a floating point number
    #[error("Error parsing a floating point number")]
    ParseFloat(#[from] ParseFloatError),
}
