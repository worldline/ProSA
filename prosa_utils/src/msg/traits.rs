//! Define traits to convert a Rust type into a TVF representation and back

use bytes::Bytes;
use chrono::{NaiveDate, NaiveDateTime};

use crate::msg::tvf::{Tvf, TvfError};

/// Convert a Rust's type into a TVF message
pub trait ToTvf<T: Tvf> {
    /// Populate the given TVF message with this Rust's type
    fn to_tvf(&self, msg: &mut T);
}

/// Convert a Rust's type into a TVF message by consuming the value
pub trait IntoTvf<T: Tvf> {
    /// Populate the given TVF message with this Rust's type
    fn into_tvf(self, msg: &mut T);
}

/// Convert a TVF message into a Rust's type
pub trait FromTvf<T: Tvf>: Sized {
    /// Construct a Rust's type from this TVF message
    fn from_tvf(msg: &T) -> Result<Self, TvfError>;
}

/// Companion trait to `ToTvf` to handle fields
pub trait ToField<T: Tvf> {
    /// Insert a field in this TVF message
    fn to_field(&self, id: usize, msg: &mut T);
}

/// Companion trait to `ToTvf` to handle fields
pub trait IntoField<T: Tvf> {
    /// Insert a field in this TVF message
    fn into_field(self, id: usize, msg: &mut T);
}

/// Companion trait to `FromTvf` to handle fields
pub trait FromField<T: Tvf>: Sized {
    /// Read a field from this TVF message
    fn from_field(msg: &T, id: usize) -> Result<Self, TvfError>;
}

// MARK: ToField

macro_rules! impl_to_field {
    ($type:ty ; $put:ident) => {
        impl<T: Tvf> ToField<T> for $type {
            #[inline]
            fn to_field(&self, id: usize, msg: &mut T) {
                msg.$put(id, *self);
            }
        }
    };
    ($type:ty ; $put:ident as $target:ty) => {
        impl<T: Tvf> ToField<T> for $type {
            #[inline]
            fn to_field(&self, id: usize, msg: &mut T) {
                msg.$put(id, *self as $target);
            }
        }
    };
}
impl_to_field![ bool ; put_byte     as u8  ];
impl_to_field![ u8   ; put_byte     as u8  ];
impl_to_field![ u16  ; put_unsigned as u64 ];
impl_to_field![ u32  ; put_unsigned as u64 ];
impl_to_field![ u64  ; put_unsigned as u64 ];
impl_to_field![ i8   ; put_signed   as i64 ];
impl_to_field![ i16  ; put_signed   as i64 ];
impl_to_field![ i32  ; put_signed   as i64 ];
impl_to_field![ i64  ; put_signed   as i64 ];
impl_to_field![ f32  ; put_float    as f64 ];
impl_to_field![ f64  ; put_float    as f64 ];
impl_to_field![ NaiveDate     ; put_date     ];
impl_to_field![ NaiveDateTime ; put_datetime ];

impl<T: Tvf> ToField<T> for str {
    #[inline]
    fn to_field(&self, id: usize, msg: &mut T){
        msg.put_string(id, self);
    }
}

impl<T: Tvf> ToField<T> for String {
    #[inline]
    fn to_field(&self, id: usize, msg: &mut T){
        msg.put_string(id, self);
    }
}

impl<T: Tvf, const S: usize> ToField<T> for [u8; S] {
    #[inline]
    fn to_field(&self, id: usize, msg: &mut T){
        msg.put_bytes(id, Bytes::copy_from_slice(self));
    }
}

impl<T: Tvf> ToField<T> for [u8] {
    #[inline]
    fn to_field(&self, id: usize, msg: &mut T){
        msg.put_bytes(id, Bytes::copy_from_slice(self));
    }
}

impl<T: Tvf> ToField<T> for Bytes {
    #[inline]
    fn to_field(&self, id: usize, msg: &mut T){
        msg.put_bytes(id, self.clone());
    }
}

/// Allow inserting a TVF sub-messages into a main message
impl<T: Tvf + Clone> ToField<T> for T {
    #[inline]
    fn to_field(&self, id: usize, msg: &mut T) {
        msg.put_buffer(id, self.clone());
    }
}

/// Insert the field into the buffer if its value is set
impl<T: Tvf, F: ToField<T>> ToField<T> for Option<F> {
    #[inline]
    fn to_field(&self, id: usize, msg: &mut T){
        if let Some(field) = self {
            field.to_field(id, msg);
        }
    }
}

// MARK: IntoField

macro_rules! impl_into_field {
    ($type:ty ; $put:ident) => {
        impl<T: Tvf> IntoField<T> for $type {
            #[inline]
            fn into_field(self, id: usize, msg: &mut T) {
                msg.$put(id, self);
            }
        }
    };
    ($type:ty ; $put:ident as $target:ty) => {
        impl<T: Tvf> IntoField<T> for $type {
            #[inline]
            fn into_field(self, id: usize, msg: &mut T) {
                msg.$put(id, self as $target);
            }
        }
    };
}
impl_into_field![ bool ; put_byte     as u8  ];
impl_into_field![ u8   ; put_byte     as u8  ];
impl_into_field![ u16  ; put_unsigned as u64 ];
impl_into_field![ u32  ; put_unsigned as u64 ];
impl_into_field![ u64  ; put_unsigned as u64 ];
impl_into_field![ i8   ; put_signed   as i64 ];
impl_into_field![ i16  ; put_signed   as i64 ];
impl_into_field![ i32  ; put_signed   as i64 ];
impl_into_field![ i64  ; put_signed   as i64 ];
impl_into_field![ f32  ; put_float    as f64 ];
impl_into_field![ f64  ; put_float    as f64 ];
impl_into_field![ NaiveDate     ; put_date     ];
impl_into_field![ NaiveDateTime ; put_datetime ];

impl<T: Tvf> IntoField<T> for &str {
    #[inline]
    fn into_field(self, id: usize, msg: &mut T){
        msg.put_string(id, self);
    }
}

impl<T: Tvf> IntoField<T> for String {
    #[inline]
    fn into_field(self, id: usize, msg: &mut T){
        msg.put_string(id, self);
    }
}

impl<T: Tvf, const S: usize> IntoField<T> for [u8; S] {
    #[inline]
    fn into_field(self, id: usize, msg: &mut T){
        msg.put_bytes(id, Bytes::from_owner(self));
    }
}

impl<T: Tvf> IntoField<T> for &[u8] {
    #[inline]
    fn into_field(self, id: usize, msg: &mut T){
        msg.put_bytes(id, Bytes::copy_from_slice(self));
    }
}

impl<T: Tvf> IntoField<T> for Bytes {
    #[inline]
    fn into_field(self, id: usize, msg: &mut T){
        msg.put_bytes(id, self);
    }
}

/// Allow inserting a TVF sub-messages into a main message
impl<T: Tvf> IntoField<T> for T {
    #[inline]
    fn into_field(self, id: usize, msg: &mut T) {
        msg.put_buffer(id, self);
    }
}

/// Insert the field into the buffer if its value is set
impl<T: Tvf, F: IntoField<T>> IntoField<T> for Option<F> {
    #[inline]
    fn into_field(self, id: usize, msg: &mut T){
        if let Some(field) = self {
            field.into_field(id, msg);
        }
    }
}


// MARK: FromField

impl<T: Tvf> FromField<T> for bool {
    #[inline]
    fn from_field(msg: &T, id: usize) -> Result<Self, TvfError> {
        Ok(msg.get_byte(id)? != 0)
    }
}

impl<T: Tvf> FromField<T> for f32 {
    #[inline]
    fn from_field(msg: &T, id: usize) -> Result<Self, TvfError> {
        Ok(msg.get_float(id)? as f32)
    }
}

macro_rules! impl_from_field {
    ($type:ty ; $get:ident) => {
        impl<T: Tvf> FromField<T> for $type {
            #[inline]
            fn from_field(msg: &T, id: usize) -> Result<Self, TvfError> {
                msg.$get(id)
            }
        }
    };
    ($type:ty ; $get:ident "try_from") => {
        impl<T: Tvf> FromField<T> for $type {
            #[inline]
            fn from_field(msg: &T, id: usize) -> Result<Self, TvfError> {
                let num = msg.$get(id)?;
                Self::try_from(num).map_err(|err| TvfError::ConvertionError(err.to_string()))
            }
        }
    };
    ($type:ty ; $get:ident "owned") => {
        impl<T: Tvf> FromField<T> for $type {
            #[inline]
            fn from_field(msg: &T, id: usize) -> Result<Self, TvfError> {
                Ok(msg.$get(id)?.into_owned())
            }
        }
    };
}
impl_from_field![ u8   ; get_byte                ];
impl_from_field![ u16  ; get_unsigned "try_from" ];
impl_from_field![ u32  ; get_unsigned "try_from" ];
impl_from_field![ u64  ; get_unsigned            ];
impl_from_field![ i8   ; get_signed   "try_from" ];
impl_from_field![ i16  ; get_signed   "try_from" ];
impl_from_field![ i32  ; get_signed   "try_from" ];
impl_from_field![ i64  ; get_signed              ];
impl_from_field![ f64  ; get_float               ];
impl_from_field![ NaiveDate     ; get_date             ];
impl_from_field![ NaiveDateTime ; get_datetime         ];
impl_from_field![ String        ; get_string   "owned" ];
impl_from_field![ Bytes         ; get_bytes    "owned" ];

/// Allow extracting a TVF sub-messages from a main message
impl<T: Tvf + Clone> FromField<T> for T {
    #[inline]
    fn from_field(msg: &T, id: usize) -> Result<Self, TvfError> {
        Ok(msg.get_buffer(id)?.into_owned())
    }
}

/// Extract a value from a buffer if the field exists
impl<T: Tvf, F: FromField<T>> FromField<T> for Option<F> {
    #[inline]
    fn from_field(msg: &T, id: usize) -> Result<Self, TvfError> {
        if let Ok(field) = F::from_field(msg, id) {
            Ok(Some(field))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::msg::simple_string_tvf::SimpleStringTvf;
    use prosa_macros::tvf;

    #[derive(Debug, Clone, PartialEq)]
    struct A {
        a: u64,
        b: f64,
        c: String,
    }

    impl<T: Tvf> ToTvf<T> for A {
        fn to_tvf(&self, msg: &mut T) {
            msg.put_unsigned(1, self.a);
            msg.put_float(2, self.b);
            msg.put_string(3, self.c.clone());
        }
    }

    impl<T: Tvf> IntoTvf<T> for A {
        fn into_tvf(self, msg: &mut T) {
            msg.put_unsigned(1, self.a);
            msg.put_float(2, self.b);
            msg.put_string(3, self.c);
        }
    }
    
    impl<T: Tvf> FromTvf<T> for A {
        fn from_tvf(msg: &T) -> Result<Self, TvfError> {
            let a = msg.get_unsigned(1)?;
            let b = msg.get_float(2)?;
            let c = msg.get_string(3)?.to_string();

            Ok(Self { a, b, c })
        }
    }

    #[test]
    fn test_to_tvf() {
        use crate as prosa_utils;

        let a0 = tvf![SimpleStringTvf {
            1 => 101u64,
            2 => 0.25,
            3 => "word",
        }];
        let a1 = A {
            a: 101,
            b: 0.25,
            c: "word".to_string(),
        };

        // serialize to TVF
        let mut a2 = SimpleStringTvf::default();
        a1.to_tvf(&mut a2);
        assert_eq!(a0, a2);

        // serialize to TVF by consuming the value
        let mut a4 = SimpleStringTvf::default();
        a1.clone().into_tvf(&mut a4);
        assert_eq!(a0, a4);

        // deserialize from TVF
        let a3 = A::from_tvf(&a0).expect("Sample struct deserialization should work");
        assert_eq!(a1, a3);
    }
}
