//! Definition of tracing object use for configuration

use serde::de;
use serde::de::Unexpected;
use serde::de::Visitor;
use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;
use std::collections::HashMap;
use std::fmt;
use tracing_core::Event;
use tracing_core::{subscriber::Interest, Metadata};
use tracing_subscriber::filter;
use tracing_subscriber::layer;

use super::ConfigError;

/// Enum to define all metrics level
#[derive(Default, Debug, Serialize, Copy, Clone, PartialEq)]
#[repr(u8)]
pub enum TelemetryLevel {
    /// No level define
    OFF = 0,
    /// Error level
    ERROR = 1,
    /// Warn level
    WARN = 2,
    /// Info level
    INFO = 3,
    /// Debug level
    DEBUG = 4,
    /// Trace level
    #[default]
    TRACE = 5,
}

impl From<TelemetryLevel> for filter::LevelFilter {
    fn from(val: TelemetryLevel) -> Self {
        match val {
            TelemetryLevel::OFF => filter::LevelFilter::OFF,
            TelemetryLevel::ERROR => filter::LevelFilter::ERROR,
            TelemetryLevel::WARN => filter::LevelFilter::WARN,
            TelemetryLevel::INFO => filter::LevelFilter::INFO,
            TelemetryLevel::DEBUG => filter::LevelFilter::DEBUG,
            TelemetryLevel::TRACE => filter::LevelFilter::TRACE,
        }
    }
}

impl From<TelemetryLevel> for &str {
    fn from(val: TelemetryLevel) -> Self {
        match val {
            TelemetryLevel::OFF => "off",
            TelemetryLevel::ERROR => "error",
            TelemetryLevel::WARN => "warn",
            TelemetryLevel::INFO => "info",
            TelemetryLevel::DEBUG => "debug",
            TelemetryLevel::TRACE => "trace",
        }
    }
}

impl TryFrom<String> for TelemetryLevel {
    type Error = ConfigError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.to_lowercase().as_str() {
            "off" => Ok(TelemetryLevel::OFF),
            "error" => Ok(TelemetryLevel::ERROR),
            "warn" => Ok(TelemetryLevel::WARN),
            "info" => Ok(TelemetryLevel::INFO),
            "debug" => Ok(TelemetryLevel::DEBUG),
            "trace" => Ok(TelemetryLevel::TRACE),
            _ => Err(ConfigError::WrongValue("TelemetryLevel".into(), value)),
        }
    }
}

impl<'de> Visitor<'de> for TelemetryLevel {
    type Value = TelemetryLevel;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str(
            "Telemetry Level from values: off[0], error[1], warn[2], info[3], debug[4], trace[5]",
        )
    }

    fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        match s.to_lowercase().as_str() {
            "off" => Ok(TelemetryLevel::OFF),
            "error" => Ok(TelemetryLevel::ERROR),
            "warn" => Ok(TelemetryLevel::WARN),
            "info" => Ok(TelemetryLevel::INFO),
            "debug" => Ok(TelemetryLevel::DEBUG),
            "trace" => Ok(TelemetryLevel::TRACE),
            _ => Err(de::Error::invalid_value(Unexpected::Str(s), &self)),
        }
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        match value {
            0 => Ok(TelemetryLevel::OFF),
            1 => Ok(TelemetryLevel::ERROR),
            2 => Ok(TelemetryLevel::WARN),
            3 => Ok(TelemetryLevel::INFO),
            4 => Ok(TelemetryLevel::DEBUG),
            5 => Ok(TelemetryLevel::TRACE),
            _ => Err(de::Error::invalid_value(Unexpected::Signed(value), &self)),
        }
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.visit_i64(value as i64)
    }
}

impl<'de> Deserialize<'de> for TelemetryLevel {
    fn deserialize<D>(deserializer: D) -> Result<TelemetryLevel, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(TelemetryLevel::default())
    }
}

/// Structure to define ProSA telemetry filter
///
/// ```
/// use prosa_utils::config::observability::Observability;
/// use prosa_utils::config::tracing::TelemetryFilter;
/// use prosa_utils::config::tracing;
/// use tracing_subscriber::filter;
///
/// // Create telemetry filter with a DEBUG level
/// let mut telemetry_filter = TelemetryFilter::new(filter::LevelFilter::DEBUG);
///
/// // Specific processor log level shouldn't be greater than the global telemetry filter level
/// telemetry_filter.add_proc_filter(String::from("prosa_test_proc"), filter::LevelFilter::INFO);
///
/// let otel_settings = Observability::default();
/// otel_settings.tracing_init(&telemetry_filter);
/// ```
#[derive(Debug, Clone)]
pub struct TelemetryFilter {
    proc_levels: HashMap<String, filter::LevelFilter>,
    pub(crate) level: filter::LevelFilter,
}

impl TelemetryFilter {
    /// Method to create a new telemetry filter
    pub fn new(level: filter::LevelFilter) -> TelemetryFilter {
        TelemetryFilter {
            proc_levels: HashMap::new(),
            level,
        }
    }

    /// Method to clone the telemetry filter and change its default level if it's less verbose
    pub fn clone_with_level(&self, level: TelemetryLevel) -> TelemetryFilter {
        let mut filter = self.clone();
        let level: filter::LevelFilter = level.into();
        if level < filter.level {
            filter.level = level;
        }

        filter
    }

    /// Method to add a filter on a specific processor
    pub fn add_proc_filter(&mut self, proc_name: String, level: filter::LevelFilter) {
        self.proc_levels.insert(proc_name, level);
    }

    fn is_enabled(&self, metadata: &Metadata<'_>) -> bool {
        let level = if let Some(value) = self.proc_levels.get(metadata.name()) {
            value
        } else if let Some(value) = self.proc_levels.get(metadata.target()) {
            value
        } else {
            &self.level
        };

        metadata.level() <= level
    }
}

impl Default for TelemetryFilter {
    fn default() -> TelemetryFilter {
        TelemetryFilter {
            proc_levels: HashMap::new(),
            level: filter::LevelFilter::TRACE,
        }
    }
}

impl<S> layer::Filter<S> for TelemetryFilter {
    fn enabled(&self, metadata: &Metadata<'_>, _: &layer::Context<'_, S>) -> bool {
        self.is_enabled(metadata)
    }

    fn callsite_enabled(&self, metadata: &'static Metadata<'static>) -> Interest {
        if self.is_enabled(metadata) {
            Interest::always()
        } else {
            Interest::never()
        }
    }

    fn event_enabled(&self, event: &Event<'_>, _: &layer::Context<'_, S>) -> bool {
        self.is_enabled(event.metadata())
    }

    fn max_level_hint(&self) -> Option<filter::LevelFilter> {
        Some(self.level)
    }
}
