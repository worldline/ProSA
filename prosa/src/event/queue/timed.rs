use crate::{core::queue::SendError, event::pending::Timers};
use prosa_utils::queue::{
    QueueChecker, QueueError,
    spmc::{LockFreeOptQueueU16, LockFreeOptQueueU32},
};
use std::sync::Arc;
use tokio::{
    sync::{Notify, Semaphore},
    time::Instant,
};

macro_rules! spmc {
    ($channel:ident, $queue:ident, $p:ty, $sender:ident, $receiver:ident) => {
        /// Sends values to the associated `Receiver`.
        pub struct $sender<T, const N: usize> {
            queue: Arc<$queue<T, N>>,
            timers: Timers<$p>,
            recv_notify: Arc<Notify>,
            send_sem: Arc<Semaphore>,
        }

        impl<T, const N: usize> $sender<T, N> {
            /// Method to check if timer are still on existing items
            fn timers_retain(&mut self, head: $p, id: $p) {
                let tail = id + 1 % self.queue.max_capacity();
                self.timers.retain(|t| prosa_utils::id_in_queue!(t, head, tail));
            }

            /// Wait to send a value in the queue until there is capacity
            async fn send_wait(&self, value: T) -> Result<($p, $p), SendError<T>> {
                match unsafe { self.queue.push(value) } {
                    Ok(head_id) => {
                        self.recv_notify.notify_one();
                        Ok(head_id)
                    },
                    Err(QueueError::<T>::Full(ret_value, _)) => {
                        if let Ok(_permit) = self.send_sem.acquire().await {
                            Box::pin(self.send_wait(ret_value)).await
                        } else {
                            Err(SendError::Drop(ret_value))
                        }
                    }
                    Err(e) => Err(e.into()),
                }
            }

            /// Sends a value, waiting until there is capacity.
            ///
            /// ```
            /// use std::ops::Add;
            /// use tokio::time::{Duration, Instant};
            /// use prosa::event::queue::{QueueChecker, timed};
            ///
            /// #[tokio::main]
            /// async fn main() {
            #[doc = concat!("   let (mut tx, _rx) = timed::", stringify!($channel), "::<i32, 4096>();")]
            ///     assert!(tx.is_empty());
            ///     assert_eq!(Ok(()), tx.send(0, Instant::now().add(Duration::from_millis(200))).await);
            /// }
            /// ```
            pub async fn send(&mut self, value: T, timeout: Instant) -> Result<(), SendError<T>> {
                match unsafe { self.queue.push(value) } {
                    Ok((head, id)) => {
                        self.recv_notify.notify_one();
                        self.timers_retain(head, id);
                        self.timers.push_at(id, timeout);
                        Ok(())
                    },
                    Err(QueueError::<T>::Full(ret_value, _)) => {
                        let (head, id) = if let Ok(_permit) = self.send_sem.acquire().await {
                            Box::pin(self.send_wait(ret_value)).await
                        } else {
                            Err(SendError::Drop(ret_value))
                        }?;

                        self.timers_retain(head, id);
                        self.timers.push_at(id, timeout);
                        Ok(())
                    }
                    Err(e) => Err(e.into()),
                }
            }

            /// Try to send a value, return an Full error if the queue is full
            ///
            /// ```
            /// use std::ops::Add;
            /// use tokio::time::{Duration, Instant};
            /// use prosa::event::queue::{QueueChecker, timed};
            ///
            #[doc = concat!("let (mut tx, _rx) = timed::", stringify!($channel), "::<i32, 4096>();")]
            /// assert!(tx.is_empty());
            /// assert_eq!(Ok(()), tx.try_send(0, Instant::now().add(Duration::from_millis(200))));
            /// ```
            pub fn try_send(&mut self, value: T, timeout: Instant) -> Result<(), SendError<T>> {
                let ret = unsafe { self.queue.push(value) };
                match ret {
                    Ok((head, id)) => {
                        self.recv_notify.notify_one();
                        self.timers_retain(head, id);
                        self.timers.push_at(id, timeout);
                        Ok(())
                    },
                    Err(e) => Err(e.into()),
                }
            }

            /// Method to retrieve expired message sent in the queue.
            ///
            /// ```
            /// use std::ops::Add;
            /// use tokio::time::{Duration, Instant, sleep};
            /// use prosa::event::queue::timed;
            ///
            /// #[tokio::main]
            /// async fn main() {
            #[doc = concat!("   let (mut tx, _rx) = timed::", stringify!($channel), "::<i32, 4096>();")]
            ///
            ///     tx.send(1, Instant::now().add(Duration::from_millis(10))).await;
            ///
            ///     // The data is not consumed during that time
            ///
            ///     assert_eq!(Some(1), tx.timeout().await);
            /// }
            /// ```
            pub async fn timeout(&mut self) -> Option<T> {
                while let Some(id) = self.timers.pull().await {
                    if let Some(item) = self.queue.try_pull_id(id) {
                        return Some(item);
                    }
                }

                None
            }
        }

        impl<T, const N: usize> std::fmt::Debug for $sender<T, N> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_struct(stringify!($sender))
                .field("queue", &self.queue)
                .finish()
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

        impl<T, const N: usize> $receiver<T, N> {
            /// Receives the next value for this receiver.
            ///
            /// ```
            /// use std::ops::Add;
            /// use tokio::time::{Duration, Instant};
            /// use prosa::event::queue::{QueueChecker, timed};
            ///
            /// #[tokio::main]
            /// async fn main() {
            #[doc = concat!("    let (mut tx, rx) = timed::", stringify!($channel), "::<i32, 4096>();")]
            ///     assert!(tx.is_empty());
            ///     assert_eq!(Ok(()), tx.try_send(0, Instant::now().add(Duration::from_millis(200))));
            ///
            ///     // If the element hasn't been consumed and it's not expired, you should get it
            ///     assert_eq!(0, rx.recv().await);
            /// }
            /// ```
            pub async fn recv(&self) -> T {
                loop {
                    match self.queue.pull() {
                        Ok(val) => {
                            return val;
                        }
                        Err(QueueError::<T>::Full(val, _)) => {
                            if self.send_sem.available_permits() == 0 {
                                self.send_sem.add_permits(1);
                            }
                            return val;
                        }
                        Err(QueueError::Empty) => if self.send_sem.available_permits() == 0 {
                            self.send_sem.add_permits(1);
                        }
                        _ => {}
                    }
                    self.recv_notify.notified().await;
                }
            }

            /// Tries to receive the next value for this receiver.
            ///
            /// If the queue is empty, it return `Err(QueueError::Empty)`
            /// If the element can't be pulled because of synchronicity, it return `Ok(None)`
            ///
            /// ```
            /// use std::ops::Add;
            /// use tokio::time::{Duration, Instant};
            /// use prosa::event::queue::{QueueChecker, timed};
            ///
            #[doc = concat!("let (mut tx, rx) = timed::", stringify!($channel), "::<i32, 4096>();")]
            /// assert!(tx.is_empty());
            /// assert_eq!(Ok(()), tx.try_send(0, Instant::now().add(Duration::from_millis(200))));
            ///
            /// // The try_recv method return an Ok
            /// // But can return either `Ok(Some(0))` or `Ok(None)` depending on internal atomic
            /// assert!(rx.try_recv().is_ok());
            /// ```
            pub fn try_recv(&self) -> Result<Option<T>, QueueError<T>> {
                match self.queue.try_pull() {
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

        impl<T, const N: usize> Clone for $receiver<T, N> {
            fn clone(&self) -> Self {
                $receiver::<T, N> {
                    queue: self.queue.clone(),
                    recv_notify: self.recv_notify.clone(),
                    send_sem: self.send_sem.clone(),
                }
            }
        }

        impl<T, const N: usize> Unpin for $receiver<T, N> {}

        impl<T, const N: usize> QueueChecker<$p> for $receiver<T, N> {
            crate::event::queue::impl_queue_checker! {queue, $p}
        }

        /// Creates a bounded timed channel for communicating between asynchronous tasks.
        ///
        /// ```
        /// use std::ops::Add;
        /// use tokio::time::{Duration, Instant};
        /// use prosa::event::queue::timed;
        ///
        /// #[tokio::main]
        /// async fn main() {
        #[doc = concat!("   let (mut tx, rx) = timed::", stringify!($channel), "::<i32, 4096>();")]
        ///
        ///     tokio::spawn(async move {
        ///         for i in 0..10 {
        ///             if let Err(_) = tx.send(i, Instant::now().add(Duration::from_millis(200))).await {
        ///                 println!("receiver dropped");
        ///                 return;
        ///             }
        ///         }
        ///
        ///         if let Some(i) = tx.timeout().await {
        ///             println!("the item expire");
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

            ($sender::<T, N> {
                queue: queue.clone(),
                timers: Timers::default(),
                recv_notify: recv_notify.clone(),
                send_sem: send_sem.clone(),
            },
            $receiver::<T, N> {
                queue,
                recv_notify,
                send_sem,
            })
        }
    };
}

spmc!(
    channel_spmc_u16,
    LockFreeOptQueueU16,
    u16,
    SenderU16,
    ReceiverU16
);
spmc!(
    channel_spmc_u32,
    LockFreeOptQueueU32,
    u32,
    SenderU32,
    ReceiverU32
);

#[cfg(test)]
mod tests {
    use super::*;
    use std::ops::Add;
    use tokio::time::{Duration, timeout};

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
    const EXPIRATION_DURATION: Duration = Duration::from_millis(100);

    macro_rules! timed_test {
        ( $channel:ident, $sender:ident, $receiver:ident ) => {
            let (mut sender, receiver) = $channel::<Data, QUEUE_CAPACITY>();
            assert!(sender.is_empty());
            assert!(receiver.is_empty());
            assert_eq!(0, sender.len());
            assert_eq!(0, receiver.len());
            assert_eq!(
                Ok(()),
                sender
                    .send(
                        Data::new("test".into()),
                        Instant::now().add(EXPIRATION_DURATION)
                    )
                    .await
            );
            assert_eq!(1, sender.len());
            assert_eq!(1, receiver.len());
            assert_eq!(Data::new("test".into()), receiver.recv().await);
            assert!(sender.is_empty());
            assert!(receiver.is_empty());
            assert_eq!(0, sender.len());
            assert_eq!(0, receiver.len());

            for i in 1..QUEUE_CAPACITY {
                sender
                    .send(
                        Data::new(format!("test{}", i)),
                        Instant::now().add(EXPIRATION_DURATION),
                    )
                    .await
                    .unwrap();
            }
            assert!(sender.is_full());

            // Try to push an element into a full queue
            assert!(
                timeout(
                    Duration::from_millis(10),
                    sender.send(
                        Data::new("testfull".into()),
                        Instant::now().add(EXPIRATION_DURATION)
                    )
                )
                .await
                .is_err()
            );
            // Pull an item to free a place
            assert!(!receiver.recv().await.val.is_empty());
            // Next send should work
            assert!(
                sender
                    .send(
                        Data::new(format!("testnonfull")),
                        Instant::now().add(EXPIRATION_DURATION)
                    )
                    .await
                    .is_ok()
            );

            // Wait item to expire
            for _ in 1..QUEUE_CAPACITY {
                assert!(sender.timeout().await.is_some());
            }

            // A recevive should be done for consumed elements
            assert!(
                timeout(
                    Duration::from_millis(10),
                    receiver.recv()
                )
                .await
                .is_err()
            );

            // The queue is now empty because all element have expired, and the queue pointer reset with the recv
            assert!(sender.is_empty());
            assert!(receiver.is_empty());
            assert_eq!(0, sender.len());
            assert_eq!(0, receiver.len());
        };
    }

    #[tokio::test]
    async fn timed_spmc_u16_test() {
        timed_test!(channel_spmc_u16, SenderU16, ReceiverU16);
    }

    #[tokio::test]
    async fn timed_spmc_u32_test() {
        timed_test!(channel_spmc_u32, SenderU32, ReceiverU32);
    }
}
