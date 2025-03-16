use super::{QueueChecker, QueueError};

crate::queue::lockfree::impl_lockfree_queue!(
    LockFreeQueueU16,
    u16,
    std::sync::atomic::AtomicU16,
    T,
    true,
    false,
    false
);

crate::queue::lockfree::impl_lockfree_queue!(
    LockFreeQueueU32,
    u32,
    std::sync::atomic::AtomicU32,
    T,
    true,
    false,
    false
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
            assert_eq!(Ok(()), queue.push(Data::new("test".into())));
            assert_eq!(1, queue.len());
            unsafe {
                assert_eq!(Ok(Data::new("test".into())), queue.pull());
            }
            assert!(queue.is_empty());
            assert_eq!(0, queue.len());
        };
    }

    #[tokio::test]
    async fn queue_atomic_u16_test() {
        queue_atomic_test!(LockFreeQueueU16);
    }

    #[tokio::test]
    async fn queue_atomic_u32_test() {
        queue_atomic_test!(LockFreeQueueU32);
    }
}
