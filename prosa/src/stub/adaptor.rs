use super::proc::StubProc;
use crate::core::{adaptor::Adaptor, error::ProcError, proc::ProcConfig};
extern crate self as prosa;
use opentelemetry::metrics::Meter;

/// Adaptator trait for the stub processor
///
/// Need to define the process_request method to know what to do with incoming requests
/// ```
/// use prosa::stub::proc::StubProc;
/// use prosa::core::adaptor::Adaptor;
/// use prosa::stub::adaptor::StubAdaptor;
/// use prosa::core::error::ProcError;
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
///         + prosa_utils::msg::tvf::Tvf
///         + std::default::Default,
/// {
///     fn new(_proc: &StubProc<M>) -> Result<Self, Box<dyn ProcError + Send + Sync>> {
///         Ok(Self {})
///     }
///     fn process_request(&mut self, service_name: &str, request: &M) -> M {
///         let mut msg = request.clone();
///         msg.put_string(1, format!("test service {}", service_name));
///         msg
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
        + prosa_utils::msg::tvf::Tvf
        + std::default::Default,
{
    /// Method called when the processor spawns
    /// This method is called only once so the processing will be thread safe
    fn new(proc: &StubProc<M>) -> Result<Self, Box<dyn ProcError + Send + Sync>>
    where
        Self: Sized;
    /// Method to process incoming requests
    fn process_request(&mut self, service_name: &str, request: &M) -> M;
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
        + prosa_utils::msg::tvf::Tvf
        + std::default::Default,
{
    fn new(proc: &StubProc<M>) -> Result<Self, Box<dyn ProcError + Send + Sync>> {
        Ok(Self {
            meter: proc.get_proc_param().meter("stub_adaptor"),
        })
    }

    fn process_request(&mut self, _service_name: &str, request: &M) -> M {
        request.clone()
    }
}
