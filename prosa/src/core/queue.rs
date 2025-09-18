use crate::core::error::ProcError;

/// Error define for Queue senders
/// Use by [`InternalMsgQueue`], and internal queues:
#[cfg_attr(feature = "queue", doc = "[`crate::event::queue::mpsc::SenderU16`]")]
#[cfg_attr(feature = "queue", doc = "[`crate::event::queue::mpsc::SenderU32`]")]
#[cfg_attr(feature = "queue", doc = "[`crate::event::queue::timed::SenderU16`]")]
#[cfg_attr(feature = "queue", doc = "[`crate::event::queue::timed::SenderU32`]")]
#[derive(Eq, thiserror::Error, PartialOrd, PartialEq)]
pub enum SendError<T> {
    /// Error indicating that the queue is full
    #[error("The queue is full, it contain {1} items")]
    Full(T, usize),
    /// Error indicating that the queue was drop and not available anymore
    #[error("The queue was dropped ")]
    Drop(T),
    /// Other error of the queue
    #[error("Other queue error: {0}")]
    Other(String),
}

impl<T> std::fmt::Debug for SendError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SendError::Full(_, n) => write!(f, "SendError::Full(_, {n})"),
            SendError::Drop(_) => write!(f, "SendError::Drop(_)"),
            SendError::Other(s) => write!(f, "SendError::Other({s})"),
        }
    }
}

impl<T> ProcError for SendError<T> {
    fn recoverable(&self) -> bool {
        matches!(self, SendError::Full(_, _))
    }
}

impl<T> From<tokio::sync::mpsc::error::SendError<T>> for SendError<T> {
    fn from(error: tokio::sync::mpsc::error::SendError<T>) -> Self {
        SendError::Drop(error.0)
    }
}

impl<T> From<tokio::sync::mpsc::error::TrySendError<T>> for SendError<T> {
    fn from(error: tokio::sync::mpsc::error::TrySendError<T>) -> Self {
        match error {
            tokio::sync::mpsc::error::TrySendError::Full(item) => SendError::Full(item, 0),
            tokio::sync::mpsc::error::TrySendError::Closed(item) => SendError::Drop(item),
        }
    }
}

#[cfg(feature = "queue")]
impl<T> From<prosa_utils::queue::QueueError<T>> for SendError<T> {
    fn from(error: prosa_utils::queue::QueueError<T>) -> Self {
        match error {
            prosa_utils::queue::QueueError::Empty => SendError::Other("Empty queue".to_string()),
            prosa_utils::queue::QueueError::Full(item, n) => SendError::Full(item, n),
            prosa_utils::queue::QueueError::Retrieve(n) => {
                SendError::Other(format!("Can't retrieve queue element {n}"))
            }
        }
    }
}
