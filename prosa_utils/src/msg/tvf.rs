//!
//! <svg width="40" height="40">
#![doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/doc_assets/tvf.svg"))]
//! </svg>
//!
//! Module to define TVF[^tvfnote] message data strcture
//!
//! [^tvfnote]: **T**ag **V**alue **F**ormat

use crate::msg::value::TvfType;

use super::value::TvfValue;
use bytes::Bytes;
use chrono::{NaiveDate, NaiveDateTime};
use std::borrow::Cow;
use std::fmt::Debug;
use thiserror::Error;

/// Error define for TVF object
/// Use by several method (serialize/unserialize/getter/setter)
#[derive(Debug, Eq, Error, PartialOrd, PartialEq, Clone)]
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

    /// Get a field from a TVF while specifying the expected output type
    fn get(&self, id: usize, expected: TvfType) -> Result<TvfValue<'_, Self>, TvfError>
    where
        Self: Clone,
    {
        match expected {
            TvfType::Byte => Ok(self.get_byte(id)?.into()),
            TvfType::Unsigned => Ok(self.get_unsigned(id)?.into()),
            TvfType::Signed => Ok(self.get_signed(id)?.into()),
            TvfType::Float => Ok(self.get_float(id)?.into()),
            TvfType::Date => Ok(self.get_date(id)?.into()),
            TvfType::DateTime => Ok(self.get_datetime(id)?.into()),
            TvfType::String => Ok(TvfValue::String(self.get_string(id)?)),
            TvfType::Bytes => Ok(TvfValue::Bytes(self.get_bytes(id)?)),
            TvfType::Buffer => Ok(TvfValue::Buffer(self.get_buffer(id)?)),
        }
    }

    /// Get a sub buffer from a TVF
    fn get_buffer(&self, id: usize) -> Result<Cow<'_, Self>, TvfError>
    where
        Self: Clone;
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

    /// Put a field into a TVF
    fn put(&mut self, id: usize, value: TvfValue<'_, Self>)
    where
        Self: Sized + Clone,
    {
        match value {
            TvfValue::Byte(byte) => self.put_byte(id, byte),
            TvfValue::Unsigned(unsigned) => self.put_unsigned(id, unsigned),
            TvfValue::Signed(signed) => self.put_signed(id, signed),
            TvfValue::Float(float) => self.put_float(id, float),
            TvfValue::Date(date) => self.put_date(id, date),
            TvfValue::DateTime(datetime) => self.put_datetime(id, datetime),
            TvfValue::String(string) => self.put_string(id, string.into_owned()),
            TvfValue::Bytes(buffer) => self.put_bytes(id, buffer.into_owned()),
            TvfValue::Buffer(buffer) => self.put_buffer(id, buffer.into_owned()),
        }
    }

    /// Put a buffer as sub field into a TVF
    fn put_buffer(&mut self, id: usize, buffer: Self);
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

    /// Follow a path to a byte
    #[inline]
    fn find_byte(&self, path: &[usize]) -> Result<u8, TvfError>
    where
        Self: Clone,
    {
        follow_path_to_byte(self, path)
    }

    /// Follow a path to an unsigned integer
    #[inline]
    fn find_unsigned(&self, path: &[usize]) -> Result<u64, TvfError>
    where
        Self: Clone,
    {
        follow_path_to_unsigned(self, path)
    }

    /// Follow a path to an signed integer
    #[inline]
    fn find_signed(&self, path: &[usize]) -> Result<i64, TvfError>
    where
        Self: Clone,
    {
        follow_path_to_signed(self, path)
    }

    /// Follow a path to an floating point number
    #[inline]
    fn find_float(&self, path: &[usize]) -> Result<f64, TvfError>
    where
        Self: Clone,
    {
        follow_path_to_float(self, path)
    }

    /// Follow a path to a string
    #[inline]
    fn find_string(&self, path: &[usize]) -> Result<Cow<'_, String>, TvfError>
    where
        Self: Clone,
    {
        follow_path_to_string(self, path)
    }

    /// Follow a path to an array of bytes
    #[inline]
    fn find_bytes(&self, path: &[usize]) -> Result<Cow<'_, Bytes>, TvfError>
    where
        Self: Clone,
    {
        follow_path_to_bytes(self, path)
    }

    /// Follow a path to a date
    #[inline]
    fn find_date(&self, path: &[usize]) -> Result<NaiveDate, TvfError>
    where
        Self: Clone,
    {
        follow_path_to_date(self, path)
    }

    /// Follow a path to a date time
    #[inline]
    fn find_datetime(&self, path: &[usize]) -> Result<NaiveDateTime, TvfError>
    where
        Self: Clone,
    {
        follow_path_to_datetime(self, path)
    }

    /// Follow a path to a sub-buffer
    #[inline]
    fn find_buffer(&self, path: &[usize]) -> Result<Cow<'_, Self>, TvfError>
    where
        Self: Clone,
    {
        follow_path_to_buffer(self, path)
    }

    /// Find a field following a path
    fn find(&self, path: &[usize], expected: TvfType) -> Result<TvfValue<'_, Self>, TvfError>
    where
        Self: Clone,
    {
        match expected {
            TvfType::Byte => Ok(self.find_byte(path)?.into()),
            TvfType::Unsigned => Ok(self.find_unsigned(path)?.into()),
            TvfType::Signed => Ok(self.find_signed(path)?.into()),
            TvfType::Float => Ok(self.find_float(path)?.into()),
            TvfType::Date => Ok(self.find_date(path)?.into()),
            TvfType::DateTime => Ok(self.find_datetime(path)?.into()),
            TvfType::String => Ok(TvfValue::String(self.find_string(path)?)),
            TvfType::Bytes => Ok(TvfValue::Bytes(self.find_bytes(path)?)),
            TvfType::Buffer => Ok(TvfValue::Buffer(self.find_buffer(path)?)),
        }
    }
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

/// Create a function follow path for each TVF type
macro_rules! make_follow_path {
    ( $func:ident use $inner:ident -> $out:ty ) => {
        pub(crate) fn $func<T>(msg: &T, path: &[usize]) -> Result<$out, TvfError>
        where
            T: Tvf + Sized + Clone,
        {
            if let Some((last, path)) = path.split_last() {
                if let Some(sub) = follow_path(msg, path)? {
                    sub.$inner(*last)
                } else {
                    msg.$inner(*last)
                }
            } else {
                Err(TvfError::FieldNotFound(0))
            }
        }
    };
    ( $func:ident use $inner:ident -> cow $out:ty ) => {
        pub(crate) fn $func<'t, T>(msg: &'t T, path: &[usize]) -> Result<Cow<'t, $out>, TvfError>
        where
            T: Tvf + Sized + Clone,
        {
            if let Some((last, path)) = path.split_last() {
                if let Some(sub) = follow_path(msg, path)? {
                    let value = sub.$inner(*last)?.into_owned();
                    Ok(Cow::Owned(value))
                } else {
                    msg.$inner(*last)
                }
            } else {
                Err(TvfError::FieldNotFound(0))
            }
        }
    };
}

make_follow_path![ follow_path_to_byte     use get_byte     -> u8            ];
make_follow_path![ follow_path_to_unsigned use get_unsigned -> u64           ];
make_follow_path![ follow_path_to_signed   use get_signed   -> i64           ];
make_follow_path![ follow_path_to_float    use get_float    -> f64           ];
make_follow_path![ follow_path_to_date     use get_date     -> NaiveDate     ];
make_follow_path![ follow_path_to_datetime use get_datetime -> NaiveDateTime ];
make_follow_path![ follow_path_to_string   use get_string   -> cow String    ];
make_follow_path![ follow_path_to_bytes    use get_bytes    -> cow Bytes     ];

/// Follow a path to a sub buffer
pub(crate) fn follow_path_to_buffer<'t, T>(
    msg: &'t T,
    path: &[usize],
) -> Result<Cow<'t, T>, TvfError>
where
    T: Tvf + Sized + Clone,
{
    if let Some(sub) = follow_path(msg, path)? {
        Ok(Cow::Owned(sub))
    } else {
        Err(TvfError::FieldNotFound(0))
    }
}

/// Follow a path to get to a sub-buffer
fn follow_path<T>(msg: &T, path: &[usize]) -> Result<Option<T>, TvfError>
where
    T: Tvf + Clone,
{
    // split off the first id of the path to avoid copying the first buffer
    if let Some((first, path)) = path.split_first() {
        // get the first sub-buffer
        let mut msg = msg.get_buffer(*first)?.into_owned();

        // follow the rest of the path cloning buffers along the
        // way such that the borrow checker does not complaing
        for id in path {
            msg = msg.get_buffer(*id)?.into_owned();
        }

        // return the buffer
        Ok(Some(msg))
    } else {
        // the path is actually empty, so we should use the initial buffer as is
        Ok(None)
    }
}
