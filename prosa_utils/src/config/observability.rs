//! Definition of Opentelemetry configuration

use opentelemetry::{KeyValue, trace::TracerProvider as _};
use opentelemetry_otlp::{ExporterBuildError, Protocol, WithExportConfig, WithHttpConfig};
use opentelemetry_sdk::{
    logs::SdkLoggerProvider,
    metrics::SdkMeterProvider,
    trace::{SdkTracerProvider, Tracer},
};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU8, Ordering};
use std::{collections::HashMap, fmt, time::Duration};
use tracing_subscriber::{filter, prelude::*};
use tracing_subscriber::{layer::SubscriberExt, util::TryInitError};
use url::Url;

use crate::config::url::{get_safe_url, url_authentication};

use super::tracing::{TelemetryFilter, TelemetryLevel};

/// Configuration struct of an **O**pen **T**e**l**emetry **P**rotocol Exporter
#[derive(Deserialize, Serialize, Clone)]
pub(crate) struct OTLPExporterCfg {
    pub(crate) level: Option<TelemetryLevel>,
    endpoint: Url,
    #[serde(skip_serializing)]
    timeout_sec: Option<u64>,
}

impl OTLPExporterCfg {
    /// Get the sanitized endpoint string (without credentials)
    pub(crate) fn get_endpoint(&self) -> String {
        let mut endpoint = self.endpoint.clone();
        if !endpoint.username().is_empty() {
            let _ = endpoint.set_username("");
        }
        if endpoint.password().is_some() {
            let _ = endpoint.set_password(None);
        }
        endpoint.to_string()
    }

    /// Get the timeout duration if configured
    pub(crate) fn get_timeout(&self) -> Option<Duration> {
        self.timeout_sec.map(Duration::from_secs)
    }

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
            // `WithHttpConfig::with_headers` URL-decodes values, so preserve literal `%`.
            headers.insert(
                "Authorization".to_string(),
                authorization.replace('%', "%25"),
            );
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

impl Default for OTLPExporterCfg {
    fn default() -> Self {
        Self {
            level: None,
            endpoint: Url::parse("grpc://localhost:4317").expect("default OTLP address is invalid"),
            timeout_sec: None,
        }
    }
}

impl fmt::Debug for OTLPExporterCfg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OTLPExporterCfg")
            .field("level", &self.level)
            .field(
                "endpoint",
                &get_safe_url(&self.endpoint).without_credentials(),
            )
            .field("timeout_sec", &self.timeout_sec)
            .finish()
    }
}

/// Requirements used to determine whether ProSA is ready.
#[derive(Default, Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
pub struct HealthCheckCfg {
    /// Processor names that must currently have at least one registered queue.
    #[serde(default)]
    required_processors: Box<[String]>,
    /// Service names that must currently have at least one registered provider.
    #[serde(default)]
    required_services: Box<[String]>,
}

impl HealthCheckCfg {
    /// Required processor names.
    pub fn required_processors(&self) -> &[String] {
        &self.required_processors
    }

    /// Required service names.
    pub fn required_services(&self) -> &[String] {
        &self.required_services
    }
}

const HEALTH_STARTED: u8 = 0b01;
const HEALTH_READY: u8 = 0b10;

/// Shared ProSA health state used by observability exporters.
#[derive(Debug, Default)]
pub struct HealthState(AtomicU8);

impl HealthState {
    /// Update readiness, permanently marking startup complete on the first ready state.
    pub fn set_ready(&self, ready: bool) {
        if ready {
            self.0
                .store(HEALTH_STARTED | HEALTH_READY, Ordering::Relaxed);
        } else {
            self.0.fetch_and(!HEALTH_READY, Ordering::Relaxed);
        }
    }

    /// Whether ProSA has reached readiness at least once.
    pub fn is_started(&self) -> bool {
        self.0.load(Ordering::Relaxed) & HEALTH_STARTED != 0
    }

    /// Whether ProSA currently satisfies its readiness requirements.
    pub fn is_ready(&self) -> bool {
        self.0.load(Ordering::Relaxed) & HEALTH_READY != 0
    }
}

#[cfg(feature = "config-observability-prometheus")]
type ObservabilityResponse = hyper::Response<http_body_util::Full<bytes::Bytes>>;

#[cfg(feature = "config-observability-prometheus")]
fn response_builder() -> hyper::http::response::Builder {
    hyper::Response::builder().header(
        hyper::header::SERVER,
        concat!("ProSA/", env!("CARGO_PKG_VERSION")),
    )
}

#[cfg(feature = "config-observability-prometheus")]
fn probe_response(
    ok: bool,
    error: &'static str,
) -> Result<ObservabilityResponse, hyper::http::Error> {
    if ok {
        response_builder().body("ok\n".into())
    } else {
        response_builder()
            .status(hyper::StatusCode::SERVICE_UNAVAILABLE)
            .body(error.into())
    }
}

#[cfg(feature = "config-observability-prometheus")]
fn metrics_response<B>(
    _request: &hyper::Request<B>,
    registry: &prometheus::Registry,
) -> Result<ObservabilityResponse, hyper::http::Error> {
    let Ok(metric_data) = prometheus::TextEncoder::new().encode_to_string(&registry.gather())
    else {
        return response_builder()
            .status(hyper::StatusCode::INTERNAL_SERVER_ERROR)
            .body("can't serialize metrics\n".into());
    };
    let response = response_builder().header(hyper::header::CONTENT_TYPE, prometheus::TEXT_FORMAT);

    #[cfg(feature = "config-observability-gzip")]
    if _request
        .headers()
        .get(hyper::header::ACCEPT_ENCODING)
        .is_some_and(|a| a.to_str().is_ok_and(|v| v.contains("gzip")))
    {
        let mut gz_encoder =
            flate2::write::GzEncoder::new(Vec::with_capacity(2048), flate2::Compression::fast());
        if std::io::Write::write_all(&mut gz_encoder, metric_data.as_bytes()).is_ok()
            && let Ok(compressed_data) = gz_encoder.finish()
        {
            return response
                .header(hyper::header::CONTENT_ENCODING, "gzip")
                .body(compressed_data.into());
        }
    }

    response.body(metric_data.into())
}

#[cfg(feature = "config-observability-prometheus")]
fn handle_observability_request<B>(
    request: &hyper::Request<B>,
    health: &HealthState,
    registry: &prometheus::Registry,
) -> Result<ObservabilityResponse, hyper::http::Error> {
    match request.uri().path() {
        "/metrics" => metrics_response(request, registry),
        "/startup" => probe_response(health.is_started(), "starting\n"),
        // Answering at all proves the process is alive
        "/live" => probe_response(true, ""),
        "/ready" => probe_response(health.is_ready(), "not ready\n"),
        _ => response_builder()
            .status(hyper::StatusCode::NOT_FOUND)
            .body("not found\n".into()),
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
                let mut builder = opentelemetry_otlp::MetricExporter::builder()
                    .with_tonic()
                    .with_endpoint(s.get_endpoint())
                    .with_protocol(s.get_protocol());
                if let Some(timeout) = s.get_timeout() {
                    builder = builder.with_timeout(timeout);
                }
                builder.build()
            } else {
                let mut builder = opentelemetry_otlp::MetricExporter::builder()
                    .with_http()
                    .with_headers(s.get_header())
                    .with_endpoint(s.get_endpoint())
                    .with_protocol(s.get_protocol());
                if let Some(timeout) = s.get_timeout() {
                    builder = builder.with_timeout(timeout);
                }
                builder.build()
            }?;
            meter_provider = meter_provider.with_periodic_exporter(exporter);
        }

        #[cfg(feature = "config-observability-prometheus")]
        {
            let exporter = opentelemetry_prometheus::exporter()
                .with_registry(registry.clone())
                .with_resource_selector(opentelemetry_prometheus::ResourceSelector::All)
                .without_target_info()
                .build()
                .map_err(|e| ExporterBuildError::InternalFailure(e.to_string()))?;
            meter_provider = meter_provider
                .with_resource(
                    opentelemetry_sdk::resource::Resource::builder()
                        .with_attributes(resource_attr)
                        .build(),
                )
                .with_reader(exporter);
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
                let mut builder = opentelemetry_otlp::LogExporter::builder()
                    .with_tonic()
                    .with_endpoint(s.get_endpoint())
                    .with_protocol(s.get_protocol());
                if let Some(timeout) = s.get_timeout() {
                    builder = builder.with_timeout(timeout);
                }
                builder.build()
            } else {
                let mut builder = opentelemetry_otlp::LogExporter::builder()
                    .with_http()
                    .with_headers(s.get_header())
                    .with_endpoint(s.get_endpoint())
                    .with_protocol(s.get_protocol());
                if let Some(timeout) = s.get_timeout() {
                    builder = builder.with_timeout(timeout);
                }
                builder.build()
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
                let mut builder = opentelemetry_otlp::SpanExporter::builder()
                    .with_tonic()
                    .with_endpoint(s.get_endpoint())
                    .with_protocol(s.get_protocol());
                if let Some(timeout) = s.get_timeout() {
                    builder = builder.with_timeout(timeout);
                }
                builder.build()
            } else {
                let mut builder = opentelemetry_otlp::SpanExporter::builder()
                    .with_http()
                    .with_headers(s.get_header())
                    .with_endpoint(s.get_endpoint())
                    .with_protocol(s.get_protocol());
                if let Some(timeout) = s.get_timeout() {
                    builder = builder.with_timeout(timeout);
                }
                builder.build()
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
    /// Shared HTTP endpoint for health probes and Prometheus metrics.
    #[cfg(feature = "config-observability-prometheus")]
    endpoint: Option<String>,
    /// Readiness requirements.
    #[serde(default)]
    health: HealthCheckCfg,
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
            #[cfg(feature = "config-observability-prometheus")]
            endpoint: None,
            health: HealthCheckCfg::default(),
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
            self.attributes.len() + 3,
        );

        if !self.attributes.contains_key("host.name")
            && let Some(hostname) = super::hostname()
        {
            scope_attr.push(KeyValue::new("host.name", hostname));
        }

        if !self.attributes.contains_key("service.instance.id") {
            scope_attr.push(KeyValue::new("service.instance.id", super::hostid()));
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

    /// Getter of the global telemetry level
    pub fn get_level(&self) -> TelemetryLevel {
        self.level
    }

    /// Get the configured shared HTTP endpoint.
    #[cfg(feature = "config-observability-prometheus")]
    pub fn get_endpoint(&self) -> Option<&str> {
        self.endpoint.as_deref()
    }

    /// Get the configured readiness requirements.
    pub fn get_health_check(&self) -> &HealthCheckCfg {
        &self.health
    }

    /// Start the observability HTTP server (metrics and health probes) if an endpoint is configured
    #[cfg(feature = "config-observability-prometheus")]
    pub fn start_http_server(
        &self,
        health: std::sync::Arc<HealthState>,
        registry: &prometheus::Registry,
    ) {
        if let Some(endpoint) = self.endpoint.clone() {
            let registry = registry.clone();
            tokio::task::spawn(async move {
                let listener = match tokio::net::TcpListener::bind(&endpoint).await {
                    Ok(listener) => listener,
                    Err(e) => {
                        log::error!(target: "prosa::observability::http_server", "Failed to bind observability server on {endpoint}: {e}");
                        return;
                    }
                };
                loop {
                    if let Ok((stream, _)) = listener.accept().await {
                        let io = hyper_util::rt::TokioIo::new(stream);
                        let health = health.clone();
                        let registry = registry.clone();
                        tokio::task::spawn(async move {
                            if let Err(err) = hyper::server::conn::http1::Builder::new()
                                .serve_connection(
                                    io,
                                    hyper::service::service_fn(|req| {
                                        std::future::ready(handle_observability_request(
                                            &req, &health, &registry,
                                        ))
                                    }),
                                )
                                .await
                            {
                                log::debug!(target: "prosa::observability::http_server", "Error serving observability connection: {err:?}");
                            }
                        });
                    }
                }
            });
        }
    }

    /// Meter provider builder
    #[cfg(feature = "config-observability-prometheus")]
    pub fn build_meter_provider(&self, registry: &prometheus::Registry) -> SdkMeterProvider {
        // Prometheus exporter is always attached, even without metrics settings
        self.metrics
            .clone()
            .unwrap_or_default()
            .build_provider(self.get_scope_attributes(), registry)
            .unwrap_or_default()
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
        filter.set_level(self.level.into());
        let subscriber = tracing_subscriber::registry().with(filter::LevelFilter::TRACE);

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
            #[cfg(feature = "config-observability-prometheus")]
            endpoint: None,
            health: HealthCheckCfg::default(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "config-observability-prometheus")]
    fn request(path: &str, health: &HealthState) -> ObservabilityResponse {
        let request = hyper::Request::get(path)
            .body(())
            .expect("request should be valid");
        handle_observability_request(&request, health, &prometheus::Registry::new())
            .expect("response should be valid")
    }

    #[test]
    fn otlp_http_authorization_preserves_literal_percent_triplets() {
        let config = OTLPExporterCfg {
            level: None,
            endpoint: Url::parse("http://:token%2541@localhost:4318")
                .expect("OTLP endpoint should be valid"),
            timeout_sec: None,
        };

        assert_eq!(
            Some("Bearer token%2541"),
            config.get_header().get("Authorization").map(String::as_str)
        );
    }

    #[test]
    fn otlp_debug_redacts_url_secrets() {
        let config = OTLPExporterCfg {
            level: None,
            endpoint: Url::parse(
                "http://user:password@localhost:4318/v1?token=secret#access_token=secret",
            )
            .expect("OTLP endpoint should be valid"),
            timeout_sec: None,
        };

        let debug = format!("{config:?}");
        assert!(debug.contains("http://localhost:4318/v1"));
        assert!(!debug.contains("user"));
        assert!(!debug.contains("password"));
        assert!(!debug.contains("secret"));
    }

    #[test]
    fn health_configuration_uses_plain_requirements() {
        let config: Observability = yaml_serde::from_str(
            r#"
health:
  required_processors: [api, "", worker, api]
  required_services: [PAYMENT, ""]
"#,
        )
        .expect("observability configuration should deserialize");

        assert_eq!(
            &[
                "api".to_string(),
                String::new(),
                "worker".to_string(),
                "api".to_string()
            ],
            config.get_health_check().required_processors()
        );
        assert_eq!(
            &["PAYMENT".to_string(), String::new()],
            config.get_health_check().required_services()
        );
    }

    #[test]
    fn health_state_keeps_startup_after_ready() {
        let health = HealthState::default();
        assert!(!health.is_started());
        health.set_ready(true);
        health.set_ready(false);
        assert!(!health.is_ready());
        assert!(health.is_started());
    }

    #[cfg(feature = "config-observability-prometheus")]
    #[test]
    fn observability_endpoint_is_used() {
        let config: Observability = yaml_serde::from_str(
            r#"
endpoint: 127.0.0.1:8080
"#,
        )
        .expect("observability configuration should deserialize");

        assert_eq!(Some("127.0.0.1:8080"), config.get_endpoint());
    }

    #[cfg(feature = "config-observability-prometheus")]
    #[test]
    fn observability_routes() {
        let health = HealthState::default();
        let status = |path| request(path, &health).status();
        assert_eq!(hyper::StatusCode::SERVICE_UNAVAILABLE, status("/startup"));
        assert_eq!(hyper::StatusCode::OK, status("/live"));
        assert_eq!(hyper::StatusCode::SERVICE_UNAVAILABLE, status("/ready"));
        assert_eq!(hyper::StatusCode::NOT_FOUND, status("/"));

        health.set_ready(true);
        assert_eq!(hyper::StatusCode::OK, status("/startup"));
        assert_eq!(hyper::StatusCode::OK, status("/ready"));

        let response = request("/metrics", &health);
        assert_eq!(hyper::StatusCode::OK, response.status());
        assert_eq!(
            Some(prometheus::TEXT_FORMAT),
            response
                .headers()
                .get(hyper::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
        );
    }
}
