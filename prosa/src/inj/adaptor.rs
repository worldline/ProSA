use std::error::Error;

use crate::core::adaptor::Adaptor;

use super::proc::InjProc;

extern crate self as prosa;

/// Adaptator trait for the inj processor
///
/// Need to define the build_transaction method to build transaction evy time it need to be send
/// ```
/// use prosa::inj::proc::InjProc;
/// use prosa::core::adaptor::Adaptor;
/// use prosa::inj::adaptor::InjAdaptor;
///
/// #[derive(Default, Adaptor)]
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
///         + prosa_utils::msg::tvf::Tvf
///         + std::default::Default,
/// {
///     fn init(&mut self, _proc: &InjProc<M>) -> Result<(), Box<dyn std::error::Error>> {
///         Ok(())
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
        + prosa_utils::msg::tvf::Tvf
        + std::default::Default,
{
    /// Method called when the processor spawns
    /// This method is called only once so the processing will be thread safe
    fn init(&mut self, proc: &InjProc<M>) -> Result<(), Box<dyn Error>>;
    /// Method to build a transaction to inject
    fn build_transaction(&mut self) -> M;
    /// Method to process transaction response of the injection (to check the return code for example)
    /// if an error is trigger, the injection and the processor will stop
    /// By default response are ignored
    fn process_response(
        &mut self,
        _response: &M,
        _service_name: &str,
    ) -> Result<(), Box<dyn Error>> {
        Ok(())
    }
}

/// Dummy adaptor for the inj processor. Use to send a very basic message with _DUMMY_ in it.
#[derive(Default, Adaptor)]
pub struct InjDummyAdaptor {}

impl<M> InjAdaptor<M> for InjDummyAdaptor
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
    fn init(&mut self, _proc: &InjProc<M>) -> Result<(), Box<dyn Error>> {
        Ok(())
    }

    fn build_transaction(&mut self) -> M {
        let mut msg = M::default();
        msg.put_string(1, "DUMMY");
        msg
    }
}
