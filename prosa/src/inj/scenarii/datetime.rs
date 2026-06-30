use crate::inj::scenarii::{Rule, numeric::RuleRandom};
use prosa_utils::msg::{
    chrono::{Days, Duration, Local, NaiveDate, NaiveDateTime, Utc},
    tvf::Tvf,
};

// MARK: traits

/// Generate a date
pub trait MakeDate {
    /// Generate a date
    fn make_date(&self) -> NaiveDate;
}

/// Generate a date & time
pub trait MakeDateTime {
    /// Generate a date & time
    fn make_datetime(&self) -> NaiveDateTime;
}

// MARK: timezone

/// Define a timezone for evaluating date & time
#[repr(u8)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, strum::EnumString)]
pub enum TimeZone {
    /// UTC
    #[default]
    Utc,

    /// Local time
    Local,
}

impl TimeZone {
    /// Get today for given timezone
    pub fn today(self) -> NaiveDate {
        match self {
            Self::Utc => Utc::now().date_naive(),
            Self::Local => Local::now().date_naive(),
        }
    }

    /// Get now for given timezone
    pub fn now(self) -> NaiveDateTime {
        match self {
            Self::Utc => Utc::now().naive_utc(),
            Self::Local => Local::now().naive_local(),
        }
    }
}

// MARK: date today

/// Generate today's date
#[derive(Debug, Clone, Copy)]
pub struct RuleToday {
    // Timezone
    timezone: TimeZone,
}

impl RuleToday {
    /// Generate today's date
    #[inline]
    fn today(&self) -> NaiveDate {
        self.timezone.today()
    }
}

impl MakeDate for RuleToday {
    /// Generate today's date
    #[inline]
    fn make_date(&self) -> NaiveDate {
        self.today()
    }
}

impl<T: Tvf> Rule<T> for RuleToday {
    #[inline]
    fn insert(&mut self, tag: usize, buffer: &mut T) {
        buffer.put_date(tag, self.make_date());
    }
}

// MARK: date & time now

/// Generate now's date & time
#[derive(Debug, Clone, Copy)]
pub struct RuleNow {
    // Timezone
    timezone: TimeZone,
}

impl RuleNow {
    /// Generate now's date & time
    #[inline]
    fn now(&self) -> NaiveDateTime {
        self.timezone.now()
    }
}

impl MakeDateTime for RuleNow {
    /// Generate now's date & time
    #[inline]
    fn make_datetime(&self) -> NaiveDateTime {
        self.now()
    }
}

impl<T: Tvf> Rule<T> for RuleNow {
    #[inline]
    fn insert(&mut self, tag: usize, buffer: &mut T) {
        buffer.put_datetime(tag, self.make_datetime());
    }
}

// MARK: random date

/// Generate random date in the specified range of days starting from today
#[derive(Debug, Clone, Copy)]
pub struct RuleRandomDate {
    /// Range of days
    range: RuleRandom<u32>,

    // Timezone
    timezone: TimeZone,
}

impl RuleRandomDate {
    /// Generate a random number in the specified range
    #[inline]
    fn random(&self) -> NaiveDate {
        self.timezone.today() + Days::new(self.range.random() as u64)
    }
}

impl MakeDate for RuleRandomDate {
    /// Generate a random number in the specified range
    #[inline]
    fn make_date(&self) -> NaiveDate {
        self.random()
    }
}

impl<T: Tvf> Rule<T> for RuleRandomDate {
    #[inline]
    fn insert(&mut self, tag: usize, buffer: &mut T) {
        buffer.put_date(tag, self.make_date());
    }
}

// MARK: random datetime

/// Generate random integer values in the specified range of milliseconds starting from now
#[derive(Debug, Clone, Copy)]
pub struct RuleRandomDateTime {
    /// Range of milliseconds
    range: RuleRandom<u32>,

    /// Timezone
    timezone: TimeZone,
}

impl RuleRandomDateTime {
    /// Generate a random number in the specified range
    #[inline]
    fn random(&self) -> NaiveDateTime {
        self.timezone.now() + Duration::milliseconds(self.range.random() as i64)
    }
}

impl MakeDateTime for RuleRandomDateTime {
    /// Generate a random number in the specified range
    #[inline]
    fn make_datetime(&self) -> NaiveDateTime {
        self.random()
    }
}

impl<T: Tvf> Rule<T> for RuleRandomDateTime {
    #[inline]
    fn insert(&mut self, tag: usize, buffer: &mut T) {
        buffer.put_datetime(tag, self.make_datetime());
    }
}
