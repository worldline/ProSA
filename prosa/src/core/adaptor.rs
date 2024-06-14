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
/// #[derive(Default, Adaptor)]
/// struct MyAdaptor {}
/// ```
pub trait Adaptor<T: Default = Self> {
    /// Method call when the ProSA need to shut down.
    /// This method is call only once so the processing will be thread safe.
    fn terminate(&mut self);
}
