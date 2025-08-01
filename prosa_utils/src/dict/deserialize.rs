//! Module to implements the `DeserializeSeed` trait on `Tvf` messages
//! The seed should contain a dictionary so that the text fields' identifiers can
//! be replaced by there corresponding numerical tags. Which will allow use to
//! obtain a regular TVF message.

use super::{Dictionary, Entry};
use crate::msg::{tvf::Tvf, value::TvfValue};
use bytes::Bytes;
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
pub struct DictDeserializer<P, T>
where
    P: Clone,
{
    /// The dictionary to use
    dictionary: Arc<Dictionary<P>>,

    /// The type of the message to output
    marker: PhantomData<T>,
}

impl<P, T> DictDeserializer<P, T>
where
    P: Clone,
{
    /// Create a labeled deserializer for TVF buffers
    pub fn new(dictionary: Arc<Dictionary<P>>) -> Self {
        Self {
            dictionary,
            marker: PhantomData,
        }
    }
}

impl<'de, P, T> DeserializeSeed<'de> for DictDeserializer<P, T>
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
        struct __Visitor<'de, P, T>
        where
            P: Clone,
        {
            dictionary: Arc<Dictionary<P>>,
            marker: PhantomData<T>,
            lifetime: PhantomData<&'de ()>,
        }

        impl<'de, P, T> Visitor<'de> for __Visitor<'de, P, T>
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
                while let Some(pair) = map.next_key_seed(IdSeed::from(self.dictionary.clone()))? {
                    if let Some(def) = pair.entry_def {
                        let value = map.next_value_seed(EntrySeed::new(def.as_ref()))?;
                        value.insert_in(&mut buffer, pair.id);
                    } else {
                        let value: TvfValue<'_, T> = map.next_value()?;
                        value.insert_in(&mut buffer, pair.id);
                    }
                }
                Ok(buffer)
            }
        }

        deserializer.deserialize_any(__Visitor {
            dictionary: self.dictionary.clone(),
            marker: PhantomData,
            lifetime: PhantomData,
        })
    }
}

/// seeded deserialize for keys
#[derive(Clone, Default)]
struct IdSeed<P>
where
    P: Clone,
{
    dictionary: Option<Arc<Dictionary<P>>>,
}

impl<P> From<Arc<Dictionary<P>>> for IdSeed<P>
where
    P: Clone,
{
    fn from(dictionary: Arc<Dictionary<P>>) -> Self {
        Self {
            dictionary: Some(dictionary),
        }
    }
}

/// Pair of identifier and entry definition
#[derive(Clone, Default)]
struct IdEntryPair<P> {
    /// The identifier of the field
    id: usize,

    /// The entry definition (if any)
    entry_def: Option<Arc<Entry<P>>>,
}

impl<P> IdEntryPair<P> {
    /// Return an orphan id
    fn new(id: usize) -> Self {
        Self {
            id,
            entry_def: None,
        }
    }

    /// Return an id with an entry definition
    fn with_def(id: usize, entry_def: Arc<Entry<P>>) -> Self {
        Self {
            id,
            entry_def: Some(entry_def),
        }
    }
}

impl<'de, P> DeserializeSeed<'de> for IdSeed<P>
where
    P: Clone,
{
    type Value = IdEntryPair<P>;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        // custom visitor over an integer or a string
        #[doc(hidden)]
        struct __Visitor<'de, P>
        where
            P: Clone,
        {
            dictionary: Arc<Dictionary<P>>,
            marker: PhantomData<usize>,
            lifetime: PhantomData<&'de ()>,
        }

        /// Used in the implementation of the visitor trait
        macro_rules! visit_integer {
            ($method:ident $type:ty) => {
                fn $method<E: de::Error>(self, v: $type) -> Result<Self::Value, E> {
                    if let Some((_, def)) = self.dictionary.id_to_label.get(&(v as usize)) {
                        Ok(IdEntryPair::with_def(v as usize, def.clone()))
                    } else {
                        Ok(IdEntryPair::new(v as usize))
                    }
                }
            };
        }

        impl<'de, P> Visitor<'de> for __Visitor<'de, P>
        where
            P: Clone,
            Dictionary<P>: ToOwned,
        {
            type Value = IdEntryPair<P>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                write![formatter, "TVF identifier"]
            }

            visit_integer![ visit_u8  u8  ];
            visit_integer![ visit_u16 u16 ];
            visit_integer![ visit_u32 u32 ];
            visit_integer![ visit_u64 u64 ];

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                let dict = self.dictionary.as_ref();

                if let Some((id, def)) = dict.label_to_id.get(v) {
                    Ok(IdEntryPair::with_def(*id, def.clone()))
                } else if let Ok(id) = v.parse::<usize>() {
                    if let Some((_, def)) = dict.id_to_label.get(&id) {
                        Ok(IdEntryPair::with_def(id, def.clone()))
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
struct EntrySeed<'t, 'e, P, T> {
    /// The definition to use
    entry: &'e Entry<P>,

    /// The type of the message to output
    phantom: PhantomData<T>,

    /// The lifetime of the message
    lifetime: PhantomData<&'t ()>,
}

/// Create a new entry seed from an entry definition
impl<'e, P, T> EntrySeed<'_, 'e, P, T>
where
    P: Clone,
{
    /// Create a new entry seed
    fn new(entry: &'e Entry<P>) -> Self {
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
                    Entry::Leaf {
                        expected_type: _,
                        payload: _,
                    } => {
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
                            field.insert_in(&mut buffer, index);
                            index += 1;
                        }
                        Ok(TvfValue::Buffer(Cow::Owned(buffer)))
                    }
                }
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                match self.definition.as_ref() {
                    Entry::Leaf {
                        expected_type: _,
                        payload: _,
                    } => {
                        // use the default deserialization method for sequences
                        let buffer = buffer_visit_map::<_, T>(map)?;
                        Ok(TvfValue::Buffer(Cow::Owned(buffer)))
                    }
                    Entry::Node(dict) => {
                        let mut buffer = T::default();
                        while let Some(pair) = map.next_key_seed(IdSeed::from(dict.clone()))? {
                            if let Some(def) = pair.entry_def {
                                let value = map.next_value_seed(EntrySeed::new(def.as_ref()))?;
                                value.insert_in(&mut buffer, pair.id);
                            } else {
                                let value: TvfValue<'_, _> = map.next_value()?;
                                value.insert_in(&mut buffer, pair.id);
                            }
                        }
                        Ok(TvfValue::Buffer(Cow::Owned(buffer)))
                    }
                    Entry::List(def) => {
                        let mut buffer = T::default();
                        while let Some(id) = map.next_key()? {
                            let value = map.next_value_seed(EntrySeed::new(def.as_ref()))?;
                            value.insert_in(&mut buffer, id);
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
        TvfValue::insert_in(&field, &mut buffer, index);
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
        TvfValue::insert_in(&field, &mut buffer, id);
    }
    Ok(buffer)
}

#[cfg(test)]
mod tests {
    use super::{DictDeserializer, Dictionary};
    use crate::{
        dict::Entry,
        msg::{simple_string_tvf::SimpleStringTvf, tvf::Tvf, value::TvfType},
    };
    use serde::de::DeserializeSeed;
    use std::sync::{Arc, OnceLock};

    fn create_dictionnary() -> &'static Dictionary<()> {
        static DICT: OnceLock<Dictionary<()>> = OnceLock::new();
        DICT.get_or_init(|| {
            let leaf = Entry::Leaf {
                expected_type: TvfType::String,
                payload: (),
            };

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
    fn test_labeled_deserialize_json() {
        let dict = create_dictionnary();

        let json = r#"
{
    "label2": true,
    "label4": {
        "first": "hello",
        "second": "world",
        "third": 0
    },
    "5": [
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
    ]
}"#;
        let mut deserializer = serde_json::Deserializer::new(serde_json::de::StrRead::new(json));

        let buffer: SimpleStringTvf = DictDeserializer::new(Arc::new(dict.clone()))
            .deserialize(&mut deserializer)
            .unwrap();
        assert_eq!(1u8, buffer.get_byte(2).unwrap());

        let sub = buffer.get_buffer(4).unwrap();
        assert_eq!("hello", sub.get_string(10).unwrap().as_str());
        assert_eq!("world", sub.get_string(20).unwrap().as_str());
        assert_eq!(0, sub.get_unsigned(30).unwrap());

        let list = buffer.get_buffer(5).unwrap();
        let entry1 = list.get_buffer(1).unwrap();
        assert_eq!("hello", entry1.get_string(10).unwrap().as_str());
        let entry2 = list.get_buffer(2).unwrap();
        assert_eq!("hello", entry2.get_string(10).unwrap().as_str());
        let entry3 = list.get_buffer(3).unwrap();
        assert_eq!("hello", entry3.get_string(10).unwrap().as_str());
    }
}
