# Observability

For observability, ProSA uses [OpenTelemetry](https://opentelemetry.io/) to collect metrics, traces and logs.

Observability is handle through the [Observability settings](https://docs.rs/prosa-utils/latest/prosa_utils/config/observability/struct.Observability.html).

## Settings

Parameters are specified in your ProSA settings file.
You can configure outputs to stdout, an OpenTelemetry collector, or act as a server to expose metrics.

Of course all configurations can be mixed. You can send your logs to an OpenTelemetry collector and to stdout simultaneously.

### Stdout

If you want to push everything to stdout, you can do something like:
```yaml
observability:
  level: debug
  metrics:
    stdout:
      level: info
  traces:
    stdout:
      level: debug
  logs:
    stdout:
      level: debug
```

### OpenTelemetry

You can also push your telemetry to an OpenTelemetry collector:
```yaml
observability:
  level: debug
  metrics:
    otlp:
      endpoint: "http://localhost:4317"
      timeout_sec: 3
      protocol: Grpc
  traces:
    otlp:
      endpoint: "http://localhost:4317"
      timeout_sec: 3
      protocol: Grpc
  logs:
    otlp:
      endpoint: "http://localhost:4317"
      timeout_sec: 3
      protocol: Grpc
```

### Prometheus server

Prometheus works as a metric puller.

``` mermaid
flowchart LR
    prosa(ProSA)
    prom(Prometheus)
    prom --> prosa
```

As such, you can't directly send metric to it.
It's the role of Prometheus to gather metrics from your application.

To do this, you need to declare a server that exposes your ProSA metrics:
```yaml
observability:
  level: debug
  metrics:
    prometheus:
      endpoint: "0.0.0.0:9090"
```
