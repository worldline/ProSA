//! Implementation of the `Deserialize` trait for a TVF message based on a dictionary.
//! This will identify fields using their text mnemonic and reassign them to their numerical identifiers.

use crate::msg::{
    dict::{TvfDict, value::TvfValue},
    tvf::{Tvf, TvfType},
};
use bytes::Bytes;
use chrono::{DateTime, NaiveDate, NaiveDateTime};

use regex::Regex;
use serde::{
    Deserialize, Deserializer,
    de::{self, DeserializeSeed, MapAccess},
};
use std::{
    borrow::Cow,
    fmt::{self},
    marker::PhantomData,
    sync::LazyLock,
};

/// Deserialize a TVF message with a dictionary
pub struct TvfDeserializer<'d, T: Tvf> {
    /// Dictionary to qualify the message
    pub dict: &'d dyn TvfDict,

    /// Ignore missing values and type definition
    pub ignore_missing: bool,

    /// The type of the message to output
    __marker: PhantomData<T>,
}

impl<'d, T: Tvf> TvfDeserializer<'d, T> {
    /// Create a deserializer from a dictionary
    #[inline]
    pub fn new(dict: &'d dyn TvfDict) -> Self {
        Self {
            dict,
            ignore_missing: true,
            __marker: PhantomData,
        }
    }
}

impl<'de, 'd, T> DeserializeSeed<'de> for TvfDeserializer<'d, T>
where
    T: Tvf + Clone + Default + Deserialize<'de>,
{
    type Value = T;

    fn deserialize<D: serde::Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_map(__VisitorBuffer {
            dict: self.dict,
            ignore_missing: self.ignore_missing,
            __lifetime: PhantomData,
            __marker: PhantomData,
        })
    }
}

// MARK: field key

// Seed for identifying tags based on label
#[doc(hidden)]
struct __SeedKey<'d> {
    dict: &'d dyn TvfDict,
    ignore_missing: bool,
}

impl<'de, 'd> de::DeserializeSeed<'de> for __SeedKey<'d> {
    type Value = Option<usize>;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        // custom visitor over an integer or a string
        #[doc(hidden)]
        struct __Visitor<'de, 'd> {
            dict: &'d dyn TvfDict,
            ignore_missing: bool,
            __lifetime: PhantomData<&'de ()>,
        }

        impl<'de, 'd> de::Visitor<'de> for __Visitor<'de, 'd> {
            type Value = Option<usize>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                write![formatter, "TVF identifier"]
            }

            #[inline]
            fn visit_i64<E: de::Error>(self, id: i64) -> Result<Self::Value, E> {
                Ok(Some(id as usize))
            }

            #[inline]
            fn visit_i128<E: de::Error>(self, id: i128) -> Result<Self::Value, E> {
                Ok(Some(id as usize))
            }

            #[inline]
            fn visit_u64<E: de::Error>(self, id: u64) -> Result<Self::Value, E> {
                Ok(Some(id as usize))
            }

            #[inline]
            fn visit_u128<E: de::Error>(self, id: u128) -> Result<Self::Value, E> {
                Ok(Some(id as usize))
            }

            fn visit_str<E: de::Error>(self, mnemo: &str) -> Result<Self::Value, E> {
                if let Some(id) = self.dict.from_mnemo(mnemo) {
                    Ok(Some(id))
                } else if self.ignore_missing {
                    Ok(None)
                } else {
                    Err(de::Error::unknown_field(
                        mnemo,
                        &["mnemonic defined in the dictionary"],
                    ))
                }
            }
        }

        deserializer.deserialize_any(__Visitor {
            dict: self.dict,
            ignore_missing: self.ignore_missing,
            __lifetime: PhantomData,
        })
    }
}

// MARK: field value

#[doc(hidden)]
struct __SeedValue<'t, 'd, T: Tvf> {
    type_hint: TvfType,
    ignore_missing: bool,
    sub_dict: Option<&'d dyn TvfDict>,
    __marker: PhantomData<&'t T>,
}

impl<'de, 't, 'd, T> de::DeserializeSeed<'de> for __SeedValue<'t, 'd, T>
where
    T: Tvf + Clone + Default,
{
    type Value = TvfValue<'t, T>;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[cfg_attr(rustfmt, rustfmt_skip)]
        let value = match self.type_hint {
            TvfType::Byte     => TvfValue::Byte    (deserializer.deserialize_u8 (__VisitorByte    ::default())?),
            TvfType::Unsigned => TvfValue::Unsigned(deserializer.deserialize_u64(__VisitorUnsigned::default())?),
            TvfType::Signed   => TvfValue::Signed  (deserializer.deserialize_i64(__VisitorSigned  ::default())?),
            TvfType::Float    => TvfValue::Float   (deserializer.deserialize_f64(__VisitorFloat   ::default())?),
            TvfType::Date     => TvfValue::Date    (deserializer.deserialize_any(__VisitorDate    ::default())?),
            TvfType::DateTime => TvfValue::DateTime(deserializer.deserialize_any(__VisitorDateTime::default())?),
            TvfType::String   => TvfValue::String(Cow::Owned(deserializer.deserialize_string  (__VisitorString::default())?)),
            TvfType::Bytes    => TvfValue::Bytes (Cow::Owned(deserializer.deserialize_byte_buf(__VisitorBytes ::default())?)),
            TvfType::Buffer   => {
                if let Some(sub_dict) = self.sub_dict {
                    TvfValue::Buffer(Cow::Owned(deserializer.deserialize_map(__VisitorBuffer {
                        dict: sub_dict,
                        ignore_missing: self.ignore_missing,
                        __lifetime: PhantomData,
                        __marker: PhantomData,
                    })?))
                } else {
                    // If the buffer is not backed by a dictionary, we raise an error
                    return Err(de::Error::custom(
                        "entry is of type TVF buffer but no dictionary is associated",
                    ));
                }
            },
            TvfType::List => {
                todo!()
            },
        };
        Ok(value)
    }
}

// MARK: visitors

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
struct __VisitorBuffer<'de, 't, 'd, T>
where
    T: Tvf + Clone + Default,
{
    dict: &'d dyn TvfDict,
    ignore_missing: bool,
    __lifetime: PhantomData<&'de ()>,
    __marker: PhantomData<&'t T>,
}

/// Trivially implement a visit method for a given type
macro_rules! visit_value {
    ($visit_method:ident ; $input_type:ty) => {
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

    visit_value![ visit_bool ; bool ];
    visit_value![ visit_char ; char ];
    visit_value![ visit_u8   ; u8   ];
    visit_value![ visit_u16  ; u16  ];
    visit_value![ visit_u32  ; u32  ];
    visit_value![ visit_u64  ; u64  ];
    visit_value![ visit_i8   ; i8   ];
    visit_value![ visit_i16  ; i16  ];
    visit_value![ visit_i32  ; i32  ];
    visit_value![ visit_i64  ; i64  ];
}

impl<'de> de::Visitor<'de> for __VisitorUnsigned<'de> {
    type Value = u64;

    #[inline]
    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write![formatter, "TVF Unsigned Integer"]
    }

    visit_value![ visit_bool ; bool ];
    visit_value![ visit_char ; char ];
    visit_value![ visit_u64  ; u64  ];
    visit_value![ visit_i64  ; i64  ];
}

impl<'de> de::Visitor<'de> for __VisitorSigned<'de> {
    type Value = i64;

    #[inline]
    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write![formatter, "TVF Signed Integer"]
    }

    visit_value![ visit_bool ; bool ];
    visit_value![ visit_char ; char ];
    visit_value![ visit_u64  ; u64  ];
    visit_value![ visit_i64  ; i64  ];
}

impl<'de> de::Visitor<'de> for __VisitorFloat<'de> {
    type Value = f64;

    #[inline]
    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write![formatter, "TVF Float"]
    }

    visit_value![ visit_u64 ; u64 ];
    visit_value![ visit_i64 ; i64 ];
    visit_value![ visit_f64 ; f64 ];
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
            && let Ok(datetime) = NaiveDateTime::parse_from_str(string, "%Y-%m-%dT%H:%T:%S%.3f")
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

impl<'de, 't, 'd, T> de::Visitor<'de> for __VisitorBuffer<'de, 't, 'd, T>
where
    T: Tvf + Clone + Default,
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

        while let Some(id) = map.next_key_seed(__SeedKey {
            dict: self.dict,
            ignore_missing: self.ignore_missing,
        })? {
            if let Some(id) = id
                && let Some(type_hint) = self.dict.type_hint(id)
            {
                let sub_dict = self.dict.sub_dict(id);
                let value = map.next_value_seed(__SeedValue {
                    type_hint,
                    ignore_missing: self.ignore_missing,
                    sub_dict,
                    __marker: PhantomData,
                })?;
                value.insert_in(&mut message, id);
            }
        }

        Ok(message)
    }
}
