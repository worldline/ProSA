use std::error::Error;

use crate::core::{adaptor::Adaptor, proc::ProcConfig};

use super::proc::StubProc;

extern crate self as prosa;

use opentelemetry::metrics::{Meter, MeterProvider as _};
use opentelemetry_sdk::metrics::MeterProvider;

/// Adaptator trait for the stub processor
///
/// Need to define the process_request method to know what to do with incomming requests
/// ```
/// use prosa::stub::proc::StubProc;
/// use prosa::core::adaptor::Adaptor;
/// use prosa::stub::adaptor::StubAdaptor;
///
/// #[derive(Default, Adaptor)]
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
///     fn init(&mut self, _proc: &StubProc<M>) -> Result<(), Box<dyn std::error::Error>> {
///         Ok(())
///     }
///     fn process_request(&self, service_name: &str, request: &M) -> M {
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
    fn init(&mut self, proc: &StubProc<M>) -> Result<(), Box<dyn Error>>;
    /// Method to process incomming requests
    fn process_request(&self, service_name: &str, request: &M) -> M;
}

/// Parot adaptor for the stub processor. Use to respond to a request with the same message
#[derive(Adaptor)]
pub struct StubParotAdaptor {
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
    fn init(&mut self, proc: &StubProc<M>) -> Result<(), Box<dyn Error>> {
        self.meter = proc.get_proc_param().meter("stub_adaptor");
        Ok(())
    }

    fn process_request(&self, _service_name: &str, request: &M) -> M {
        request.clone()
    }
}

impl Default for StubParotAdaptor {
    fn default() -> Self {
        Self {
            meter: MeterProvider::default().meter("prosa_parot"),
        }
    }
}
