use super::{QueueChecker, QueueError};

crate::queue::lockfree::impl_lockfree_queue!(
    LockFreeOptQueueU16,
    u16,
    std::sync::atomic::AtomicU16,
    Option<T>,
    "single-producer",
    "multi-consumers",
    "optional"
);

crate::queue::lockfree::impl_lockfree_queue!(
    LockFreeOptQueueU32,
    u32,
    std::sync::atomic::AtomicU32,
    Option<T>,
    "single-producer",
    "multi-consumers",
    "optional"
);

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

    macro_rules! queue_atomic_test {
        ( $queue:ident ) => {
            let queue = $queue::<Data, 4096>::default();
            assert!(queue.is_empty());
            assert_eq!(0, queue.len());
            unsafe { assert_eq!(Ok((0, 0)), queue.push(Data::new("test".into()))) };
            assert_eq!(1, queue.len());
            assert_eq!(Ok(Data::new("test".into())), queue.pull());
            assert!(queue.is_empty());
            assert_eq!(0, queue.len());
        };
    }

    #[tokio::test]
    async fn queue_atomic_u16_test() {
        queue_atomic_test!(LockFreeOptQueueU16);
    }

    #[tokio::test]
    async fn queue_atomic_u32_test() {
        queue_atomic_test!(LockFreeOptQueueU32);
    }
}
