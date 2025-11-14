//! Type specifying all the variant types supported by the TVF trait

use crate::msg::tvf::{Tvf, TvfError};
use bytes::Bytes;
use chrono::{NaiveDate, NaiveDateTime};
use std::{borrow::Cow, fmt::Debug};

/// Enumerate all possible TVF types
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TvfType {
    /// single byte
    Byte,

    /// 64-bits unsigned integer
    Unsigned,

    /// 64-bits signed integer
    Signed,

    /// 64-bits floating point number
    Float,

    /// string
    String,

    /// array of bytes
    Bytes,

    /// date
    Date,

    /// date and time
    DateTime,

    /// TVF sub buffer
    Buffer,
}

/// A TVF value stored in the field of a TVF message
#[derive(Debug)]
pub enum TvfValue<'v, T>
where
    T: Tvf + Clone,
{
    /// A byte value
    Byte(u8),

    /// An unsigned integer value
    Unsigned(u64),

    /// A signed integer value
    Signed(i64),

    /// A floating point number value
    Float(f64),

    /// A string value
    String(Cow<'v, String>),

    /// A buffer of bytes value
    Bytes(Cow<'v, Bytes>),

    /// A date value
    Date(NaiveDate),

    /// A date and time value
    DateTime(NaiveDateTime),

    /// A TVF sub buffer
    Buffer(Cow<'v, T>),
}

impl<'v, T> ToOwned for TvfValue<'v, T>
where
    T: Tvf + Clone,
{
    type Owned = TvfValue<'v, T>;

    fn to_owned(&self) -> Self::Owned {
        #[cfg_attr(rustfmt, rustfmt_skip)]
        match &self {
            Self::Byte    (value) => Self::Byte    (*value),
            Self::Unsigned(value) => Self::Unsigned(*value),
            Self::Signed  (value) => Self::Signed  (*value),
            Self::Float   (value) => Self::Float   (*value),
            Self::String  (value) => Self::String  (value.to_owned()),
            Self::Bytes   (value) => Self::Bytes   (value.to_owned()),
            Self::Date    (value) => Self::Date    (*value),
            Self::DateTime(value) => Self::DateTime(*value),
            Self::Buffer  (value) => Self::Buffer  (value.to_owned()),
        }
    }

    fn clone_into(&self, target: &mut Self::Owned) {
        *target = self.to_owned();
    }
}

macro_rules! impl_from {
    ($num_type:ty => $enum_type:ident as $cast_type:ty) => {
        impl<'v, T: Tvf + Clone> From<$num_type> for TvfValue<'v, T> {
            fn from(value: $num_type) -> Self {
                Self::$enum_type(value as $cast_type)
            }
        }
    };
}

impl_from!(u8 => Byte as u8);
impl_from!(u16 => Unsigned as u64);
impl_from!(u32 => Unsigned as u64);
impl_from!(u64 => Unsigned as u64);
impl_from!(usize => Unsigned as u64);
impl_from!(i8 => Signed as i64);
impl_from!(i16 => Signed as i64);
impl_from!(i32 => Signed as i64);
impl_from!(i64 => Signed as i64);
impl_from!(isize => Signed as i64);
impl_from!(f32 => Float as f64);
impl_from!(f64 => Float as f64);

impl<T: Tvf + Clone> From<&str> for TvfValue<'_, T> {
    fn from(value: &str) -> Self {
        Self::String(Cow::Owned(value.to_string()))
    }
}

impl<T: Tvf + Clone> From<String> for TvfValue<'_, T> {
    fn from(value: String) -> Self {
        Self::String(Cow::Owned(value))
    }
}

impl<T: Tvf + Clone> From<Bytes> for TvfValue<'_, T> {
    fn from(value: Bytes) -> Self {
        Self::Bytes(Cow::Owned(value))
    }
}

impl<T: Tvf + Clone> From<NaiveDate> for TvfValue<'_, T> {
    fn from(value: NaiveDate) -> Self {
        Self::Date(value)
    }
}

impl<T: Tvf + Clone> From<NaiveDateTime> for TvfValue<'_, T> {
    fn from(value: NaiveDateTime) -> Self {
        Self::DateTime(value)
    }
}

impl<T: Tvf + Clone> From<T> for TvfValue<'_, T> {
    fn from(value: T) -> Self {
        Self::Buffer(Cow::Owned(value))
    }
}

impl<'v, T> TvfValue<'v, T>
where
    T: Tvf + Clone,
{
    /// Extract a TVF value from a message with an expected type
    pub fn from_message(
        message: &'v T,
        id: usize,
        expected_type: TvfType,
    ) -> Result<Self, TvfError> {
        #[cfg_attr(rustfmt, rustfmt_skip)]
        match expected_type {
            TvfType::Byte     => Ok(Self::Byte    (message.get_byte    (id)?)),
            TvfType::Unsigned => Ok(Self::Unsigned(message.get_unsigned(id)?)),
            TvfType::Signed   => Ok(Self::Signed  (message.get_signed  (id)?)),
            TvfType::Float    => Ok(Self::Float   (message.get_float   (id)?)),
            TvfType::String   => Ok(Self::String  (message.get_string  (id)?)),
            TvfType::Bytes    => Ok(Self::Bytes   (message.get_bytes   (id)?)),
            TvfType::Date     => Ok(Self::Date    (message.get_date    (id)?)),
            TvfType::DateTime => Ok(Self::DateTime(message.get_datetime(id)?)),
            TvfType::Buffer   => Ok(Self::Buffer  (message.get_buffer  (id)?)),
        }
    }

    /// Insert the TVF value in the specified buffer
    pub fn insert_in(&self, message: &mut T, id: usize) {
        #[cfg_attr(rustfmt, rustfmt_skip)]
        match self {
            Self::Byte    (value) => message.put_byte    (id, *value),
            Self::Unsigned(value) => message.put_unsigned(id, *value),
            Self::Signed  (value) => message.put_signed  (id, *value),
            Self::Float   (value) => message.put_float   (id, *value),
            Self::String  (value) => message.put_string  (id, value.as_str()),
            Self::Bytes   (value) => message.put_bytes   (id, value.clone().into_owned()),
            Self::Date    (value) => message.put_date    (id, *value),
            Self::DateTime(value) => message.put_datetime(id, *value),
            Self::Buffer  (value) => message.put_buffer  (id, value.clone().into_owned()),
        }
    }
}
