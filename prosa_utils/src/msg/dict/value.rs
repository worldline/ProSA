//! Wrapper for any value type encountered in a TVF message.
//! This simplify the implementation of `Serialize`  and `Deserialize` traits from the `serde` crate.

use crate::msg::tvf::{Tvf, TvfError, TvfType};
use bytes::Bytes;
use chrono::{NaiveDate, NaiveDateTime};
use std::borrow::Cow;

/// A TVF value stored in the field of a TVF message
#[derive(Debug)]
pub(crate) enum TvfValue<'v, T>
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

impl_from! { u8    => Byte     as u8  }
impl_from! { u16   => Unsigned as u64 }
impl_from! { u32   => Unsigned as u64 }
impl_from! { u64   => Unsigned as u64 }
impl_from! { usize => Unsigned as u64 }
impl_from! { i8    => Signed   as i64 }
impl_from! { i16   => Signed   as i64 }
impl_from! { i32   => Signed   as i64 }
impl_from! { i64   => Signed   as i64 }
impl_from! { isize => Signed   as i64 }
impl_from! { f32   => Float    as f64 }
impl_from! { f64   => Float    as f64 }

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
    pub(crate) fn from_message(
        msg: &'v T,
        id: usize,
        type_hint: TvfType,
    ) -> Result<Self, TvfError> {
        #[cfg_attr(rustfmt, rustfmt_skip)]
        match type_hint {
            TvfType::Byte     => Ok(Self::Byte    (msg.get_byte    (id)?)),
            TvfType::Unsigned => Ok(Self::Unsigned(msg.get_unsigned(id)?)),
            TvfType::Signed   => Ok(Self::Signed  (msg.get_signed  (id)?)),
            TvfType::Float    => Ok(Self::Float   (msg.get_float   (id)?)),
            TvfType::String   => Ok(Self::String  (msg.get_string  (id)?)),
            TvfType::Bytes    => Ok(Self::Bytes   (msg.get_bytes   (id)?)),
            TvfType::Date     => Ok(Self::Date    (msg.get_date    (id)?)),
            TvfType::DateTime => Ok(Self::DateTime(msg.get_datetime(id)?)),
            TvfType::Buffer | TvfType::List => Ok(Self::Buffer(msg.get_buffer(id)?)),
        }
    }

    /// Insert the TVF value in the specified buffer
    pub(crate) fn insert_in(&self, msg: &mut T, id: usize) {
        #[cfg_attr(rustfmt, rustfmt_skip)]
        match self {
            Self::Byte    (value) => msg.put_byte    (id, *value),
            Self::Unsigned(value) => msg.put_unsigned(id, *value),
            Self::Signed  (value) => msg.put_signed  (id, *value),
            Self::Float   (value) => msg.put_float   (id, *value),
            Self::String  (value) => msg.put_string  (id, value.as_str()),
            Self::Bytes   (value) => msg.put_bytes   (id, value.clone().into_owned()),
            Self::Date    (value) => msg.put_date    (id, *value),
            Self::DateTime(value) => msg.put_datetime(id, *value),
            Self::Buffer  (value) => msg.put_buffer  (id, value.clone().into_owned()),
        }
    }

    /// Try to get a Byte from this TVF Value
    #[inline]
    #[allow(unused)]
    pub(crate) fn to_byte(&self) -> Option<u8> {
        if let Self::Byte(value) = self {
            Some(*value)
        } else {
            None
        }
    }

    /// Try to get a Unsigned from this TVF Value
    #[inline]
    #[allow(unused)]
    pub(crate) fn to_unsigned(&self) -> Option<u64> {
        if let Self::Unsigned(value) = self {
            Some(*value)
        } else {
            None
        }
    }

    /// Try to get a Signed from this TVF Value
    #[inline]
    #[allow(unused)]
    pub(crate) fn to_signed(&self) -> Option<i64> {
        if let Self::Signed(value) = self {
            Some(*value)
        } else {
            None
        }
    }

    /// Try to get a Float from this TVF Value
    #[inline]
    #[allow(unused)]
    pub(crate) fn to_float(&self) -> Option<f64> {
        if let Self::Float(value) = self {
            Some(*value)
        } else {
            None
        }
    }

    /// Try to get a String from this TVF Value
    #[inline]
    #[allow(unused)]
    pub(crate) fn to_string(&self) -> Option<Cow<'_, String>> {
        if let Self::String(value) = self {
            Some(value.clone())
        } else {
            None
        }
    }

    /// Try to get a Bytes from this TVF Value
    #[inline]
    #[allow(unused)]
    pub(crate) fn to_bytes(&self) -> Option<Cow<'_, Bytes>> {
        if let Self::Bytes(value) = self {
            Some(value.clone())
        } else {
            None
        }
    }

    /// Try to get a Date from this TVF Value
    #[inline]
    #[allow(unused)]
    pub(crate) fn to_date(&self) -> Option<NaiveDate> {
        if let Self::Date(value) = self {
            Some(*value)
        } else {
            None
        }
    }

    /// Try to get a DateTime from this TVF Value
    #[inline]
    #[allow(unused)]
    pub(crate) fn to_datetime(&self) -> Option<NaiveDateTime> {
        if let Self::DateTime(value) = self {
            Some(*value)
        } else {
            None
        }
    }

    /// Try to get a Buffer from this TVF Value
    #[inline]
    #[allow(unused)]
    pub(crate) fn to_buffer(&self) -> Option<Cow<'_, T>> {
        if let Self::Buffer(value) = self {
            Some(value.clone())
        } else {
            None
        }
    }
}
