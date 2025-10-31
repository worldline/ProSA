//!
//! <svg width="40" height="40">
#![doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/doc_assets/tvf.svg"))]
//! </svg>
//!
//! Module to define TVF[^tvfnote] message data strcture
//!
//! [^tvfnote]: **T**ag **V**alue **F**ormat

use bytes::Bytes;
use chrono::{NaiveDate, NaiveDateTime};
use std::borrow::Cow;
use std::fmt::Debug;
use thiserror::Error;

/// Error define for TVF object
/// Use by several method (serialize/unserialize/getter/setter)
#[derive(Debug, Eq, Error, PartialOrd, PartialEq)]
pub enum TvfError {
    /// Error that indicate the field is not found in the TVF. Missing key
    #[error("The key `{0}` is not present in the Tvf")]
    FieldNotFound(usize),
    /// Error that indicate the field can't be retrieve because the type is not compatible
    #[error("The type can't be retrieve from the Tvf")]
    TypeMismatch,
    /// Error that indicate the field can't be converted due to an other error
    #[error("The field can't be Converted. {0}")]
    ConvertionError(String),
    /// Error encountered during serialization or deserializarion process
    #[error("Serialization error: {0}")]
    SerializationError(String),
}

/// Trait that define a TVF[^tvfnote]
/// Use to define a key/value object that structure a data
///
/// [^tvfnote]: **T**ag **V**alue **F**ormat
///
/// ```
/// use prosa_utils::msg::tvf::Tvf;
///
/// fn sample<T: Tvf>(mut tvf: T) {
///     tvf.put_string(1, "toto");
///     assert!(tvf.contains(1));
///     assert_eq!("toto", tvf.get_string(1).unwrap().as_str());
///
///     tvf.remove(1);
///     assert!(!tvf.contains(1));
/// }
pub trait Tvf {
    /// Method to know if the TVF is empty
    fn is_empty(&self) -> bool;
    /// Method to know the number of field in the TVF (not recursive)
    fn len(&self) -> usize;

    /// Method to know is the TVF contain a field the id key
    fn contains(&self, id: usize) -> bool;

    /// Method to remove a TVF field
    fn remove(&mut self, id: usize);

    /// Method to convert the TVF into vector ok keys
    fn into_keys(self) -> Vec<usize>;

    /// Get all the keys for this TVF
    fn keys(&self) -> Vec<usize>;

    /// Get a sub buffer from a TVF
    fn get_buffer(&self, id: usize) -> Result<Cow<'_, Self>, TvfError>
    where
        Self: Tvf + Clone;
    /// Get an unsigned value from a TVF
    fn get_unsigned(&self, id: usize) -> Result<u64, TvfError>;
    /// Get a signed value from a TVF
    fn get_signed(&self, id: usize) -> Result<i64, TvfError>;
    /// Get a byte value from a TVF
    fn get_byte(&self, id: usize) -> Result<u8, TvfError>;
    /// Get a float value from a TVF
    fn get_float(&self, id: usize) -> Result<f64, TvfError>;
    /// Get a string value from a TVF
    fn get_string(&self, id: usize) -> Result<Cow<'_, String>, TvfError>;
    /// Get a buffer of bytes from a TVF
    fn get_bytes(&self, id: usize) -> Result<Cow<'_, Bytes>, TvfError>;
    /// Get a date field from a TVF
    fn get_date(&self, id: usize) -> Result<NaiveDate, TvfError>;
    /// Get a datetime field from  a TVF.
    /// The timestamp is considered to be UTC.
    fn get_datetime(&self, id: usize) -> Result<NaiveDateTime, TvfError>;

    /// Put a buffer as sub field into a TVF
    fn put_buffer(&mut self, id: usize, buffer: Self)
    where
        Self: Tvf;
    /// Put an unsigned value to a TVF
    fn put_unsigned(&mut self, id: usize, unsigned: u64);
    /// Put a signed value to a TVF
    fn put_signed(&mut self, id: usize, signed: i64);
    /// Put a byte into a TVF
    fn put_byte(&mut self, id: usize, byte: u8);
    /// Put a float value to a TVF
    fn put_float(&mut self, id: usize, float: f64);
    /// Put a string value to a TVF
    fn put_string<T: Into<String>>(&mut self, id: usize, string: T);
    /// Put some bytes into a TVF
    fn put_bytes(&mut self, id: usize, buffer: Bytes);
    /// Put a date into a TVF
    fn put_date(&mut self, id: usize, date: NaiveDate);
    /// Put a datetime into a TVF.
    /// The timestamp is considered to be UTC.
    fn put_datetime(&mut self, id: usize, datetime: NaiveDateTime);
}

/// Trait to define a TVF[^tvfnote] filter.
/// Useful to filter sensitive data.
///
/// [^tvfnote]: **T**ag **V**alue **F**ormat
///
/// ```
/// use prosa_utils::msg::tvf::{Tvf, TvfFilter};
/// use prosa_utils::msg::simple_string_tvf::SimpleStringTvf;
///
/// enum TvfTestFilter {}
///
/// impl TvfFilter for TvfTestFilter {
///     fn filter<T: Tvf>(mut buf: T) -> T {
///         buf = <TvfTestFilter as TvfFilter>::mask_tvf_str_field(buf, 1, "*");
///         <TvfTestFilter as TvfFilter>::mask_tvf_str_field(buf, 2, "0")
///     }
/// }
///
/// let mut tvf: SimpleStringTvf = Default::default();
/// tvf.put_string(1, "plain");
/// tvf.put_string(2, "1234");
/// tvf.put_string(3, "clear");
///
/// let tvf_filtered = TvfTestFilter::filter(tvf);
/// assert_eq!(Ok(std::borrow::Cow::Owned(String::from("*****"))), tvf_filtered.get_string(1));
/// assert_eq!(Ok(std::borrow::Cow::Owned(String::from("0000"))), tvf_filtered.get_string(2));
/// assert_eq!(Ok(std::borrow::Cow::Owned(String::from("clear"))), tvf_filtered.get_string(3));
/// ```
pub trait TvfFilter {
    /// Method to filter the TVF buffer
    fn filter<T: Tvf>(tvf: T) -> T;

    /// Function to mask a TVF string field
    ///
    /// Replace in the TVF buf the string value at the id with a fill character
    fn mask_tvf_str_field<T: Tvf>(mut tvf: T, id: usize, fill_char: &str) -> T {
        if tvf.contains(id) {
            if let Ok(str) = tvf.get_string(id) {
                tvf.put_string(id, fill_char.repeat(str.len()));
            } else {
                // If the field can't be mask, just remove it to prevent any data leak
                tvf.remove(id);
            }
        }

        tvf
    }
}
