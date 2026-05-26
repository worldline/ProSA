# Error Recovery Reference

## ProcError Trait

Every processor must define a custom error type implementing `ProcError`:

```rust
pub trait ProcError: std::error::Error {
    fn recoverable(&self) -> bool;
    fn recovery_duration(&self) -> Duration;
}
```

- `recoverable()` — if `true`, the processor is automatically restarted; if `false`, it crashes
- `recovery_duration()` — initial wait before restart attempt

## Automatic Restart Logic

When `internal_run()` returns an error:

1. Check `error.recoverable()` — if `false`, processor status = **crashed** (red), no restart
2. If recoverable, wait `recovery_duration()` then restart `internal_run()`
3. On each subsequent error, the wait increases (backoff)
4. Wait is capped at `proc_max_restart_period`

### Configuration

```yaml
my_proc:
  proc_restart_duration_period: 50    # milliseconds between restart attempts (default: 50ms)
  proc_max_restart_period: 300        # max backoff multiplier (default: 300)
```

These fields are auto-added by the `#[proc_settings]` macro.

## Error Classification Guidelines

| Error type | Recoverable? | Rationale |
|-----------|-------------|-----------|
| I/O errors (connection reset, broken pipe) | Yes | Transient network issues |
| Timeout errors | Yes | Temporary load or latency |
| Protocol errors (remote API) | Yes | Remote service may recover |
| SSL/TLS negotiation errors | Yes | Certificate or handshake may be retried |
| Configuration errors | **No** | Invalid config won't fix itself |
| Serialization errors | **No** | Data format issues are permanent |
| Channel send errors (`SendError`) | Yes | Queue congestion may clear |
| Tokio `JoinError` (cancelled) | Yes | Task may have been preempted |

## ServiceError

The error type for service-level responses between processors:

```rust
pub enum ServiceError {
    UnableToReachService(String),  // Service down, stop sending, do service test
    Timeout(String, u64),          // Processing too slow, propagate to caller
    ProtocolError(String),         // API version mismatch, check configuration
}
```

### Converting to ServiceError

```rust
impl From<MyProcError> for ServiceError {
    fn from(e: MyProcError) -> Self {
        match e {
            MyProcError::Io(e) => ServiceError::UnableToReachService(e.to_string()),
            MyProcError::Protocol(e) => ServiceError::ProtocolError(e),
            MyProcError::Timeout(s, ms) => ServiceError::Timeout(s, ms),
        }
    }
}
```

## Error Propagation Pattern

When a processor cannot handle a request, return the error to the sender:

```rust
InternalMsg::Request(mut msg) => {
    match process(&msg) {
        Ok(response) => {
            let _ = msg.return_result_to_sender(response);
        }
        Err(e) => {
            let _ = msg.return_error_to_sender(
                None,
                ServiceError::from(e),
            ).await;
        }
    }
}
```

## Built-in Recoverable Types

ProSA already implements `ProcError` for common error types:

- `std::io::Error` — recoverable for transient I/O kinds (ConnectionReset, BrokenPipe, TimedOut, etc.)
- `tokio::sync::mpsc::error::SendError<InternalMsg<M>>` — recoverable (queue pressure)
- OpenSSL `ssl::Error` — recoverable (protocol-level)
- `tokio::task::JoinError` — recoverable if cancelled
- `ConfigError` — **not recoverable**
- OpenSSL `ErrorStack` — **not recoverable**
