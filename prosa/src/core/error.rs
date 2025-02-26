use std::time::Duration;

use super::msg::InternalMsg;
use prosa_utils::msg::tvf::{Tvf, TvfError};
use tokio::sync::mpsc;

/// Processor error
pub trait ProcError: std::error::Error {
    /// Method to know if the processor can be restart because the error is temporary and can be recover
    fn recoverable(&self) -> bool;
    /// Method to know the period between the restart of the processor (restart immediatly by default)
    fn recovery_duration(&self) -> Duration {
        Duration::ZERO
    }
}

impl<'a, E: ProcError + 'a> From<E> for Box<dyn ProcError + 'a> {
    fn from(err: E) -> Box<dyn ProcError + 'a> {
        Box::new(err)
    }
}

impl<'a, E: ProcError + Send + Sync + 'a> From<E> for Box<dyn ProcError + Send + Sync + 'a> {
    fn from(err: E) -> Box<dyn ProcError + Send + Sync + 'a> {
        Box::new(err)
    }
}

impl<M> ProcError for tokio::sync::mpsc::error::SendError<InternalMsg<M>>
where
    M: Sized + Clone + Tvf,
{
    fn recoverable(&self) -> bool {
        true
    }
}

impl ProcError for std::io::Error {
    fn recoverable(&self) -> bool {
        matches!(
            self.kind(),
            std::io::ErrorKind::ConnectionReset
                | std::io::ErrorKind::ConnectionAborted
                | std::io::ErrorKind::NotConnected
                | std::io::ErrorKind::BrokenPipe
                | std::io::ErrorKind::WouldBlock
                | std::io::ErrorKind::InvalidData
                | std::io::ErrorKind::TimedOut
                | std::io::ErrorKind::WriteZero
                | std::io::ErrorKind::Interrupted
                | std::io::ErrorKind::UnexpectedEof
                | std::io::ErrorKind::OutOfMemory
        )
    }
}

impl ProcError for openssl::error::Error {
    fn recoverable(&self) -> bool {
        if let Some(reason) = self.reason() {
            // If it's an SSL protocol error, consider that can be recoverable. It's may be temporary related to a distant.
            reason.contains("SSL_")
        } else {
            false
        }
    }
}

impl ProcError for openssl::error::ErrorStack {
    fn recoverable(&self) -> bool {
        for error in self.errors() {
            if !error.recoverable() {
                return false;
            }
        }

        true
    }
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

impl ProcError for BusError {
    fn recoverable(&self) -> bool {
        matches!(self, BusError::InternalMainQueue(_, _, _))
    }
}

impl<M> From<mpsc::error::SendError<InternalMsg<M>>> for BusError
where
    M: Sized + Clone + Tvf,
{
    fn from(error: mpsc::error::SendError<InternalMsg<M>>) -> Self {
        BusError::InternalQueue(error.to_string())
    }
}
