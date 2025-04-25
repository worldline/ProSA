//! Implementation of a simple String TVF

use bytes::Bytes;
use chrono::{NaiveDate, NaiveDateTime};

use crate::msg::tvf::{Tvf, TvfError};
use std::{borrow::Cow, collections::hash_map::HashMap};

/// Struct that define a simple string TVF
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SimpleStringTvf {
    fields: HashMap<usize, String>,
}

static SIMPLE_DATE_FMT: &str = "%Y-%m-%d";
static SIMPLE_DATETIME_FMT: &str = "%Y-%m-%dT%H:%M:%S";

/// TVF implementation of simple string TVF
impl Tvf for SimpleStringTvf {
    /// Test if the TVF is empty (no value in it)
    ///
    /// # Examples
    ///
    /// ```
    /// use prosa_utils::msg::tvf::Tvf;
    /// use prosa_utils::msg::simple_string_tvf::SimpleStringTvf;
    ///
    /// let tvf: SimpleStringTvf = Default::default();
    ///
    /// assert_eq!(true, tvf.is_empty());
    /// ```
    fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    /// Get the length of the TVF (number of value in it)
    ///
    /// # Examples
    ///
    /// ```
    /// use prosa_utils::msg::tvf::Tvf;
    /// use prosa_utils::msg::simple_string_tvf::SimpleStringTvf;
    ///
    /// let mut tvf: SimpleStringTvf = Default::default();
    /// tvf.put_string(1, String::from("first_val"));
    /// tvf.put_string(2, String::from("second_val"));
    ///
    /// assert_eq!(2, tvf.len());
    /// ```
    fn len(&self) -> usize {
        self.fields.len()
    }

    fn contains(&self, id: usize) -> bool {
        self.fields.contains_key(&id)
    }

    fn remove(&mut self, id: usize) {
        self.fields.remove(&id);
    }

    fn into_keys(self) -> Vec<usize> {
        self.fields.into_keys().collect::<_>()
    }

    fn keys(&self) -> Vec<usize> {
        self.fields.keys().cloned().collect()
    }

    fn get_buffer(&self, id: usize) -> Result<Cow<'_, SimpleStringTvf>, TvfError> {
        match self.fields.get(&id) {
            Some(str_value) => Ok(Cow::Owned(SimpleStringTvf::deserialize(str_value)?)),
            None => Err(TvfError::FieldNotFound(id)),
        }
    }

    fn get_unsigned(&self, id: usize) -> Result<u64, TvfError> {
        match self.fields.get(&id) {
            Some(str_value) => match str_value.parse::<u64>() {
                Ok(value) => Ok(value),
                Err(_) => Err(TvfError::TypeMismatch),
            },
            None => Err(TvfError::FieldNotFound(id)),
        }
    }

    fn get_signed(&self, id: usize) -> Result<i64, TvfError> {
        match self.fields.get(&id) {
            Some(str_value) => match str_value.parse::<i64>() {
                Ok(value) => Ok(value),
                Err(_) => Err(TvfError::TypeMismatch),
            },
            None => Err(TvfError::FieldNotFound(id)),
        }
    }

    fn get_byte(&self, id: usize) -> Result<u8, TvfError> {
        match self.fields.get(&id) {
            Some(str_value) => match hex::decode(str_value) {
                Ok(bytes) => Ok(bytes[0]),
                Err(_) => Err(TvfError::TypeMismatch),
            },
            None => Err(TvfError::FieldNotFound(id)),
        }
    }

    fn get_float(&self, id: usize) -> Result<f64, TvfError> {
        match self.fields.get(&id) {
            Some(str_value) => match str_value.parse::<f64>() {
                Ok(value) => Ok(value),
                Err(_) => Err(TvfError::TypeMismatch),
            },
            None => Err(TvfError::FieldNotFound(id)),
        }
    }

    fn get_string(&self, id: usize) -> Result<Cow<'_, String>, TvfError> {
        match self.fields.get(&id) {
            Some(value) => Ok(Cow::Borrowed(value)),
            None => Err(TvfError::FieldNotFound(id)),
        }
    }

    fn get_bytes(&self, id: usize) -> Result<Cow<'_, Bytes>, TvfError> {
        match self.fields.get(&id) {
            Some(str_value) => match hex::decode(str_value) {
                Ok(bytes) => Ok(Cow::Owned(Bytes::from(bytes))),
                Err(_) => Err(TvfError::TypeMismatch),
            },
            None => Err(TvfError::FieldNotFound(id)),
        }
    }

    fn get_date(&self, id: usize) -> Result<NaiveDate, TvfError> {
        match self.fields.get(&id) {
            Some(str_value) => match NaiveDate::parse_from_str(str_value, SIMPLE_DATE_FMT) {
                Ok(d) => Ok(d),
                Err(e) => Err(TvfError::ConvertionError(e.to_string())),
            },
            None => Err(TvfError::FieldNotFound(id)),
        }
    }

    fn get_datetime(&self, id: usize) -> Result<NaiveDateTime, TvfError> {
        match self.fields.get(&id) {
            Some(str_value) => {
                match NaiveDateTime::parse_from_str(str_value, SIMPLE_DATETIME_FMT) {
                    Ok(d) => Ok(d),
                    Err(e) => Err(TvfError::ConvertionError(e.to_string())),
                }
            }
            None => Err(TvfError::FieldNotFound(id)),
        }
    }

    //fn get_datetime_at<T: chrono::Offset + chrono::TimeZone>(&self, id: usize, offset: T) -> Result<chrono::DateTime<T>, TvfError> {
    //    match self.fields.get(&id) {
    //        Some(str_value) => match NaiveDateTime::parse_from_str(str_value, SIMPLE_DATETIME_FMT) {
    //            Ok(d) => Ok(offset.from_utc_datetime(&d)),
    //            Err(e) => Err(TvfError::ConvertionError(e.to_string()))
    //        },
    //        None => Err(TvfError::FieldNotFound(id)),
    //    }
    //}

    fn put_buffer(&mut self, id: usize, buffer: SimpleStringTvf) {
        self.fields.insert(id, buffer.serialize());
    }

    fn put_unsigned(&mut self, id: usize, unsigned: u64) {
        self.fields.insert(id, unsigned.to_string());
    }

    fn put_signed(&mut self, id: usize, signed: i64) {
        self.fields.insert(id, signed.to_string());
    }

    fn put_byte(&mut self, id: usize, byte: u8) {
        let s = hex::encode(vec![byte]);
        self.fields.insert(id, s);
    }

    fn put_float(&mut self, id: usize, float: f64) {
        self.fields.insert(id, float.to_string());
    }

    fn put_string<T: Into<String>>(&mut self, id: usize, string: T) {
        self.fields.insert(id, string.into());
    }

    fn put_bytes(&mut self, id: usize, buffer: bytes::Bytes) {
        let s = hex::encode(buffer);
        self.fields.insert(id, s);
    }

    fn put_date(&mut self, id: usize, date: chrono::NaiveDate) {
        self.fields
            .insert(id, date.format(SIMPLE_DATE_FMT).to_string());
    }

    fn put_datetime(&mut self, id: usize, datetime: chrono::NaiveDateTime) {
        self.fields
            .insert(id, datetime.format(SIMPLE_DATETIME_FMT).to_string());
    }

    //fn put_datetime_tz<T: chrono::Offset + chrono::TimeZone>(&mut self, id: usize, datetime: chrono::DateTime<T>) {
    //    self.fields.insert(id, datetime.naive_utc().format(SIMPLE_DATETIME_FMT).to_string());
    //}
}

impl SimpleStringTvf {
    /// Serialize this TVF to String
    pub fn serialize(&self) -> String {
        let mut out_str = String::new();
        //let mut writer = BufWriter::new(&out_str);

        for (k, v) in self.fields.iter() {
            let len = v.len();
            out_str.push_str(&format!("{k};{len};{v};"));
        }

        out_str
    }

    /// Load a TVF from String
    pub fn deserialize(serial: &str) -> Result<SimpleStringTvf, TvfError> {
        let mut buffer: SimpleStringTvf = Default::default();
        let mut w_serial = serial;

        while let Some((k, lv)) = w_serial.split_once(';') {
            let key = k
                .parse::<usize>()
                .map_err(|e| TvfError::SerializationError(e.to_string()))?;

            if let Some((l, rest)) = lv.split_once(';') {
                let len = l
                    .parse::<usize>()
                    .map_err(|e| TvfError::SerializationError(e.to_string()))?;
                buffer.fields.insert(key, String::from(&rest[0..len]));
                if rest.chars().nth(len) != Some(';') {
                    return Err(TvfError::SerializationError(
                        "Bad field termination char".into(),
                    ));
                }
                w_serial = &rest[len + 1..];
            } else {
                return Err(TvfError::SerializationError("No len after key".into()));
            }
        }

        Ok(buffer)
    }
}

#[cfg(test)]
mod tests {
    use crate::msg::tvf::TvfFilter;

    use super::*;
    use std::fmt::Debug;

    fn test_tvf<T: Tvf + Default + Debug + PartialEq + Clone>(tvf: &mut T) {
        let mut sub_buffer: T = Default::default();
        sub_buffer.put_float(200, 154.5);
        sub_buffer.put_string(201, "Hello rust!");
        sub_buffer.remove(201);
        assert!(!sub_buffer.contains(201));
        sub_buffer.put_string(201, "Hello world!");
        assert!(sub_buffer.contains(201));

        assert!(tvf.is_empty());
        tvf.put_unsigned(1, 42);
        assert_eq!(1, tvf.len());
        assert!(!tvf.contains(2));
        tvf.put_string(2, String::from("The great string"));
        assert!(tvf.contains(2));
        tvf.put_signed(3, -1);
        assert_eq!(-1, tvf.get_signed(3).unwrap());
        tvf.put_float(5, 6.56);
        tvf.put_byte(6, 32u8);
        tvf.put_bytes(7, Bytes::from(hex::decode("aabb77ff").unwrap()));
        tvf.put_date(8, NaiveDate::from_ymd_opt(2023, 6, 5).unwrap());
        tvf.put_datetime(
            9,
            NaiveDate::from_ymd_opt(2023, 6, 5)
                .unwrap()
                .and_hms_opt(15, 2, 0)
                .unwrap(),
        );
        tvf.put_buffer(10, sub_buffer.clone());
        assert_eq!(Err(TvfError::TypeMismatch), tvf.get_unsigned(2));
        assert_eq!(Err(TvfError::TypeMismatch), tvf.get_signed(2));
        assert_eq!(Err(TvfError::TypeMismatch), tvf.get_float(2));
        assert_eq!(Err(TvfError::TypeMismatch), tvf.get_byte(2));
        assert_eq!(Err(TvfError::TypeMismatch), tvf.get_bytes(2));
        assert_eq!(
            Err(TvfError::ConvertionError(
                "input contains invalid characters".to_string()
            )),
            tvf.get_date(2)
        );
        assert_eq!(
            Err(TvfError::ConvertionError(
                "input contains invalid characters".to_string()
            )),
            tvf.get_datetime(2)
        );

        assert_eq!(Ok(42), tvf.get_unsigned(1));
        assert_eq!(
            Ok(Cow::Borrowed(&String::from("The great string"))),
            tvf.get_string(2)
        );
        assert_eq!(Ok(-1), tvf.get_signed(3));
        assert_eq!(Err(TvfError::FieldNotFound(4)), tvf.get_float(4));
        assert_eq!(Ok(6.56), tvf.get_float(5));
        assert_eq!(Ok(32), tvf.get_byte(6));
        assert_eq!(
            Ok(Cow::Owned(Bytes::from(hex::decode("aabb77ff").unwrap()))),
            tvf.get_bytes(7)
        );
        assert_eq!(
            Ok(NaiveDate::parse_from_str("2023-06-05", SIMPLE_DATE_FMT).unwrap()),
            tvf.get_date(8)
        );
        assert_eq!(
            Ok(NaiveDateTime::parse_from_str("2023-06-05T15:02:00", SIMPLE_DATETIME_FMT).unwrap()),
            tvf.get_datetime(9)
        );
        assert_eq!(Ok(Cow::Owned(sub_buffer)), tvf.get_buffer(10));

        assert_eq!(Err(TvfError::FieldNotFound(100)), tvf.get_unsigned(100));
        assert_eq!(Err(TvfError::FieldNotFound(110)), tvf.get_signed(110));
        assert_eq!(Err(TvfError::FieldNotFound(120)), tvf.get_float(120));
        assert_eq!(Err(TvfError::FieldNotFound(130)), tvf.get_string(130));
        assert_eq!(Err(TvfError::FieldNotFound(140)), tvf.get_byte(140));
        assert_eq!(Err(TvfError::FieldNotFound(150)), tvf.get_bytes(150));
        assert_eq!(Err(TvfError::FieldNotFound(160)), tvf.get_date(160));
        assert_eq!(Err(TvfError::FieldNotFound(170)), tvf.get_datetime(170));
        assert_eq!(Err(TvfError::FieldNotFound(180)), tvf.get_buffer(180));
    }

    #[test]
    fn test_simple_tvf() {
        let mut simple_tvf: SimpleStringTvf = Default::default();
        test_tvf(&mut simple_tvf);
        assert!(!format!("{simple_tvf:?}").is_empty());
        let keys = simple_tvf.keys();
        let into_keys = simple_tvf.clone().into_keys();
        assert_eq!(keys, into_keys);
        assert_eq!(9, keys.len());
        let serial = simple_tvf.serialize();
        let unserial = SimpleStringTvf::deserialize(&serial).unwrap();
        assert_eq!(simple_tvf, unserial);

        assert_eq!(
            Err(TvfError::SerializationError(
                "invalid digit found in string".into()
            )),
            SimpleStringTvf::deserialize("jean;luc")
        );
        assert_eq!(
            Err(TvfError::SerializationError("No len after key".into())),
            SimpleStringTvf::deserialize("1;")
        );
        assert_eq!(
            Err(TvfError::SerializationError(
                "invalid digit found in string".into()
            )),
            SimpleStringTvf::deserialize("1;jean;")
        );
        assert_eq!(
            Err(TvfError::SerializationError(
                "Bad field termination char".into()
            )),
            SimpleStringTvf::deserialize("1;2;to,")
        );
    }

    enum TvfTestFilter {}

    impl TvfFilter for TvfTestFilter {
        fn filter<T: Tvf>(mut buf: T) -> T {
            buf = <TvfTestFilter as TvfFilter>::mask_tvf_str_field(buf, 1, "0");
            buf
        }
    }

    #[test]
    fn test_tvf_filter() {
        let mut simple_tvf: SimpleStringTvf = Default::default();
        simple_tvf.put_string(1, "1234");
        simple_tvf.put_string(2, "1234");
        assert_eq!(2, simple_tvf.len());

        simple_tvf = TvfTestFilter::filter(simple_tvf);
        assert_eq!(2, simple_tvf.len());
        assert_eq!("0000", simple_tvf.get_string(1).unwrap().as_str());
        assert_eq!("1234", simple_tvf.get_string(2).unwrap().as_str());
    }
}
