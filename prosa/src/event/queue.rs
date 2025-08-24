pub use crate::core::queue::SendError;
pub use prosa_utils::queue::{QueueChecker, QueueError};

/// Multi producer / Single consumer queue
pub mod mpsc;

/// Single producer, multiple consumer with expiration queue
pub mod timed;

/// Macro to define queue inner method related to Queue trait
macro_rules! impl_queue_checker {
    ( $queue:ident, $p:ty ) => {
        fn is_empty(&self) -> bool {
            self.$queue.is_empty()
        }

        fn is_full(&self) -> bool {
            self.$queue.is_full()
        }

        fn len(&self) -> $p {
            self.$queue.len()
        }

        fn max_capacity(&self) -> $p {
            self.$queue.max_capacity()
        }
    };
}
pub(crate) use impl_queue_checker;
