use super::proc::InjProc;
use crate::core::{adaptor::Adaptor, error::ProcError, msg::Tvf};
extern crate self as prosa;

/// Adaptator trait for the inj processor
///
/// Need to define the build_transaction method to build transaction evy time it need to be send
/// ```
/// use prosa::inj::proc::InjProc;
/// use prosa::core::adaptor::Adaptor;
/// use prosa::inj::adaptor::InjAdaptor;
/// use prosa::core::error::ProcError;
/// use prosa::core::msg::Tvf;
///
/// #[derive(Adaptor)]
/// pub struct MyInjAdaptor { }
///
/// impl<M> InjAdaptor<M> for MyInjAdaptor
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
///     fn new(_proc: &InjProc<M>) -> Result<Self, Box<dyn ProcError + Send + Sync>> {
///         Ok(Self {})
///     }
///     fn build_transaction(&mut self) -> M {
///         let mut msg = M::default();
///         msg.put_string(1, format!("transaction"));
///         msg
///     }
/// }
/// ```
pub trait InjAdaptor<M>
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
    fn new(proc: &InjProc<M>) -> Result<Self, Box<dyn ProcError + Send + Sync>>
    where
        Self: Sized;
    /// Method to build a transaction to inject
    fn build_transaction(&mut self) -> M;
    /// Method to process transaction response of the injection (to check the return code for example)
    /// if an error (other than a timeout or closed service) is trigger, the injection and the processor will stop
    /// By default response are ignored
    fn process_response(
        &mut self,
        _response: M,
        _service_name: &str,
    ) -> Result<(), Box<dyn ProcError + Send + Sync>> {
        Ok(())
    }
}

/// Dummy adaptor for the inj processor. Use to send a very basic message with _DUMMY_ in it.
#[derive(Adaptor)]
pub struct InjDummyAdaptor {}

impl<M> InjAdaptor<M> for InjDummyAdaptor
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
        Ok(Self {})
    }

    fn build_transaction(&mut self) -> M {
        let mut msg = M::default();
        msg.put_string(1, "DUMMY");
        msg
    }
}
