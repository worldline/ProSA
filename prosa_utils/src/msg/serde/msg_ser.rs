//! This module implements the `Serialize` trait for the `Tvf` trait

use super::{Dictionary, Entry, LabeledTvf};
use crate::msg::{serialize::SerialTvf, tvf::Tvf, value::TvfValue};
use serde::{
    Serialize, Serializer,
    ser::{SerializeMap, SerializeSeq},
};

/// Wrapper to implement the `Serialize` trait for types that implement the `Tvf` trait.
pub struct SerialTvf<'t, T: Tvf>(pub &'t T);

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
            Self::Bytes(bytes) => {
                if serializer.is_human_readable() {
                    let data = format!("0x{}", hex::encode(bytes.as_ref()));
                    serializer.serialize_str(&data)
                } else {
                    serializer.serialize_bytes(bytes.as_ref())
                }
            }
            Self::Date(date) => date.serialize(serializer),
            Self::DateTime(datetime) => datetime.serialize(serializer),
            Self::Buffer(buffer) => SerialTvf(buffer.as_ref()).serialize(serializer),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_json() {
        let message = tvf!(SimpleStringTvf {
            2 => 7u8,
            4 => {
                10 => "hello",
                20 => "world",
                30 => 11,
                40 => 4.5,
                50 => "2025-01-01" as Date,
                60 => "2023-06-05 15:02:00.000" as DateTime,
                70 => 0x00010203_04050607_08090A0B_0C0D0E0F as Bytes,
            },
            5 => [
                {
                    10 => "hola",
                    20 => "mondo",
                    30 => 21
                },
                {
                    10 => "bonjour",
                    20 => "monde",
                    30 => 31
                }
            ],
            6 => "a line",
            7 => {
                1 => 100,
                2 => "text"
            }
        });

        let serialized = serde_json::to_value(&message).unwrap();

        assert_eq!(
            serde_json::json!({
                "2": 7,
                "4": {
                    "10": "hello",
                    "20": "world",
                    "30": 11,
                    "40": 4.5,
                    "50": "2025-01-01",
                    "60": "2023-06-05T15:02:00",
                    "70": "0x000102030405060708090a0b0c0d0e0f",
                },
                "5": [
                    {
                        "first": "hola",
                        "second": "mondo",
                        "third": 21
                    },
                    {
                        "first": "bonjour",
                        "second": "monde",
                        "third": 31
                    }
                ],
                "6": "a line",
                "7": {
                    "1": 100,
                    "2": "text"
                }
            }),
            serialized
        );
    }
}
