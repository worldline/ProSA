use crate::inj::scenarii::{
    datetime::{MakeDate, MakeDateTime},
    numeric::MakeNumber,
    util::Number,
};
use std::marker::PhantomData;

// MARK: traits

/// Generate a string
pub trait MakeString {
    /// Generate a string
    fn make_string(&mut self) -> String;
}

// MARK: from number

/// Generate a string from a number
#[derive(Debug, Clone, Copy)]
pub struct RuleStringNumber<M, N>
where
    M: MakeNumber<N>,
{
    /// Generator for numeric values
    make: M,

    /// Format for converting numbers into strings
    format: Option<NumberFormat>,

    /// Bind the numeric type to this rule
    __maker: PhantomData<N>,
}

/// Simple formatting options for converting a number into a string
#[derive(Debug, Clone, Copy)]
pub struct NumberFormat {
    /// Size of the string to generate
    fixed_size: usize,

    /// Leading character to use ('0' or ' ')
    leading_char: char,
}

impl Default for NumberFormat {
    #[inline]
    fn default() -> Self {
        Self {
            fixed_size: 10,
            leading_char: '0',
        }
    }
}

impl<M, N> MakeString for RuleStringNumber<M, N>
where
    M: MakeNumber<N>,
    N: Number + Copy + ToString,
{
    fn make_string(&mut self) -> String {
        let num = self.make.make_number();
        let text = num.abs().to_string();

        if let Some(format) = self.format
            && text.len() < format.fixed_size
        {
            // We allocate a new buffer to add the leading spaces or zeroes
            let mut buffer = vec![format.leading_char; format.fixed_size];

            // where to start writing digits in the buffer
            let start = format.fixed_size - text.len();

            // copy the digits
            for (i, digit) in text.chars().enumerate() {
                buffer[start + i] = digit;
            }

            // add the minus sign if the value is negative
            if num.is_neg() {
                buffer[0] = '-';
            }

            // output a string
            buffer.iter().collect::<String>()
        } else {
            // no formatting, just output as is
            num.to_string()
        }
    }
}

// MARK: from date

/// Generate a string from a date
#[derive(Debug, Clone, Copy)]
pub struct RuleStringDate<M>
where
    M: MakeDate,
{
    /// Generator for date values
    make: M,
}

// MARK: from date & time

/// Generate a string from a date & time
#[derive(Debug, Clone, Copy)]
pub struct RuleStringDateTime<M>
where
    M: MakeDateTime,
{
    /// Generator for date & time values
    make: M,
}
