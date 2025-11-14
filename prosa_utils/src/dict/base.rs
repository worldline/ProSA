use super::*;
use crate::{dict::field_type::TvfValue, msg::tvf::Tvf};

impl<P> Dictionary<P> {
    /// Create a new empty dictionary
    #[inline]
    pub fn new() -> Self {
        Self {
            tag_to_label: HashMap::new(),
            label_to_tag: HashMap::new(),
        }
    }

    /// Create a new dictionary with a capacity
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            tag_to_label: HashMap::with_capacity(capacity),
            label_to_tag: HashMap::with_capacity(capacity),
        }
    }

    /// Get the label of a field
    #[inline]
    pub fn get_label(&self, id: usize) -> Option<&str> {
        self.tag_to_label.get(&id).map(|(label, _)| label.as_ref())
    }

    /// Get the id of a field
    #[inline]
    pub fn get_id(&self, label: &str) -> Option<usize> {
        self.label_to_tag.get(label).map(|(id, _)| *id)
    }

    /// Get the label of a field and its corresponding entry
    #[inline]
    pub fn get_label_and_entry(&self, id: usize) -> Option<(&str, &Entry<P>)> {
        self.tag_to_label
            .get(&id)
            .map(|(label, entry)| (label.as_ref(), entry.as_ref()))
    }

    /// Get the id of a field and its corresponding entry
    #[inline]
    pub fn get_id_and_entry(&self, label: &str) -> Option<(usize, &Entry<P>)> {
        self.label_to_tag
            .get(label)
            .map(|(id, entry)| (*id, entry.as_ref()))
    }

    /// Add a new field definition to the dictionary
    pub fn add_entry(&mut self, id: usize, label: &str, entry: Entry<P>) {
        let label: Arc<str> = label.into();
        let def = Arc::new(entry);
        self.tag_to_label.insert(id, (label.clone(), def.clone()));
        self.label_to_tag.insert(label, (id, def));
    }
}

impl<P> Entry<P> {
    /// Create a new entry with the requested type
    #[inline]
    pub fn new(tvf_type: TvfType, payload: P) -> Self {
        Self {
            is_repeatable: false,
            entry_type: EntryType::Leaf(tvf_type),
            payload,
        }
    }

    /// Create a new entry using the provided sub-directory
    #[inline]
    pub fn new_sub_dict(sub_dict: Arc<Dictionary<P>>, payload: P) -> Self {
        Self {
            is_repeatable: false,
            entry_type: EntryType::Node(sub_dict),
            payload,
        }
    }

    /// Create a new list of entries with the requested type
    #[inline]
    pub fn new_list(tvf_type: TvfType, payload: P) -> Self {
        Self {
            is_repeatable: true,
            entry_type: EntryType::Leaf(tvf_type),
            payload,
        }
    }

    /// Create a new list of entries using the provided sub-directory
    #[inline]
    pub fn new_list_sub_dict(sub_dict: Arc<Dictionary<P>>, payload: P) -> Self {
        Self {
            is_repeatable: true,
            entry_type: EntryType::Node(sub_dict),
            payload,
        }
    }

    /// Get the payload of the entry
    #[inline]
    pub fn get_payload(&self) -> &P {
        &self.payload
    }

    /// Get the TVF type of the entry
    #[inline]
    pub fn get_type(&self) -> TvfType {
        self.entry_type.get_type()
    }

    /// Get the sub-dictionary of this entry if any
    #[inline]
    pub fn get_sub_dict(&self) -> Option<Arc<Dictionary<P>>> {
        match &self.entry_type {
            EntryType::Leaf(_) => None,
            EntryType::Node(dict) => Some(dict.clone()),
        }
    }
}

impl<P> EntryType<P> {
    /// Get the type expected for this entry
    #[inline]
    pub fn get_type(&self) -> TvfType {
        match self {
            Self::Leaf(tvf_type) => *tvf_type,
            Self::Node(_) => TvfType::Buffer,
        }
    }
}

impl<'dict, 'tvf, P, T> LabeledTvf<'dict, 'tvf, P, T>
where
    P: Clone,
    T: Clone + Tvf,
{
    /// Get a field using a numeric tag
    pub fn get_with_id(&'tvf self, id: usize) -> Option<TvfValue<'tvf, T>> {
        if let Some((_, entry)) = self.dictionary.tag_to_label.get(&id)
            && let Ok(value) = TvfValue::from_message(self.message.as_ref(), id, entry.get_type())
        {
            Some(value)
        } else {
            None
        }
    }

    /// Get a field using a label
    pub fn get_with_label(&'tvf self, label: &str) -> Option<TvfValue<'tvf, T>> {
        if let Some((id, entry)) = self.dictionary.label_to_tag.get(label)
            && let Ok(value) = TvfValue::from_message(self.message.as_ref(), *id, entry.get_type())
        {
            Some(value)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        dict::*,
        msg::{simple_string_tvf::SimpleStringTvf, tvf::Tvf},
    };
    use bytes::Bytes;
    use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
    use std::sync::LazyLock;

    #[derive(Debug, Clone, Copy)]
    enum SomePayload {
        A,
        B,
        C,
    }

    /// A sample dictionary for testing
    static DICT: LazyLock<Dictionary<SomePayload>> = LazyLock::new(|| {
        // Create a sub-directory to insert into the main one
        let mut sub_dict = Dictionary::with_capacity(3);
        sub_dict.add_entry(10, "name", Entry::new(TvfType::String, SomePayload::A));
        sub_dict.add_entry(20, "birth", Entry::new(TvfType::Date, SomePayload::B));
        sub_dict.add_entry(30, "address", Entry::new(TvfType::String, SomePayload::C));
        let sub_dict = Arc::new(sub_dict);

        // Create the main dictionary
        let mut dict = Dictionary::with_capacity(6);
        dict.add_entry(1, "first", Entry::new(TvfType::Signed, SomePayload::A));
        dict.add_entry(2, "second", Entry::new(TvfType::Float, SomePayload::B));
        dict.add_entry(3, "third", Entry::new(TvfType::Bytes, SomePayload::C));
        dict.add_entry(
            4,
            "fourth",
            Entry::new_sub_dict(sub_dict.clone(), SomePayload::B),
        );
        dict.add_entry(
            5,
            "fifth",
            Entry::new_list(TvfType::DateTime, SomePayload::C),
        );
        dict.add_entry(
            6,
            "sixth",
            Entry::new_list_sub_dict(sub_dict.clone(), SomePayload::C),
        );

        dict
    });

    /// A sample message for testing
    static MSG: LazyLock<SimpleStringTvf> = LazyLock::new(|| {
        // Create a sub-message
        let mut sub_tvf = SimpleStringTvf::default();
        sub_tvf.put_string(10, "TinTin");
        sub_tvf.put_date(20, NaiveDate::from_ymd_opt(1929, 1, 10).unwrap());
        sub_tvf.put_string(30, "Moulinsart");

        // Create a list of timestamps
        let mut list = SimpleStringTvf::default();

        // helper for writing timestamps
        macro_rules! datetime {
            ($year:literal - $month:literal - $day:literal $hour:literal : $minute:literal : $second:literal , $millis:literal) => {
                NaiveDateTime::new(
                    NaiveDate::from_ymd_opt($year, $month, $day).unwrap(),
                    NaiveTime::from_hms_milli_opt($hour, $minute, $second, $millis).unwrap(),
                )
            };
        }
        list.put_datetime(1, datetime!(2025-11-10 10:40:00,000));
        list.put_datetime(2, datetime!(2025-12-25 18:30:00,000));
        list.put_datetime(3, datetime!(2026-01-01 00:00:00,000));

        // Create the main message
        let mut tvf = SimpleStringTvf::default();
        tvf.put_signed(1, 100);
        tvf.put_float(2, 0.5);
        tvf.put_bytes(3, Bytes::from_static(&[0xff, 0x01, 0x02, 0x03]));
        tvf.put_buffer(4, sub_tvf.clone());

        tvf
    });

    #[test]
    fn dict_get_labels() {
        assert_eq!("first", DICT.get_label(1).unwrap());
        assert_eq!("second", DICT.get_label(2).unwrap());

        let (label, entry) = DICT.get_label_and_entry(3).unwrap();
        assert_eq!("third", label);
        assert_eq!(TvfType::Bytes, entry.get_type());

        let (label, entry) = DICT.get_label_and_entry(4).unwrap();
        assert_eq!("fourth", label);
        assert_eq!(TvfType::Buffer, entry.get_type());
        if let Some(sub_dict) = entry.get_sub_dict() {
            assert_eq!("name", sub_dict.get_label(10).unwrap());
            assert_eq!("birth", sub_dict.get_label(20).unwrap());
            assert_eq!("address", sub_dict.get_label(30).unwrap());
        } else {
            panic!("Expected a sub-dictionary");
        }
    }

    #[test]
    fn dict_get_ids() {
        assert_eq!(1, DICT.get_id("first").unwrap());
        assert_eq!(2, DICT.get_id("second").unwrap());

        let (id, entry) = DICT.get_id_and_entry("third").unwrap();
        assert_eq!(3, id);
        assert_eq!(TvfType::Bytes, entry.get_type());

        let (id, entry) = DICT.get_id_and_entry("fourth").unwrap();
        assert_eq!(4, id);
        assert_eq!(TvfType::Buffer, entry.get_type());
        if let Some(sub_dict) = entry.get_sub_dict() {
            assert_eq!(10, sub_dict.get_id("name").unwrap());
            assert_eq!(20, sub_dict.get_id("birth").unwrap());
            assert_eq!(30, sub_dict.get_id("address").unwrap());
        } else {
            panic!("Expected a sub-dictionary");
        }
    }
}
