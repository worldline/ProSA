//! This module implements the `Serialize` trait for the `Tvf` trait

use crate::msg::{
    tvf::Tvf,
    value::{TvfType, TvfValue},
};
use serde::{Serialize, Serializer, ser::SerializeMap};
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
            // TODO
            let expected_type = TvfType::Byte;
            // Get the value for the given id
            let value = TvfValue::from_buffer(self.0, id, expected_type).map_err(|err| {
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
        match self {
            Self::Byte(value) => serializer.serialize_u8(*value),
            Self::Unsigned(value) => serializer.serialize_u64(*value),
            Self::Signed(value) => serializer.serialize_i64(*value),
            Self::Float(value) => serializer.serialize_f64(*value),
            Self::String(string) => serializer.serialize_str(string.as_ref()),
            Self::Bytes(bytes) => serializer.serialize_bytes(bytes.as_ref()),
            Self::Date(date) => date.serialize(serializer),
            Self::DateTime(datetime) => datetime.serialize(serializer),
            Self::Buffer(buffer) => SerialTvf(buffer.as_ref()).serialize(serializer),
        }
    }
}
