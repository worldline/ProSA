# Troubleshooting Reference

## Configuration Validation

Always validate before deploying:

```bash
prosa_binary -c config.yaml --dry_run
```

This parses config, checks processor registration, and exits without starting. If the config file doesn't exist, it generates one with defaults.

## Common Pitfalls

### Processor never receives messages

**Cause**: Missing `add_proc()` call before the event loop.

```rust
// WRONG — no registration, processor is invisible to the bus
loop {
    if let Some(msg) = self.internal_rx_queue.recv().await { ... }
}

// CORRECT
self.proc.add_proc().await?;
self.proc.add_service_proc(self.settings.service_names.clone()).await?;
loop {
    if let Some(msg) = self.internal_rx_queue.recv().await { ... }
}
```

### Processor panics at runtime

**Cause**: `.unwrap()` on a `None` or `Err`. ProSA bans `unwrap()` via `clippy::unwrap_used = "deny"`.

Fix: use `?`, `if let`, `match`, or `.expect("reason")` only when truly infallible.

### Missing InternalMsg variants cause warnings

**Cause**: Non-exhaustive `match` on `InternalMsg`. CI treats warnings as errors (`RUSTFLAGS=-Dwarnings`).

```rust
// WRONG — missing variants
match msg {
    InternalMsg::Request(msg) => { ... }
    InternalMsg::Shutdown => { ... }
    _ => {}  // OK but may mask new variants
}

// CORRECT — handle all explicitly
match msg {
    InternalMsg::Request(msg) => { ... }
    InternalMsg::Response(msg) => { ... }
    InternalMsg::Error(err) => { ... }
    InternalMsg::Command(_) => {}
    InternalMsg::Config => {}
    InternalMsg::Service(table) => self.service = table,
    InternalMsg::Shutdown => {
        adaptor.terminate();
        self.proc.remove_proc(None).await?;
        return Ok(());
    }
}
```

### Shutdown hangs or resources leak

**Cause**: Wrong shutdown sequence.

```rust
// WRONG — remove_proc before terminate
self.proc.remove_proc(None).await?;
adaptor.terminate(); // too late, resources may leak

// CORRECT
adaptor.terminate();                    // clean up resources first
self.proc.remove_proc(None).await?;     // then deregister
return Ok(());
```

### Service not found despite being registered

**Cause**: Stale `ServiceTable`. Ensure you update on every `Service` message:

```rust
InternalMsg::Service(table) => {
    self.service = table;
}
```

### Responses never arrive

**Cause**: Sending requests with the wrong queue. Each subtask must use its own queue as the response channel:

```rust
let trans = RequestMsg::new(
    "SERVICE".to_string(),
    data,
    self.proc.get_service_queue(),  // THIS processor's queue
);
```

For subtasks, use the subtask's `tx_queue.clone()`, not the main processor's queue.

### Timeout errors with PendingMsgs

**Cause**: Missing `if !pending_msgs.is_empty()` guard on the `pull()` branch.

```rust
// WRONG — polls even when empty, wastes CPU
Some(msg) = pending_msgs.pull() => { ... }

// CORRECT
Some(msg) = pending_msgs.pull(), if !pending_msgs.is_empty() => { ... }
```

### `extern crate self as prosa` errors

- **Needed**: when writing processors inside the `prosa` crate itself (macros reference `prosa::` internally)
- **Not needed**: in external crates that depend on `prosa`

### Compilation fails with unused import warnings

CI uses `RUSTFLAGS=-Dwarnings`. Remove unused imports, dead code, or suppress with `#[allow()]` only when justified.

## Log Analysis

### Enable debug logging

```yaml
observability:
  level: debug
  traces:
    stdout:
      level: debug
```

### Key log patterns to watch

- `"Request received"` — confirms processor is handling traffic
- `"Error received"` — upstream service returned an error
- `"Failed to get request data"` — malformed TVF message
- Processor restart logs — check frequency and error type
- Service table updates — topology changes

### Correlating with traces

Enter message spans to attach logs to the transaction trace:

```rust
let _enter = msg.enter_span();
tracing::info!("Processing request for service {}", msg.get_service());
```

This lets you follow a single transaction across all processors in Grafana/Jaeger.
