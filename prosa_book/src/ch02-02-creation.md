# Adaptor creation

Adaptors strongly depend on the underlying processor implementation.
Their structure varies according to the choices of the processor developer.
Sometimes you will have a wide latitude for customization, while other times the processor will need to be restrictive, especially regarding secrets or security considerations.

However, we'll describe good practices for adaptor design to help you understand concepts that you'll encounter most of the time.

## Instantiation

Processor uses Adaptors to adapt messages, so you typically need a single Adaptor instance to perform this role.
This adaptor instance must be both `Send` and `Sync`.

```rust,noplayground
pub trait MyTraitAdaptor<M>
where
    M: 'static
        + std::marker::Send
        + std::marker::Sync
        + std::marker::Sized
        + std::clone::Clone
        + std::fmt::Debug
        + prosa_utils::msg::tvf::Tvf
        + std::default::Default,
{
    /// Method called when the processor spawns
    /// This method is called only once, so the processing will be thread safe
    fn new(proc: &MyProc<M>) -> Result<Self, Box<dyn ProcError + Send + Sync>>
    where
        Self: Sized;
}
```

Most of the time, the processor is provided as a parameter to the adaptor's constructor, allowing you to retrieve all necessary information (e.g., settings, name, etc.).

It's preferable to provide a `new()` method to create your adaptor, rather relying on [`default()`](https://doc.rust-lang.org/std/default/trait.Default.html), because `new()` gives you access to processor settings and other information.
Additionally, `new()` can fail with a `ProcError` or a dedicated error type if the processor cannot start.

## Processing

When you use or develop an adaptor, you need to consider that you may have to process both internal TVF request/response messages, as well as message objects intended for external systems.

To summarize, here’s a graph with all typical interactions:
```mermaid
flowchart LR
    internal[ProSA internal]
    adaptor[Adaptor / Processor]
    external[External system]
    internal-- request (TVF) -->adaptor
    adaptor-- response (TVF) -->internal
    adaptor-- request (protocol) -->external
    external-- response(protocol) -->adaptor
```

In this architecture, if your adaptor needs to send external requests originating from internal messages, it may look like this:
```rust,noplayground
# pub struct ExternalObjectRequest {}
# pub struct ExternalObjectResponse {}
#
pub trait MyTraitAdaptor<M>
where
    M: 'static
        + std::marker::Send
        + std::marker::Sync
        + std::marker::Sized
        + std::clone::Clone
        + std::fmt::Debug
        + prosa_utils::msg::tvf::Tvf
        + std::default::Default,
{
    /// Method to process incomming requests from internal
    fn process_internal_request(&mut self, request: &M) -> ExternalObjectRequest;

    /// Method to process outgoing requests to external system
    fn process_external_response(&mut self, response: &ExternalObjectResponse) -> M;
}
```

Conversely, if your adaptor needs to handle incoming external requests and provide corresponding internal responses, it may take this shape:
```rust,noplayground
# pub struct ExternalObjectRequest {}
# pub struct ExternalObjectResponse {}
#
pub trait MyTraitAdaptor<M>
where
    M: 'static
        + std::marker::Send
        + std::marker::Sync
        + std::marker::Sized
        + std::clone::Clone
        + std::fmt::Debug
        + prosa_utils::msg::tvf::Tvf
        + std::default::Default,
{
    /// Method to process incomming requests from external system
    fn process_external_request(&mut self, request: &ExternalObjectRequest) -> M;

    /// Method to process outgoing requests to internal
    fn process_internal_response(&mut self, response: &M) -> ExternalObjectResponse;
}
```

## Additional features

You can leverage Rust traits to enhence the adaptor specification.
For example, you can use associated `const` values in traits, such as setting a [user agent](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/User-Agent).

```rust,noplayground
pub trait MyTraitAdaptor
{
    const USER_AGENT: &str;
}

impl<M> StubAdaptor<M> for StubParotAdaptor
where
    M: 'static
        + std::marker::Send
        + std::marker::Sync
        + std::marker::Sized
        + std::clone::Clone
        + std::fmt::Debug
        + prosa_utils::msg::tvf::Tvf
        + std::default::Default,
{
    const USER_AGENT: &str = "ProSA user agent";
}
```
