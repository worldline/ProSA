//!
//! <svg width="40" height="40">
#![doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/doc_assets/adaptor.svg"))]
//! </svg>
//!
//! This component is an interface between protocol objects and internal messages.
//! For a given protocol, multiple adaptor can be implemented.
//! External librairies can add adaptor to existing protocols.
//!
//! When a processor is create, it must have a single adaptor in its execution context.
//!
//! The adaptor will be executed in the same thread of the protocol.
//! So if several task in multiple thread are running, there will be concurency.
//!
//! An adaptor should be seen as a routine call to know what to do with a protocol message. How to convert it in internal message, and have an attach configuration to have routing rule.

use std::{fmt, pin::Pin};

/// Implement the trait [`Adaptor`].
pub use prosa_macros::Adaptor;

#[cfg_attr(doc, aquamarine::aquamarine)]
/// Generic ProSA Adaptor.
/// Define generic function call that are use by every processor.
///
/// ```mermaid
/// graph LR
///     task[Task]
///     adaptor[Adaptor]
///     bus([Internal service bus])
///     adaptor <--> bus
///     subgraph proc[ProSA Processor]
///     task <--> adaptor
///     end
/// ```
///
/// To implement the adaptor without init or terminate function you derive it by default:
/// ```
/// use prosa::core::adaptor::Adaptor;
///
/// #[derive(Adaptor)]
/// struct MyAdaptor {}
/// ```
///
/// If you have to use the message type in your adaptor
/// ```
/// use prosa::core::adaptor::Adaptor;
///
/// #[derive(Adaptor)]
/// struct MyAdaptor<M>
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
///     _phantom: std::marker::PhantomData<M>,
/// }
/// ```
pub trait Adaptor {
    /// Method call when the ProSA need to shut down.
    /// This method is call only once so the processing will be thread safe.
    fn terminate(&self);
}

/// An enum that can represent either an immediately available value
/// or a future that will produce the value asynchronously
/// Useful for adaptor to either return a value directly or a future that will resolve to the value later
///
/// ```
/// use prosa::core::adaptor::MaybeAsync;
/// use prosa::maybe_async;
///
/// fn sync_func() -> MaybeAsync<String> {
///     "Synchronous value".to_string().into()
/// }
///
/// fn sync_func_with_macro() -> MaybeAsync<String> {
///     maybe_async!("Synchronous value with macro".to_string())
/// }
///
/// fn async_func() -> MaybeAsync<String> {
///     MaybeAsync::Future(Box::pin(async { "Asynchronous value".to_string() }))
/// }
///
/// fn async_func_with_macro() -> MaybeAsync<String> {
///     maybe_async!(async {
///         let val = "Asynchronous value with macro".to_string();
///         val.to_string()
///     })
/// }
///
/// fn async_func_with_move_macro(val: String) -> MaybeAsync<String> {
///     maybe_async!(async move {
///         println!("Processing value: {}", val);
///         val.to_string()
///     })
/// }
///
/// fn process_maybe_async(maybe: MaybeAsync<String>) {
///     match maybe {
///         MaybeAsync::Ready(value) => println!("Got ready value: {}", value),
///         MaybeAsync::Future(future_value) => {
///             tokio::spawn(async move {
///                 let value = future_value.await;
///                 println!("Got future value: {}", value);
///             });
///         }
///     }
/// }
/// ```
pub enum MaybeAsync<T> {
    /// Value is immediately available (synchronous case)
    Ready(T),
    /// Value will be available through a future (asynchronous case)
    Future(Pin<Box<dyn Future<Output = T> + Send>>),
}

/// Implement `From<T>` for direct values (synchronous case)
impl<T> From<T> for MaybeAsync<T> {
    fn from(value: T) -> Self {
        MaybeAsync::Ready(value)
    }
}

/// Implement From for boxed trait object futures
impl<T> From<Pin<Box<dyn Future<Output = T> + Send>>> for MaybeAsync<T> {
    fn from(future: Pin<Box<dyn Future<Output = T> + Send>>) -> Self {
        MaybeAsync::Future(future)
    }
}

impl<T> fmt::Debug for MaybeAsync<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "MaybeAsync::{}",
            match self {
                MaybeAsync::Ready(_) => "Ready",
                MaybeAsync::Future(_) => "Future",
            }
        )
    }
}

/// Macro to make [`MaybeAsync`] creation more ergonomic
///
/// ```
/// use prosa::core::adaptor::MaybeAsync;
/// use prosa::maybe_async;
///
/// let sync_value = maybe_async!("Synchronous value".to_string());
/// let async_value = maybe_async!(async { "Asynchronous value".to_string() });
/// ```
#[macro_export]
macro_rules! maybe_async {
    // For async move blocks
    (async move $block:expr) => {
        MaybeAsync::Future(Box::pin(async move { $block }))
    };
    // For async blocks
    (async $block:expr) => {
        MaybeAsync::Future(Box::pin(async { $block }))
    };
    // For ready values
    ($expr:expr) => {
        MaybeAsync::Ready($expr)
    };
}

#[cfg(test)]
mod tests {
    extern crate self as prosa;

    use crate::core::adaptor::MaybeAsync;

    const MAYBE_VAL: &str = "value";

    fn test_ready(macro_def: bool) -> MaybeAsync<String> {
        if macro_def {
            maybe_async!(MAYBE_VAL.to_string())
        } else {
            MaybeAsync::Ready(MAYBE_VAL.to_string())
        }
    }

    fn test_future(macro_def: bool) -> MaybeAsync<String> {
        if macro_def {
            maybe_async!(async MAYBE_VAL.to_string())
        } else {
            MaybeAsync::Future(Box::pin(async { MAYBE_VAL.to_string() }))
        }
    }

    #[tokio::test]
    async fn test_maybe_async() {
        if let MaybeAsync::Ready(val) = test_ready(false) {
            assert_eq!(val, MAYBE_VAL);
        } else {
            panic!("Expected MaybeAsync::Ready, got something else");
        }

        if let MaybeAsync::Ready(val) = test_ready(true) {
            assert_eq!(val, MAYBE_VAL);
        } else {
            panic!("Expected MaybeAsync::Ready, got something else");
        }

        if let MaybeAsync::Future(val) = test_future(false) {
            assert_eq!(val.await, MAYBE_VAL);
        } else {
            panic!("Expected MaybeAsync::Future, got something else");
        }

        if let MaybeAsync::Future(val) = test_future(true) {
            assert_eq!(val.await, MAYBE_VAL);
        } else {
            panic!("Expected MaybeAsync::Future, got something else");
        }
    }
}
