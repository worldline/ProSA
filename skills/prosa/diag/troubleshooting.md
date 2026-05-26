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

---

## Runtime Diagnostics

### Requests go nowhere / service not found

- [ ] Processor called `add_proc()` before the event loop
- [ ] Processor called `add_service_proc()` with correct service names
- [ ] `InternalMsg::Service(table)` handler updates `self.service = table`
- [ ] Service name matches exactly between sender and listener (case-sensitive)
- [ ] If startup race: listener may not be registered yet when sender starts — add retry or wait for `Service` table update

### Timeout floods (PendingMsgs expiring constantly)

- [ ] Remote service responding within the timeout duration passed to `push()`
- [ ] `Regulator` configured — check `max_concurrents_send` not flooding remote
- [ ] `pending_msgs.pull()` branch has `if !pending_msgs.is_empty()` guard
- [ ] Check if remote is under load (network latency, slow processing)
- [ ] Verify timeout duration is realistic for the operation

### Processor crash/restart loop

- [ ] Check which error type triggers the crash — `ProcError::recoverable()` returning `true` on a permanent error?
- [ ] Configuration errors should return `recoverable = false`
- [ ] Check `proc_restart_duration_period` / `proc_max_restart_period` values
- [ ] Check `adaptor.terminate()` for resource leaks (open connections, file handles)
- [ ] Look at restart logs to identify the repeating error

### Shutdown hangs

- [ ] `InternalMsg::Shutdown` arm exists in the main `match`
- [ ] `adaptor.terminate()` called **before** `remove_proc()` (not after)
- [ ] `adaptor.terminate()` does not block indefinitely (no infinite loops, no blocking I/O)
- [ ] Subtasks also handle `Shutdown` and exit their loops
- [ ] No pending I/O operations blocking the Tokio runtime

### UnableToReachService errors

- [ ] Target service is registered and running (check processor state)
- [ ] Service table is up to date (`Service(table)` handler exists)
- [ ] Target processor has not crashed (check restart/crash state)
- [ ] No processor ID collision (each processor needs a unique ID)

### ProtocolError responses

- [ ] Check adaptor's protocol parsing — is the remote API format expected?
- [ ] Version mismatch between sender and listener adaptors
- [ ] TVF message structure matches what the target expects
- [ ] Check remote service logs for rejected requests

---

## Configuration Diagnostics

### Config parse failure

- [ ] YAML indentation correct (spaces, not tabs)
- [ ] Field names match the `#[proc_settings]` struct fields exactly
- [ ] Instance name in code uses dashes (`stub-1`) but YAML section uses underscores (`stub_1:`)
- [ ] `Default` impl has `#[proc_settings]` attribute (not just the struct)
- [ ] Run `--dry_run` to get detailed parse error messages

### Observability not working

- [ ] OTLP collector reachable at configured endpoint
- [ ] Correct protocol: `grpc://` for gRPC, `http://` for HTTP
- [ ] Prometheus endpoint not already bound by another process
- [ ] `prometheus` feature enabled in Cargo.toml if using Prometheus
- [ ] If both `traces` and `logs` configured, only `traces` applies (logs are embedded in traces)
- [ ] Check `level` filter — `error` will suppress most output
- [ ] Grafana Cloud: verify base64 token encoding (`=` → `%3D` in URL)

### SSL/TLS failures

- [ ] Certificate file path is correct and readable
- [ ] `openssl` or `openssl-vendored` feature enabled
- [ ] Certificate not expired (check with `openssl x509 -enddate -noout -in cert.pem`)
- [ ] ALPN protocols match between client and server
- [ ] For mTLS: both store (CA) and cert/key configured on both sides
- [ ] Self-signed: ensure the CA is in the trust store
- [ ] `modern_security: true` may reject older TLS versions — check compatibility

### Stream binding / connection errors

- [ ] Address already in use → another process on the same port, or previous instance not fully shut down
- [ ] Connection refused → target not listening, wrong port, firewall
- [ ] Proxy errors → `http-proxy` feature enabled, proxy URL correct, proxy reachable
- [ ] UNIX socket → path exists, permissions correct

---

## Compilation Diagnostics

### Trait bound errors on generic `M`

If you see errors about missing trait bounds on `M`, the full bound is:

```rust
where M: 'static + Send + Sync + Sized + Clone + std::fmt::Debug + Tvf + Default
```

Ensure your processor's `impl` block includes these bounds, or use the `#[proc]` macro which adds them automatically.

### Macro errors on processor struct

- `#[proc]` must be on both the struct and the `impl` block
- No custom fields in the `#[proc(settings = ...)]` struct — use local variables in `internal_run()`
- `#[proc_settings]` must be on both the struct and the `Default` impl

### Wrong adaptor trait

Each processor type expects a specific adaptor trait:

| Processor | Required adaptor trait |
|-----------|----------------------|
| Custom processor | Your own trait + `#[derive(Adaptor)]` on struct |
| `StubProc` | `StubAdaptor<M>` |
| `InjProc` | `InjAdaptor<M>` |

Mismatch causes "trait bound not satisfied" errors at compile time.

### MaybeAsync return type errors

`StubAdaptor::process_request()` returns `MaybeAsync<Result<M, ServiceError>>`:

```rust
// Synchronous — return the result directly
fn process_request(&self, service: &str, data: M) -> MaybeAsync<Result<M, ServiceError>> {
    Ok(data).into()
}

// Asynchronous — return a boxed future
fn process_request(&self, service: &str, data: M) -> MaybeAsync<Result<M, ServiceError>> {
    Box::pin(async move {
        Ok(data)
    }).into()
}
```

---

## Architecture Mistakes

### Blocking the async runtime

**Symptom**: processor becomes unresponsive, timeouts spike, other tasks on the same runtime freeze.

**Cause**: synchronous blocking call (file I/O, DNS, heavy computation) in an `async` context.

**Fix**: use `tokio::task::spawn_blocking()` for blocking operations, or use async alternatives (`tokio::fs`, `tokio::net`).

### Processor ID collision

**Symptom**: messages routed to wrong processor, unexpected behavior, service table inconsistencies.

**Cause**: two processors created with the same ID (the `u32` passed to `create()` / `create_raw()`).

**Fix**: ensure every processor instance has a unique ID. `cargo-prosa` handles this automatically; manual `main.rs` must assign IDs carefully.
