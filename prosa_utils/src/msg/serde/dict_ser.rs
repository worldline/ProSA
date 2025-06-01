//! This module implements the `Serialize` trait for the `Tvf` trait

use super::{Dictionary, Entry, LabeledTvf};
use crate::msg::{serialize::SerialTvf, tvf::Tvf, value::TvfValue};
use serde::{
    Serialize, Serializer,
    ser::{SerializeMap, SerializeSeq},
};

/// Bind a field definition with a TVF sub buffer to proceed with serialization
struct SerializeDef<'def, 't, P, T>
where
    T: Tvf + Clone,
{
    /// The definition to use
    definition: &'def Entry<P>,

    /// The message to serialize
    message: &'t T,
}

impl<P, T> Serialize for LabeledTvf<'_, '_, P, T>
where
    P: Clone,
    T: Clone + Tvf,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serialize_message(self.dictionary.as_ref(), self.message.as_ref(), serializer)
    }
}

impl<P, T> Serialize for SerializeDef<'_, '_, P, T>
where
    T: Clone + Tvf,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self.definition {
            // The entry is a subdictionary
            Entry::Node(dict) => serialize_message(dict, self.message, serializer),
            // The entry is a list of entries
            Entry::List(sub_def) => {
                let mut seq = serializer.serialize_seq(Some(self.message.len()))?;

                // iterate the message in the order of the ids
                let ids = {
                    let mut ids = self.message.keys();
                    ids.sort();
                    ids
                };

                for id in ids {
                    let value = self.message.get(id).map_err(|err| {
                        serde::ser::Error::custom(format!(
                            "Error while serializing field {}: {}",
                            id, err
                        ))
                    })?;
                    // if the field is a sub-message, we may serialize it with a corresponding sub-dictionary
                    if let TvfValue::Buffer(sub_buffer) = value {
                        seq.serialize_element(&SerializeDef {
                            definition: sub_def,
                            message: sub_buffer.as_ref(),
                        })?;
                    } else {
                        seq.serialize_element(&value)?;
                    }
                }
                seq.end()
            }
            // No further definitions provided, fallback to regular serialization
            _ => SerialTvf(self.message).serialize(serializer),
        }
    }
}

/// Serialize a TVF message with the given dictionary and the given serializer
#[inline]
fn serialize_message<S, P, T>(
    dictionary: &Dictionary<P>,
    message: &T,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
    T: Tvf + Clone,
{
    // iterate the message in the order of the ids
    let ids = {
        let mut ids = message.keys();
        if serializer.is_human_readable() {
            ids.sort();
        }
        ids
    };

    // Serialize the message using a map
    let mut map = serializer.serialize_map(Some(message.len()))?;

    for id in ids {
        let value = message.get(id).map_err(|err| {
            serde::ser::Error::custom(format!("Error while serializing field {}: {}", id, err))
        })?;

        if let Some((label, def)) = dictionary.id_to_label.get(&id) {
            // a label has been found for the given field id
            if let TvfValue::Buffer(sub_buffer) = value {
                // if the field is a sub-message, we may serialize it with a corresponding sub-dictionary
                map.serialize_entry(
                    label.as_ref(),
                    &SerializeDef {
                        definition: def.as_ref(),
                        message: sub_buffer.as_ref(),
                    },
                )?;
            } else {
                map.serialize_entry(label.as_ref(), &value)?;
            }
        } else {
            map.serialize_entry(&id, &value)?;
        }
    }

    map.end()
}

#[cfg(test)]
mod tests {
    use super::{Dictionary, LabeledTvf};
    use crate::{dict::Entry, msg::simple_string_tvf::SimpleStringTvf};
    use prosa_macros::tvf;
    use std::{borrow::Cow, sync::OnceLock};
    extern crate self as prosa_utils;

    fn create_dictionnary() -> &'static Dictionary<()> {
        static DICT: OnceLock<Dictionary<()>> = OnceLock::new();
        DICT.get_or_init(|| {
            let leaf = Entry::Leaf(());

            let mut sub_dict = Dictionary::default();
            sub_dict.add_entry(10, "first", leaf.clone());
            sub_dict.add_entry(20, "second", leaf.clone());
            sub_dict.add_entry(30, "third", leaf.clone());

            let mut dict = Dictionary::default();
            dict.add_entry(2, "label2", leaf.clone());
            dict.add_entry(4, "label4", Entry::new_dictionary(sub_dict.clone()));
            dict.add_entry(
                5,
                "label5",
                Entry::new_repeatable_dictionary(sub_dict.clone()),
            );

            dict
        })
    }

    #[test]
    fn test_labeled_serialize_json() {
        let dict = create_dictionnary();

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

        let labeled = LabeledTvf::new(Cow::Borrowed(dict), Cow::Borrowed(&message));
        let serialized = serde_json::to_value(&labeled).unwrap();

        assert_eq!(
            serde_json::json!({
                "label2": 7,
                "label4": {
                    "first": "hello",
                    "second": "world",
                    "third": 11,
                    "40": 4.5,
                    "50": "2025-01-01",
                    "60": "2023-06-05T15:02:00",
                    "70": "0x000102030405060708090a0b0c0d0e0f",
                },
                "label5": [
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
