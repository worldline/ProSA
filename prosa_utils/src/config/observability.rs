//! Definition of Opentelemetry configuration

use opentelemetry::{KeyValue, trace::TracerProvider as _};
use opentelemetry_otlp::{
    ExportConfig, ExporterBuildError, Protocol, WithExportConfig, WithHttpConfig,
};
use opentelemetry_sdk::{
    logs::SdkLoggerProvider,
    metrics::SdkMeterProvider,
    trace::{SdkTracerProvider, Tracer},
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, time::Duration};
use tracing_subscriber::{filter, prelude::*};
use tracing_subscriber::{layer::SubscriberExt, util::TryInitError};
use url::Url;

use crate::config::url_authentication;

use super::tracing::{TelemetryFilter, TelemetryLevel};

/// Configuration struct of an **O**pen **T**e**l**emetry **P**rotocol Exporter
#[derive(Debug, Deserialize, Serialize, Clone)]
pub(crate) struct OTLPExporterCfg {
    pub(crate) level: Option<TelemetryLevel>,
    endpoint: Url,
    #[serde(skip_serializing)]
    timeout_sec: Option<u64>,
}

impl OTLPExporterCfg {
    pub(crate) fn get_protocol(&self) -> Protocol {
        match self.endpoint.scheme().to_lowercase().as_str() {
            "grpc" => Protocol::Grpc,
            "http/json" => Protocol::HttpJson,
            _ => Protocol::HttpBinary,
        }
    }

    pub(crate) fn get_header(&self) -> HashMap<String, String> {
        let mut headers = HashMap::with_capacity(1);
        if let Some(authorization) = url_authentication(&self.endpoint) {
            headers.insert("Authorization".to_string(), authorization);
        }
        headers
    }

    pub(crate) fn get_resource(
        &self,
        attr: Vec<KeyValue>,
    ) -> opentelemetry_sdk::resource::Resource {
        opentelemetry_sdk::resource::Resource::builder()
            .with_attributes(attr)
            .with_attribute(opentelemetry::KeyValue::new(
                "process.creation.time",
                chrono::Utc::now().to_rfc3339(),
            ))
            .with_attribute(opentelemetry::KeyValue::new(
                "process.pid",
                opentelemetry::Value::I64(std::process::id() as i64),
            ))
            .build()
    }
}

impl From<OTLPExporterCfg> for ExportConfig {
    fn from(value: OTLPExporterCfg) -> Self {
        let protocol = value.get_protocol();
        let mut endpoint = value.endpoint;
        if !endpoint.username().is_empty() {
            let _ = endpoint.set_username("");
        }
        if endpoint.password().is_some() {
            let _ = endpoint.set_password(None);
        }

        ExportConfig {
            endpoint: Some(endpoint.to_string()),
            timeout: value.timeout_sec.map(Duration::from_secs),
            protocol,
        }
    }
}

impl Default for OTLPExporterCfg {
    fn default() -> Self {
        Self {
            level: None,
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
                    Ok(listener) => {
                        loop {
                            if let Ok((stream, _)) = listener.accept().await {
                                let io = hyper_util::rt::TokioIo::new(stream);
                                let registry = registry.clone();
                                tokio::task::spawn(async move {
                                    if let Err(err) = hyper::server::conn::http1::Builder::new()
                                    .serve_connection(
                                        io,
                                        #[allow(unused)]
                                        hyper::service::service_fn(|req| {
                                            let registry = registry.clone();
                                            async move {
                                                let metric_families = registry.gather();
                                                let encoder = prometheus::TextEncoder::new();
                                                if let Ok(metric_data) =
                                                    encoder.encode_to_string(&metric_families)
                                                {
                                                    let response = hyper::Response::builder()
                                                        .header(hyper::header::SERVER, concat!("ProSA/", env!("CARGO_PKG_VERSION")))
                                                        .header(
                                                            hyper::header::CONTENT_TYPE,
                                                            "text/plain; version=1.0.0",
                                                        );

                                                    #[cfg(feature = "config-observability-gzip")]
                                                    if req.headers().get(hyper::header::ACCEPT_ENCODING).is_some_and(|a| a.to_str().is_ok_and(|v| v.contains("gzip"))) {
                                                        let mut gz_encoder = flate2::write::GzEncoder::new(Vec::with_capacity(2048), flate2::Compression::fast());
                                                        if std::io::Write::write_all(&mut gz_encoder, metric_data.as_bytes()).is_ok()
                                                            && let Ok(compressed_data) = gz_encoder.finish()
                                                        {
                                                            return response
                                                                .header(hyper::header::CONTENT_ENCODING, "gzip")
                                                                .body(http_body_util::Full::new(
                                                                    bytes::Bytes::from(compressed_data)),
                                                                )
                                                                .map_err(|e| e.to_string());
                                                        }
                                                    }

                                                    response
                                                        .body(http_body_util::Full::new(
                                                            bytes::Bytes::from(metric_data),
                                                        ))
                                                        .map_err(|e| e.to_string())
                                                } else {
                                                    Err("Can't serialize metrics".to_string())
                                                }
                                            }
                                        }),
                                    )
                                    .await
                                {
                                    log::debug!(target: "prosa::observability::prometheus_server", "Error serving prometheus connection: {err:?}");
                                }
                                });
                            }
                        }
                    }
                    Err(e) => {
                        log::error!(target: "prosa::observability::prometheus_server", "Failed to bind Prometheus metrics server: {e}");
                    }
                }
            });
        }

        Ok(())
    }

    pub(crate) fn get_resource(
        &self,
        attr: Vec<KeyValue>,
    ) -> opentelemetry_sdk::resource::Resource {
        opentelemetry_sdk::resource::Resource::builder()
            .with_attributes(attr)
            .build()
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
        #[cfg(feature = "config-observability-prometheus")] resource_attr: Vec<KeyValue>,
        #[cfg(feature = "config-observability-prometheus")] registry: &prometheus::Registry,
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
                    .with_headers(s.get_header())
                    .with_export_config(s.clone().into())
                    .build()
            }?;
            meter_provider = meter_provider.with_periodic_exporter(exporter);
        }

        #[cfg(feature = "config-observability-prometheus")]
        if let Some(prom) = &self.prometheus {
            // configure OpenTelemetry to use this registry
            let exporter = opentelemetry_prometheus::exporter()
                .with_registry(registry.clone())
                .with_resource_selector(opentelemetry_prometheus::ResourceSelector::All)
                .without_target_info()
                .build()
                .map_err(|e| ExporterBuildError::InternalFailure(e.to_string()))?;
            meter_provider = meter_provider
                .with_resource(prom.get_resource(resource_attr))
                .with_reader(exporter);

            // Initialize the Prometheus server if needed
            prom.init_prometheus_server(registry)?;
        }

        if self.stdout.is_some() {
            let exporter = opentelemetry_stdout::MetricExporter::default();
            meter_provider = meter_provider.with_periodic_exporter(exporter);
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
    fn build_logger_provider(
        &self,
        resource_attr: Vec<KeyValue>,
    ) -> Result<(SdkLoggerProvider, TelemetryLevel), ExporterBuildError> {
        let logs_provider = SdkLoggerProvider::builder();
        if let Some(s) = &self.otlp {
            let exporter = if s.get_protocol() == Protocol::Grpc {
                opentelemetry_otlp::LogExporter::builder()
                    .with_tonic()
                    .with_export_config(s.clone().into())
                    .build()
            } else {
                opentelemetry_otlp::LogExporter::builder()
                    .with_http()
                    .with_headers(s.get_header())
                    .with_export_config(s.clone().into())
                    .build()
            }?;
            Ok((
                logs_provider
                    .with_resource(s.get_resource(resource_attr))
                    .with_batch_exporter(exporter)
                    .build(),
                s.level.unwrap_or_default(),
            ))
        } else if let Some(stdout) = &self.stdout {
            Ok((
                logs_provider
                    .with_simple_exporter(opentelemetry_stdout::LogExporter::default())
                    .build(),
                stdout.level.unwrap_or_default(),
            ))
        } else {
            Ok((logs_provider.build(), TelemetryLevel::OFF))
        }
    }

    /// Build a tracer provider based on the self configuration
    fn build_tracer_provider(
        &self,
        resource_attr: Vec<KeyValue>,
    ) -> Result<SdkTracerProvider, ExporterBuildError> {
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
                    .with_headers(s.get_header())
                    .with_export_config(s.clone().into())
                    .build()
            }?;

            trace_provider = trace_provider
                .with_resource(s.get_resource(resource_attr))
                .with_batch_exporter(exporter);
        }

        Ok(trace_provider.build())
    }

    /// Build a tracer provider based on the self configuration
    fn build_tracer(
        &self,
        name: &str,
        resource_attr: Vec<KeyValue>,
    ) -> Result<Tracer, ExporterBuildError> {
        self.build_tracer_provider(resource_attr)
            .map(|p| p.tracer(name.to_string()))
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

/// Open telemetry settings of a ProSA
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
    /// Additional attributes for all telemetry data
    #[serde(default)]
    attributes: HashMap<String, String>,
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
    pub(crate) fn common_scope_attributes(service_name: String, capacity: usize) -> Vec<KeyValue> {
        let mut scope_attributes = Vec::with_capacity(capacity + 3);
        scope_attributes.push(KeyValue::new("service.name", service_name));

        match std::env::consts::ARCH {
            "x86_64" => scope_attributes.push(KeyValue::new("host.arch", "amd64")),
            "aarch64" => scope_attributes.push(KeyValue::new("host.arch", "arm64")),
            "arm" => scope_attributes.push(KeyValue::new("host.arch", "arm32")),
            _ => {}
        }

        match std::env::consts::OS {
            "linux" => scope_attributes.push(KeyValue::new("os.type", "linux")),
            "macos" => scope_attributes.push(KeyValue::new("os.type", "darwin")),
            "freebsd" => scope_attributes.push(KeyValue::new("os.type", "freebsd")),
            "openbsd" => scope_attributes.push(KeyValue::new("os.type", "openbsd")),
            "netbsd" => scope_attributes.push(KeyValue::new("os.type", "netbsd")),
            "windows" => scope_attributes.push(KeyValue::new("os.type", "windows")),
            _ => {}
        }

        scope_attributes
    }

    /// Create an observability object with inline parameter instead of getting it from an external configuration
    pub fn new(level: TelemetryLevel) -> Observability {
        Observability {
            attributes: HashMap::new(),
            level,
            metrics: Some(TelemetryMetrics::default()),
            logs: Some(TelemetryData::default()),
            traces: Some(TelemetryData::default()),
        }
    }

    /// Getter of the observability `service.name` attributes
    pub fn get_service_name(&self) -> &str {
        self.attributes
            .get("service.name")
            .map(|s| s.as_ref())
            .unwrap_or("prosa")
    }

    /// Setter of the ProSA name for all observability `service.name` attributes
    pub fn set_prosa_name(&mut self, name: &str) {
        self.attributes
            .entry("service.name".to_string())
            .or_insert_with(|| name.to_string());
    }

    /// Getter of the common scope attributes
    pub fn get_scope_attributes(&self) -> Vec<KeyValue> {
        // start with common attributes
        let mut scope_attr = Self::common_scope_attributes(
            self.get_service_name().to_string(),
            self.attributes.len() + 2,
        );

        if !self.attributes.contains_key("host.name")
            && let Some(hostname) = super::hostname()
        {
            scope_attr.push(KeyValue::new("host.name", hostname));
        }

        if !self.attributes.contains_key("service.version") {
            scope_attr.push(KeyValue::new("service.version", env!("CARGO_PKG_VERSION")));
        }

        // append custom attributes from configuration
        scope_attr.append(
            self.attributes
                .iter()
                .map(|(k, v)| {
                    KeyValue::new(k.clone(), opentelemetry::Value::String(v.clone().into()))
                })
                .collect::<Vec<KeyValue>>()
                .as_mut(),
        );

        scope_attr
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
    #[cfg(feature = "config-observability-prometheus")]
    pub fn build_meter_provider(&self, registry: &prometheus::Registry) -> SdkMeterProvider {
        if let Some(settings) = &self.metrics {
            settings
                .build_provider(self.get_scope_attributes(), registry)
                .unwrap_or_default()
        } else {
            SdkMeterProvider::default()
        }
    }

    /// Meter provider builder
    #[cfg(not(feature = "config-observability-prometheus"))]
    pub fn build_meter_provider(&self) -> SdkMeterProvider {
        if let Some(settings) = &self.metrics {
            settings.build_provider().unwrap_or_default()
        } else {
            SdkMeterProvider::default()
        }
    }

    /// Logger provider builder
    pub fn build_logger_provider(&self) -> (SdkLoggerProvider, TelemetryLevel) {
        if let Some(settings) = &self.logs {
            match settings.build_logger_provider(self.get_scope_attributes()) {
                Ok(m) => m,
                Err(_) => (
                    SdkLoggerProvider::builder().build(),
                    TelemetryLevel::default(),
                ),
            }
        } else {
            (
                SdkLoggerProvider::builder().build(),
                TelemetryLevel::default(),
            )
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
            settings
                .build_tracer_provider(self.get_scope_attributes())
                .unwrap_or_default()
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
            match settings.build_tracer(self.get_service_name(), self.get_scope_attributes()) {
                Ok(m) => m,
                Err(_) => SdkTracerProvider::default().tracer(self.get_service_name().to_string()),
            }
        } else {
            SdkTracerProvider::default().tracer(self.get_service_name().to_string())
        }
    }

    /// Method to init `tracing`
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
        } else if let Some(logs) = &self.logs
            && let Ok((logger_provider, level)) =
                logs.build_logger_provider(self.get_scope_attributes())
            && level > TelemetryLevel::OFF
        {
            let logger_filter = filter.clone_with_level(level);
            subscriber
                .with(
                    opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge::new(
                        &logger_provider,
                    )
                    .with_filter(logger_filter),
                )
                .try_init()
        } else {
            subscriber.try_init()
        }
    }
}

impl Default for Observability {
    fn default() -> Self {
        Self {
            attributes: HashMap::new(),
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
