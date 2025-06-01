//! This module implements the `Deserialize` trait for the `Tvf` trait

use crate::msg::{
    dict::{Dictionary, Entry},
    tvf::Tvf,
    value::TvfValue,
};
use bytes::Bytes;
use chrono::{NaiveDate, NaiveDateTime};
use regex::Regex;
use serde::{
    Deserialize, Deserializer,
    de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor},
};
use std::{
    borrow::Cow,
    fmt::{self, Debug},
    marker::PhantomData,
    sync::Arc,
};

/// Bind a dictionary to this deserializer to convert a serde compatible payload
/// into a TVF buffer.
pub struct DictDeserializer<'dict, P, T>
where
    P: Clone,
{
    /// The dictionary to use
    dictionary: Cow<'dict, Dictionary<P>>,

    /// The type of the message to output
    marker: PhantomData<T>,
}

impl<'dict, P, T> DictDeserializer<'dict, P, T>
where
    P: Clone,
{
    /// Create a labeled deserializer for TVF buffers
    pub fn new(dictionary: Cow<'dict, Dictionary<P>>) -> Self {
        Self {
            dictionary,
            marker: PhantomData,
        }
    }
}

impl<'de, P, T> DeserializeSeed<'de> for DictDeserializer<'_, P, T>
where
    P: Clone + 'de,
    T: Default + Tvf + Clone + Debug + Deserialize<'de>,
{
    type Value = T;

    fn deserialize<D: serde::Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        // visitor over the sequence or map
        #[doc(hidden)]
        struct __Visitor<'de, 'dict, P, T>
        where
            P: Clone,
        {
            dictionary: &'dict Dictionary<P>,
            marker: PhantomData<T>,
            lifetime: PhantomData<&'de ()>,
        }

        impl<'de, P, T> Visitor<'de> for __Visitor<'de, '_, P, T>
        where
            P: Clone + 'de,
            T: Default + Tvf + Clone + Debug + Deserialize<'de>,
        {
            type Value = T;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                write![formatter, "TVF buffer"]
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut buffer = T::default();
                // iterate over the whole map,
                // keys can be either integers or strings
                while let Some(pair) = map.next_key_seed(IdSeed::new(self.dictionary))? {
                    if let Some(def) = pair.entry_def {
                        buffer.put(pair.id, map.next_value_seed(EntrySeed::new(def))?);
                    } else {
                        buffer.put(pair.id, map.next_value()?);
                    }
                }
                Ok(buffer)
            }
        }

        deserializer.deserialize_any(__Visitor {
            dictionary: &self.dictionary,
            marker: PhantomData,
            lifetime: PhantomData,
        })
    }
}

/// seeded deserialize for keys
#[derive(Clone, Default)]
struct IdSeed<'dict, P> {
    dictionary: Option<&'dict Dictionary<P>>,
}

impl<'dict, P> IdSeed<'dict, P> {
    /// Create a new id seed
    fn new(dictionary: &'dict Dictionary<P>) -> Self {
        Self {
            dictionary: Some(dictionary),
        }
    }
}

/// Pair of identifier and entry definition
#[derive(Clone, Default)]
struct IdEntryPair<'entry, P> {
    /// The identifier of the field
    id: usize,

    /// The entry definition (if any)
    entry_def: Option<&'entry Entry<P>>,
}

impl<'entry, P> IdEntryPair<'entry, P> {
    /// Return an orphan id
    fn new(id: usize) -> Self {
        Self {
            id,
            entry_def: None,
        }
    }

    /// Return an id with an entry definition
    fn with_def(id: usize, entry_def: &'entry Entry<P>) -> Self {
        Self {
            id,
            entry_def: Some(entry_def),
        }
    }
}

impl<'de, 'dict, P> DeserializeSeed<'de> for IdSeed<'dict, P>
where
    P: Clone,
{
    type Value = IdEntryPair<'dict, P>;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        // custom visitor over an integer or a string
        #[doc(hidden)]
        struct __Visitor<'de, 'dict, P>
        where
            P: Clone,
        {
            dictionary: &'dict Dictionary<P>,
            marker: PhantomData<usize>,
            lifetime: PhantomData<&'de ()>,
        }

        /// Used in the implementation of the visitor trait
        macro_rules! visit_integer {
            ($method:ident $type:ty) => {
                fn $method<E: de::Error>(self, v: $type) -> Result<Self::Value, E> {
                    if let Some((_, def)) = self.dictionary.id_to_label.get(&(v as usize)) {
                        Ok(IdEntryPair::with_def(v as usize, &def))
                    } else {
                        Ok(IdEntryPair::new(v as usize))
                    }
                }
            };
        }

        impl<'de, 'dict, P> Visitor<'de> for __Visitor<'de, 'dict, P>
        where
            P: Clone,
            Dictionary<P>: ToOwned,
        {
            type Value = IdEntryPair<'dict, P>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                write![formatter, "TVF identifier"]
            }

            visit_integer![ visit_u8  u8  ];
            visit_integer![ visit_u16 u16 ];
            visit_integer![ visit_u32 u32 ];
            visit_integer![ visit_u64 u64 ];

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                let dict = self.dictionary;

                if let Some((id, def)) = dict.label_to_id.get(v) {
                    Ok(IdEntryPair::with_def(*id, def))
                } else if let Ok(id) = v.parse::<usize>() {
                    if let Some((_, def)) = dict.id_to_label.get(&id) {
                        Ok(IdEntryPair::with_def(id, def))
                    } else {
                        Ok(IdEntryPair::new(id))
                    }
                } else {
                    Err(de::Error::unknown_field(v, &["a label in the dictionary"]))
                }
            }
        }

        // check if a dictionary has been defined
        if let Some(dictionary) = self.dictionary {
            // if a dictionary exists, use a custom visitor
            deserializer.deserialize_any(__Visitor {
                dictionary,
                marker: PhantomData,
                lifetime: PhantomData,
            })
        } else {
            // otherwise just extract a usize
            Ok(IdEntryPair::new(usize::deserialize(deserializer)?))
        }
    }
}

/// seeded deserialize for labeled buffer
#[derive(Clone)]
struct EntrySeed<'t, 'entry, P, T> {
    /// The definition to use
    entry: &'entry Entry<P>,

    /// The type of the message to output
    phantom: PhantomData<T>,

    /// The lifetime of the message
    lifetime: PhantomData<&'t ()>,
}

/// Create a new entry seed from an entry definition
impl<'entry, P, T> EntrySeed<'_, 'entry, P, T>
where
    P: Clone,
{
    /// Create a new entry seed
    fn new(entry: &'entry Entry<P>) -> Self {
        Self {
            entry,
            phantom: PhantomData,
            lifetime: PhantomData,
        }
    }
}
impl<'de, 't, P, T> DeserializeSeed<'de> for EntrySeed<'t, '_, P, T>
where
    P: Clone + 'de,
    T: Tvf + Default + Debug + Clone + Deserialize<'de> + 't,
{
    type Value = TvfValue<'t, T>;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        // visitor over the sequence or map
        #[doc(hidden)]
        struct __Visitor<'de, 't, P, T> {
            definition: Arc<Entry<P>>,
            marker: PhantomData<T>,
            lifetime: PhantomData<&'de ()>,
            lifetime2: PhantomData<&'t ()>,
        }

        macro_rules! visit_number {
            ($method:ident for $itype:ty => $field:ident as $otype:ty) => {
                fn $method<E: de::Error>(self, v: $itype) -> Result<Self::Value, E> {
                    Ok(TvfValue::$field(v as $otype))
                }
            };
        }

        impl<'de, 't, P, T> Visitor<'de> for __Visitor<'de, 't, P, T>
        where
            P: Clone + 'de,
            T: Tvf + Default + Debug + Deserialize<'de> + Clone + 't,
        {
            type Value = TvfValue<'t, T>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                write![formatter, "TVF field"]
            }

            visit_number![ visit_bool for bool => Byte     as u8  ];
            visit_number![ visit_u8   for u8   => Byte     as u8  ];
            visit_number![ visit_u16  for u16  => Unsigned as u64 ];
            visit_number![ visit_u32  for u32  => Unsigned as u64 ];
            visit_number![ visit_u64  for u64  => Unsigned as u64 ];
            visit_number![ visit_i8   for i8   => Signed   as i64 ];
            visit_number![ visit_i16  for i16  => Signed   as i64 ];
            visit_number![ visit_i32  for i32  => Signed   as i64 ];
            visit_number![ visit_i64  for i64  => Signed   as i64 ];
            visit_number![ visit_f32  for f32  => Float    as f64 ];
            visit_number![ visit_f64  for f64  => Float    as f64 ];

            fn visit_string<E: de::Error>(self, v: String) -> Result<Self::Value, E> {
                Ok(TvfValue::String(Cow::Owned(v)))
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Ok(TvfValue::String(Cow::Owned(v.to_string())))
            }

            fn visit_bytes<E: de::Error>(self, v: &[u8]) -> Result<Self::Value, E> {
                Ok(TvfValue::Bytes(Cow::Owned(Bytes::copy_from_slice(v))))
            }

            /// implementation of the visit method for sequences
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                match self.definition.as_ref() {
                    Entry::Leaf(_) => {
                        // use the default deserialization method for sequences
                        let buffer = buffer_visit_seq::<_, T>(seq)?;
                        Ok(TvfValue::Buffer(Cow::Owned(buffer)))
                    }
                    Entry::Node(_) => {
                        // while iterating a sequence, it doesn't make sense to try to match it with a dictionary
                        Err(de::Error::invalid_type(
                            de::Unexpected::Seq,
                            &"a sub-buffer",
                        ))
                    }
                    Entry::List(def) => {
                        let mut buffer = T::default();
                        let mut index = 1;
                        while let Some(field) =
                            seq.next_element_seed(EntrySeed::new(def.as_ref()))?
                        {
                            buffer.put(index, field);
                            index += 1;
                        }
                        Ok(TvfValue::Buffer(Cow::Owned(buffer)))
                    }
                }
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                match self.definition.as_ref() {
                    Entry::Leaf(_) => {
                        // use the default deserialization method for sequences
                        let buffer = buffer_visit_map::<_, T>(map)?;
                        Ok(TvfValue::Buffer(Cow::Owned(buffer)))
                    }
                    Entry::Node(dict) => {
                        let mut buffer = T::default();
                        while let Some(pair) = map.next_key_seed(IdSeed::new(dict))? {
                            if let Some(def) = pair.entry_def {
                                buffer.put(pair.id, map.next_value_seed(EntrySeed::new(def))?);
                            } else {
                                buffer.put(pair.id, map.next_value()?);
                            }
                        }
                        Ok(TvfValue::Buffer(Cow::Owned(buffer)))
                    }
                    Entry::List(def) => {
                        let mut buffer = T::default();
                        while let Some(id) = map.next_key()? {
                            buffer.put(id, map.next_value_seed(EntrySeed::new(def.as_ref()))?);
                        }
                        Ok(TvfValue::Buffer(Cow::Owned(buffer)))
                    }
                }
            }
        }

        deserializer.deserialize_any(__Visitor {
            definition: Arc::new(self.entry.clone()),
            marker: PhantomData,
            lifetime: PhantomData,
            lifetime2: PhantomData,
        })
    }
}

/// implementation of the visit method for sequences
fn buffer_visit_seq<'de, A, T>(mut seq: A) -> Result<T, A::Error>
where
    A: SeqAccess<'de>,
    T: Default + Tvf + Clone + Debug + Deserialize<'de>,
{
    let mut buffer = T::default();
    let mut index = 1;
    while let Some(field) = seq.next_element()? {
        buffer.put(index, field);
        index += 1;
    }
    Ok(buffer)
}

/// implementation of the visit method for maps
fn buffer_visit_map<'de, A, T>(mut map: A) -> Result<T, A::Error>
where
    A: MapAccess<'de>,
    T: Default + Tvf + Clone + Debug + Deserialize<'de>,
{
    let mut buffer = T::default();
    while let Some((id, field)) = map.next_entry()? {
        buffer.put(id, field);
    }
    Ok(buffer)
}

#[cfg(test)]
mod tests {
    use super::{DictDeserializer, Dictionary};
    use crate::{
        dict::Entry,
        msg::{simple_string_tvf::SimpleStringTvf, tvf::Tvf},
    };
    use bytes::Bytes;
    use chrono::{NaiveDate, NaiveDateTime};
    use serde::de::DeserializeSeed;
    use std::{borrow::Cow, str::FromStr, sync::OnceLock};

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
    fn test_labeled_deserialize_json() {
        let dict = create_dictionnary();

        let json = r#"
{
    "label2": true,
    "label4": {
        "first": "hello",
        "second": "world",
        "third": 21,
        "5": "2025-01-01",
        "6": "2023-06-05T15:02:00",
        "7": "0xF0E1D2C3B4A5968778695A4B3C2D1E0F"
    },
    "5": [
        {
            "first": "guten tag",
            "third": 22
        },
        {
            "first": "bonjour",
            "second": "tous",
            "third": 23
        },
        {
            "first": "egun on",
            "second": "mundua",
            "third": 24
        }
    ],
    "6": "another line",
    "7": {
        "1": 200,
        "2": "text"
    }
}"#;
        let mut deserializer = serde_json::Deserializer::new(serde_json::de::StrRead::new(json));

        let buffer: SimpleStringTvf = DictDeserializer::new(Cow::Borrowed(&dict))
            .deserialize(&mut deserializer)
            .unwrap();
        assert_eq!(1u8, buffer.get_byte(2).unwrap());

        let sub = buffer.get_buffer(4).unwrap();
        assert_eq!("hello", sub.get_string(10).unwrap().as_str());
        assert_eq!("world", sub.get_string(20).unwrap().as_str());
        assert_eq!(21, sub.get_unsigned(30).unwrap());
        assert_eq!(
            NaiveDate::from_ymd_opt(2025, 01, 01).unwrap(),
            sub.get_date(5).unwrap()
        );
        assert_eq!(
            NaiveDateTime::from_str("2023-06-05T15:02:00").unwrap(),
            sub.get_datetime(6).unwrap()
        );
        static BUFFER: [u8; 16] = [
            0xF0, 0xE1, 0xD2, 0xC3, 0xB4, 0xA5, 0x96, 0x87, 0x78, 0x69, 0x5A, 0x4B, 0x3C, 0x2D,
            0x1E, 0x0F,
        ];
        assert_eq!(
            Bytes::from_static(&BUFFER),
            sub.get_bytes(7).unwrap().as_ref()
        );

        let list = buffer.get_buffer(5).unwrap();
        let entry1 = list.get_buffer(1).unwrap();
        assert_eq!("guten tag", entry1.get_string(10).unwrap().as_str());
        let entry2 = list.get_buffer(2).unwrap();
        assert_eq!("bonjour", entry2.get_string(10).unwrap().as_str());
        let entry3 = list.get_buffer(3).unwrap();
        assert_eq!("egun on", entry3.get_string(10).unwrap().as_str());

        assert_eq!("another line", buffer.get_string(6).unwrap().as_str());
    }
}
