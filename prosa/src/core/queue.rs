use crate::core::{
    error::ProcError,
    msg::{InternalMsg, Tvf},
};

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

impl<T> SendError<T> {
    /// Maps a `SendError<T>` to `SendError<F>` by applying a function to a contained value (if [`SendError::Full`] or [`SendError::Drop`]) or returns [`SendError::Other`].
    ///
    /// # Examples
    ///
    /// Calculates the length of a <code>SendError<[String]></code> as an
    /// <code>SendError<[usize]></code>, consuming the original:
    /// ```
    /// use prosa::core::queue::SendError;
    ///
    /// let send_err_string = SendError::Drop(String::from("Hello, World!"));
    /// // `SendError` takes self *by value*, consuming `send_err_string`
    /// let send_err_len = send_err_string.map(|s| s.len());
    /// assert_eq!(send_err_len, SendError::Drop(13));
    ///
    /// let x = SendError::Other(String::from("Hello, World!"));
    /// assert_eq!(x.map(|s: String| s.len()), SendError::Other(String::from("Hello, World!")));
    /// ```
    pub fn map<F, O>(self, op: O) -> SendError<F>
    where
        O: FnOnce(T) -> F,
    {
        match self {
            SendError::Full(item, n) => SendError::Full(op(item), n),
            SendError::Drop(item) => SendError::Drop(op(item)),
            SendError::Other(s) => SendError::Other(s),
        }
    }
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

impl<T> From<T> for SendError<T> {
    fn from(item: T) -> Self {
        SendError::Drop(item)
    }
}

impl<T> From<T> for Box<SendError<T>> {
    fn from(item: T) -> Self {
        Box::new(SendError::Drop(item))
    }
}

impl<T> From<tokio::sync::mpsc::error::SendError<T>> for SendError<T> {
    fn from(error: tokio::sync::mpsc::error::SendError<T>) -> Self {
        SendError::Drop(error.0)
    }
}

impl<T> From<tokio::sync::mpsc::error::SendError<T>> for Box<SendError<T>> {
    fn from(error: tokio::sync::mpsc::error::SendError<T>) -> Self {
        Box::new(SendError::Drop(error.0))
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

impl<T> From<tokio::sync::mpsc::error::TrySendError<T>> for Box<SendError<T>> {
    fn from(error: tokio::sync::mpsc::error::TrySendError<T>) -> Self {
        match error {
            tokio::sync::mpsc::error::TrySendError::Full(item) => {
                Box::new(SendError::Full(item, 0))
            }
            tokio::sync::mpsc::error::TrySendError::Closed(item) => Box::new(SendError::Drop(item)),
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

/// Enum that define all response queue type to response to an internal request message
#[derive(Default)]
pub enum InternalMsgQueue<M>
where
    M: Sized + Clone + Tvf,
{
    /// No queue if the response has already been made
    #[default]
    None,
    /// [Tokio mpsc](https://docs.rs/tokio/latest/tokio/sync/mpsc/index.html) queue use to have a flow of messages
    TokioMpsc(tokio::sync::mpsc::Sender<InternalMsg<M>>),
    /// [Tokio oneshot](https://docs.rs/tokio/latest/tokio/sync/oneshot/index.html) queue use to respond to a single message
    TokioOneshot(tokio::sync::oneshot::Sender<InternalMsg<M>>),
    #[cfg(feature = "queue")]
    /// [ProSA mpsc](https://docs.rs/prosa/latest/prosa/event/queue/mpsc/index.html) queue use to have a flow of messages
    ProsaMpsc(std::sync::Arc<dyn crate::event::queue::mpsc::Sender<InternalMsg<M>> + Send + Sync>),
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
    pub fn send(self, msg: InternalMsg<M>) -> Result<(), Box<SendError<InternalMsg<M>>>> {
        match self {
            InternalMsgQueue::None => Err(Box::new(SendError::Drop(msg))),
            InternalMsgQueue::TokioMpsc(sender) => Ok(sender.try_send(msg)?),
            InternalMsgQueue::TokioOneshot(sender) => Ok(sender.send(msg)?),
            #[cfg(feature = "queue")]
            InternalMsgQueue::ProsaMpsc(sender) => sender.try_send(msg).map_err(Box::new),
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
        InternalMsgQueue::ProsaMpsc(std::sync::Arc::new(sender))
    }
}

#[cfg(feature = "queue")]
impl<M, const N: usize> From<crate::event::queue::mpsc::SenderU32<InternalMsg<M>, N>>
    for InternalMsgQueue<M>
where
    M: 'static + Sized + Clone + Tvf,
{
    fn from(sender: crate::event::queue::mpsc::SenderU32<InternalMsg<M>, N>) -> Self {
        InternalMsgQueue::ProsaMpsc(std::sync::Arc::new(sender))
    }
}
