# Observability

For observability, ProSA uses [OpenTelemetry](https://opentelemetry.io/) to collect metrics, traces, and logs.

Observability is handled through the [Observability settings](https://docs.rs/prosa-utils/latest/prosa_utils/config/observability/struct.Observability.html).

## Settings

Parameters are specified in your ProSA settings file.
You can configure your observability outputs to be redirected to stdout or an OpenTelemetry collector.
You can also configure your processor to act as a server that exposes those metrics itself.

Of course all configurations can be mixed. You can send your logs to an OpenTelemetry collector and to stdout simultaneously.

### Attributes

For each observability signal, you can configure attributes that add labels to the exported data.

These attributes should follow the [OpenTelemetry resource conventions](https://github.com/open-telemetry/semantic-conventions/blob/main/docs/resource/README.md).

Some attributes are populated automatically by ProSA depending on the environment:
- `service.name` from the _ProSA name_
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
    # Override the version
    service.version: "1.0.0"
  metrics: # metrics params
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

HTTP OTLP endpoints can carry authorization credentials in their URL:

- `http://user:password@collector/...` produces a Basic authorization header.
- `http://:token@collector/...` produces a Bearer authorization header.

Username, password, and token values are percent-decoded before the header is built. Percent-encode
characters that have a special meaning in URL user information, such as `%`, `:`, `@`, `/`, `?`,
and `#`. Credentials, query parameters, and fragments are redacted from debug output.

> URL credentials are currently applied only to HTTP OTLP exporters. A `grpc://` endpoint does not
> convert URL credentials into gRPC metadata.

#### Grafana Cloud

You can connect ProSA directly to Grafana Cloud to send metrics, logs, and traces.
To do so, you need to create an OpenTelemetry Collector Grafana Cloud datasource.

To set it up, you have to:
- Select OpenTelemetry SDK, with Other as language (or Rust if it's available)
- Use Linux as infrastructure
- Use a direct connection with a token
- Decode the base64-encoded basic authorization token from the `Create an Instrumentation Instance`. You'll get an `id:password` to set OTLP credentials with.

> Percent-encode reserved characters in the password. For example, encode a trailing `=` as `%3D`
> and a literal `%` as `%25`.

With this information, set up your observability stack (look before if you want to set up traces):
```yaml
observability:
  # For the datasource setup
  attributes:
    service.name: my-app
  level: debug
  metrics:
    otlp:
      endpoint: "https://1234567:glc_<value>@otlp-gateway-prod-eu-west-2.grafana.net/otlp/v1/metrics"
  traces:
    otlp:
      endpoint: "https://1234567:glc_<value>@otlp-gateway-prod-eu-west-2.grafana.net/otlp/v1/traces"
  logs:
    otlp:
      endpoint: "https://1234567:glc_<value>@otlp-gateway-prod-eu-west-2.grafana.net/otlp/v1/logs"
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
