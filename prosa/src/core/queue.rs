#[cfg(feature = "queue")]
use std::sync::Arc;

use prosa_utils::msg::tvf::Tvf;

use crate::core::{error::ProcError, msg::InternalMsg};

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

/// Enum that define all possible queue for processor internal messaging
pub enum ProcQueue<M>
where
    M: Sized + Clone + Tvf,
{
    /// [Tokio mpsc](https://docs.rs/tokio/latest/tokio/sync/mpsc/index.html) queue use to have a flow of messages
    Tokio(tokio::sync::mpsc::Sender<InternalMsg<M>>),
    #[cfg(feature = "queue")]
    /// [ProSA mpsc](https://docs.rs/prosa/latest/prosa/event/queue/mpsc/index.html) queue use to have a flow of messages
    Prosa(Arc<dyn crate::event::queue::mpsc::Sender<InternalMsg<M>> + Send + Sync>),
}

impl<M> ProcQueue<M>
where
    M: Sized + Clone + Tvf,
{
    /// Method to send an internal message
    pub async fn send(&self, msg: InternalMsg<M>) -> Result<(), SendError<InternalMsg<M>>> {
        match self {
            ProcQueue::Tokio(sender) => Ok(sender.send(msg).await?),
            #[cfg(feature = "queue")]
            ProcQueue::Prosa(sender) => sender.try_send(msg),
        }
    }
}

impl<M> Clone for ProcQueue<M>
where
    M: Sized + Clone + Tvf,
{
    fn clone(&self) -> Self {
        match self {
            ProcQueue::Tokio(sender) => ProcQueue::Tokio(sender.clone()),
            #[cfg(feature = "queue")]
            ProcQueue::Prosa(sender) => ProcQueue::Prosa(sender.clone()),
        }
    }
}

impl<M> std::fmt::Debug for ProcQueue<M>
where
    M: Sized + Clone + Tvf,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProcQueue::Tokio(_sender) => write!(f, "Tokio mpsc"),
            #[cfg(feature = "queue")]
            ProcQueue::Prosa(_sender) => write!(f, "ProSA mpsc"),
        }
    }
}

impl<M> From<tokio::sync::mpsc::Sender<InternalMsg<M>>> for ProcQueue<M>
where
    M: Sized + Clone + Tvf,
{
    fn from(sender: tokio::sync::mpsc::Sender<InternalMsg<M>>) -> Self {
        ProcQueue::Tokio(sender)
    }
}

#[cfg(feature = "queue")]
impl<M, const N: usize> From<crate::event::queue::mpsc::SenderU16<InternalMsg<M>, N>>
    for ProcQueue<M>
where
    M: 'static + Sized + Clone + Tvf,
{
    fn from(sender: crate::event::queue::mpsc::SenderU16<InternalMsg<M>, N>) -> Self {
        ProcQueue::Prosa(Arc::new(sender))
    }
}

#[cfg(feature = "queue")]
impl<M, const N: usize> From<crate::event::queue::mpsc::SenderU32<InternalMsg<M>, N>>
    for ProcQueue<M>
where
    M: 'static + Sized + Clone + Tvf,
{
    fn from(sender: crate::event::queue::mpsc::SenderU32<InternalMsg<M>, N>) -> Self {
        ProcQueue::Prosa(Arc::new(sender))
    }
}

/// Enum that define all response queue type to response to an internal request message
pub enum InternalMsgQueue<M>
where
    M: Sized + Clone + Tvf,
{
    /// No queue if the response has already been made
    None,
    /// [Tokio mpsc](https://docs.rs/tokio/latest/tokio/sync/mpsc/index.html) queue use to have a flow of messages
    TokioMpsc(tokio::sync::mpsc::Sender<InternalMsg<M>>),
    /// [Tokio oneshot](https://docs.rs/tokio/latest/tokio/sync/oneshot/index.html) queue use to respond to a single message
    TokioOneshot(tokio::sync::oneshot::Sender<InternalMsg<M>>),
    #[cfg(feature = "queue")]
    /// [ProSA mpsc](https://docs.rs/prosa/latest/prosa/event/queue/mpsc/index.html) queue use to have a flow of messages
    ProsaMpsc(Arc<dyn crate::event::queue::mpsc::Sender<InternalMsg<M>> + Send + Sync>),
}

impl<M> InternalMsgQueue<M>
where
    M: Sized + Clone + Tvf,
{
    /// Method to know if the queue has been used to respond to a message.
    /// Return `true` if it's the case, false otherwise.
    pub fn is_none(&self) -> bool {
        matches!(self, InternalMsgQueue::None)
    }

    /// Method to take the ownership of the Sender and let None to indicate the the response has been made
    pub fn take(&mut self) -> InternalMsgQueue<M> {
        std::mem::replace(self, InternalMsgQueue::None)
    }

    /// Method to send an internal message
    pub async fn send(self, msg: InternalMsg<M>) -> Result<(), SendError<InternalMsg<M>>> {
        match self {
            InternalMsgQueue::None => Err(SendError::Drop(msg)),
            InternalMsgQueue::TokioMpsc(sender) => Ok(sender.send(msg).await?),
            InternalMsgQueue::TokioOneshot(sender) => sender.send(msg).map_err(SendError::Drop),
            #[cfg(feature = "queue")]
            InternalMsgQueue::ProsaMpsc(sender) => sender.try_send(msg),
        }
    }
}

impl<M> std::fmt::Debug for InternalMsgQueue<M>
where
    M: Sized + Clone + Tvf,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InternalMsgQueue::None => write!(f, "None"),
            InternalMsgQueue::TokioMpsc(_sender) => write!(f, "Tokio mpsc"),
            InternalMsgQueue::TokioOneshot(_sender) => write!(f, "Tokio oneshot"),
            #[cfg(feature = "queue")]
            InternalMsgQueue::ProsaMpsc(_sender) => write!(f, "ProSA mpsc"),
        }
    }
}

impl<M> From<ProcQueue<M>> for InternalMsgQueue<M>
where
    M: Sized + Clone + Tvf,
{
    fn from(proc_queue: ProcQueue<M>) -> Self {
        match proc_queue {
            ProcQueue::Tokio(sender) => InternalMsgQueue::TokioMpsc(sender),
            #[cfg(feature = "queue")]
            ProcQueue::Prosa(sender) => InternalMsgQueue::ProsaMpsc(sender),
        }
    }
}

impl<M> From<tokio::sync::mpsc::Sender<InternalMsg<M>>> for InternalMsgQueue<M>
where
    M: Sized + Clone + Tvf,
{
    fn from(sender: tokio::sync::mpsc::Sender<InternalMsg<M>>) -> Self {
        InternalMsgQueue::TokioMpsc(sender)
    }
}

impl<M> From<tokio::sync::oneshot::Sender<InternalMsg<M>>> for InternalMsgQueue<M>
where
    M: Sized + Clone + Tvf,
{
    fn from(sender: tokio::sync::oneshot::Sender<InternalMsg<M>>) -> Self {
        InternalMsgQueue::TokioOneshot(sender)
    }
}

#[cfg(feature = "queue")]
impl<M, const N: usize> From<crate::event::queue::mpsc::SenderU16<InternalMsg<M>, N>>
    for InternalMsgQueue<M>
where
    M: 'static + Sized + Clone + Tvf,
{
    fn from(sender: crate::event::queue::mpsc::SenderU16<InternalMsg<M>, N>) -> Self {
        InternalMsgQueue::ProsaMpsc(Arc::new(sender))
    }
}

#[cfg(feature = "queue")]
impl<M, const N: usize> From<crate::event::queue::mpsc::SenderU32<InternalMsg<M>, N>>
    for InternalMsgQueue<M>
where
    M: 'static + Sized + Clone + Tvf,
{
    fn from(sender: crate::event::queue::mpsc::SenderU32<InternalMsg<M>, N>) -> Self {
        InternalMsgQueue::ProsaMpsc(Arc::new(sender))
    }
}
