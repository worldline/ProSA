//! Module for ProSA internal queueing utilitary

/// Atomic queue implementation
pub(crate) mod lockfree;

/// Queue for single producer and multiple consumers
pub mod spmc;

/// Queue for Multiple producers and single consumer
pub mod mpsc;

/// Error define for Queues
/// Use by utilitary queues an their implementation in ProSA.
#[derive(Debug, Eq, thiserror::Error, PartialOrd, PartialEq)]
pub enum QueueError<T> {
    /// Error indicating that the queue is empty
    #[error("The queue is empty")]
    Empty,
    /// Error indicating that the queue is full
    #[error("The queue is full, it contain {1} items")]
    Full(T, usize),
    /// Can't retrieve the element
    #[error("Can't retrieve the element {0}")]
    Retrieve(usize),
}

/// Trait to define all information getter from the queue
///
/// ```no_run
/// use prosa_utils::queue::QueueChecker;
///
/// fn queue_checker<Q>(queue: Q)
/// where
///     Q: QueueChecker<usize>,
/// {
///     if queue.is_empty() {
///         assert!(!queue.is_full());
///         assert_eq!(0, queue.len());
///     } else if queue.is_full() {
///         assert!(!queue.is_empty());
///         assert_eq!(queue.max_capacity(), queue.len());
///     }
/// }
/// ```
pub trait QueueChecker<P> {
    /// Checks if the queue is empty.
    /// Prefer this method over `len() != 0`
    fn is_empty(&self) -> bool;
    /// Checks if the queue is full.
    /// Prefer this method over `len() != max_capacity()`
    fn is_full(&self) -> bool;
    /// Returns the number of item in the queue.
    fn len(&self) -> P;
    /// Returns the maximum item capacity of the queue.
    fn max_capacity(&self) -> P;
}

/// Macro to define queue inner method related to `QueueChecker` trait
macro_rules! impl_queue_checker {
    ( $p:ty ) => {
        fn is_empty(&self) -> bool {
            self.get_head() == self.get_tail()
        }

        fn is_full(&self) -> bool {
            (self.get_tail() + 1) % (N as $p) == self.get_head()
        }

        fn len(&self) -> $p {
            let head = self.get_head();
            let tail = self.get_tail();

            if tail >= head {
                tail - head
            } else {
                (self.max_capacity() - head) + tail
            }
        }

        fn max_capacity(&self) -> $p {
            N as $p
        }
    };
}
pub(crate) use impl_queue_checker;

#[macro_export]
/// Macro of an expression to know if an id is still contain by the queue that have a circular buffer
///
/// The buffer have two pointer that indicate head and tail.
/// Every ID in this range is considered for the queue.
///
/// In the following example:
/// - `h` represent head position
/// - `t` represent tail position
/// - `o` active items
/// - `x` represent inactive items
///
/// If head is before the tail:
/// `x h[ o [t x`
///
/// If head is after the tail:
/// `o [t x h[ o`
macro_rules! id_in_queue {
    ( $id:ident, $head:ident, $tail:ident ) => {
        ($head > $tail && ($id >= $head || $id < $tail)) || ($id >= $head && $id < $tail)
    };
}
pub use id_in_queue;
