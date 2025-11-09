# Observability

For observability, ProSA uses [OpenTelemetry](https://opentelemetry.io/) to collect metrics, traces, and logs.

Observability is handled through the [Observability settings](https://docs.rs/prosa-utils/latest/prosa_utils/config/observability/struct.Observability.html).

## Settings

Parameters are specified in your ProSA settings file.
You can configure your observability outputs to be redirected to stdout or an OpenTelemetry collector.
You can also configure your processor to act as a server that exposes those metrics itself.

Of course all configurations can be mixed. You can send your logs to an OpenTelemetry collector and to stdout simultaneously.

### Stdout

If you want to direct all logs to stdout, you can do something like this:
```yaml
observability:
  level: debug
  metrics:
    stdout:
      level: info
  traces:
    stdout:
      level: debug
```

With `tracing` you'll have more fancy log than with `log`:
```yaml
observability:
  level: debug
  metrics:
    stdout:
      level: info
  logs:
    stdout:
      level: debug
```

If you specify both _traces_ and _logs_, `tracing` will be used.

### OpenTelemetry

#### gRPC

You can also push your telemetry to a gRPC OpenTelemetry collector:
```yaml
observability:
  level: debug
  metrics:
    otlp:
      endpoint: "grpc://localhost:4317"
  traces:
    otlp:
      endpoint: "grpc://localhost:4317"
```

If you specify _traces_, you'll have no log send, only traces containing all your logs.
You have to define only _logs_ if you want to have log sent to the collectors:
```yaml
observability:
  level: debug
  metrics:
    otlp:
      endpoint: "grpc://localhost:4317"
  logs:
    otlp:
      endpoint: "grpc://localhost:4317"
```

#### HTTP

If you want to go with an HTTP Opentelemetry collector:
```yaml
observability:
  level: debug
  metrics:
    otlp:
      endpoint: "http://localhost:4318/v1/metrics"
  traces:
    otlp:
      endpoint: "http://localhost:4318/v1/traces"
```

Same as before with gRPC, to send logs, you need to specify only _logs_ and not _traces_:
```yaml
observability:
  level: debug
  metrics:
    otlp:
      endpoint: "http://localhost:4318/v1/metrics"
  logs:
    otlp:
      endpoint: "http://localhost:4318/v1/logs"
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
