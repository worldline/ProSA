//! Define traits to convert a Rust type into a TVF representation and back

use crate::msg::tvf::{Tvf, TvfError};
use bytes::Bytes;
use chrono::{NaiveDate, NaiveDateTime};
use num_traits::AsPrimitive;
use std::{
    borrow::Cow,
    collections::{BTreeMap, HashMap},
    hash::Hash,
};

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
    fn to_field(&self, id: usize, msg: &mut T) {
        msg.put_string(id, self);
    }
}

impl<T: Tvf> ToField<T> for String {
    #[inline]
    fn to_field(&self, id: usize, msg: &mut T) {
        msg.put_string(id, self);
    }
}

impl<T: Tvf> ToField<T> for Bytes {
    #[inline]
    fn to_field(&self, id: usize, msg: &mut T) {
        msg.put_bytes(id, self.clone());
    }
}

/*
/// Allow inserting a TVF sub-messages into a main message
impl<T: Tvf + Clone> ToField<T> for T {
    #[inline]
    fn to_field(&self, id: usize, msg: &mut T) {
        msg.put_buffer(id, self.clone());
    }
}
*/

/// Insert the field into the buffer if its value is set
impl<T: Tvf, F: ToField<T>> ToField<T> for Option<F> {
    #[inline]
    fn to_field(&self, id: usize, msg: &mut T) {
        if let Some(field) = self {
            field.to_field(id, msg);
        }
    }
}

impl<T: Tvf, F: ToField<T>> ToField<T> for Box<F> {
    #[inline]
    fn to_field(&self, id: usize, msg: &mut T) {
        self.as_ref().to_field(id, msg);
    }
}

impl<'f, T: Tvf, F: ToField<T> + Clone> ToField<T> for Cow<'f, F> {
    #[inline]
    fn to_field(&self, id: usize, msg: &mut T) {
        self.as_ref().to_field(id, msg);
    }
}

/// Implement the ToField traits if the type implement ToTvf
macro_rules! impl_to_field_tvf {
    ( impl[$($generics:tt)*] ToField<$tvf:ident> for $type:ty where $($where:tt)* ) => {
        impl< $($generics)* > ToField<$tvf> for $type where $($where)* {
            #[inline]
            fn to_field(&self, id: usize, msg: &mut T) {
                let mut buffer = T::default();
                self.to_tvf(&mut buffer);
                msg.put_buffer(id, buffer);
            }
        }
    };
}

/// Convert a vector of fields into a TVF message
impl<T: Tvf, F: ToField<T>> ToTvf<T> for Vec<F> {
    fn to_tvf(&self, msg: &mut T) {
        for (i, f) in self.iter().enumerate() {
            f.to_field(i + 1, msg);
        }
    }
}
impl_to_field_tvf! {
    impl[T, F] ToField<T> for Vec<F>
    where
        T: Tvf + Default,
        F: ToField<T>,
}

/// Convert a list of fields into a TVF message
impl<T: Tvf, F: ToField<T>, const S: usize> ToTvf<T> for [F; S] {
    fn to_tvf(&self, msg: &mut T) {
        for (i, f) in self.iter().enumerate() {
            f.to_field(i + 1, msg);
        }
    }
}
impl_to_field_tvf! {
    impl[T, F, const S: usize] ToField<T> for [F; S]
    where
        T: Tvf + Default,
        F: ToField<T>,
}

/// Convert a list of fields into a TVF message
impl<T: Tvf, F: ToField<T>> ToTvf<T> for [F] {
    fn to_tvf(&self, msg: &mut T) {
        for (i, f) in self.iter().enumerate() {
            f.to_field(i + 1, msg);
        }
    }
}
impl_to_field_tvf! {
    impl[T, F] ToField<T> for [F]
    where
        T: Tvf + Default,
        F: ToField<T>,
}

/// Convert a map of fields into a TVF message
impl<T, I, F> ToTvf<T> for HashMap<I, F>
where
    T: Tvf,
    I: AsPrimitive<usize> + Copy,
    F: ToField<T>,
{
    fn to_tvf(&self, msg: &mut T) {
        for (i, f) in self.iter() {
            f.to_field((*i).as_(), msg);
        }
    }
}
impl_to_field_tvf! {
    impl[T, I, F] ToField<T> for HashMap<I, F>
    where
        T: Tvf + Default,
        I: AsPrimitive<usize> + Copy,
        F: ToField<T>,
}

/// Convert a map of fields into a TVF message
impl<T, I, F> ToTvf<T> for BTreeMap<I, F>
where
    T: Tvf,
    I: AsPrimitive<usize> + Copy,
    F: ToField<T>,
{
    fn to_tvf(&self, msg: &mut T) {
        for (i, f) in self.iter() {
            f.to_field((*i).as_(), msg);
        }
    }
}
impl_to_field_tvf! {
    impl[T, I, F] ToField<T> for BTreeMap<I, F>
    where
        T: Tvf + Default,
        I: AsPrimitive<usize> + Copy,
        F: ToField<T>,
}

/// Implement `ToTvf` trait for generic tuples
macro_rules! impl_to_tvf_tuple {
    ( $($index:tt => $F:ident),+ ) => {
        impl<T: Tvf, $($F),+> ToTvf<T> for ($($F),+)
            where $( $F: ToField<T> ),+
        {
            fn to_tvf(&self, msg: &mut T) {
                $(
                    self.$index.to_field($index + 1, msg);
                )+
            }
        }
    };
}
impl_to_tvf_tuple![ 0 => F0, 1 => F1 ];
impl_to_tvf_tuple![ 0 => F0, 1 => F1, 2 => F2 ];
impl_to_tvf_tuple![ 0 => F0, 1 => F1, 2 => F2, 3 => F3 ];
impl_to_tvf_tuple![ 0 => F0, 1 => F1, 2 => F2, 3 => F3, 4 => F4 ];
impl_to_tvf_tuple![ 0 => F0, 1 => F1, 2 => F2, 3 => F3, 4 => F4, 5 => F5 ];
impl_to_tvf_tuple![ 0 => F0, 1 => F1, 2 => F2, 3 => F3, 4 => F4, 5 => F5, 6 => F6 ];
impl_to_tvf_tuple![ 0 => F0, 1 => F1, 2 => F2, 3 => F3, 4 => F4, 5 => F5, 6 => F6, 7 => F7 ];
impl_to_tvf_tuple![ 0 => F0, 1 => F1, 2 => F2, 3 => F3, 4 => F4, 5 => F5, 6 => F6, 7 => F7, 8 => F8 ];
impl_to_tvf_tuple![ 0 => F0, 1 => F1, 2 => F2, 3 => F3, 4 => F4, 5 => F5, 6 => F6, 7 => F7, 8 => F8, 9 => F9 ];
impl_to_tvf_tuple![ 0 => F0, 1 => F1, 2 => F2, 3 => F3, 4 => F4, 5 => F5, 6 => F6, 7 => F7, 8 => F8, 9 => F9, 10 => F10 ];
impl_to_tvf_tuple![ 0 => F0, 1 => F1, 2 => F2, 3 => F3, 4 => F4, 5 => F5, 6 => F6, 7 => F7, 8 => F8, 9 => F9, 10 => F10, 11 => F11 ];
impl_to_tvf_tuple![ 0 => F0, 1 => F1, 2 => F2, 3 => F3, 4 => F4, 5 => F5, 6 => F6, 7 => F7, 8 => F8, 9 => F9, 10 => F10, 11 => F11, 12 => F12 ];

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
    fn into_field(self, id: usize, msg: &mut T) {
        msg.put_string(id, self);
    }
}

impl<T: Tvf> IntoField<T> for String {
    #[inline]
    fn into_field(self, id: usize, msg: &mut T) {
        msg.put_string(id, self);
    }
}

impl<T: Tvf> IntoField<T> for Bytes {
    #[inline]
    fn into_field(self, id: usize, msg: &mut T) {
        msg.put_bytes(id, self);
    }
}

/*
/// Allow inserting a TVF sub-messages into a main message
impl<T: Tvf> IntoField<T> for T {
    #[inline]
    fn into_field(self, id: usize, msg: &mut T) {
        msg.put_buffer(id, self);
    }
}
*/

/// Insert the field into the buffer if its value is set
impl<T: Tvf, F: IntoField<T>> IntoField<T> for Option<F> {
    #[inline]
    fn into_field(self, id: usize, msg: &mut T) {
        if let Some(field) = self {
            field.into_field(id, msg);
        }
    }
}

impl<T: Tvf, F: IntoField<T>> IntoField<T> for Box<F> {
    #[inline]
    fn into_field(self, id: usize, msg: &mut T) {
        let field: F = *self;
        field.into_field(id, msg);
    }
}

impl<'f, T: Tvf, F: IntoField<T> + Clone> IntoField<T> for Cow<'f, F> {
    #[inline]
    fn into_field(self, id: usize, msg: &mut T) {
        let field: F = self.into_owned();
        field.into_field(id, msg);
    }
}

/// Implement the IntoField traits if the type implement IntoTvf
macro_rules! impl_into_field_tvf {
    ( impl[$($generics:tt)*] IntoField<$tvf:ident> for $type:ty where $($where:tt)* ) => {
        impl< $($generics)* > IntoField<$tvf> for $type where $($where)* {
            #[inline]
            fn into_field(self, id: usize, msg: &mut T) {
                let mut buffer = T::default();
                self.into_tvf(&mut buffer);
                msg.put_buffer(id, buffer);
            }
        }
    };
}

/// Convert a vector of fields into a TVF message
impl<T: Tvf, F: IntoField<T>> IntoTvf<T> for Vec<F> {
    fn into_tvf(self, msg: &mut T) {
        for (i, f) in self.into_iter().enumerate() {
            f.into_field(i + 1, msg);
        }
    }
}
impl_into_field_tvf! {
    impl[T, F] IntoField<T> for Vec<F>
    where
        T: Tvf + Default,
        F: IntoField<T>,
}

/// Convert a list of fields into a TVF message
impl<T: Tvf, F: IntoField<T>, const S: usize> IntoTvf<T> for [F; S] {
    fn into_tvf(self, msg: &mut T) {
        for (i, f) in self.into_iter().enumerate() {
            f.into_field(i + 1, msg);
        }
    }
}
impl_into_field_tvf! {
    impl[T, F, const S: usize] IntoField<T> for [F; S]
    where
        T: Tvf + Default,
        F: IntoField<T>,
}

/// Convert a map of fields into a TVF message
impl<T, I, F> IntoTvf<T> for HashMap<I, F>
where
    T: Tvf,
    I: AsPrimitive<usize> + Copy,
    F: IntoField<T>,
{
    fn into_tvf(self, msg: &mut T) {
        for (i, f) in self {
            f.into_field(i.as_(), msg);
        }
    }
}
impl_into_field_tvf! {
    impl[T, I, F] IntoField<T> for HashMap<I, F>
    where
        T: Tvf + Default,
        I: AsPrimitive<usize> + Copy,
        F: IntoField<T>,
}

/// Convert a map of fields into a TVF message
impl<T, I, F> IntoTvf<T> for BTreeMap<I, F>
where
    T: Tvf,
    I: AsPrimitive<usize> + Copy,
    F: IntoField<T>,
{
    fn into_tvf(self, msg: &mut T) {
        for (i, f) in self {
            f.into_field(i.as_(), msg);
        }
    }
}
impl_into_field_tvf! {
    impl[T, I, F] IntoField<T> for BTreeMap<I, F>
    where
        T: Tvf + Default,
        I: AsPrimitive<usize> + Copy,
        F: IntoField<T>,
}

/// Implement `IntoTvf` trait for generic tuples
macro_rules! impl_into_tvf_tuple {
    ( $($index:tt => $F:ident),+ ) => {
        impl<T: Tvf, $($F),+> IntoTvf<T> for ($($F),+)
            where $( $F: IntoField<T> ),+
        {
            fn into_tvf(self, msg: &mut T) {
                $(
                    self.$index.into_field($index + 1, msg);
                )+
            }
        }
    };
}
impl_into_tvf_tuple![ 0 => F0, 1 => F1 ];
impl_into_tvf_tuple![ 0 => F0, 1 => F1, 2 => F2 ];
impl_into_tvf_tuple![ 0 => F0, 1 => F1, 2 => F2, 3 => F3 ];
impl_into_tvf_tuple![ 0 => F0, 1 => F1, 2 => F2, 3 => F3, 4 => F4 ];
impl_into_tvf_tuple![ 0 => F0, 1 => F1, 2 => F2, 3 => F3, 4 => F4, 5 => F5 ];
impl_into_tvf_tuple![ 0 => F0, 1 => F1, 2 => F2, 3 => F3, 4 => F4, 5 => F5, 6 => F6 ];
impl_into_tvf_tuple![ 0 => F0, 1 => F1, 2 => F2, 3 => F3, 4 => F4, 5 => F5, 6 => F6, 7 => F7 ];
impl_into_tvf_tuple![ 0 => F0, 1 => F1, 2 => F2, 3 => F3, 4 => F4, 5 => F5, 6 => F6, 7 => F7, 8 => F8 ];
impl_into_tvf_tuple![ 0 => F0, 1 => F1, 2 => F2, 3 => F3, 4 => F4, 5 => F5, 6 => F6, 7 => F7, 8 => F8, 9 => F9 ];
impl_into_tvf_tuple![ 0 => F0, 1 => F1, 2 => F2, 3 => F3, 4 => F4, 5 => F5, 6 => F6, 7 => F7, 8 => F8, 9 => F9, 10 => F10 ];
impl_into_tvf_tuple![ 0 => F0, 1 => F1, 2 => F2, 3 => F3, 4 => F4, 5 => F5, 6 => F6, 7 => F7, 8 => F8, 9 => F9, 10 => F10, 11 => F11 ];
impl_into_tvf_tuple![ 0 => F0, 1 => F1, 2 => F2, 3 => F3, 4 => F4, 5 => F5, 6 => F6, 7 => F7, 8 => F8, 9 => F9, 10 => F10, 11 => F11, 12 => F12 ];

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

/*
/// Allow extracting a TVF sub-messages from a main message
impl<T: Tvf + Clone> FromField<T> for T {
    #[inline]
    fn from_field(msg: &T, id: usize) -> Result<Self, TvfError> {
        Ok(msg.get_buffer(id)?.into_owned())
    }
}
*/

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

impl<T: Tvf, F: FromField<T>> FromField<T> for Box<F> {
    #[inline]
    fn from_field(msg: &T, id: usize) -> Result<Self, TvfError> {
        let field = F::from_field(msg, id)?;
        Ok(Box::new(field))
    }
}

impl<'f, T: Tvf, F: FromField<T> + Clone> FromField<T> for Cow<'f, F> {
    #[inline]
    fn from_field(msg: &T, id: usize) -> Result<Self, TvfError> {
        let field = F::from_field(msg, id)?;
        Ok(Cow::Owned(field))
    }
}

/// Implement the FromField traits if the type implement FromTvf
macro_rules! impl_from_field_tvf {
    ( impl[$($generics:tt)*] FromField<$tvf:ident> for $type:ty where $($where:tt)* ) => {
        impl< $($generics)* > FromField<$tvf> for $type where $($where)* {
            #[inline]
            fn from_field(msg: &T, id: usize) -> Result<Self, TvfError> {
                let buffer = msg.get_buffer(id)?;
                Self::from_tvf(&buffer)
            }
        }
    };
}

/// Extract a vector of fields from a TVF message
impl<T: Tvf, F: FromField<T>> FromTvf<T> for Vec<F> {
    fn from_tvf(msg: &T) -> Result<Self, TvfError> {
        // Collect fields into a vector
        let count = msg.len();
        let mut vector = Vec::with_capacity(count);
        for id in 1..=count {
            vector.push(F::from_field(msg, id)?);
        }
        Ok(vector)
    }
}
impl_from_field_tvf! {
    impl[T, F] FromField<T> for Vec<F>
    where
        T: Tvf + Clone,
        F: FromField<T>,
}

/// Extract a list of fields from a TVF message
impl<T: Tvf, F: FromField<T>, const S: usize> FromTvf<T> for [F; S] {
    fn from_tvf(msg: &T) -> Result<Self, TvfError> {
        // If the message contains less fields than the array we want to generate,
        // we can already tell that extracting the list of fields will fail.
        if msg.len() < S {
            Err(TvfError::ConvertionError(format!(
                "TVF message of length {} does not contains enough fields to populate an array for {} elements",
                msg.len(),
                S
            )))
        } else {
            // Collect fields into a vector
            let mut array = Vec::with_capacity(S);
            for id in 1..=S {
                array.push(F::from_field(msg, id)?);
            }
            // Convert the vector into a fixed size array
            array.try_into().map_err(|_| {
                TvfError::ConvertionError("Failed to convert vector into array".to_string())
            })
        }
    }
}
impl_from_field_tvf! {
    impl[T, F, const S: usize] FromField<T> for [F; S]
    where
        T: Tvf + Clone,
        F: FromField<T>,
}

/// Convert a map of fields into a TVF message
impl<T, I, F> FromTvf<T> for HashMap<I, F>
where
    T: Tvf,
    I: 'static + Copy + Hash + Eq,
    usize: AsPrimitive<I>,
    F: FromField<T>,
{
    fn from_tvf(msg: &T) -> Result<Self, TvfError> {
        let keys = msg.keys();
        let mut map = Self::with_capacity(keys.len());
        for key in keys {
            let field = F::from_field(msg, key)?;
            map.insert(key.as_(), field);
        }
        Ok(map)
    }
}
impl_from_field_tvf! {
    impl[T, I, F] FromField<T> for HashMap<I, F>
    where
        T: Tvf + Clone,
        I: 'static + Copy + Hash + Eq,
        usize: AsPrimitive<I>,
        F: FromField<T>,
}

/// Convert a map of fields into a TVF message
impl<T, I, F> FromTvf<T> for BTreeMap<I, F>
where
    T: Tvf,
    I: 'static + Copy + Ord,
    usize: AsPrimitive<I>,
    F: FromField<T>,
{
    fn from_tvf(msg: &T) -> Result<Self, TvfError> {
        let mut map = Self::new();
        for key in msg.keys() {
            let field = F::from_field(msg, key)?;
            map.insert(key.as_(), field);
        }
        Ok(map)
    }
}
impl_from_field_tvf! {
    impl[T, I, F] FromField<T> for BTreeMap<I, F>
    where
        T: Tvf + Clone,
        I: 'static + Copy + Ord,
        usize: AsPrimitive<I>,
        F: FromField<T>,
}

/// Implement `FromTvf` trait for generic tuples
macro_rules! impl_from_tvf_tuple {
    ( $($index:tt => $F:ident),+ ) => {
        impl<T: Tvf, $($F),+> FromTvf<T> for ($($F),+)
            where $( $F: FromField<T> ),+
        {
            fn from_tvf(msg: &T) -> Result<Self, TvfError> {
                Ok(($(
                    $F::from_field(msg, $index + 1)?
                ),+))
            }
        }
    };
}
impl_from_tvf_tuple![ 0 => F0, 1 => F1 ];
impl_from_tvf_tuple![ 0 => F0, 1 => F1, 2 => F2 ];
impl_from_tvf_tuple![ 0 => F0, 1 => F1, 2 => F2, 3 => F3 ];
impl_from_tvf_tuple![ 0 => F0, 1 => F1, 2 => F2, 3 => F3, 4 => F4 ];
impl_from_tvf_tuple![ 0 => F0, 1 => F1, 2 => F2, 3 => F3, 4 => F4, 5 => F5 ];
impl_from_tvf_tuple![ 0 => F0, 1 => F1, 2 => F2, 3 => F3, 4 => F4, 5 => F5, 6 => F6 ];
impl_from_tvf_tuple![ 0 => F0, 1 => F1, 2 => F2, 3 => F3, 4 => F4, 5 => F5, 6 => F6, 7 => F7 ];
impl_from_tvf_tuple![ 0 => F0, 1 => F1, 2 => F2, 3 => F3, 4 => F4, 5 => F5, 6 => F6, 7 => F7, 8 => F8 ];
impl_from_tvf_tuple![ 0 => F0, 1 => F1, 2 => F2, 3 => F3, 4 => F4, 5 => F5, 6 => F6, 7 => F7, 8 => F8, 9 => F9 ];
impl_from_tvf_tuple![ 0 => F0, 1 => F1, 2 => F2, 3 => F3, 4 => F4, 5 => F5, 6 => F6, 7 => F7, 8 => F8, 9 => F9, 10 => F10 ];
impl_from_tvf_tuple![ 0 => F0, 1 => F1, 2 => F2, 3 => F3, 4 => F4, 5 => F5, 6 => F6, 7 => F7, 8 => F8, 9 => F9, 10 => F10, 11 => F11 ];
impl_from_tvf_tuple![ 0 => F0, 1 => F1, 2 => F2, 3 => F3, 4 => F4, 5 => F5, 6 => F6, 7 => F7, 8 => F8, 9 => F9, 10 => F10, 11 => F11, 12 => F12 ];

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
