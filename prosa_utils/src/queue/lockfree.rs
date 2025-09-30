macro_rules! impl_producer_queue {
    // Single producer, non optional
    ("single-producer", "non-optional", $p:ty, $n:ident) => {
        /// Push an item in the queue.
        /// Return an error if it can't
        ///
        /// # Safety
        ///
        /// - Only one thread can push into the queue otherwise the queue may block.
        pub unsafe fn push(&self, val: T) -> Result<(), QueueError<T>> {
            let tail = self.tail.load(std::sync::atomic::Ordering::Relaxed);
            let next_tail = (tail + 1) % self.max_capacity();
            if next_tail != self.get_head() {
                match self.items[tail as usize].compare_exchange(
                    std::ptr::null_mut(),
                    Box::into_raw(Box::new(val)),
                    std::sync::atomic::Ordering::Release,
                    std::sync::atomic::Ordering::SeqCst,
                ) {
                    Ok(_) => {
                        self.tail
                            .store(next_tail, std::sync::atomic::Ordering::Relaxed);
                        Ok(())
                    }
                    Err(item_ptr) => {
                        let item: Box<T>;
                        unsafe {
                            item = Box::from_raw(item_ptr);
                        }

                        Err(QueueError::Full(item, self.len() as usize))
                    }
                }
            } else {
                Err(QueueError::Full(val, $n))
            }
        }
    };
    // Single producer, optional
    ("single-producer", "optional", $p:ty, $n:ident) => {
        /// Push an item in the queue.
        /// Return the id of the element in the queue, or an error if it can't push the item
        ///
        /// # Safety
        ///
        /// - Only one thread can push into the queue otherwise the queue may block.
        pub unsafe fn push(&self, val: T) -> Result<($p, $p), QueueError<T>> {
            let tail = self.tail.load(std::sync::atomic::Ordering::Relaxed);
            let next_tail = (tail + 1) % self.max_capacity();
            if next_tail != self.get_head() {
                match self.items[tail as usize].compare_exchange(
                    std::ptr::null_mut(),
                    Box::into_raw(Box::new(Some(val))),
                    std::sync::atomic::Ordering::Release,
                    std::sync::atomic::Ordering::SeqCst,
                ) {
                    Ok(_) => {
                        self.tail
                            .store(next_tail, std::sync::atomic::Ordering::Relaxed);
                        Ok((self.get_head(), tail))
                    }
                    Err(item_ptr) => {
                        let item: Box<Option<T>>;
                        unsafe {
                            item = Box::from_raw(item_ptr);
                        }

                        Err(QueueError::Full(item.unwrap(), self.len() as usize))
                    }
                }
            } else {
                Err(QueueError::Full(val, $n))
            }
        }
    };
    // Multiple producers, non optional
    ("multi-producers", "non-optional", $p:ty, $n:ident) => {
        /// Push an item in the queue.
        /// Return an error if it can't
        pub fn push(&self, val: T) -> Result<(), QueueError<T>> {
            loop {
                let tail = self.tail.load(std::sync::atomic::Ordering::Acquire);
                let next_tail = (tail + 1) % self.max_capacity();
                if next_tail != self.get_head() {
                    if self
                        .tail
                        .compare_exchange_weak(
                            tail,
                            next_tail,
                            std::sync::atomic::Ordering::Release,
                            std::sync::atomic::Ordering::SeqCst,
                        )
                        .is_ok()
                    {
                        let val_ptr = Box::into_raw(Box::new(val));
                        while self.items[tail as usize]
                            .compare_exchange(
                                std::ptr::null_mut(),
                                val_ptr,
                                std::sync::atomic::Ordering::Release,
                                std::sync::atomic::Ordering::SeqCst,
                            )
                            .is_err()
                        {}
                        return Ok(());
                    }
                } else {
                    return Err(QueueError::Full(val, $n));
                }
            }
        }
    };
}
pub(crate) use impl_producer_queue;

macro_rules! impl_consumer_id_queue {
    // Standard queue
    ("non-optional", $p:ty) => {
        /// Try to pull an item from the queue after a consume.
        ///
        /// # Safety
        ///
        /// - The `id` need to be take from the consume method, otherwise the queue may block.
        pub unsafe fn try_pull_id(&self, id: $p) -> Option<T> {
            let item_ptr = self.items[id as usize]
                .swap(std::ptr::null_mut(), std::sync::atomic::Ordering::Release);
            if !item_ptr.is_null() {
                let item: Box<T>;
                unsafe {
                    item = Box::from_raw(item_ptr);
                }

                Some(*item)
            } else {
                None
            }
        }
    };
    // Optional queue
    ("optional", $p:ty) => {
        /// Try to pull an item from the queue.
        /// The item need to be in the queue otherwise it will not try.
        pub fn try_pull_id(&self, id: $p) -> Option<T> {
            // The id that want to be pulled, need to be in the queue range to avoid consuming outside objects
            let head = self.get_head();
            let tail = self.get_tail();
            if crate::queue::id_in_queue!(id, head, tail) {
                let item_ptr = self.items[id as usize].swap(
                    Box::into_raw(Box::new(None)),
                    std::sync::atomic::Ordering::Release,
                );
                if !item_ptr.is_null() {
                    let item: Box<Option<T>>;
                    unsafe {
                        item = Box::from_raw(item_ptr);
                    }

                    *item
                } else {
                    // If the pointer was null put back a null in it
                    let item_ptr = self.items[id as usize]
                        .swap(std::ptr::null_mut(), std::sync::atomic::Ordering::Release);
                    if !item_ptr.is_null() {
                        let item: Box<Option<T>>;
                        unsafe {
                            item = Box::from_raw(item_ptr);
                        }

                        *item
                    } else {
                        None
                    }
                }
            } else {
                None
            }
        }
    };
}
pub(crate) use impl_consumer_id_queue;

macro_rules! impl_consume_queue {
    // Non optional, Single consumer
    ("non-optional", "single-consumer", $p:ty) => {
        /// Try to consume an item from the queue
        ///
        /// # Safety
        ///
        /// - Only one thread can consume from the queue otherwise the queue may block.
        pub unsafe fn try_consume(&self) -> Result<Option<$p>, QueueError<T>> {
            if !self.is_empty() {
                Ok(self
                    .head
                    .fetch_update(
                        std::sync::atomic::Ordering::Relaxed,
                        std::sync::atomic::Ordering::Relaxed,
                        |head| Some((head + 1) % self.max_capacity()),
                    )
                    .ok())
            } else {
                Err(QueueError::Empty)
            }
        }

        /// Consume an item from the queue
        ///
        /// # Safety
        ///
        /// - Only one thread can consume from the queue otherwise the queue may block.
        pub unsafe fn consume(&self) -> Result<$p, QueueError<T>> {
            if !self.is_empty() {
                self.head
                    .fetch_update(
                        std::sync::atomic::Ordering::Relaxed,
                        std::sync::atomic::Ordering::Relaxed,
                        |head| Some((head + 1) % self.max_capacity()),
                    )
                    .map_err(|e| QueueError::Retrieve(e as usize))
            } else {
                Err(QueueError::Empty)
            }
        }
    };
    // Non optional, Multiple consumers
    ("non-optional", "multi-consumers", $p:ty) => {
        /// Try to consume an item from the queue
        pub fn try_consume(&self) -> Result<Option<$p>, QueueError<T>> {
            let head = self.head.load(std::sync::atomic::Ordering::Acquire);
            if head != self.get_tail() {
                if self
                    .head
                    .compare_exchange_weak(
                        head,
                        (head + 1) % self.max_capacity(),
                        std::sync::atomic::Ordering::Release,
                        std::sync::atomic::Ordering::SeqCst,
                    )
                    .is_ok()
                {
                    Ok(Some(head))
                } else {
                    Ok(None)
                }
            } else {
                Err(QueueError::Empty)
            }
        }

        /// Consume an item from the queue
        pub fn consume(&self) -> Result<$p, QueueError<T>> {
            loop {
                let head = self.head.load(std::sync::atomic::Ordering::Acquire);
                if head != self.get_tail() {
                    if self
                        .head
                        .compare_exchange(
                            head,
                            (head + 1) % self.max_capacity(),
                            std::sync::atomic::Ordering::Release,
                            std::sync::atomic::Ordering::SeqCst,
                        )
                        .is_ok()
                    {
                        return Ok(head);
                    }
                } else {
                    return Err(QueueError::Empty);
                }
            }
        }
    };
    // Optional
    ("optional", "single-consumer", $p:ty) => {};
    ("optional", "multi-consumers", $p:ty) => {};
}
pub(crate) use impl_consume_queue;

macro_rules! impl_consumer_queue {
    // Single consumer, non optional
    ("single-consumer", "non-optional", $p:ty) => {
        /// Try to pull an item from the queue.
        ///
        /// For a single consumer, it return a `Full` error if the queue was full to notify that item can be push again in the queue.
        ///
        /// # Safety
        ///
        /// - Only one thread can pull from the queue otherwise the queue may block.
        pub unsafe fn try_pull(&self) -> Result<Option<T>, QueueError<T>> {
            let head = self.head.load(std::sync::atomic::Ordering::Relaxed);
            let tail = self.tail.load(std::sync::atomic::Ordering::Relaxed);
            if head != tail {
                let item_ptr = self.items[head as usize]
                    .swap(std::ptr::null_mut(), std::sync::atomic::Ordering::Release);
                if !item_ptr.is_null() {
                    let item: Box<T>;
                    unsafe {
                        item = Box::from_raw(item_ptr);
                    }

                    self.head.store(
                        (head + 1) % self.max_capacity(),
                        std::sync::atomic::Ordering::Relaxed,
                    );

                    if (tail + 1) % (N as $p) != head {
                        Ok(Some(*item))
                    } else {
                        Err(QueueError::<T>::Full(*item, N))
                    }
                } else {
                    Ok(None)
                }
            } else {
                Err(QueueError::Empty)
            }
        }

        /// Pull an item from the queue.
        ///
        /// For a single consumer, it return a `Full` error if the queue was full to notify that item can be push again in the queue.
        ///
        /// # Safety
        ///
        /// - Only one thread can pull from the queue otherwise the queue may block.
        pub unsafe fn pull(&self) -> Result<T, QueueError<T>> {
            let head = self.head.load(std::sync::atomic::Ordering::Relaxed);
            let tail = self.tail.load(std::sync::atomic::Ordering::Relaxed);
            if head != tail {
                let mut item_ptr = self.items[head as usize]
                    .swap(std::ptr::null_mut(), std::sync::atomic::Ordering::Release);
                while item_ptr.is_null() {
                    item_ptr = self.items[head as usize]
                        .swap(std::ptr::null_mut(), std::sync::atomic::Ordering::Release);
                }

                let item: Box<T>;
                unsafe {
                    item = Box::from_raw(item_ptr);
                }

                self.head.store(
                    (head + 1) % self.max_capacity(),
                    std::sync::atomic::Ordering::Relaxed,
                );

                if (tail + 1) % (N as $p) != head {
                    Ok(*item)
                } else {
                    Err(QueueError::<T>::Full(*item, N))
                }
            } else {
                Err(QueueError::Empty)
            }
        }
    };
    // Multiple consumers, non optional
    ("multi-consumers", "non-optional", $p:ty) => {
        /// Try to pull an item from the queue.
        pub fn try_pull(&self) -> Result<Option<T>, QueueError<T>> {
            let head = self.head.load(std::sync::atomic::Ordering::Acquire);
            if head != self.get_tail() {
                if self
                    .head
                    .compare_exchange_weak(
                        head,
                        (head + 1) % self.max_capacity(),
                        std::sync::atomic::Ordering::Release,
                        std::sync::atomic::Ordering::SeqCst,
                    )
                    .is_ok()
                {
                    let mut item_ptr = self.items[head as usize]
                        .swap(std::ptr::null_mut(), std::sync::atomic::Ordering::Release);
                    while item_ptr.is_null() {
                        item_ptr = self.items[head as usize]
                            .swap(std::ptr::null_mut(), std::sync::atomic::Ordering::Release);
                    }

                    let item: Box<T>;
                    unsafe {
                        item = Box::from_raw(item_ptr);
                    }

                    Ok(Some(*item))
                } else {
                    Ok(None)
                }
            } else {
                Err(QueueError::Empty)
            }
        }

        /// Pull an item from the queue.
        pub fn pull(&self) -> Result<T, QueueError<T>> {
            loop {
                let head = self.head.load(std::sync::atomic::Ordering::Acquire);
                if head != self.get_tail() {
                    if self
                        .head
                        .compare_exchange(
                            head,
                            (head + 1) % self.max_capacity(),
                            std::sync::atomic::Ordering::Release,
                            std::sync::atomic::Ordering::SeqCst,
                        )
                        .is_ok()
                    {
                        let mut item_ptr = self.items[head as usize]
                            .swap(std::ptr::null_mut(), std::sync::atomic::Ordering::Release);
                        while item_ptr.is_null() {
                            item_ptr = self.items[head as usize]
                                .swap(std::ptr::null_mut(), std::sync::atomic::Ordering::Release);
                        }

                        let item: Box<T>;
                        unsafe {
                            item = Box::from_raw(item_ptr);
                        }

                        return Ok(*item);
                    }
                } else {
                    return Err(QueueError::Empty);
                }
            }
        }
    };
    // Multiple consumers, optional
    ("multi-consumers", "optional", $p:ty) => {
        /// Try to pull an item from the queue.
        pub fn try_pull(&self) -> Result<Option<T>, QueueError<T>> {
            let head = self.head.load(std::sync::atomic::Ordering::Acquire);
            if head != self.get_tail() {
                if self
                    .head
                    .compare_exchange_weak(
                        head,
                        (head + 1) % self.max_capacity(),
                        std::sync::atomic::Ordering::Release,
                        std::sync::atomic::Ordering::SeqCst,
                    )
                    .is_ok()
                {
                    let mut item_ptr = self.items[head as usize]
                        .swap(std::ptr::null_mut(), std::sync::atomic::Ordering::Release);
                    while item_ptr.is_null() {
                        item_ptr = self.items[head as usize]
                            .swap(std::ptr::null_mut(), std::sync::atomic::Ordering::Release);
                    }

                    let item: Box<Option<T>>;
                    unsafe {
                        item = Box::from_raw(item_ptr);
                    }

                    Ok(*item)
                } else {
                    Ok(None)
                }
            } else {
                Err(QueueError::Empty)
            }
        }

        /// Pull an item from the queue.
        pub fn pull(&self) -> Result<T, QueueError<T>> {
            loop {
                let head = self.head.load(std::sync::atomic::Ordering::Acquire);
                if head != self.get_tail() {
                    if self
                        .head
                        .compare_exchange(
                            head,
                            (head + 1) % self.max_capacity(),
                            std::sync::atomic::Ordering::Release,
                            std::sync::atomic::Ordering::SeqCst,
                        )
                        .is_ok()
                    {
                        let mut item_ptr = self.items[head as usize]
                            .swap(std::ptr::null_mut(), std::sync::atomic::Ordering::Release);
                        while item_ptr.is_null() {
                            item_ptr = self.items[head as usize]
                                .swap(std::ptr::null_mut(), std::sync::atomic::Ordering::Release);
                        }

                        let item: Box<Option<T>>;
                        unsafe {
                            item = Box::from_raw(item_ptr);
                        }

                        if let Some(item) = *item {
                            // An item is available
                            return Ok(item);
                        } else {
                            // The item has been pulled before, call pull again for another item
                            return self.pull();
                        }
                    }
                } else {
                    return Err(QueueError::Empty);
                }
            }
        }
    };
}
pub(crate) use impl_consumer_queue;

/// Macro to define lockfree queue
macro_rules! impl_lockfree_queue {
    ( $queue:ident, $p:ty, $atomic:ty, $atomic_ptr_data:ty, $producer:tt, $consumer:tt, $optional:tt ) => {
        /// Implementation of an Atomic queue
        pub struct $queue<T, const N: usize> {
            /// Items of the queue
            items: [std::sync::atomic::AtomicPtr<$atomic_ptr_data>; N],
            /// Atomic position in the array of the head value
            head: $atomic,
            /// Atomic position in the array of the tail value
            tail: $atomic,
        }

        impl<T, const N: usize> $queue<T, N> {
            /// Getter of the head position without blocking (not synchronized)
            pub(crate) fn get_head(&self) -> $p {
                self.head.load(std::sync::atomic::Ordering::Relaxed)
            }
            /// Getter of the tail position without blocking (not synchronized)
            pub(crate) fn get_tail(&self) -> $p {
                self.tail.load(std::sync::atomic::Ordering::Relaxed)
            }

            crate::queue::lockfree::impl_consumer_id_queue!($optional, $p);
            crate::queue::lockfree::impl_consume_queue!($optional, $consumer, $p);
            crate::queue::lockfree::impl_producer_queue!($producer, $optional, $p, N);
            crate::queue::lockfree::impl_consumer_queue!($consumer, $optional, $p);
        }

        impl<T, const N: usize> Drop for $queue<T, N> {
            fn drop(&mut self) {
                for atomic_ptr in &self.items {
                    let item_ptr = atomic_ptr.load(std::sync::atomic::Ordering::Relaxed);
                    if !item_ptr.is_null() {
                        unsafe {
                            drop(Box::from_raw(item_ptr));
                        }
                    }
                }
            }
        }

        impl<T, const N: usize> Default for $queue<T, N> {
            fn default() -> Self {
                $queue::<T, N> {
                    items: [const { std::sync::atomic::AtomicPtr::new(std::ptr::null_mut()) }; N],
                    head: <$atomic>::new(0),
                    tail: <$atomic>::new(0),
                }
            }
        }

        impl<T, const N: usize> QueueChecker<$p> for $queue<T, N> {
            crate::queue::impl_queue_checker! {$p}
        }

        impl<T, const N: usize> std::fmt::Debug for $queue<T, N> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_struct(stringify!($queue))
                    .field("head", &self.get_head())
                    .field("tail", &self.get_tail())
                    .field("len", &self.len())
                    .field("max_capacity", &self.max_capacity())
                    .finish()
            }
        }
    };
}
pub(crate) use impl_lockfree_queue;
