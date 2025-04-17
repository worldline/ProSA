//! Type specifying all the variant types supported by the TVF trait

use super::tvf::Tvf;
use bytes::Bytes;
use chrono::{NaiveDate, NaiveDateTime};
use std::borrow::Cow;

/// A TVF value stored in the field of a TVF buffer
#[derive(Debug, Clone)]
pub enum TvfValue<'m, M: Tvf + Clone> {
    /// A byte value
    Byte(u8),

    /// An unsigned integer value
    Unsigned(u64),

    /// A signed integer value
    Signed(i64),

    /// A floating point number value
    Float(f64),

    /// A string value
    String(Cow<'m, String>),

    /// A buffer of bytes value
    Bytes(Cow<'m, Bytes>),

    /// A date value
    Date(NaiveDate),

    /// A date and time value
    DateTime(NaiveDateTime),

    /// A TVF sub buffer
    Buffer(Cow<'m, M>),
}

macro_rules! impl_from {
    ($num_type:ty => $enum_type:ident as $cast_type:ty) => {
        impl<'m, M: Tvf + Clone> From<$num_type> for TvfValue<'m, M> {
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

impl<M: Tvf + Clone> From<&str> for TvfValue<'_, M> {
    fn from(value: &str) -> Self {
        Self::String(Cow::Owned(value.to_string()))
    }
}

impl<M: Tvf + Clone> From<String> for TvfValue<'_, M> {
    fn from(value: String) -> Self {
        Self::String(Cow::Owned(value))
    }
}

impl<M: Tvf + Clone> From<Bytes> for TvfValue<'_, M> {
    fn from(value: Bytes) -> Self {
        Self::Bytes(Cow::Owned(value))
    }
}

impl<M: Tvf + Clone> From<NaiveDate> for TvfValue<'_, M> {
    fn from(value: NaiveDate) -> Self {
        Self::Date(value)
    }
}

impl<M: Tvf + Clone> From<NaiveDateTime> for TvfValue<'_, M> {
    fn from(value: NaiveDateTime) -> Self {
        Self::DateTime(value)
    }
}

impl<M: Tvf + Clone> From<M> for TvfValue<'_, M> {
    fn from(value: M) -> Self {
        Self::Buffer(Cow::Owned(value))
    }
}
