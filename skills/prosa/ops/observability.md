# Observability Reference

ProSA uses [OpenTelemetry](https://opentelemetry.io/) for metrics, traces, and logs.

## Configuration

All observability settings go under the `observability:` key in the YAML config.

### Attributes

Labels added to all observability data. Follow [OpenTelemetry resource conventions](https://github.com/open-telemetry/semantic-conventions/blob/main/docs/resource/README.md).

Auto-populated attributes:
- `service.name` — from ProSA name
- `service.version` — package version
- `host.arch` — if detected
- `os.type` — if detected
- `process.pid` — automatic (traces/logs only)
- `process.creation.time` — automatic (traces/logs only)

```yaml
observability:
  attributes:
    service.name: "my_service"
    service.version: "1.0.0"
```

## Metrics

### Stdout

```yaml
observability:
  level: debug
  metrics:
    stdout:
      level: info
```

### Prometheus (pull mode)

Requires feature `prometheus` enabled in ProSA.

```yaml
observability:
  metrics:
    prometheus:
      endpoint: "0.0.0.0:9090"
```

Prometheus scrapes this endpoint for metrics.

### OTLP gRPC

```yaml
observability:
  metrics:
    otlp:
      endpoint: "grpc://localhost:4317"
```

### OTLP HTTP

```yaml
observability:
  metrics:
    otlp:
      endpoint: "http://localhost:4318/v1/metrics"
```

### Grafana Cloud (direct)

Decode the base64 authorization token from Grafana Cloud setup to get `id:password`. Replace trailing `=` with `%3D` in URLs.

```yaml
observability:
  attributes:
    service.name: "my-app"
  metrics:
    otlp:
      endpoint: "https://<id>:<token>@otlp-gateway-prod-eu-west-2.grafana.net/otlp/v1/metrics"
  traces:
    otlp:
      endpoint: "https://<id>:<token>@otlp-gateway-prod-eu-west-2.grafana.net/otlp/v1/traces"
  logs:
    otlp:
      endpoint: "https://<id>:<token>@otlp-gateway-prod-eu-west-2.grafana.net/otlp/v1/logs"
```

## Traces

### Stdout

```yaml
observability:
  level: debug
  traces:
    stdout:
      level: debug
```

### OTLP

```yaml
observability:
  traces:
    otlp:
      endpoint: "grpc://localhost:4317"
```

If both `traces` and `logs` stdout are configured, only `traces` is applied (logs are included in traces).

### Using traces in code

Traces are auto-configured by cargo-prosa. Use the `tracing` crate:

```rust
// Enter a message span to record timing
let _enter = msg.enter_span();
tracing::info!("Processing message: {msg:?}");

// Access the span for linking
let msg_span = msg.get_span();
```

## Logs

### Stdout

```yaml
observability:
  logs:
    stdout:
      level: debug
```

### OTLP

```yaml
observability:
  logs:
    otlp:
      endpoint: "grpc://localhost:4317"
```

Use the `log` crate for standalone logs (not attached to spans):

```rust
log::info!("Standalone log message");
```

If traces are configured, logs are automatically included as trace events.

## Custom Metrics in Processors

```rust
// Get a meter from the processor
let meter = proc_param.meter("my_metric_namespace");

// Synchronous gauge
let gauge = meter.u64_gauge("request_count")
    .with_description("Number of requests processed")
    .build();
gauge.record(42, &[KeyValue::new("type", "http")]);

// Async observable gauge (updated via watch channel)
let (tx, rx) = tokio::sync::watch::channel(0u64);
let _observable = meter.u64_observable_gauge("active_connections")
    .with_description("Current active connections")
    .with_callback(move |observer| {
        observer.observe(*rx.borrow(), &[KeyValue::new("type", "tcp")]);
    })
    .build();
// Later: tx.send(new_value);
```

OpenTelemetry dependency:

```toml
opentelemetry = { version = "0.29", features = ["metrics", "trace", "logs"] }
```

## System Metrics

Requires feature `system-metrics`. Provides RAM metrics:
- `virtual` — virtual RAM used
- `physical` — physical RAM used

## Monitoring Dashboard

ProSA provides a node graph view in Grafana showing:
- **Processors**: green (running), orange (restarted), grey (stopped), red (crashed)
- **Services**: green (available), grey (unavailable)
- **Links**: connections between processors and their exposed services
