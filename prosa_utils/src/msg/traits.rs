//! Define traits to convert a Rust type into a TVF representation and back

use bytes::Bytes;
use chrono::{NaiveDate, NaiveDateTime};

use crate::msg::tvf::{Tvf, TvfError};

/// Convert a Rust's type into a TVF message
pub trait ToTvf<T: Tvf> {
    /// Populate the given TVF message with this Rust's type
    fn to_tvf(&self, msg: &mut T);
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
    ($type:ty ; $put:ident "ref") => {
        impl<T: Tvf> ToField<T> for $type {
            #[inline]
            fn to_field(&self, id: usize, msg: &mut T) {
                msg.$put(id, self);
            }
        }
    };
    ($type:ty ; $put:ident "clone") => {
        impl<T: Tvf> ToField<T> for $type {
            #[inline]
            fn to_field(&self, id: usize, msg: &mut T) {
                msg.$put(id, self.clone());
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
impl_to_field![ NaiveDate     ; put_date           ];
impl_to_field![ NaiveDateTime ; put_datetime       ];
impl_to_field![ str           ; put_string "ref"   ];
impl_to_field![ String        ; put_string "ref"   ];
impl_to_field![ Bytes         ; put_bytes  "clone" ];

impl<T: Tvf + Clone> ToField<T> for T {
    #[inline]
    fn to_field(&self, id: usize, msg: &mut T) {
        msg.put_buffer(id, self.clone());
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

impl<T: Tvf + Clone> FromField<T> for T {
    #[inline]
    fn from_field(msg: &T, id: usize) -> Result<Self, TvfError> {
        Ok(msg.get_buffer(id)?.into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::{FromTvf, ToTvf, Tvf, TvfError};
    use crate::msg::simple_string_tvf::SimpleStringTvf;
    use prosa_macros::tvf;

    #[derive(Debug, PartialEq)]
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

        // deserialize from TVF
        let a3 = A::from_tvf(&a0).expect("Sample struct deserialization should work");
        assert_eq!(a1, a3);
    }
}
