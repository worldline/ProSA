use super::{ParseError, ParseResult};
use prosa_utils::msg::chrono::{NaiveDate, NaiveDateTime};
use std::{iter, str::FromStr};

/// Parse a range in the form "min..max"
pub fn parse_range<N>(expr: &str) -> ParseResult<[Option<N>; 2]>
where
    N: FromStr,
    ParseError: From<<N as FromStr>::Err>,
{
    let mut values = expr.splitn(2, "..");
    if let Some(vmin) = values.next()
        && let Some(vmax) = values.next()
    {
        // read minimal bound
        let mut min = None;
        if !vmin.is_empty() {
            min = Some(vmin.parse::<N>()?);
        }

        // read maximal bound
        let mut max = None;
        if !vmax.is_empty() {
            max = Some(vmax.parse::<N>()?);
        }

        Ok([min, max])
    } else {
        Err(ParseError::InvalidValue(expr.to_string()))
    }
}

/// Parse a range with an optional step parameter in the form "min..max by step"
pub fn parse_range_with_step<N>(expr: &str) -> ParseResult<[Option<N>; 3]>
where
    N: FromStr,
    ParseError: From<<N as FromStr>::Err>,
{
    // The step value is optional
    let mut values = expr.splitn(2, "by");
    if let Some(range) = values.next() {
        let [min, max] = parse_range(range)?;

        // check if a step was provided or not
        let mut step = None;
        if let Some(vstep) = values.next() {
            step = Some(vstep.parse::<N>()?);
        }

        Ok([min, max, step])
    } else {
        Err(ParseError::InvalidValue(expr.to_string()))
    }
}

/// Parse a date
#[inline]
pub fn parse_date(expr: &str) -> ParseResult<NaiveDate> {
    Ok(NaiveDate::parse_from_str(expr, "%Y-%m-%d")?)
}

/// Parse a date & time
#[inline]
pub fn parse_datetime(expr: &str) -> ParseResult<NaiveDateTime> {
    Ok(NaiveDateTime::parse_from_str(expr, "%Y-%m-%d %H:%M:%S%.f")?)
}

/// Create a string of a fixed size with a given character
#[inline]
pub fn string_padding(c: char, count: usize) -> String {
    String::from_iter(iter::repeat_n(c, count))
}

/// Trait for helping convert numbers into string
pub trait Number {
    /// True if the number is negative
    fn is_neg(self) -> bool;

    /// Get the absolute value of the number
    fn abs(self) -> Self;
}

macro_rules! impl_number {
    ( "unsigned" $num:ty ) => {
        impl Number for $num {
            #[inline]
            fn is_neg(self) -> bool {
                false
            }

            #[inline]
            fn abs(self) -> Self {
                self
            }
        }
    };
    ( "signed" $num:ty ) => {
        impl Number for $num {
            #[inline]
            fn is_neg(self) -> bool {
                self.is_negative()
            }

            #[inline]
            fn abs(self) -> Self {
                self.abs()
            }
        }
    };
    ( "float" $num:ty ) => {
        impl Number for $num {
            #[inline]
            fn is_neg(self) -> bool {
                self.is_sign_negative()
            }

            #[inline]
            fn abs(self) -> Self {
                self.abs()
            }
        }
    };
}
impl_number![ "unsigned" u8    ];
impl_number![ "unsigned" u16   ];
impl_number![ "unsigned" u32   ];
impl_number![ "unsigned" u64   ];
impl_number![ "unsigned" usize ];
impl_number![ "signed"   i8    ];
impl_number![ "signed"   i16   ];
impl_number![ "signed"   i32   ];
impl_number![ "signed"   i64   ];
impl_number![ "signed"   isize ];
impl_number![ "float"    f32   ];
impl_number![ "float"    f64   ];
