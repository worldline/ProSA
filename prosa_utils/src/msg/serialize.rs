//! This module implements the `Serialize` trait for the `Tvf` trait

use crate::msg::{
    tvf::{Tvf, TvfError},
    value::TvfValue,
};
use serde::{ser::{self, SerializeMap}, Serialize, Serializer};
use core::fmt;
use std::fmt::Debug;

/// Wrapper for the TVF trait to serialize it
pub struct SerialTvf<'t, T>(pub &'t T);

/// Serialize the TVF trait
impl<T> Serialize for SerialTvf<'_, T>
where
    T: Tvf + Clone + Default + Debug,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Get the list of ids in the TVF
        let ids = {
            let mut ids = self.0.keys();
            ids.sort();
            ids
        };

        // Create a map to serialize the TVF
        let mut map = serializer.serialize_map(Some(ids.len()))?;

        // Iterate over the ids and serialize each field
        for id in ids {
            // Get the value for the given id
            let value = TvfValue::from_buffer(self.0, id).map_err(|err| {
                serde::ser::Error::custom(format!("Error while serializing field {id}: {err}"))
            })?;

            // Serialize the value
            map.serialize_entry(&id, &value)?;
        }
        map.end()
    }
}

/// Serialize the TVF value
impl<T> Serialize for TvfValue<'_, T>
where
    T: Tvf + Clone + Default + Debug,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        #[cfg_attr(rustfmt, rustfmt_skip)]
        match self {
            Self::Byte    (value) => serializer.serialize_u8(*value),
            Self::Unsigned(value) => serializer.serialize_u64(*value),
            Self::Signed  (value) => serializer.serialize_i64(*value),
            Self::Float   (value) => serializer.serialize_f64(*value),
            Self::String  (value) => serializer.serialize_str(value.as_ref()),
            Self::Bytes   (value) => serializer.serialize_bytes(value.as_ref()),
            Self::Date    (value) => value.serialize(serializer),
            Self::DateTime(value) => value.serialize(serializer),
            Self::Buffer  (value) => SerialTvf(value.as_ref()).serialize(serializer),
        }
    }
}


impl ser::Error for TvfError {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        TvfError::SerializationError(msg.to_string())
    }
}

