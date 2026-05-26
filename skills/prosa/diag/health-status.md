# Health & Status Reference

## Processor States

ProSA tracks processor health via metrics, visible in the Grafana node graph:

| State | Color | Meaning |
|-------|-------|---------|
| Running | Green | Processor is healthy and processing messages |
| Restarted | Orange | Processor experienced at least one restart (recovered from error) |
| Stopped | Grey | Processor has been cleanly shut down |
| Crashed | Red | Processor terminated with a non-recoverable error |

An orange state is not necessarily a problem — it means recovery worked. Monitor the restart frequency: frequent restarts indicate a persistent transient issue.

## Service Availability

Services are the routing layer between processors:

| State | Color | Meaning |
|-------|-------|---------|
| Available | Green | At least one processor exposes this service |
| Unavailable | Grey | No processor is serving this service |

### Checking availability in code

```rust
// Check if a service exists
if self.service.exist_proc_service("SERVICE_NAME") {
    // Service has at least one processor
}

// Get the next processor for a service (round-robin)
if let Some(service) = self.service.get_proc_service("SERVICE_NAME") {
    // Send a request via service.proc_queue
}
```

### Service updates

Processors receive `InternalMsg::Service(table)` whenever the service topology changes (processor added/removed, service registered/unregistered). Always update your local copy:

```rust
InternalMsg::Service(table) => {
    self.service = table;
}
```

## Service Routing

`ServiceTable` uses round-robin load balancing with an atomic counter:

- Single processor on a service: direct routing
- Multiple processors: `fetch_add(1, Relaxed) % count` selects the next one
- Routing is lock-free and low-overhead

## Built-in Metrics

ProSA exposes metrics for monitoring without custom instrumentation:

- **Processor state** — running/restarted/stopped/crashed per processor
- **Service links** — which processors expose which services
- **System metrics** (with `system-metrics` feature):
  - `virtual` — virtual RAM usage
  - `physical` — physical RAM usage

## Monitoring Setup

1. Enable Prometheus or OTLP in observability config
2. Import the ProSA Grafana dashboard (node graph view)
3. View:
   - Processor health (color-coded nodes)
   - Service availability (color-coded nodes)
   - Links between processors and services
   - Transaction traces across the service bus

## Version Inspection

Use `--version` (long form) to verify all compiled components:

```bash
$ prosa_binary --version
prosa 0.1.0 - core::main::MainProc = { crate = prosa, version = 0.2.0 }
  stub
    Processor: stub::proc::StubProc = { crate = prosa, version = 0.2.0 }
    Adaptor  : stub::adaptor::StubParotAdaptor = { crate = prosa, version = 0.2.0 }
```

Useful for diagnosing version mismatches between processor crates.
