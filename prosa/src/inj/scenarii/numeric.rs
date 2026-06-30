use super::{
    ParseError, Rule, RuleParse,
    util::{parse_range, parse_range_with_step},
};
use prosa_utils::msg::tvf::Tvf;
use rand::{
    RngExt,
    distr::uniform::{SampleRange, SampleUniform},
};
use std::ops::{AddAssign, Range};

// MARK: traits

/// Generate a number
pub trait MakeNumber<N> {
    /// Generate a number
    fn make_number(&mut self) -> N;
}

// MARK: random number

/// Generate random integer values in the specified range
#[derive(Debug, Clone, Copy)]
pub struct RuleRandom<N> {
    /// Minimal bound for random numbers
    min: N,

    /// Maximal bound for random numbers
    max: N,
}

macro_rules! impl_default {
    ( $num:ty ) => {
        impl Default for RuleRandom<$num> {
            #[inline]
            fn default() -> Self {
                Self {
                    min: <$num>::MIN,
                    max: <$num>::MAX,
                }
            }
        }
    };
}
impl_default!(u8);
impl_default!(u16);
impl_default!(u32);
impl_default!(u64);
impl_default!(usize);
impl_default!(i8);
impl_default!(i16);
impl_default!(i32);
impl_default!(i64);
impl_default!(isize);
impl_default!(f32);
impl_default!(f64);

impl<N> RuleRandom<N>
where
    N: Copy + SampleUniform,
    Range<N>: SampleRange<N>,
{
    /// Generate a random number in the specified range
    #[inline]
    pub fn random(&self) -> N {
        let mut rng = rand::rng();
        rng.random_range(self.min..self.max)
    }
}

impl<N> MakeNumber<N> for RuleRandom<N>
where
    N: Copy + SampleUniform,
    Range<N>: SampleRange<N>,
{
    /// Generate a random number in the specified range
    #[inline]
    fn make_number(&mut self) -> N {
        self.random()
    }
}

/// Quickly implement `Rule`
macro_rules! impl_rule_random {
    ( $num:ty ; $put:ident ) => {
        impl<T: Tvf> Rule<T> for RuleRandom<$num> {
            #[inline]
            fn insert(&mut self, tag: usize, buffer: &mut T) {
                buffer.$put(tag, self.random());
            }
        }
    };
}
impl_rule_random![ u8  ; put_byte     ];
impl_rule_random![ u64 ; put_unsigned ];
impl_rule_random![ i64 ; put_signed   ];
impl_rule_random![ f64 ; put_float    ];

/// Quickly implement `RuleParse`
macro_rules! impl_parse_random {
    ( $num:ty ; $label:literal ) => {
        impl RuleParse for RuleRandom<$num> {
            const LABEL: &'static str = $label;

            fn parse(expr: &str) -> Result<Self, ParseError> {
                let [vmin, vmax] = parse_range(expr)?;
                Ok(Self {
                    min: vmin.unwrap_or(<$num>::MIN),
                    max: vmax.unwrap_or(<$num>::MAX),
                })
            }
        }
    };
}
impl_parse_random![ u8  ; "random_byte"     ];
impl_parse_random![ u64 ; "random_unsigned" ];
impl_parse_random![ i64 ; "random_signed"   ];
impl_parse_random![ f64 ; "random_float"    ];

// MARK: round robin

/// Generate random integer values in the specified range
#[derive(Debug, Clone, Copy)]
pub struct RuleRoundRobin<N> {
    /// Current value
    current: N,

    /// Minimal bound for random numbers
    min: N,

    /// Maximal bound for random numbers
    max: N,

    /// Iteration step (default = 1)
    step: N,
}

impl<N> RuleRoundRobin<N>
where
    N: Copy + AddAssign + PartialOrd,
{
    /// Generate a random number in the specified range
    pub fn next(&mut self) -> N {
        let value = self.current;
        self.current += self.step;
        if self.current >= self.max {
            self.current = self.min;
        }
        value
    }
}

impl<N> MakeNumber<N> for RuleRoundRobin<N>
where
    N: Copy + AddAssign + PartialOrd,
{
    /// Generate a random number in the specified range
    fn make_number(&mut self) -> N {
        self.next()
    }
}

/// Quickly implement `Rule`
macro_rules! impl_rule_round_robin {
    ( $num:ty ; $put:ident ) => {
        impl<T: Tvf> Rule<T> for RuleRoundRobin<$num> {
            #[inline]
            fn insert(&mut self, tag: usize, buffer: &mut T) {
                buffer.$put(tag, self.next());
            }
        }
    };
}
impl_rule_round_robin![ u8  ; put_byte     ];
impl_rule_round_robin![ u64 ; put_unsigned ];
impl_rule_round_robin![ i64 ; put_signed   ];
impl_rule_round_robin![ f64 ; put_float    ];

/// Quickly implement `RuleParse`
macro_rules! impl_parse_round_robin {
    ( $num:ty ; $label:literal ) => {
        impl RuleParse for RuleRoundRobin<$num> {
            const LABEL: &'static str = $label;

            fn parse(expr: &str) -> Result<Self, ParseError> {
                let [vmin, vmax, vstep] = parse_range_with_step(expr)?;
                let min = vmin.unwrap_or(<$num>::MIN);
                let max = vmax.unwrap_or(<$num>::MAX);
                let step = vstep.unwrap_or(1 as $num);
                Ok(Self {
                    current: min,
                    min,
                    max,
                    step,
                })
            }
        }
    };
}
impl_parse_round_robin![ u8  ; "round_robin_byte"     ];
impl_parse_round_robin![ u64 ; "round_robin_unsigned" ];
impl_parse_round_robin![ i64 ; "round_robin_signed"   ];
impl_parse_round_robin![ f64 ; "round_robin_float"    ];
