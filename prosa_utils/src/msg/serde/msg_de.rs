//! This module implements the `Deserialize` trait for the `Tvf` trait

use super::{Dictionary, Entry};
use crate::msg::{tvf::Tvf, value::TvfValue};
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

/// Deserialize the TVF value
impl<'de, T> Deserialize<'de> for TvfValue<'_, T>
where
    T: Tvf + Clone + Default + Debug + Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        /// Visitor for the field value of TVF buffers
        #[doc(hidden)]
        struct __Visitor<'de, 'v, T>
        where
            T: Tvf + Clone,
        {
            marker: PhantomData<TvfValue<'v, T>>,
            lifetime: PhantomData<&'de ()>,
        }

        /// Macro to define the visit method for the field value visitor
        macro_rules! fn_visit_value {
            // used for all native types
            ( $method:ident($value_type:ty) => $field:ident as $output_type:ty ) => {
                fn $method<E: serde::de::Error>(
                    self,
                    value: $value_type,
                ) -> Result<Self::Value, E> {
                    Ok(TvfValue::$field(value as $output_type))
                }
            };
        }

        impl<'de, 'v, T> Visitor<'de> for __Visitor<'de, 'v, T>
        where
            T: Tvf + Clone + Debug + Default + Deserialize<'de>,
        {
            type Value = TvfValue<'v, T>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                write![formatter, "TVF value"]
            }

            // implementation of the visit methods for any native type available
            fn_visit_value![ visit_bool(bool) => Byte     as u8  ];
            fn_visit_value![ visit_u8  (u8  ) => Unsigned as u64 ];
            fn_visit_value![ visit_u16 (u16 ) => Unsigned as u64 ];
            fn_visit_value![ visit_u32 (u32 ) => Unsigned as u64 ];
            fn_visit_value![ visit_u64 (u64 ) => Unsigned as u64 ];
            fn_visit_value![ visit_i8  (i8  ) => Signed   as i64 ];
            fn_visit_value![ visit_i16 (i16 ) => Signed   as i64 ];
            fn_visit_value![ visit_i32 (i32 ) => Signed   as i64 ];
            fn_visit_value![ visit_i64 (i64 ) => Signed   as i64 ];
            fn_visit_value![ visit_f32 (f32 ) => Float    as f64 ];
            fn_visit_value![ visit_f64 (f64 ) => Float    as f64 ];

            /// implementation of the visit method for strings
            fn visit_str<E: serde::de::Error>(self, string: &str) -> Result<Self::Value, E> {
                // if human readable format,
                // some strings may be reinterpreted as other types

                // evaluate the regular expressions used to detect date, datetime and byte buffer values
                static REGEX_DATE: LazyLock<Regex> =
                    LazyLock::new(|| Regex::new(r"^\d{4}-\d{2}-\d{2}$").unwrap());
                static REGEX_DATETIME: LazyLock<Regex> = LazyLock::new(|| {
                    Regex::new(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d{3})?$").unwrap()
                });
                static REGEX_BYTES: LazyLock<Regex> =
                    LazyLock::new(|| Regex::new(r"^0x[0-9A-Za-z]+$").unwrap());

                // try to parse the string as a date
                if REGEX_DATE.is_match(string) {
                    if let Ok(date) = NaiveDate::parse_from_str(string, "%Y-%m-%d") {
                        return Ok(TvfValue::Date(date));
                    }
                }

                // try to parse the string as a datetime
                if REGEX_DATETIME.is_match(string) {
                    if let Ok(datetime) =
                        NaiveDateTime::parse_from_str(string, "%Y-%m-%dT%H:%T:%S%.3f")
                    {
                        return Ok(TvfValue::DateTime(datetime));
                    }
                }

                // try to parse the string as a buffer of bytes
                if REGEX_BYTES.is_match(string) {
                    let (_, string) = string.split_at(2);
                    if let Ok(buffer) = hex::decode(string) {
                        return Ok(TvfValue::Bytes(Cow::Owned(Bytes::from(buffer))));
                    }
                }

                // the value is a string
                Ok(TvfValue::String(Cow::Owned(string.to_string())))
            }

            /// implementation of the visit method for byte arrays
            fn visit_bytes<E: serde::de::Error>(self, bytes: &[u8]) -> Result<Self::Value, E> {
                Ok(TvfValue::Bytes(Cow::Owned(Bytes::copy_from_slice(bytes))))
            }

            /// implementation of the visit method for maps
            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let mut buffer = T::default();
                while let Some((id, field)) = map.next_entry()? {
                    buffer.put(id, field);
                }
                Ok(TvfValue::Buffer(Cow::Owned(buffer)))
            }
        }

        deserializer.deserialize_any(__Visitor {
            marker: PhantomData,
            lifetime: PhantomData,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_json() {
        let json = r#"
{
    "2": true,
    "4": {
        "10": "hello",
        "20": "world",
        "30": 21,
        "5": "2025-01-01",
        "6": "2023-06-05T15:02:00",
        "7": "0xF0E1D2C3B4A5968778695A4B3C2D1E0F"
    },
    "5": [
        {
            "10": "guten tag",
            "30": 22
        },
        {
            "10": "bonjour",
            "20": "tous",
            "30": 23
        },
        {
            "10": "egun on",
            "20": "mundua",
            "30": 24
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
