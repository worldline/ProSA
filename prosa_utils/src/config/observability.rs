//! Definition of Opentelemetry configuration

use opentelemetry::{
    logs::LogError, metrics::MetricsError, trace::TraceError, trace::TracerProvider as _,
};
use opentelemetry_otlp::{ExportConfig, Protocol, WithExportConfig};
use opentelemetry_sdk::{
    logs::LoggerProvider,
    metrics::{
        reader::{DefaultAggregationSelector, DefaultTemporalitySelector},
        MeterProvider, PeriodicReader,
    },
    runtime,
    trace::{Tracer, TracerProvider},
};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing_subscriber::{filter, prelude::*};
use tracing_subscriber::{layer::SubscriberExt, util::TryInitError};

use super::tracing::{TelemetryFilter, TelemetryLevel};

/// Configuration struct of an **O**pen **T**e**l**emetry **P**rotocol Exporter
#[derive(Debug, Deserialize, Serialize, Clone)]
pub(crate) struct OTLPExporterCfg {
    pub(crate) level: Option<TelemetryLevel>,
    #[serde(default = "OTLPExporterCfg::get_default_name")]
    name: Option<String>,
    endpoint: String,
    timeout_sec: u64,
    protocol: String,
}

impl OTLPExporterCfg {
    pub(crate) const DEFAULT_TRACER_NAME: &'static str = "prosa";

    fn get_default_name() -> Option<String> {
        Some(Self::DEFAULT_TRACER_NAME.into())
    }

    /// Method to get the name of the exporter
    pub(crate) fn get_name(&self) -> String {
        if let Some(name) = &self.name {
            name.clone()
        } else {
            Self::DEFAULT_TRACER_NAME.into()
        }
    }
}

impl From<OTLPExporterCfg> for ExportConfig {
    fn from(value: OTLPExporterCfg) -> Self {
        let mut protoc = Protocol::HttpBinary; // by default
        if value.protocol.to_lowercase() == "grpc" {
            protoc = Protocol::Grpc;
        }
        ExportConfig {
            endpoint: value.endpoint,
            timeout: Duration::from_secs(value.timeout_sec),
            protocol: protoc,
        }
    }
}

impl Default for OTLPExporterCfg {
    fn default() -> Self {
        Self {
            level: None,
            name: Some(OTLPExporterCfg::DEFAULT_TRACER_NAME.into()),
            endpoint: "http://localhost:4317".into(),
            timeout_sec: 10,
            protocol: "grpc".into(),
        }
    }
}

/// Configuration struct of an stdout exporter
#[derive(Default, Debug, Deserialize, Serialize, Copy, Clone)]
pub(crate) struct StdoutExporterCfg {
    #[serde(default)]
    pub(crate) level: Option<TelemetryLevel>,
}

/// Telemetry data define for metrics
#[derive(Default, Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "lowercase")]
pub struct TelemetryMetrics {
    otlp: Option<OTLPExporterCfg>,
    stdout: Option<StdoutExporterCfg>,
}

impl TelemetryMetrics {
    /// Build a meter provider based on the self configuration
    fn build_provider(&self) -> Result<MeterProvider, MetricsError> {
        let mut meter_provider = MeterProvider::builder();
        if let Some(s) = &self.otlp {
            let c = ExportConfig::from(s.clone());
            let agregator = Box::new(DefaultAggregationSelector::new());
            let temporality = Box::new(DefaultTemporalitySelector::new());
            let exporter = opentelemetry_otlp::new_exporter()
                .tonic()
                .with_export_config(c)
                .build_metrics_exporter(agregator, temporality)?;
            let reader =
                PeriodicReader::builder(exporter, opentelemetry_sdk::runtime::Tokio).build();
            meter_provider = meter_provider.with_reader(reader);
        }

        if self.stdout.is_some() {
            let exporter = opentelemetry_stdout::MetricsExporter::default();
            let reader = PeriodicReader::builder(exporter, runtime::Tokio).build();
            meter_provider = meter_provider.with_reader(reader);
        }

        Ok(meter_provider.build())
    }
}

/// Telemetry data define for metrics, logs, traces
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "lowercase")]
pub struct TelemetryData {
    otlp: Option<OTLPExporterCfg>,
    stdout: Option<StdoutExporterCfg>,
}

impl TelemetryData {
    /// Build a logger provider based on the self configuration
    fn build_logger_provider(&self) -> Result<LoggerProvider, LogError> {
        let mut logs_provider = LoggerProvider::builder();
        if let Some(s) = &self.otlp {
            let c = ExportConfig::from(s.clone());
            let exporter = opentelemetry_otlp::new_exporter()
                .tonic()
                .with_export_config(c)
                .build_log_exporter()?;
            logs_provider =
                logs_provider.with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio);
        }

        Ok(logs_provider.build())
    }

    /// Build a tracer provider based on the self configuration
    fn build_tracer_provider(&self) -> Result<TracerProvider, TraceError> {
        let mut trace_provider = TracerProvider::builder();
        if let Some(s) = &self.otlp {
            let c = ExportConfig::from(s.clone());
            let exporter = opentelemetry_otlp::new_exporter()
                .tonic()
                .with_export_config(c)
                .build_span_exporter()?;
            trace_provider =
                trace_provider.with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio);
        }

        Ok(trace_provider.build())
    }

    /// Build a tracer provider based on the self configuration
    fn build_tracer(&self) -> Result<Tracer, TraceError> {
        let mut trace_provider = TracerProvider::builder();
        if let Some(s) = &self.otlp {
            let c = ExportConfig::from(s.clone());
            let exporter = opentelemetry_otlp::new_exporter()
                .tonic()
                .with_export_config(c)
                .build_span_exporter()?;
            trace_provider =
                trace_provider.with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio);
            Ok(trace_provider.build().tracer(s.get_name()))
        } else {
            Ok(trace_provider
                .build()
                .tracer(OTLPExporterCfg::DEFAULT_TRACER_NAME))
        }
    }
}

impl Default for TelemetryData {
    fn default() -> Self {
        TelemetryData {
            otlp: None,
            stdout: Some(StdoutExporterCfg::default()),
        }
    }
}

/// Open telemetry settings of an ProSA
///
/// See [`TelemetryFilter`] to configure a specific filter for ProSA processors.
///
/// ```
/// use opentelemetry::global;
/// use prosa_utils::config::observability::Observability;
/// use prosa_utils::config::tracing::TelemetryFilter;
///
/// #[tokio::main]
/// async fn main() {
///     let observability_settings = Observability::default();
///
///     // trace
///     //global::set_tracer_provider(observability_settings.build_tracer_provider());
///     let filter = TelemetryFilter::default();
///     observability_settings.tracing_init(&filter);
///
///     // meter
///     global::set_meter_provider(observability_settings.build_meter_provider());
///
///     // logger
///     global::set_logger_provider(observability_settings.build_logger_provider());
/// }
/// ```
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Observability {
    /// Global level for observability
    #[serde(default)]
    level: TelemetryLevel,
    /// Metrics settings of a ProSA
    metrics: Option<TelemetryMetrics>,
    /// Logs settings of a ProSA
    logs: Option<TelemetryData>,
    /// Traces settings of a ProSA
    traces: Option<TelemetryData>,
}

impl Observability {
    /// Create an observability object with inline parameter instead of getting it from an external configuration
    pub fn new(level: TelemetryLevel) -> Observability {
        Observability {
            level,
            metrics: Some(TelemetryMetrics::default()),
            logs: Some(TelemetryData::default()),
            traces: Some(TelemetryData::default()),
        }
    }

    /// Meter provider builder
    pub fn build_meter_provider(&self) -> MeterProvider {
        if let Some(settings) = &self.metrics {
            match settings.build_provider() {
                Ok(m) => m,
                Err(_) => MeterProvider::default(),
            }
        } else {
            MeterProvider::default()
        }
    }

    /// Logger provider builder
    pub fn build_logger_provider(&self) -> LoggerProvider {
        if let Some(settings) = &self.logs {
            match settings.build_logger_provider() {
                Ok(m) => m,
                Err(_) => LoggerProvider::builder().build(),
            }
        } else {
            LoggerProvider::builder().build()
        }
    }

    /// Tracer provider builder
    ///
    /// ```
    /// use opentelemetry::{global, trace::TracerProvider};
    /// use prosa_utils::config::observability::Observability;
    ///
    /// let otel_settings = Observability::default();
    /// let tracer = otel_settings
    ///     .build_tracer_provider()
    ///     .tracer("prosa_proc_example");
    /// ```
    pub fn build_tracer_provider(&self) -> TracerProvider {
        if let Some(settings) = &self.traces {
            match settings.build_tracer_provider() {
                Ok(m) => m,
                Err(_) => TracerProvider::default(),
            }
        } else {
            TracerProvider::default()
        }
    }

    /// Tracer builder
    ///
    /// ```
    /// use opentelemetry::{global, trace::Tracer};
    /// use prosa_utils::config::observability::Observability;
    ///
    /// let otel_settings = Observability::default();
    /// let tracer = otel_settings
    ///     .build_tracer();
    /// ```
    pub fn build_tracer(&self) -> Tracer {
        if let Some(settings) = &self.traces {
            match settings.build_tracer() {
                Ok(m) => m,
                Err(_) => TracerProvider::default().tracer(OTLPExporterCfg::DEFAULT_TRACER_NAME),
            }
        } else {
            TracerProvider::default().tracer(OTLPExporterCfg::DEFAULT_TRACER_NAME)
        }
    }

    /// Method to init tracing traces
    pub fn tracing_init(&self, filter: &TelemetryFilter) -> Result<(), TryInitError> {
        let global_level: filter::LevelFilter = self.level.into();
        let subscriber = tracing_subscriber::registry().with(global_level);

        if let Some(traces) = &self.traces {
            if let Some(otlp) = &traces.otlp {
                let tracer = self.build_tracer();
                let subscriber_filter = filter.clone_with_level(otlp.level.unwrap_or_default());
                let subscriber = subscriber.with(
                    tracing_opentelemetry::layer()
                        .with_tracer(tracer)
                        .with_filter(subscriber_filter),
                );

                if let Some(stdout) = traces.stdout {
                    let subscriber_filter =
                        filter.clone_with_level(stdout.level.unwrap_or_default());
                    subscriber
                        .with(tracing_subscriber::fmt::Layer::new().with_filter(subscriber_filter))
                        .try_init()
                } else {
                    subscriber.try_init()
                }
            } else if let Some(stdout) = traces.stdout {
                let subscriber_filter = filter.clone_with_level(stdout.level.unwrap_or_default());
                subscriber
                    .with(tracing_subscriber::fmt::Layer::new().with_filter(subscriber_filter))
                    .try_init()
            } else {
                subscriber.try_init()
            }
        } else {
            subscriber.try_init()
        }
    }
}

impl Default for Observability {
    fn default() -> Self {
        Self {
            level: TelemetryLevel::default(),
            metrics: Some(TelemetryMetrics::default()),
            logs: Some(TelemetryData {
                otlp: None,
                stdout: Some(StdoutExporterCfg {
                    level: Some(TelemetryLevel::DEBUG),
                }),
            }),
            traces: Some(TelemetryData {
                otlp: None,
                stdout: Some(StdoutExporterCfg {
                    level: Some(TelemetryLevel::DEBUG),
                }),
            }),
        }
    }
}
