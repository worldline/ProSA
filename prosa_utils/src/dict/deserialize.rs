use super::Dictionary;
use crate::{
    dict::{
        EntryType,
        field_type::{TvfType, TvfValue},
    },
    msg::tvf::Tvf,
};
use bytes::Bytes;
use chrono::{DateTime, NaiveDate, NaiveDateTime};
use regex::Regex;
use serde::{
    Deserialize, Deserializer,
    de::{self, DeserializeSeed, MapAccess, SeqAccess},
};
use std::{
    borrow::Cow,
    fmt::{self},
    marker::PhantomData,
    sync::{Arc, LazyLock},
};

/// Bind a dictionary to this deserializer to convert a serde compatible payload into a TVF message.
pub struct DictDeserializer<P, T>
where
    P: Clone,
{
    /// Ignore unknown fields when serializing
    pub ignore_unknown: bool,

    /// The dictionary to use
    pub dictionary: Arc<Dictionary<P>>,

    /// The type of the message to output
    marker: PhantomData<T>,
}

impl<P, T> DictDeserializer<P, T>
where
    P: Clone,
{
    /// Create a labeled deserializer for TVF buffers
    pub fn new(dictionary: Arc<Dictionary<P>>, ignore_unknown: bool) -> Self {
        Self {
            ignore_unknown,
            dictionary,
            marker: PhantomData,
        }
    }
}

impl<'de, P, T> DeserializeSeed<'de> for DictDeserializer<P, T>
where
    P: Clone + 'de,
    T: Clone + Tvf + Default + Deserialize<'de>,
{
    type Value = T;

    fn deserialize<D: serde::Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_map(__VisitorBuffer::new(
            self.ignore_unknown,
            self.dictionary.as_ref(),
        ))
    }
}

// Seed for identifying tags based on label
#[doc(hidden)]
struct __SeedTag<'dict, P>
where
    P: Clone,
{
    ignore_unknown: bool,
    dictionary: &'dict Dictionary<P>,
}

#[doc(hidden)]
enum __FieldTagged<'dict, P> {
    /// The field could not be identified from the label
    Unknown,

    /// The field has been identified
    Identified {
        /// Tag of the field
        tag: usize,

        /// Does the field depict a list of entries
        is_repeatable: bool,

        /// Type of the one or multiple entries
        entry_type: &'dict EntryType<P>,
    },
}

impl<'dict, P> __SeedTag<'dict, P>
where
    P: Clone,
{
    /// Construct a new seed for deserializing a TVF tag
    fn new(ignore_unknown: bool, dictionary: &'dict Dictionary<P>) -> Self {
        Self {
            ignore_unknown,
            dictionary,
        }
    }
}

impl<'de, 'dict, P> de::DeserializeSeed<'de> for __SeedTag<'dict, P>
where
    P: Clone,
{
    type Value = __FieldTagged<'dict, P>;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        // custom visitor over an integer or a string
        #[doc(hidden)]
        struct __Visitor<'de, 'dict, P>
        where
            P: Clone,
        {
            ignore_unknown: bool,
            dictionary: &'dict Dictionary<P>,
            lifetime: PhantomData<&'de ()>,
        }

        impl<'de, 'dict, P> de::Visitor<'de> for __Visitor<'de, 'dict, P>
        where
            P: Clone,
        {
            type Value = __FieldTagged<'dict, P>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                write![formatter, "TVF identifier"]
            }

            fn visit_str<E: de::Error>(self, label: &str) -> Result<Self::Value, E> {
                if let Some((tag, entry)) = self.dictionary.label_to_tag.get(label) {
                    let entry = entry.as_ref();
                    Ok(__FieldTagged::Identified {
                        tag: *tag,
                        is_repeatable: entry.is_repeatable,
                        entry_type: &entry.entry_type,
                    })
                } else if self.ignore_unknown {
                    Ok(__FieldTagged::Unknown)
                } else {
                    Err(de::Error::unknown_field(
                        label,
                        &["a label in the dictionary"],
                    ))
                }
            }
        }

        deserializer.deserialize_any(__Visitor {
            ignore_unknown: self.ignore_unknown,
            dictionary: self.dictionary,
            lifetime: PhantomData,
        })
    }
}

#[doc(hidden)]
struct __SeedValue<'dict, 'tvf, P, T>
where
    P: Clone,
{
    /// Ignore unknown fields instead of raising an error
    ignore_unknown: bool,

    /// Is the field a list or a single value
    is_repeatable: bool,

    /// The type of value we expect to deserialize
    entry_type: &'dict EntryType<P>,

    /// Bind lifetime and type of TVF message to generate
    marker: PhantomData<&'tvf T>,
}

impl<'dict, 'tvf, P, T> __SeedValue<'dict, 'tvf, P, T>
where
    P: Clone,
{
    /// Construct a new seed for deserializing a TVF value
    fn new(ignore_unknown: bool, is_repeatable: bool, entry_type: &'dict EntryType<P>) -> Self {
        Self {
            ignore_unknown,
            is_repeatable,
            entry_type,
            marker: PhantomData,
        }
    }
}

impl<'de, 'dict, 'tvf, P, T> de::DeserializeSeed<'de> for __SeedValue<'dict, 'tvf, P, T>
where
    P: Clone,
    T: Clone + Tvf + Default + 'tvf,
{
    type Value = TvfValue<'tvf, T>;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[doc(hidden)]
        #[derive(Default)]
        struct __VisitorByte<'de>(PhantomData<&'de ()>);

        #[doc(hidden)]
        #[derive(Default)]
        struct __VisitorUnsigned<'de>(PhantomData<&'de ()>);

        #[doc(hidden)]
        #[derive(Default)]
        struct __VisitorSigned<'de>(PhantomData<&'de ()>);

        #[doc(hidden)]
        #[derive(Default)]
        struct __VisitorFloat<'de>(PhantomData<&'de ()>);

        #[doc(hidden)]
        #[derive(Default)]
        struct __VisitorString<'de>(PhantomData<&'de ()>);

        #[doc(hidden)]
        #[derive(Default)]
        struct __VisitorBytes<'de>(PhantomData<&'de ()>);

        #[doc(hidden)]
        #[derive(Default)]
        struct __VisitorDate<'de>(PhantomData<&'de ()>);

        #[doc(hidden)]
        #[derive(Default)]
        struct __VisitorDateTime<'de>(PhantomData<&'de ()>);

        #[doc(hidden)]
        struct __VisitorList<'de, 'dict, 'tvf, P, T>
        where
            P: Clone,
            T: Tvf + Default,
        {
            ignore_unknown: bool,
            entry_type: &'dict EntryType<P>,
            lifetime: PhantomData<&'de ()>,
            marker: PhantomData<&'tvf T>,
        }

        /// Trivially implement a visit method for a given type
        macro_rules! visit_value {
            ($visit_method:ident($input_type:ty)) => {
                #[inline]
                fn $visit_method<E>(self, value: $input_type) -> Result<Self::Value, E>
                where
                    E: de::Error,
                {
                    Ok(value as Self::Value)
                }
            };
        }

        impl<'de> de::Visitor<'de> for __VisitorByte<'de> {
            type Value = u8;

            #[inline]
            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                write![formatter, "TVF Byte"]
            }

            visit_value!(visit_bool(bool));
            visit_value!(visit_char(char));
            visit_value!(visit_u8(u8));
            visit_value!(visit_u16(u16));
            visit_value!(visit_u32(u32));
            visit_value!(visit_u64(u64));
            visit_value!(visit_i8(i8));
            visit_value!(visit_i16(i16));
            visit_value!(visit_i32(i32));
            visit_value!(visit_i64(i64));
        }

        impl<'de> de::Visitor<'de> for __VisitorUnsigned<'de> {
            type Value = u64;

            #[inline]
            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                write![formatter, "TVF Unsigned Integer"]
            }

            visit_value!(visit_bool(bool));
            visit_value!(visit_char(char));
            visit_value!(visit_u64(u64));
            visit_value!(visit_i64(i64));
        }

        impl<'de> de::Visitor<'de> for __VisitorSigned<'de> {
            type Value = i64;

            #[inline]
            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                write![formatter, "TVF Signed Integer"]
            }

            visit_value!(visit_bool(bool));
            visit_value!(visit_char(char));
            visit_value!(visit_u64(u64));
            visit_value!(visit_i64(i64));
        }

        impl<'de> de::Visitor<'de> for __VisitorFloat<'de> {
            type Value = f64;

            #[inline]
            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                write![formatter, "TVF Float"]
            }

            visit_value!(visit_u64(u64));
            visit_value!(visit_i64(i64));
            visit_value!(visit_f64(f64));
        }

        impl<'de> de::Visitor<'de> for __VisitorString<'de> {
            type Value = String;

            #[inline]
            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                write![formatter, "TVF String"]
            }

            #[inline]
            fn visit_string<E: de::Error>(self, string: String) -> Result<Self::Value, E> {
                Ok(string)
            }

            #[inline]
            fn visit_str<E: de::Error>(self, string: &str) -> Result<Self::Value, E> {
                Ok(string.to_string())
            }
        }

        impl<'de> de::Visitor<'de> for __VisitorBytes<'de> {
            type Value = Bytes;

            #[inline]
            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                write![formatter, "TVF Bytes"]
            }

            fn visit_str<E: de::Error>(self, string: &str) -> Result<Self::Value, E> {
                if let Ok(bytes) = hex::decode(string) {
                    Ok(Bytes::from_owner(bytes))
                } else {
                    Err(de::Error::invalid_value(
                        de::Unexpected::Str(string),
                        &"Provided string is not a valid hexadecimal sequence",
                    ))
                }
            }

            #[inline]
            fn visit_bytes<E: de::Error>(self, bytes: &[u8]) -> Result<Self::Value, E> {
                Ok(Bytes::copy_from_slice(bytes))
            }
        }

        impl<'de> de::Visitor<'de> for __VisitorDate<'de> {
            type Value = NaiveDate;

            #[inline]
            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                write![formatter, "TVF Date"]
            }

            fn visit_u32<E: de::Error>(self, integer: u32) -> Result<Self::Value, E> {
                if let Some(date) = NaiveDate::from_epoch_days(integer as i32) {
                    Ok(date)
                } else {
                    Err(de::Error::invalid_value(
                        de::Unexpected::Unsigned(integer as u64),
                        &"Could not convert integer into a date",
                    ))
                }
            }

            fn visit_i32<E: de::Error>(self, integer: i32) -> Result<Self::Value, E> {
                if let Some(date) = NaiveDate::from_epoch_days(integer) {
                    Ok(date)
                } else {
                    Err(de::Error::invalid_value(
                        de::Unexpected::Signed(integer as i64),
                        &"Could not convert integer into a date",
                    ))
                }
            }

            #[inline]
            fn visit_u64<E: de::Error>(self, integer: u64) -> Result<Self::Value, E> {
                self.visit_u32(integer as u32)
            }

            #[inline]
            fn visit_i64<E: de::Error>(self, integer: i64) -> Result<Self::Value, E> {
                self.visit_i32(integer as i32)
            }

            fn visit_str<E: de::Error>(self, string: &str) -> Result<Self::Value, E> {
                static REGEX_DATE: LazyLock<Regex> =
                    LazyLock::new(|| Regex::new(r"^\d{4}-\d{2}-\d{2}$").unwrap());

                if REGEX_DATE.is_match(string)
                    && let Ok(date) = NaiveDate::parse_from_str(string, "%Y-%m-%d")
                {
                    Ok(date)
                } else {
                    Err(de::Error::invalid_value(
                        de::Unexpected::Str(string),
                        &"Could not parse string into a date",
                    ))
                }
            }
        }

        impl<'de> de::Visitor<'de> for __VisitorDateTime<'de> {
            type Value = NaiveDateTime;

            #[inline]
            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                write![formatter, "TVF DateTime"]
            }

            fn visit_u64<E: de::Error>(self, integer: u64) -> Result<Self::Value, E> {
                if let Some(datetime) = DateTime::from_timestamp_millis(integer as i64) {
                    Ok(datetime.naive_utc())
                } else {
                    Err(de::Error::invalid_value(
                        de::Unexpected::Unsigned(integer),
                        &"Could not convert integer into a datetime",
                    ))
                }
            }

            fn visit_i64<E: de::Error>(self, integer: i64) -> Result<Self::Value, E> {
                if let Some(datetime) = DateTime::from_timestamp_millis(integer as i64) {
                    Ok(datetime.naive_utc())
                } else {
                    Err(de::Error::invalid_value(
                        de::Unexpected::Signed(integer),
                        &"Could not convert integer into a datetime",
                    ))
                }
            }

            fn visit_str<E: de::Error>(self, string: &str) -> Result<Self::Value, E> {
                static REGEX_DATETIME: LazyLock<Regex> = LazyLock::new(|| {
                    Regex::new(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d{3})?$").unwrap()
                });

                if REGEX_DATETIME.is_match(string)
                    && let Ok(datetime) =
                        NaiveDateTime::parse_from_str(string, "%Y-%m-%dT%H:%T:%S%.3f")
                {
                    Ok(datetime)
                } else {
                    Err(de::Error::invalid_value(
                        de::Unexpected::Str(string),
                        &"Could not parse string into a datetime",
                    ))
                }
            }
        }

        impl<'de, 'dict, 'tvf, P, T> de::Visitor<'de> for __VisitorList<'de, 'dict, 'tvf, P, T>
        where
            P: Clone,
            T: Clone + Tvf + Default,
        {
            type Value = T;

            #[inline]
            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                write![formatter, "TVF Buffer representing a List"]
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let mut message = T::default();
                let mut index = 1;

                while let Some(value) = seq.next_element_seed(__SeedValue::new(
                    self.ignore_unknown,
                    false,
                    self.entry_type,
                ))? {
                    value.insert_in(&mut message, index);
                    index += 1;
                }

                Ok(message)
            }
        }

        // Given expected type for this entry, use the appropriate visitor
        if self.is_repeatable {
            let value =
                TvfValue::Buffer(Cow::Owned(deserializer.deserialize_seq(__VisitorList {
                    ignore_unknown: self.ignore_unknown,
                    entry_type: self.entry_type,
                    lifetime: PhantomData,
                    marker: PhantomData,
                })?));
            Ok(value)
        } else {
            let value = match self.entry_type {
                EntryType::Leaf(TvfType::Byte) => {
                    TvfValue::Byte(deserializer.deserialize_u8(__VisitorByte::default())?)
                }
                EntryType::Leaf(TvfType::Unsigned) => {
                    TvfValue::Unsigned(deserializer.deserialize_u64(__VisitorUnsigned::default())?)
                }
                EntryType::Leaf(TvfType::Signed) => {
                    TvfValue::Signed(deserializer.deserialize_i64(__VisitorSigned::default())?)
                }
                EntryType::Leaf(TvfType::Float) => {
                    TvfValue::Float(deserializer.deserialize_f64(__VisitorFloat::default())?)
                }
                EntryType::Leaf(TvfType::String) => TvfValue::String(Cow::Owned(
                    deserializer.deserialize_string(__VisitorString::default())?,
                )),
                EntryType::Leaf(TvfType::Bytes) => TvfValue::Bytes(Cow::Owned(
                    deserializer.deserialize_byte_buf(__VisitorBytes::default())?,
                )),
                EntryType::Leaf(TvfType::Date) => {
                    TvfValue::Date(deserializer.deserialize_any(__VisitorDate::default())?)
                }
                EntryType::Leaf(TvfType::DateTime) => {
                    TvfValue::DateTime(deserializer.deserialize_any(__VisitorDateTime::default())?)
                }
                EntryType::Leaf(TvfType::Buffer) => {
                    // If the buffer is not backed by a dictionary, we raise an error
                    return Err(de::Error::custom(
                        "entry is of type TVF buffer but no dictionary is associated",
                    ));
                }
                EntryType::Node(dict) => {
                    TvfValue::Buffer(Cow::Owned(deserializer.deserialize_map(
                        __VisitorBuffer::new(self.ignore_unknown, dict.as_ref()),
                    )?))
                }
            };
            Ok(value)
        }
    }
}

#[doc(hidden)]
struct __VisitorBuffer<'de, 'dict, 'tvf, P, T>
where
    P: Clone,
    T: Tvf + Default,
{
    ignore_unknown: bool,
    dictionary: &'dict Dictionary<P>,
    lifetime: PhantomData<&'de ()>,
    marker: PhantomData<&'tvf T>,
}

impl<'de, 'dict, 'tvf, P, T> __VisitorBuffer<'de, 'dict, 'tvf, P, T>
where
    P: Clone,
    T: Tvf + Default,
{
    /// Construct a new visitor for deserializing a TVF buffer
    fn new(ignore_unknown: bool, dictionary: &'dict Dictionary<P>) -> Self {
        Self {
            ignore_unknown,
            dictionary,
            lifetime: PhantomData,
            marker: PhantomData,
        }
    }
}

impl<'de, 'dict, 'tvf, P, T> de::Visitor<'de> for __VisitorBuffer<'de, 'dict, 'tvf, P, T>
where
    P: Clone,
    T: Clone + Tvf + Default,
{
    type Value = T;

    #[inline]
    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write![formatter, "TVF Buffer"]
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut message = T::default();

        while let Some(field) =
            map.next_key_seed(__SeedTag::new(self.ignore_unknown, self.dictionary))?
        {
            // If option ignore_unknown is set, we may simply skip unknown nodes
            if let __FieldTagged::Identified {
                tag,
                is_repeatable,
                entry_type,
            } = field
            {
                let value = map.next_value_seed(__SeedValue::new(
                    self.ignore_unknown,
                    is_repeatable,
                    entry_type,
                ))?;
                value.insert_in(&mut message, tag);
            }
        }

        Ok(message)
    }
}
