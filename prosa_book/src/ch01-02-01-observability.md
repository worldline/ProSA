# Observability

For observability, ProSA uses [OpenTelemetry](https://opentelemetry.io/) to collect metrics, traces, and logs.

Observability is handled through the [Observability settings](https://docs.rs/prosa-utils/latest/prosa_utils/config/observability/struct.Observability.html).

## Settings

Parameters are specified in your ProSA settings file.
You can configure your observability outputs to be redirected to stdout or an OpenTelemetry collector.
You can also configure your processor to act as a server that exposes those metrics itself.

Of course all configurations can be mixed. You can send your logs to an OpenTelemetry collector and to stdout simultaneously.

### Attributes

For each of your observability data, you can configure attribute that will add labels on your data.

These attribute should follow the [OpenTelemetry resource conventions](https://github.com/open-telemetry/semantic-conventions/blob/main/docs/resource/README.md).

Some of these attributtes are automaticcaly field from ProSA depending of your environment:
- `service.name` took from _prosa name_
- `host.arch` if detected from the compilation
- `os.type` if the OS was detected
- `service.version` the package version

For your logs and traces (but not metrics to avoid overloading metrics indexes), you'll find:
- `process.creation.time`
- `process.pid`

In the configuration you'll have:
```yaml
observability:
  attributes:
    # Override the service.name from ProSA
    service.name: "my_service"
    # Overried the version
    service.version: "1.0.0"
  metric: # metrics params
  traces: # traces params
  logs:   # logs params
```

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

If you use `tracing`, you will get richer log output compared to `log`:
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

If both _traces_ and _logs_ are configured, only the _traces_ configuration will be applied.

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

If you specify _traces_, only _traces_ (including _logs_) will be sent.
To send _logs_ separately, use the **logs**:
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

To use an HTTP OpenTelemetry collector:
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

To send _logs_ via HTTP, specify the **logs** (without the _traces_):
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

> You also need to enable the feature `prometheus` for ProSA.
