//! Definition of Opentelemetry configuration

use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::{ExportConfig, ExporterBuildError, Protocol, WithExportConfig};
use opentelemetry_sdk::{
    logs::SdkLoggerProvider,
    metrics::{PeriodicReader, SdkMeterProvider},
    trace::{SdkTracerProvider, Tracer},
};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing_subscriber::{filter, prelude::*};
use tracing_subscriber::{layer::SubscriberExt, util::TryInitError};
use url::Url;

use super::tracing::{TelemetryFilter, TelemetryLevel};

/// Configuration struct of an **O**pen **T**e**l**emetry **P**rotocol Exporter
#[derive(Debug, Deserialize, Serialize, Clone)]
pub(crate) struct OTLPExporterCfg {
    pub(crate) level: Option<TelemetryLevel>,
    #[serde(default = "OTLPExporterCfg::get_default_name")]
    name: String,
    endpoint: Url,
    #[serde(skip_serializing)]
    timeout_sec: Option<u64>,
}

impl OTLPExporterCfg {
    pub(crate) const DEFAULT_TRACER_NAME: &'static str = "prosa";

    fn get_default_name() -> String {
        Self::DEFAULT_TRACER_NAME.into()
    }

    pub(crate) fn get_protocol(&self) -> Protocol {
        if self.endpoint.scheme().to_lowercase() == "grpc" {
            Protocol::Grpc
        } else {
            Protocol::HttpBinary
        }
    }
}

impl From<OTLPExporterCfg> for ExportConfig {
    fn from(value: OTLPExporterCfg) -> Self {
        ExportConfig {
            endpoint: Some(value.endpoint.to_string()),
            timeout: value.timeout_sec.map(Duration::from_secs),
            protocol: value.get_protocol(),
        }
    }
}

impl Default for OTLPExporterCfg {
    fn default() -> Self {
        Self {
            level: None,
            name: Self::get_default_name(),
            endpoint: Url::parse("grpc://localhost:4317").unwrap(),
            timeout_sec: None,
        }
    }
}

#[cfg(feature = "config-observability-prometheus")]
/// Configuration struct of a prometheus metric exporter
#[derive(Default, Debug, Deserialize, Serialize, Clone)]
pub struct PrometheusExporterCfg {
    endpoint: Option<String>,
}

#[cfg(feature = "config-observability-prometheus")]
impl PrometheusExporterCfg {
    /// Start an HTTP server to expose the metrics if needed
    pub(crate) fn init_prometheus_server(
        &self,
        registry: &prometheus::Registry,
    ) -> Result<(), ExporterBuildError> {
        if let Some(endpoint) = self.endpoint.clone() {
            let registry = registry.clone();
            tokio::task::spawn(async move {
                match tokio::net::TcpListener::bind(endpoint).await {
                    Ok(listener) => loop {
                        if let Ok((stream, _)) = listener.accept().await {
                            let io = hyper_util::rt::TokioIo::new(stream);
                            let registry = registry.clone();
                            tokio::task::spawn(async move {
                                if let Err(err) = hyper::server::conn::http1::Builder::new()
                                    .serve_connection(
                                        io,
                                        hyper::service::service_fn(|_req| {
                                            let registry = registry.clone();
                                            async move {
                                                let metric_families = registry.gather();
                                                let encoder = prometheus::TextEncoder::new();
                                                if let Ok(metric_data) =
                                                    encoder.encode_to_string(&metric_families)
                                                {
                                                    Ok(hyper::Response::new(
                                                        http_body_util::Full::new(
                                                            bytes::Bytes::from(metric_data),
                                                        ),
                                                    ))
                                                } else {
                                                    Err("Can't serialize metrics")
                                                }
                                            }
                                        }),
                                    )
                                    .await
                                {
                                    log::debug!("Error serving prometheus connection: {err:?}");
                                }
                            });
                        }
                    },
                    Err(e) => {
                        log::error!("Failed to bind Prometheus metrics server: {e}");
                    }
                }
            });
        }

        Ok(())
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
pub struct TelemetryMetrics {
    otlp: Option<OTLPExporterCfg>,
    #[cfg(feature = "config-observability-prometheus")]
    prometheus: Option<PrometheusExporterCfg>,
    stdout: Option<StdoutExporterCfg>,
}

impl TelemetryMetrics {
    /// Build a meter provider based on the self configuration
    fn build_provider(
        &self,
        registry: &prometheus::Registry,
    ) -> Result<SdkMeterProvider, ExporterBuildError> {
        let mut meter_provider = SdkMeterProvider::builder();
        if let Some(s) = &self.otlp {
            let exporter = if s.get_protocol() == Protocol::Grpc {
                opentelemetry_otlp::MetricExporter::builder()
                    .with_tonic()
                    .with_export_config(s.clone().into())
                    .build()
            } else {
                opentelemetry_otlp::MetricExporter::builder()
                    .with_http()
                    .with_export_config(s.clone().into())
                    .build()
            }?;
            let reader = PeriodicReader::builder(exporter).build();
            meter_provider = meter_provider.with_reader(reader);
        }

        #[cfg(feature = "config-observability-prometheus")]
        if let Some(prom) = &self.prometheus {
            // configure OpenTelemetry to use this registry
            let exporter = opentelemetry_prometheus::exporter()
                .with_registry(registry.clone())
                .without_target_info()
                .without_scope_info()
                .build()
                .map_err(|e| ExporterBuildError::InternalFailure(e.to_string()))?;
            meter_provider = meter_provider.with_reader(exporter);

            // Initialize the Prometheus server if needed
            prom.init_prometheus_server(registry)?;
        }

        if self.stdout.is_some() {
            let exporter = opentelemetry_stdout::MetricExporter::default();
            let reader = PeriodicReader::builder(exporter).build();
            meter_provider = meter_provider.with_reader(reader);
        }

        Ok(meter_provider.build())
    }
}

/// Telemetry data define for metrics, logs, traces
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TelemetryData {
    otlp: Option<OTLPExporterCfg>,
    stdout: Option<StdoutExporterCfg>,
}

impl TelemetryData {
    /// Get the greater log level of the configuration (log level that include both OpenTelemetry and stdout)
    fn get_max_level(&self) -> TelemetryLevel {
        if let Some(otlp_level) = self.otlp.as_ref().and_then(|o| o.level) {
            if let Some(stdout_level) = self.stdout.as_ref().and_then(|l| l.level) {
                if otlp_level > stdout_level {
                    otlp_level
                } else {
                    stdout_level
                }
            } else {
                otlp_level
            }
        } else if let Some(stdout_level) = self.stdout.as_ref().and_then(|l| l.level) {
            stdout_level
        } else {
            TelemetryLevel::TRACE
        }
    }

    /// Build a logger provider based on the self configuration
    fn build_logger_provider(&self) -> Result<SdkLoggerProvider, ExporterBuildError> {
        let mut logs_provider = SdkLoggerProvider::builder();
        if let Some(s) = &self.otlp {
            let exporter = if s.get_protocol() == Protocol::Grpc {
                opentelemetry_otlp::LogExporter::builder()
                    .with_tonic()
                    .with_export_config(s.clone().into())
                    .build()
            } else {
                opentelemetry_otlp::LogExporter::builder()
                    .with_http()
                    .with_export_config(s.clone().into())
                    .build()
            }?;
            logs_provider = logs_provider.with_batch_exporter(exporter);
        }

        Ok(logs_provider.build())
    }

    /// Build a tracer provider based on the self configuration
    fn build_tracer_provider(&self) -> Result<SdkTracerProvider, ExporterBuildError> {
        let mut trace_provider = SdkTracerProvider::builder();
        if let Some(s) = &self.otlp {
            let exporter = if s.get_protocol() == Protocol::Grpc {
                opentelemetry_otlp::SpanExporter::builder()
                    .with_tonic()
                    .with_export_config(s.clone().into())
                    .build()
            } else {
                opentelemetry_otlp::SpanExporter::builder()
                    .with_http()
                    .with_export_config(s.clone().into())
                    .build()
            }?;
            trace_provider = trace_provider.with_batch_exporter(exporter);
        }

        Ok(trace_provider.build())
    }

    /// Build a tracer provider based on the self configuration
    fn build_tracer(&self) -> Result<Tracer, ExporterBuildError> {
        let mut trace_provider = SdkTracerProvider::builder();
        if let Some(s) = &self.otlp {
            let exporter = if s.get_protocol() == Protocol::Grpc {
                opentelemetry_otlp::SpanExporter::builder()
                    .with_tonic()
                    .with_export_config(s.clone().into())
                    .build()
            } else {
                opentelemetry_otlp::SpanExporter::builder()
                    .with_http()
                    .with_export_config(s.clone().into())
                    .build()
            }?;
            trace_provider = trace_provider.with_batch_exporter(exporter);
            Ok(trace_provider.build().tracer(s.name.clone()))
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
///     let filter = TelemetryFilter::default();
///     observability_settings.tracing_init(&filter);
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

    /// Getter of the log level (max value)
    pub fn get_logger_level(&self) -> TelemetryLevel {
        if let Some(logs) = &self.logs {
            let logs_level = logs.get_max_level();
            if logs_level > self.level {
                logs_level
            } else {
                self.level
            }
        } else {
            self.level
        }
    }

    /// Meter provider builder
    pub fn build_meter_provider(&self, registry: &prometheus::Registry) -> SdkMeterProvider {
        if let Some(settings) = &self.metrics {
            settings.build_provider(registry).unwrap_or_default()
        } else {
            SdkMeterProvider::default()
        }
    }

    /// Logger provider builder
    pub fn build_logger_provider(&self) -> SdkLoggerProvider {
        if let Some(settings) = &self.logs {
            match settings.build_logger_provider() {
                Ok(m) => m,
                Err(_) => SdkLoggerProvider::builder().build(),
            }
        } else {
            SdkLoggerProvider::builder().build()
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
    pub fn build_tracer_provider(&self) -> SdkTracerProvider {
        if let Some(settings) = &self.traces {
            settings.build_tracer_provider().unwrap_or_default()
        } else {
            SdkTracerProvider::default()
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
                Err(_) => SdkTracerProvider::default().tracer(OTLPExporterCfg::DEFAULT_TRACER_NAME),
            }
        } else {
            SdkTracerProvider::default().tracer(OTLPExporterCfg::DEFAULT_TRACER_NAME)
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
