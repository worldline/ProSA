use crate::{
    dict::{Dictionary, EntryType, LabeledTvf, field_type::TvfValue},
    msg::tvf::{Tvf, TvfError},
};
use serde::{
    Serialize, Serializer,
    ser::{self, SerializeMap, SerializeSeq},
};

/// Serialize a labeled TVF message with serde
impl<P, T> Serialize for LabeledTvf<'_, '_, P, T>
where
    P: Clone,
    T: Clone + Tvf + Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let serial = __SerialTvf {
            ignore_unknown: self.ignore_unknown,
            dictionary: self.dictionary.as_ref(),
            message: self.message.as_ref(),
        };
        serial.serialize(serializer)
    }
}

/// Helper struct for serializing a TVF message with an associated dictionary
#[doc(hidden)]
struct __SerialTvf<'dict, 'tvf, P, T>
where
    T: Tvf + Clone,
{
    /// If an entry is not known in the dictionary, simply ignore it
    ignore_unknown: bool,

    /// The type of data expected in this dictionary
    dictionary: &'dict Dictionary<P>,

    /// TVF message to serialize
    message: &'tvf T,
}

/// Helper struct for serializing a TVF representing a list
#[doc(hidden)]
struct __SerialListTvf<'dict, 'tvf, P, T>
where
    T: Tvf + Clone,
{
    /// If an entry is not known in the dictionary, simply ignore it
    ignore_unknown: bool,

    /// The type of data expected in this dictionary
    entry_type: &'dict EntryType<P>,

    /// TVF message to serialize
    message: &'tvf T,
}

impl<P, T> Serialize for __SerialTvf<'_, '_, P, T>
where
    P: Clone,
    T: Clone + Tvf + Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // Do we target a human redable format? If so, reorder the tags.
        let human_readable = serializer.is_human_readable();
        let mut tags = self.message.keys();
        if human_readable {
            tags.sort();
        }

        // Prepare a map for serializing the message
        let mut map = serializer.serialize_map(Some(self.message.len()))?;

        // For each tag in the message, serialize using the dictionary
        for tag in tags {
            if let Some((label, entry)) = self.dictionary.tag_to_label.get(&tag) {
                let entry = entry.as_ref();

                if entry.is_repeatable {
                    let sub_list = match self.message.get_buffer(tag) {
                        Ok(list) => list,
                        Err(err) => return Err(convert_error(&err)),
                    };
                    let sub_serial = __SerialListTvf {
                        ignore_unknown: self.ignore_unknown,
                        entry_type: &entry.entry_type,
                        message: sub_list.as_ref(),
                    };
                    let _ = map.serialize_entry(label.as_ref(), &sub_serial)?;
                } else {
                    match &entry.entry_type {
                        EntryType::Leaf(expected_type) => {
                            // Try to extract the expected type
                            let value = TvfValue::from_message(self.message, tag, *expected_type)
                                .map_err(|err| convert_error(&err))?;

                            // serialize the field
                            let _ = map.serialize_entry(label.as_ref(), &value)?;
                        }
                        EntryType::Node(sub_dict) => {
                            // We expect a sub buffer
                            let sub_msg = match self.message.get_buffer(tag) {
                                Ok(sub_msg) => sub_msg,
                                Err(err) => return Err(convert_error(&err)),
                            };
                            let sub_serial = __SerialTvf {
                                ignore_unknown: self.ignore_unknown,
                                dictionary: sub_dict.as_ref(),
                                message: sub_msg.as_ref(),
                            };

                            // serialize the sub message
                            let _ = map.serialize_entry(label.as_ref(), &sub_serial)?;
                        }
                    }
                }
            }
        }
        map.end()
    }
}

impl<P, T> Serialize for __SerialListTvf<'_, '_, P, T>
where
    P: Clone,
    T: Clone + Tvf + Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // Since we are serailizing a sequence, reorder the tags
        let mut tags = self.message.keys();
        tags.sort();

        // Prepare a seq for serializing the message
        let mut seq = serializer.serialize_seq(Some(self.message.len()))?;

        // Handle the type of entries expected in the list
        match &self.entry_type {
            EntryType::Leaf(expected_type) => {
                for tag in tags {
                    // Try to extract the expected type
                    let value = TvfValue::from_message(self.message, tag, *expected_type)
                        .map_err(|err| convert_error(&err))?;

                    // serialize the field
                    let _ = seq.serialize_element(&value)?;
                }
            }
            EntryType::Node(sub_dict) => {
                for tag in tags {
                    // We expect a sub buffer
                    let sub_msg = match self.message.get_buffer(tag) {
                        Ok(sub_msg) => sub_msg,
                        Err(err) => return Err(convert_error(&err)),
                    };
                    let sub_serial = __SerialTvf {
                        ignore_unknown: self.ignore_unknown,
                        dictionary: sub_dict.as_ref(),
                        message: sub_msg.as_ref(),
                    };

                    // serialize the sub message
                    let _ = seq.serialize_element(&sub_serial)?;
                }
            }
        }
        seq.end()
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

#[inline]
fn convert_error<E>(err: &TvfError) -> E
where
    E: ser::Error,
{
    E::custom(err.to_string())
}
