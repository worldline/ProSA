use super::{Dictionary, Entry, LabeledTvf};
use crate::msg::{tvf::Tvf, value::TvfValue};
use serde::{
    Serialize,
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
    value: TvfValue<'t, T>,
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
        if let TvfValue::Buffer(buffer) = &self.value {
            match self.definition {
                Entry::Node(dict) => serialize_message(dict, buffer.as_ref(), serializer),
                Entry::List(sub_def) => {
                    let mut seq = serializer.serialize_seq(Some(buffer.len()))?;

                    // iterate the message in the order of the ids
                    let ids = {
                        let mut ids = buffer.keys();
                        ids.sort();
                        ids
                    };
                    for id in ids {
                        let value = buffer.get(id).map_err(|err| {
                            serde::ser::Error::custom(format!(
                                "Error while serializing field {}: {}",
                                id, err
                            ))
                        })?;
                        // if the field is a sub-message, we may serialize it with a corresponding sub-dictionary
                        if let TvfValue::Buffer(_) = value {
                            seq.serialize_element(&SerializeDef {
                                definition: sub_def,
                                value,
                            })?;
                        } else {
                            seq.serialize_element(&value)?;
                        }
                    }
                    seq.end()
                }
                _ => self.value.serialize(serializer),
            }
        } else {
            self.value.serialize(serializer)
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
    let mut map = serializer.serialize_map(Some(message.len()))?;

    // iterate the message in the order of the ids
    let ids = {
        let mut ids = message.keys();
        ids.sort();
        ids
    };

    for id in ids {
        let value = message.get(id).map_err(|err| {
            serde::ser::Error::custom(format!("Error while serializing field {}: {}", id, err))
        })?;

        if let Some((label, def)) = dictionary.id_to_label.get(&id) {
            // a label has been found for the given field id
            if let TvfValue::Buffer(_) = value {
                // if the field is a sub-message, we may serialize it with a corresponding sub-dictionary
                map.serialize_entry(
                    label.as_ref(),
                    &SerializeDef {
                        definition: def.as_ref(),
                        value: value.clone(),
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
    use std::{
        borrow::Cow,
        sync::{Arc, OnceLock},
    };
    extern crate self as prosa_utils;

    fn create_dictionnary() -> &'static Dictionary<()> {
        static DICT: OnceLock<Dictionary<()>> = OnceLock::new();
        DICT.get_or_init(|| {
            let leaf = Entry::Leaf(());

            let mut sub_dict = Dictionary::default();
            sub_dict.add_entry(10, "first", leaf.clone());
            sub_dict.add_entry(20, "second", leaf.clone());
            sub_dict.add_entry(30, "third", leaf.clone());
            let sub_dict = Arc::new(sub_dict);

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
                30 => 0
            },
            5 => [
                {
                    10 => "hello",
                    20 => "world",
                    30 => 0
                },
                {
                    10 => "hello",
                    20 => "world",
                    30 => 0
                },
                {
                    10 => "hello",
                    20 => "world",
                    30 => 0
                }
            ],
            6 => "a line"
        });

        let labeled = LabeledTvf::new(Cow::Borrowed(dict), Cow::Borrowed(&message));

        let serialized = serde_json::to_value(&labeled).unwrap();

        assert_eq!(
            serde_json::json!({
                "label2": 7,
                "label4": {
                    "first": "hello",
                    "second": "world",
                    "third": 0
                },
                "label5": [
                    {
                        "first": "hello",
                        "second": "world",
                        "third": 0
                    },
                    {
                        "first": "hello",
                        "second": "world",
                        "third": 0
                    },
                    {
                        "first": "hello",
                        "second": "world",
                        "third": 0
                    }
                ],
                "6": "a line"
            }),
            serialized
        );
    }
}
