# Adaptor Patterns Reference

Three adaptor patterns for ProSA processors, from simplest to most specialized.

## Pattern 1: Simple Adaptor

For processors that handle everything internally and don't need adaptor customization.

```rust
use prosa::core::adaptor::Adaptor;

#[derive(Default, Adaptor)]
pub struct MyAdaptor {}
```

The `#[derive(Adaptor)]` macro generates `impl Adaptor for MyAdaptor { fn terminate(&self) {} }`.

Use with `Default` trait when the processor's where clause requires `A: Default + Adaptor + Send + Sync`.

## Pattern 2: Processor-Specific Adaptor Trait

The most common production pattern. Define a trait for your processor, then users implement it to customize behavior.

### Defining the trait (in the processor crate)

```rust
use prosa::core::error::ProcError;
use prosa::core::msg::Tvf;

/// Adaptor trait for MyProc — handles protocol translation
pub trait MyAdaptorTrait<M>
where
    M: 'static
        + std::marker::Send
        + std::marker::Sync
        + std::marker::Sized
        + std::clone::Clone
        + std::fmt::Debug
        + Tvf
        + std::default::Default,
{
    /// Called once when the processor starts. Receives the processor
    /// for access to settings, metrics, and other configuration.
    fn new(proc: &MyProc<M>) -> Result<Self, Box<dyn ProcError + Send + Sync>>
    where
        Self: Sized;

    /// Transform an internal TVF request into an external protocol request
    fn process_internal_request(&self, service_name: &str, request: M) -> M;

    /// Transform an external protocol response into an internal TVF response
    fn process_external_response(&self, response: &[u8]) -> M;
}
```

### Implementing the trait (by the adaptor developer)

```rust
use prosa::core::adaptor::Adaptor;
use opentelemetry::metrics::Meter;

#[derive(Adaptor)]
pub struct MyDefaultAdaptor {
    meter: Meter,
}

impl<M> MyAdaptorTrait<M> for MyDefaultAdaptor
where
    M: 'static
        + std::marker::Send
        + std::marker::Sync
        + std::marker::Sized
        + std::clone::Clone
        + std::fmt::Debug
        + Tvf
        + std::default::Default,
{
    fn new(proc: &MyProc<M>) -> Result<Self, Box<dyn ProcError + Send + Sync>> {
        Ok(Self {
            meter: proc.get_proc_param().meter("my_adaptor"),
        })
    }

    fn process_internal_request(&self, _service_name: &str, request: M) -> M {
        // Transform TVF to external format
        request
    }

    fn process_external_response(&self, _response: &[u8]) -> M {
        M::default()
    }
}
```

### Using associated constants

Traits can include associated constants for configuration:

```rust
pub trait MyAdaptorTrait<M>
where
    M: /* bounds */,
{
    const USER_AGENT: &str;
    // ... methods
}

impl<M> MyAdaptorTrait<M> for MyDefaultAdaptor
where
    M: /* bounds */,
{
    const USER_AGENT: &str = "ProSA/1.0";
    // ... methods
}
```

### Using in the processor's where clause

```rust
#[proc]
impl<A> Proc<A> for MyProc
where
    A: Adaptor + MyAdaptorTrait<M> + std::marker::Send + std::marker::Sync,
{
    async fn internal_run(&mut self) -> Result<(), Box<dyn ProcError + Send + Sync>> {
        let adaptor = A::new(self)?;
        // ...
    }
}
```

## Pattern 3: Built-in StubAdaptor Trait

For creating adaptors for the built-in StubProc (service mocking).

```rust
use prosa::stub::proc::StubProc;
use prosa::stub::adaptor::StubAdaptor;
use prosa::core::adaptor::{Adaptor, MaybeAsync};
use prosa::core::error::ProcError;
use prosa::core::msg::Tvf;
use prosa::core::service::ServiceError;

#[derive(Adaptor)]
pub struct MyStubAdaptor {}

impl<M> StubAdaptor<M> for MyStubAdaptor
where
    M: 'static
        + std::marker::Send
        + std::marker::Sync
        + std::marker::Sized
        + std::clone::Clone
        + std::fmt::Debug
        + Tvf
        + std::default::Default,
{
    fn new(_proc: &StubProc<M>) -> Result<Self, Box<dyn ProcError + Send + Sync>> {
        Ok(Self {})
    }

    // Synchronous processing — return MaybeAsync::Ready via .into()
    fn process_request(&self, service_name: &str, request: M) -> MaybeAsync<Result<M, ServiceError>> {
        let mut msg = request.clone();
        msg.put_string(1, format!("response for {}", service_name));
        Ok(msg).into()
    }
}
```

### Async stub adaptor

Use `maybe_async!` macro for asynchronous request processing:

```rust
use prosa::maybe_async;

#[derive(Adaptor)]
pub struct MyAsyncStubAdaptor {}

impl<M> StubAdaptor<M> for MyAsyncStubAdaptor
where
    M: 'static + std::marker::Send + std::marker::Sync + std::marker::Sized
        + std::clone::Clone + std::fmt::Debug + Tvf + std::default::Default,
{
    fn new(_proc: &StubProc<M>) -> Result<Self, Box<dyn ProcError + Send + Sync>> {
        Ok(Self {})
    }

    fn process_request(&self, service_name: &str, request: M) -> MaybeAsync<Result<M, ServiceError>> {
        let service_name = service_name.to_string();
        maybe_async!(async move {
            // Async operations here (HTTP calls, DB queries, etc.)
            let mut msg = request.clone();
            msg.put_string(1, format!("async response for {}", service_name));
            Ok(msg)
        })
    }
}
```

## Pattern 4: Built-in InjAdaptor Trait

For creating adaptors for the built-in InjProc (transaction injection / load testing).

```rust
use prosa::inj::proc::InjProc;
use prosa::inj::adaptor::InjAdaptor;
use prosa::core::adaptor::Adaptor;
use prosa::core::error::ProcError;
use prosa::core::msg::Tvf;

#[derive(Adaptor)]
pub struct MyInjAdaptor {
    counter: u64,
}

impl<M> InjAdaptor<M> for MyInjAdaptor
where
    M: 'static
        + std::marker::Send
        + std::marker::Sync
        + std::marker::Sized
        + std::clone::Clone
        + std::fmt::Debug
        + Tvf
        + std::default::Default,
{
    fn new(_proc: &InjProc<M>) -> Result<Self, Box<dyn ProcError + Send + Sync>> {
        Ok(Self { counter: 0 })
    }

    /// Called each time the injector needs a new transaction to send
    fn build_transaction(&mut self) -> M {
        self.counter += 1;
        let mut msg = M::default();
        msg.put_unsigned(1, self.counter);
        msg.put_string(2, format!("transaction_{}", self.counter));
        msg
    }

    /// Called when a response is received (optional, default ignores responses)
    fn process_response(
        &mut self,
        _response: M,
        _service_name: &str,
    ) -> Result<(), Box<dyn ProcError + Send + Sync>> {
        // Validate response, update counters, etc.
        Ok(())
    }
}
```
