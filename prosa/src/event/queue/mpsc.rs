use crate::core::queue::SendError;
use prosa_utils::queue::{
    QueueChecker, QueueError,
    mpsc::{LockFreeQueueU16, LockFreeQueueU32},
};
use std::sync::Arc;
use tokio::sync::{Notify, Semaphore};

/// Trait that define mpsc queue sender methods for global exposure
pub trait Sender<T> {
    /// Try to send a value, return a Full error if the queue is full
    ///
    /// ```
    /// use prosa::event::queue::{QueueChecker, QueueError, mpsc::Sender};
    ///
    /// async fn sender_process<S, T, P>(sender: S)
    /// where
    ///     S: Sender<T> + QueueChecker<P>,
    ///     T: std::cmp::PartialEq + std::fmt::Debug + std::default::Default,
    /// {
    ///     if sender.is_full() {
    ///         assert!(sender.try_send(T::default()).is_err());
    ///     } else {
    ///         assert_eq!(Ok(()), sender.try_send(T::default()));
    ///     }
    /// }
    /// ```
    fn try_send(&self, value: T) -> Result<(), SendError<T>>;
}

macro_rules! mpsc {
    ($channel:ident, $queue:ident, $p:ty, $sender:ident, $receiver:ident) => {
        /// Sends values to the associated `Receiver`.
        pub struct $sender<T, const N: usize> {
            queue: Arc<$queue<T, N>>,
            recv_notify: Arc<Notify>,
            send_sem: Arc<Semaphore>,
        }

        impl<T, const N: usize> $sender<T, N> {
            /// Sends a value, waiting until there is capacity.
            ///
            /// ```
            /// use prosa::event::queue::{QueueChecker, mpsc, mpsc::Sender};
            ///
            /// #[tokio::main]
            /// async fn main() {
            #[doc = concat!("    let (mut tx, _rx) = mpsc::", stringify!($channel), "::<i32, 4096>();")]
            ///     assert!(tx.is_empty());
            ///     assert_eq!(Ok(()), tx.send(0).await);
            /// }
            /// ```
            pub async fn send(&self, mut value: T) -> Result<(), SendError<T>> {
                loop {
                    match self.queue.push(value) {
                        Ok(()) => {
                            self.recv_notify.notify_one();
                            return Ok(())
                        }
                        Err(QueueError::<T>::Full(ret_value, _)) => {
                            if let Ok(_permit) = self
                                .send_sem
                                .acquire()
                                .await
                            {
                                value = ret_value;
                            }
                            else {
                                return Err(SendError::Drop(ret_value))
                            }
                        }
                        Err(e) => return Err(e.into()),
                    }
                }
            }
        }

        impl<T, const N: usize> Sender<T> for $sender<T, N> {
            /// Try to send a value, return a Full error if the queue is full
            ///
            /// ```
            /// use prosa::event::queue::{QueueChecker, mpsc, mpsc::Sender};
            ///
            #[doc = concat!("let (mut tx, _rx) = mpsc::", stringify!($channel), "::<i32, 4096>();")]
            /// assert!(tx.is_empty());
            /// assert_eq!(Ok(()), tx.try_send(0));
            /// ```
            fn try_send(&self, value: T) -> Result<(), SendError<T>> {
                let ret = self.queue.push(value);
                if ret.is_ok() {
                    self.recv_notify.notify_one();
                }
                Ok(ret?)
            }
        }

        impl<T, const N: usize> std::fmt::Debug for $sender<T, N> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_struct(stringify!($sender))
                .field("queue", &self.queue)
                .finish()
            }
        }

        impl<T, const N: usize> Clone for $sender<T, N> {
            fn clone(&self) -> Self {
                $sender::<T, N> {
                    queue: self.queue.clone(),
                    recv_notify: self.recv_notify.clone(),
                    send_sem: self.send_sem.clone(),
                }
            }
        }

        impl<T, const N: usize> QueueChecker<$p> for $sender<T, N> {
            crate::event::queue::impl_queue_checker! {queue, $p}
        }

        /// Receives values from the associated `Sender`.
        pub struct $receiver<T, const N: usize> {
            queue: Arc<$queue<T, N>>,
            recv_notify: Arc<Notify>,
            send_sem: Arc<Semaphore>,
        }

        impl<T, const N: usize> $receiver<T, N>
        {
            /// Receives the next value for this receiver.
            ///
            /// ```
            /// use prosa::event::queue::{QueueChecker, mpsc, mpsc::Sender};
            ///
            /// #[tokio::main]
            /// async fn main() {
            #[doc = concat!("    let (mut tx, rx) = mpsc::", stringify!($channel), "::<i32, 4096>();")]
            ///     assert_eq!(Ok(()), tx.try_send(0));
            ///     assert!(!tx.is_empty());
            ///
            ///     assert_eq!(0, rx.recv().await);
            /// }
            /// ```
            pub async fn recv(&self) -> T {
                loop {
                    match unsafe { self.queue.pull() } {
                        Ok(val) => {
                            return val;
                        }
                        Err(QueueError::<T>::Full(val, _)) => {
                            if self.send_sem.available_permits() == 0 {
                                self.send_sem.add_permits(1);
                            }
                            return val;
                        }
                        Err(QueueError::Empty) => {
                            if self.send_sem.available_permits() == 0 {
                                self.send_sem.add_permits(1);
                            }
                        }
                        _ => {}
                    }
                    self.recv_notify.notified().await;
                }
            }

            /// Receives the next value for this receiver.
            ///
            /// ```
            /// use prosa::event::queue::{QueueChecker, QueueError, mpsc, mpsc::Sender};
            ///
            #[doc = concat!("let (mut tx, rx) = mpsc::", stringify!($channel), "::<i32, 4096>();")]
            /// assert_eq!(Err(QueueError::Empty), rx.try_recv());
            /// assert_eq!(Ok(()), tx.try_send(0));
            /// assert!(!tx.is_empty());
            ///
            /// assert!(rx.try_recv().is_ok());
            /// ```
            pub fn try_recv(&self) -> Result<Option<T>, QueueError<T>> {
                match unsafe { self.queue.try_pull() } {
                    Err(QueueError::<T>::Full(val, _)) => {
                        if self.send_sem.available_permits() == 0 {
                            self.send_sem.add_permits(1);
                        }
                        Ok(Some(val))
                    }
                    Err(QueueError::Empty) => {
                        if self.send_sem.available_permits() == 0 {
                            self.send_sem.add_permits(1);
                        }
                        Err(QueueError::Empty)
                    }
                    v => v,
                }
            }
        }

        impl<T, const N: usize> std::fmt::Debug for $receiver<T, N> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_struct(stringify!($receiver))
                .field("queue", &self.queue)
                .finish()
            }
        }

        impl<T, const N: usize> Unpin for $receiver<T, N> {}

        impl<T, const N: usize> QueueChecker<$p> for $receiver<T, N> {
            crate::event::queue::impl_queue_checker! {queue, $p}
        }

        /// Creates a bounded mpsc channel for communicating between asynchronous tasks.
        ///
        /// ```
        /// use prosa::event::queue::{mpsc, mpsc::Sender};
        ///
        /// #[tokio::main]
        /// async fn main() {
        #[doc = concat!("    let (tx, rx) = mpsc::", stringify!($channel), "::<i32, 4096>();")]
        ///
        ///     tokio::spawn(async move {
        ///         for i in 0..10 {
        ///             if let Err(_) = tx.send(i).await {
        ///                 println!("receiver dropped");
        ///                 return;
        ///             }
        ///         }
        ///     });
        ///
        ///     let i = rx.recv().await;
        ///     println!("got = {}", i);
        /// }
        /// ```
        pub fn $channel<T, const N: usize>() -> ($sender<T, N>, $receiver<T, N>) {
            let queue = Arc::new($queue::<T, N>::default());
            let recv_notify = Arc::new(Notify::new());
            let send_sem = Arc::new(Semaphore::new(0));

            (
                $sender::<T, N> {
                    queue: queue.clone(),
                    recv_notify: recv_notify.clone(),
                    send_sem: send_sem.clone(),
                },
                $receiver::<T, N> {
                    queue,
                    recv_notify,
                    send_sem,
                },
            )
        }
    };
}

mpsc!(channel_u16, LockFreeQueueU16, u16, SenderU16, ReceiverU16);
mpsc!(channel_u32, LockFreeQueueU32, u32, SenderU32, ReceiverU32);

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    struct Data {
        val: String,
    }

    impl Data {
        fn new(val: String) -> Data {
            Data { val }
        }
    }

    const QUEUE_CAPACITY: usize = 4096;

    macro_rules! mpsc_test {
        ( $channel:ident, $sender:ident, $receiver:ident ) => {
            let (sender, receiver) = $channel::<Data, QUEUE_CAPACITY>();
            assert!(sender.is_empty());
            assert!(receiver.is_empty());
            assert_eq!(0, sender.len());
            assert_eq!(0, receiver.len());
            assert_eq!(Ok(()), sender.send(Data::new("test".into())).await);
            assert_eq!(1, sender.len());
            assert_eq!(1, receiver.len());
            assert_eq!(Data::new("test".into()), receiver.recv().await);
            assert!(sender.is_empty());
            assert!(receiver.is_empty());
            assert_eq!(0, sender.len());
            assert_eq!(0, receiver.len());

            for i in 1..QUEUE_CAPACITY {
                sender.send(Data::new(format!("test{}", i))).await.unwrap();
            }
            assert!(sender.is_full());

            // Try to push an element into a full queue
            assert!(
                tokio::time::timeout(
                    std::time::Duration::from_millis(100),
                    sender.send(Data::new("testfull".into()))
                )
                .await
                .is_err()
            );
            // Pull an item to free a place
            assert!(!receiver.recv().await.val.is_empty());
            // Next pull should work
            assert!(sender.send(Data::new(format!("testnonfull"))).await.is_ok());
        };
    }

    #[tokio::test]
    async fn mpsc_u16_test() {
        mpsc_test!(channel_u16, SenderU16, ReceiverU16);
    }

    #[tokio::test]
    async fn mpsc_u32_test() {
        mpsc_test!(channel_u32, SenderU32, ReceiverU32);
    }
}
