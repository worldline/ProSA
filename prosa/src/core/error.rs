use super::msg::InternalMsg;
use prosa_utils::msg::tvf::{Tvf, TvfError};
use tokio::sync::mpsc;

/// Processor Error
#[derive(thiserror::Error, Debug)]
pub enum ProcError {
    /// Error encountered when reading the request
    #[error("Could not read the request {0}")]
    Bus(#[from] BusError),

    /// Error encountered when processing the request
    #[error("Could not process the request {0}")]
    Send(#[from] SendError),

    /// Error raised when creating the adaptor to process the request
    #[error("Failed to create the adaptor: {0}")]
    NewAdaptor(#[from] NewAdaptorError),

    /// Error raised by the adaptor when treating the message
    #[error("Could not adapt the message: {0}")]
    Adapt(#[from] AdaptError),
}

/// Error define for ProSA bus error (for message exchange)
#[derive(Debug, Eq, thiserror::Error, PartialEq)]
pub enum BusError {
    /// Error that indicate the queue can forward the internal main message
    #[error("The Queue can't send the internal main message {0}, proc_id={1}, reason={2}")]
    InternalMainQueue(String, u32, String),

    /// Error that indicate the queue can forward the internal message
    #[error("The Queue can't send the internal message: {0}")]
    InternalQueue(String),

    /// Error that indicate the queue can forward the internal message
    #[error("The Processor {0}/{1} can't be contacted: {2}")]
    ProcComm(u32, u32, String),

    /// Error on the internal TVF message use for internal exchange
    #[error("The internal message is not correct: {0}")]
    InternalTvfMsg(#[from] TvfError),
}

impl<M> From<mpsc::error::SendError<InternalMsg<M>>> for BusError
where
    M: Sized + Clone + Tvf,
{
    fn from(error: mpsc::error::SendError<InternalMsg<M>>) -> Self {
        BusError::InternalQueue(error.to_string())
    }
}

/// Error encountered when sending a message
#[derive(thiserror::Error, Debug)]
pub enum SendError {
    /// Error encountered when sending a message
    #[error("Could not send the message {0}")]
    Send(String),
}

impl<M> From<mpsc::error::SendError<M>> for SendError {
    fn from(error: mpsc::error::SendError<M>) -> Self {
        SendError::Send(error.to_string())
    }
}

impl<M> From<mpsc::error::SendError<M>> for ProcError {
    fn from(error: mpsc::error::SendError<M>) -> Self {
        ProcError::Send(SendError::Send(error.to_string()))
    }
}

/// Error raised when creating the adaptor to process the request
#[derive(thiserror::Error, Debug)]
pub enum NewAdaptorError {
    /// Error raised when the adaptor could not be created
    #[error("Unexpected error: {0}")]
    Unexpected(String),
}

/// Error raised by the adaptor when treating the message
#[derive(thiserror::Error, Debug)]
pub enum AdaptError {
    /// Error raised when the message could not be transformed
    #[error("Unexpected error: {0}")]
    Unexpected(String),
}
