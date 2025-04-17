//! This module implements the `Deserialize` trait for the `Tvf` trait

use crate::msg::{tvf::Tvf, value::TvfValue};
use bytes::Bytes;
use chrono::{NaiveDate, NaiveDateTime};
use regex::Regex;
use serde::{Deserialize, Deserializer, de::Visitor};
use std::{
    borrow::Cow,
    fmt::{self, Debug},
    marker::PhantomData,
    sync::LazyLock,
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

                // evaluate the regular expressions used to detect date and datetime values
                static REGEX_DATE: LazyLock<Regex> =
                    LazyLock::new(|| Regex::new(r"^\d{4}-\d{2}-\d{2}$").unwrap());
                static REGEX_DATETIME: LazyLock<Regex> = LazyLock::new(|| {
                    Regex::new(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d{3})?$").unwrap()
                });

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
