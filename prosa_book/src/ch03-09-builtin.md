# Built-in Processors

ProSA ships with two built-in processors designed for testing and development: **InjProc** (Injector) and **StubProc** (Stub). Together, they let you validate your ProSA setup without writing any custom processor code.

## InjProc — Injector

The injector processor sends transactions to a target service at a regulated speed. It is useful for **load testing** and **functional validation**.

### How it works

1. Waits for the target service to appear in the service table
2. Calls the adaptor's `build_transaction()` to create a message
3. Sends the message to the target service at a controlled rate
4. Receives responses and calls the adaptor's `process_response()`
5. Handles errors: timeouts and unreachable services trigger a cooldown; other errors stop the processor

### Settings

Configure the injector through `InjSettings` in your YAML:

```yaml
inj-1:
  service_name: "MY_SERVICE"  # Target service name to inject into
  max_speed: 5.0              # Maximum transactions per second (default: 5.0)
  timeout_threshold:
    secs: 10                  # Cooldown duration on timeout/unreachable (default: 10s)
    nanos: 0
  max_concurrents_send: 1     # Max parallel transactions (default: 1)
  speed_interval: 15          # Number of samples for speed calculation (default: 15)
```

### InjAdaptor Trait

To customize what the injector sends and how it processes responses, implement the `InjAdaptor` trait:

```rust,noplayground
pub trait InjAdaptor<M>
where
    M: Tvf + Clone + Debug + Default + Send + Sync + 'static,
{
    /// Called once when the processor starts
    fn new(proc: &InjProc<M>) -> Result<Self, Box<dyn ProcError + Send + Sync>>
    where
        Self: Sized;

    /// Build the next transaction to inject
    fn build_transaction(&mut self) -> M;

    /// Process the response (optional — default ignores responses)
    fn process_response(
        &mut self,
        response: M,
        service_name: &str,
    ) -> Result<(), Box<dyn ProcError + Send + Sync>> {
        Ok(())
    }
}
```

### InjDummyAdaptor

The built-in `InjDummyAdaptor` creates minimal messages with `"DUMMY"` in field 1. It is useful for quick smoke tests:

```rust,noplayground
#[derive(Adaptor)]
pub struct InjDummyAdaptor {}

impl<M> InjAdaptor<M> for InjDummyAdaptor
where
    M: Tvf + Clone + Debug + Default + Send + Sync + 'static,
{
    fn new(_proc: &InjProc<M>) -> Result<Self, Box<dyn ProcError + Send + Sync>> {
        Ok(Self {})
    }

    fn build_transaction(&mut self) -> M {
        let mut msg = M::default();
        msg.put_string(1, "DUMMY");
        msg
    }
}
```

### Metrics

The injector exposes a histogram metric `prosa_inj_request_duration` (in seconds) with attributes:
- `proc`: processor name
- `service`: target service name
- `err_code`: `"0"` for success, or the service error code

---

## StubProc — Stub

The stub processor responds to incoming requests. It is useful for **mocking services** during development or testing.

### How it works

1. Registers for the configured service names
2. Receives incoming `Request` messages
3. Calls the adaptor's `process_request()` to produce a response
4. Returns the response to the caller

### Settings

Configure the stub through `StubSettings` in your YAML:

```yaml
stub-1:
  service_names:
    - "MY_SERVICE"
    - "ANOTHER_SERVICE"
```

### StubAdaptor Trait

To customize how the stub responds, implement the `StubAdaptor` trait:

```rust,noplayground
pub trait StubAdaptor<M>
where
    M: Tvf + Clone + Debug + Default + Send + Sync + 'static,
{
    /// Called once when the processor starts
    fn new(proc: &StubProc<M>) -> Result<Self, Box<dyn ProcError + Send + Sync>>
    where
        Self: Sized;

    /// Process an incoming request and return a response
    fn process_request(
        &self,
        service_name: &str,
        request: M,
    ) -> MaybeAsync<Result<M, ServiceError>>;
}
```

The return type `MaybeAsync` lets you choose between synchronous and asynchronous processing.

### Synchronous response

Return a value directly using `.into()`:

```rust,noplayground
fn process_request(&self, service_name: &str, request: M) -> MaybeAsync<Result<M, ServiceError>> {
    // Echo the request back
    Ok(request).into()
}
```

### Asynchronous response

Use the `maybe_async!` macro for async processing:

```rust,noplayground
fn process_request(&self, service_name: &str, request: M) -> MaybeAsync<Result<M, ServiceError>> {
    let service_name = service_name.to_string();
    maybe_async!(async move {
        // Simulate async work (e.g., call an external API)
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let mut msg = request.clone();
        msg.put_string(1, format!("response for {}", service_name));
        Ok(msg)
    })
}
```

### Built-in Adaptors

**StubParotAdaptor** — echoes the request back as-is (synchronous):

```rust,noplayground
fn process_request(&self, _service_name: &str, request: M) -> MaybeAsync<Result<M, ServiceError>> {
    Ok(request.clone()).into()
}
```

**StubAsyncParotAdaptor** — echoes the request back after a 100ms delay (asynchronous):

```rust,noplayground
fn process_request(&self, _service_name: &str, request: M) -> MaybeAsync<Result<M, ServiceError>> {
    maybe_async!(async move {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        Ok(request)
    })
}
```

---

## Using Injector and Stub Together

The injector and stub pair is ideal for validating your ProSA setup. Wire them together by pointing the injector's `service_name` at a service declared by the stub:

```yaml
# Stub responds to "TEST_SERVICE"
stub-1:
  service_names:
    - "TEST_SERVICE"

# Injector sends to "TEST_SERVICE"
inj-1:
  service_name: "TEST_SERVICE"
  max_speed: 10.0
```

When you run ProSA with this configuration, the injector will continuously send messages to the stub, which echoes them back. You can observe the transaction flow through metrics and traces.
