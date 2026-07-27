//! Implementation of the `Serialize` trait for a TVF message binded with a dictionary.
//! This will replace numerical identifiers with a corresponding text mnemonic.

use crate::msg::{
    dict::{TvfDict, value::TvfValue},
    tvf::{Tvf, TvfType},
};
use serde::{Serialize, Serializer, ser::SerializeMap};

/// Serialize a TVF message with a dictionary
pub struct TvfSerializer<'t, 'd, T>
where
    T: Tvf,
{
    /// TVF message to qualify
    pub msg: &'t T,

    /// Dictionary to qualify the message
    pub dict: &'d dyn TvfDict,

    /// Ignore missing values and type definition
    pub ignore_missing: bool,
}

impl<'t, 'd, T> TvfSerializer<'t, 'd, T>
where
    T: Tvf,
{
    /// Bind a TVF message with a dictionary
    #[inline]
    pub fn new(msg: &'t T, dict: &'d dyn TvfDict) -> Self {
        Self {
            msg,
            dict,
            ignore_missing: true,
        }
    }
}

impl<'t, 'd, T> Serialize for TvfSerializer<'t, 'd, T>
where
    T: Tvf + Clone + Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // Do we target a human redable format? If so, reorder the tags.
        let human_readable = serializer.is_human_readable();
        let mut keys = self.msg.keys();
        if human_readable {
            keys.sort();
        }

        // Prepare a map for serializing the message
        let mut map = serializer.serialize_map(Some(self.msg.len()))?;

        // For each tag in the message, serialize using the dictionary
        for id in keys {
            // We necessarely need a type hint to know how extract the field
            if let Some(type_hint) = self.dict.type_hint(id) {
                // If the type hint is wrong we cannot handle the message
                let Ok(value) = TvfValue::from_message(self.msg, id, type_hint) else {
                    return Err(serde::ser::Error::custom("Type hint is wrong"));
                };

                // Then we either write the key as an integer or as a string
                if let Some(mnemo) = self.dict.from_id(id) {
                    map.serialize_key(mnemo)?;
                } else {
                    map.serialize_key(&id)?;
                }

                // And finally we write the value
                match type_hint {
                    TvfType::Buffer => {
                        if let Some(sub_dict) = self.dict.sub_dict(id) {
                            let sub_msg = value.to_buffer().unwrap();
                            let bind = TvfSerializer::new(sub_msg.as_ref(), sub_dict);
                            map.serialize_value(&bind)?;
                        } else {
                            map.serialize_value(&value)?;
                        }
                    }
                    TvfType::List => {
                        todo!()
                    }
                    _ => {
                        map.serialize_value(&value)?;
                    }
                }
            } else if !self.ignore_missing {
                return Err(serde::ser::Error::custom("Missing type hint for field"));
            }
            // Without a type hint, we cannot know which get method will work
        }
        map.end()
    }
}

/// Serialize the TVF value
impl<T> Serialize for TvfValue<'_, T>
where
    T: Tvf + Clone + Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        #[cfg_attr(rustfmt, rustfmt_skip)]
        match self {
            Self::Byte    (value) => serializer.serialize_u8(*value),
            Self::Unsigned(value) => serializer.serialize_u64(*value),
            Self::Signed  (value) => serializer.serialize_i64(*value),
            Self::Float   (value) => serializer.serialize_f64(*value),
            Self::String  (value) => serializer.serialize_str(value.as_ref()),
            Self::Bytes   (value) => serializer.serialize_bytes(value.as_ref()),
            Self::Date    (value) => value.serialize(serializer),
            Self::DateTime(value) => value.serialize(serializer),
            Self::Buffer  (value) => value.as_ref().serialize(serializer),
        }
    }
}
