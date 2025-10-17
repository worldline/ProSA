use super::proc::StubProc;
use crate::{
    core::{
        adaptor::{Adaptor, MaybeAsync},
        error::ProcError,
        msg::Tvf,
        proc::ProcConfig,
        service::ServiceError,
    },
    maybe_async,
};
extern crate self as prosa;
use opentelemetry::metrics::Meter;

/// Adaptator trait for the stub processor
///
/// Need to define the process_request method to know what to do with incoming requests
/// ```
/// use prosa::stub::proc::StubProc;
/// use prosa::core::adaptor::{Adaptor, MaybeAsync};
/// use prosa::stub::adaptor::StubAdaptor;
/// use prosa::core::error::ProcError;
/// use prosa::core::msg::Tvf;
/// use prosa::core::service::ServiceError;
///
/// #[derive(Adaptor)]
/// pub struct MyStubAdaptor { }
///
/// impl<M> StubAdaptor<M> for MyStubAdaptor
/// where
///     M: 'static
///         + std::marker::Send
///         + std::marker::Sync
///         + std::marker::Sized
///         + std::clone::Clone
///         + std::fmt::Debug
///         + Tvf
///         + std::default::Default,
/// {
///     fn new(_proc: &StubProc<M>) -> Result<Self, Box<dyn ProcError + Send + Sync>> {
///         Ok(Self {})
///     }
///
///     fn process_request(&self, service_name: &str, request: M) -> MaybeAsync<Result<M, ServiceError>> {
///         let mut msg = request.clone();
///         msg.put_string(1, format!("test service {}", service_name));
///         Ok(msg).into()
///     }
/// }
/// ```
///
/// You also have the possibility to do an async request processing for your stub adaptor:
/// ```
/// use prosa::stub::proc::StubProc;
/// use prosa::core::adaptor::{Adaptor, MaybeAsync};
/// use prosa::stub::adaptor::StubAdaptor;
/// use prosa::core::error::ProcError;
/// use prosa::core::msg::Tvf;
/// use prosa::core::service::ServiceError;
/// use prosa::maybe_async;
///
/// #[derive(Adaptor)]
/// pub struct MyAsyncStubAdaptor { }
///
/// impl<M> StubAdaptor<M> for MyAsyncStubAdaptor
/// where
///     M: 'static
///         + std::marker::Send
///         + std::marker::Sync
///         + std::marker::Sized
///         + std::clone::Clone
///         + std::fmt::Debug
///         + Tvf
///         + std::default::Default,
/// {
///     fn new(_proc: &StubProc<M>) -> Result<Self, Box<dyn ProcError + Send + Sync>> {
///         Ok(Self {})
///     }
///
///     fn process_request(&self, service_name: &str, request: M) -> MaybeAsync<Result<M, ServiceError>> {
///         let service_name = service_name.to_string();
///         maybe_async!(async move {
///             // You can do async things here
///             let mut msg = request.clone();
///             msg.put_string(1, format!("test service {}", service_name));
///             Ok(msg)
///         })
///     }
/// }
/// ```
pub trait StubAdaptor<M>
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
    /// Method called when the processor spawns
    /// This method is called only once so the processing will be thread safe
    fn new(proc: &StubProc<M>) -> Result<Self, Box<dyn ProcError + Send + Sync>>
    where
        Self: Sized;

    /// Method to process incoming requests
    fn process_request(
        &self,
        service_name: &str,
        request: M,
    ) -> MaybeAsync<Result<M, ServiceError>>;
}

/// Parot adaptor for the stub processor. Use to respond to a request with the same message
#[derive(Adaptor)]
pub struct StubParotAdaptor {
    #[allow(unused)]
    meter: Meter,
}

impl<M> StubAdaptor<M> for StubParotAdaptor
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
    fn new(proc: &StubProc<M>) -> Result<Self, Box<dyn ProcError + Send + Sync>> {
        Ok(Self {
            meter: proc.get_proc_param().meter("stub_adaptor"),
        })
    }

    fn process_request(
        &self,
        _service_name: &str,
        request: M,
    ) -> MaybeAsync<Result<M, ServiceError>> {
        Ok(request.clone()).into()
    }
}

/// Parot adaptor for the stub processor. Use to respond to a request with the same message
#[derive(Adaptor)]
pub struct StubAsyncParotAdaptor {}

impl<M> StubAdaptor<M> for StubAsyncParotAdaptor
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

    fn process_request(
        &self,
        _service_name: &str,
        request: M,
    ) -> MaybeAsync<Result<M, ServiceError>> {
        maybe_async!(async move {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            Ok(request)
        })
    }
}
