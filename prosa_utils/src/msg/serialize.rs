//! This module implements the `Serialize` trait for the `Tvf` trait

use crate::msg::{tvf::Tvf, value::TvfValue};
use serde::{Serialize, Serializer, ser::SerializeMap};

/// Wrapper for the TVF trait to serialize it
pub struct SerialTvf<'t, T>(pub &'t T);

/// Serialize the TVF trait
impl<T> Serialize for SerialTvf<'_, T>
where
    T: Tvf + Clone,
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
            let value = self.0.get(id).map_err(|err| {
                serde::ser::Error::custom(format!("Error while serializing field {}: {}", id, err))
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
    T: Tvf + Clone,
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
            Self::String(string) => serializer.serialize_str(string.as_ref().as_str()),
            Self::Bytes(bytes) => serializer.serialize_bytes(bytes.as_ref()),
            Self::Date(date) => date.serialize(serializer),
            Self::DateTime(datetime) => datetime.serialize(serializer),
            Self::Buffer(buffer) => SerialTvf(buffer.as_ref()).serialize(serializer),
        }
    }
}
