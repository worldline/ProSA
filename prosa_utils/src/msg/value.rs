//! Type specifying all the variant types supported by the TVF trait

use super::tvf::Tvf;
use bytes::Bytes;
use chrono::{NaiveDate, NaiveDateTime};
use std::{borrow::Cow, fmt::Debug};

/// A TVF value stored in the field of a TVF buffer
#[derive(Debug, Clone)]
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

/// Identify the type of a TVF field
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TvfType {
    /// A byte type
    Byte,

    /// An unsigned integer type
    Unsigned,

    /// A signed integer type
    Signed,

    /// A floating point number type
    Float,

    /// A string type
    String,

    /// A buffer of bytes
    Bytes,

    /// A date type
    Date,

    /// A date and time type
    DateTime,

    /// A TVF sub buffer
    Buffer,
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

impl<T: Tvf + Clone + PartialEq> PartialEq for TvfValue<'_, T> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Byte(a), Self::Byte(b)) => a == b,
            (Self::Signed(a), Self::Signed(b)) => a == b,
            (Self::Unsigned(a), Self::Unsigned(b)) => a == b,
            (Self::Float(a), Self::Float(b)) => a == b,
            (Self::Date(a), Self::Date(b)) => a == b,
            (Self::DateTime(a), Self::DateTime(b)) => a == b,
            (Self::String(a), Self::String(b)) => a == b,
            (Self::Bytes(a), Self::Bytes(b)) => a == b,
            (Self::Buffer(a), Self::Buffer(b)) => a == b,
            _ => false,
        }
    }
}

impl<'v, T> TvfValue<'v, T>
where
    T: Tvf + Clone,
{
    /// Get the type of the TVF value
    pub fn get_type(&self) -> TvfType {
        match &self {
            Self::Byte(_) => TvfType::Byte,
            Self::Unsigned(_) => TvfType::Unsigned,
            Self::Signed(_) => TvfType::Signed,
            Self::Float(_) => TvfType::Float,
            Self::String(_) => TvfType::String,
            Self::Bytes(_) => TvfType::Bytes,
            Self::Date(_) => TvfType::Date,
            Self::DateTime(_) => TvfType::DateTime,
            Self::Buffer(_) => TvfType::Buffer,
        }
    }
}

impl<T> TvfValue<'static, T>
where
    T: Tvf + Clone,
{
    /// If the inner value is borrowed, clone it to get an owned value
    pub fn into_owned(self) -> TvfValue<'static, T> {
        match self {
            Self::Byte(a) => Self::Byte(a),
            Self::Unsigned(a) => Self::Unsigned(a),
            Self::Signed(a) => Self::Signed(a),
            Self::Float(a) => Self::Float(a),
            Self::Date(a) => Self::Date(a),
            Self::DateTime(a) => Self::DateTime(a),
            Self::String(a) => Self::String(Cow::Owned(a.into_owned())),
            Self::Bytes(a) => Self::Bytes(Cow::Owned(a.into_owned())),
            Self::Buffer(a) => Self::Buffer(Cow::Owned(a.into_owned())),
        }
    }
}
