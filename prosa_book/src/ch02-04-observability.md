# Observability

As discussed in the [Configuration](ch01-02-01-observability.md) chapter, ProSA uses the [Opentelemetry](https://opentelemetry.io/docs/languages/rust/) stack to provide metrics, traces and logs.
All required settings are described in this chapter.

This section explains how you can use observability features for your own purposes.
When you create an adaptor, you may want to generate custom metrics, traces or logs from relevant data you are processing.

It is important to understand how to implement these features within ProSA, as ProSA handles much of the integration for you.

Each time you'll need to include opentelemetry dependency to your project:
```toml
opentelemetry = { version = "0.29", features = ["metrics", "trace", "logs"] }
```

## Metrics

Metrics in ProSA are managed using the [OpenTelemetry Meter](https://docs.rs/opentelemetry/latest/opentelemetry/metrics/struct.Meter.html).
With the meter, you can declare counters, gauges, and more.

A meter is created from the [main task](https://docs.rs/prosa/latest/prosa/core/main/struct.Main.html#method.meter) or from [processors](https://docs.rs/prosa/latest/prosa/core/proc/struct.ProcParam.html#method.meter).
You create your metrics using this meter object.

```rust,noplayground
fn create_metrics<M>(proc_param: prosa::core::proc::ProcParam<M>)
where
  M: Sized + Clone + Tvf,
{
  // Get a meter to create your metrics
  let meter = proc_param.meter("prosa_metric_name");

  // Create a gauge for your metric
  let gauge_meter = meter
    .u64_gauge("prosa_gauge_metric_name")
    .with_description("Custom ProSA gauge metric")
    .build();

  // Record your value for the gauge with custom keys
  gauge_meter.record(
    42u64,
    &[
        KeyValue::new("prosa_name", "MyAwesomeProSA"),
        KeyValue::new("type", "custom"),
    ],
  );
}
```

If you want to create asynchronous metrics with regular updates, for example triggered by messages, you can do:
```rust,noplayground
fn create_async_metrics<M>(proc_param: prosa::core::proc::ProcParam<M>) -> tokio::sync::watch::Sender<u64>
where
  M: Sized + Clone + Tvf,
{
  // Get a meter to create your metrics
  let meter = proc_param.meter("prosa_async_metric_name");

  let (value, watch_value) = tokio::sync::watch::channel(0u64);
  let _observable_gauge = meter
    .u64_observable_gauge("prosa_gauge_async_metric_name")
    .with_description("Custom ProSA gauge async metric")
    .with_callback(move |observer| {
      let value = *watch_value.borrow();
      observer.observe(
        value,
        &[
          KeyValue::new("prosa_name", "MyAwesomeProSA"),
          KeyValue::new("type", "custom"),
        ],
      );

      // You can call `observe()` multiple time if you have metrics with different labels
    })
    .build();

  value
}

fn push_metric(metric_sender: tokio::sync::watch::Sender<u64>) {
  metric_sender.send(42);
  // Alternatively, use `send_modify()` if you need to modify the value in place
}
```

## Traces

If you package ProSA with cargo-prosa, traces are automatically configured.
If you want to set it up manually in your Observability setting, use the [`tracing_init()`](https://docs.rs/prosa-utils/latest/prosa_utils/config/observability/struct.Observability.html#method.tracing_init) method.

Once tracing is configured, you can use the [Tracing](https://docs.rs/tracing/latest/tracing/) crate anywhere in your code.
Tracing create spans and send them automatically to the configured [tracing endpoint](ch01-02-01-observability.html#opentelemetry) or to [stdout](ch01-02-01-observability.html#stdout).

Traces is also deeply integrated into ProSA's internal [messaging](https://docs.rs/prosa/latest/prosa/core/msg/trait.Msg.html).
ProSA messages (which inplement the `prosa::core::msg::Msg` trait) have an internal span that represents the flow of a message through ProSA services.

```rust,noplayground
fn process_prosa_msg<M>(msg: prosa::core::msg::Msg<M>)
where
  M: Sized + Clone + Tvf,
{
  // Enter the span: record the begin time
  // When it drop at the end of function, it end the span.
  let _enter = msg.enter_span();

  tracing::info!("Add an info with the message to the entered span: {msg:?}");

  let msg_span = msg.get_span();
  // You can also retrieve the span of the message if you want to link it to something else.
}
```

## Logs

With traces, standalone logs are often less useful, since events are better attached to spans, making it easier to know which transaction produced a given log message.

However, if you want to log messages, you can use the [log](https://docs.rs/log/latest/log/) crate.
Like tracing, logging is provisioned automatically with ProSA.

```rust,noplayground
log::info!("Generate an info log (will not be attached to a trace)");
```

If you configure traces, your logs will automatically be included as part of the traces.
This behavior is inherent to how OpenTelemetry tracing works.
