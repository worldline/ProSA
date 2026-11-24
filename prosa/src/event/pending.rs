use std::{cmp::Ordering, future::poll_fn, marker::PhantomData, ops::Add, time::Duration};

use prosa_utils::{
    hash::{BuildIntHasher, IntHashMap},
    msg::tvf::Tvf,
};
use tokio::time::{Instant, Sleep, sleep_until};
use tokio_util::time::{DelayQueue, delay_queue::Key};

use crate::core::msg::Msg;

/// Pending timer use to track timeout with timer ID, and their associate timeout
#[derive(Debug)]
struct PendingTimer<T>
where
    T: Copy,
{
    timer_id: T,
    timeout: Instant,
}

impl<T> PendingTimer<T>
where
    T: Copy,
{
    /// Method to create a new pending timer from an id and a duration
    pub(crate) fn new(timer_id: T, timeout_duration: Duration) -> PendingTimer<T> {
        PendingTimer {
            timer_id,
            timeout: Instant::now().add(timeout_duration),
        }
    }

    /// Method to create a new pending timer from an id and an instant
    pub(crate) fn new_at(timer_id: T, timeout: Instant) -> PendingTimer<T> {
        PendingTimer { timer_id, timeout }
    }

    /// Getter of the timer id (object link to the timer)
    pub(crate) fn get_timer_id(&self) -> T {
        self.timer_id
    }

    /// Method to know if the timer is already expire
    pub(crate) fn is_expired(&self) -> bool {
        self.timeout <= Instant::now()
    }

    /// Method to get a Tokio Sleep object to wait on
    pub(crate) fn sleep(&self) -> Sleep {
        sleep_until(self.timeout)
    }
}

impl<T> Ord for PendingTimer<T>
where
    T: Copy,
{
    fn cmp(&self, other: &Self) -> Ordering {
        self.timeout.cmp(&other.timeout)
    }
}

impl<T> PartialOrd for PendingTimer<T>
where
    T: Copy,
{
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<T> PartialEq for PendingTimer<T>
where
    T: Copy,
{
    fn eq(&self, other: &Self) -> bool {
        self.timeout == other.timeout
    }
}

impl<T> Eq for PendingTimer<T> where T: Copy {}

/// ProSA pending timer to have a timer list
/// This object is not thread safe, you must use it within the same Tokio thread
///
/// ```
/// use std::time::Duration;
/// use prosa::event::pending::Timers;
///
/// async fn processing() {
///     let mut pending_timer: Timers<u64> = Default::default();
///     tokio::select! {
///         Some(timer_id) = pending_timer.pull(), if !pending_timer.is_empty() => {
///             println!("Timer {:?}", timer_id);
///             // Do your processing
///         },
///     }
/// }
/// ```
#[derive(Debug, Default)]
pub struct Timers<T>
where
    T: Copy,
{
    timers: Vec<PendingTimer<T>>,
}

impl<T> Timers<T>
where
    T: Copy,
{
    /// Returns the number of pending timers, also referred to as its ‘length’.
    pub fn len(&self) -> usize {
        self.timers.len()
    }

    /// Returns the capacity of the internal timer list.
    pub fn capacity(&self) -> usize {
        self.timers.capacity()
    }

    /// Returns true if there is no pending timer
    pub fn is_empty(&self) -> bool {
        self.timers.is_empty()
    }

    /// Method to create a new pending timer with a specific capacity
    pub fn with_capacity(capacity: usize) -> Self {
        Timers {
            timers: Vec::with_capacity(capacity),
        }
    }

    /// Method to push a pending timer
    fn push_timer(&mut self, timer: PendingTimer<T>) {
        let mut timer_iter = self.timers.iter();
        let index = loop {
            if let Some(val) = timer_iter.next() {
                if timer > *val {
                    break self.timers.len() - (timer_iter.count() + 1);
                }
            } else {
                break self.timers.len();
            }
        };

        self.timers.insert(index, timer);
    }

    /// Method to push a pending timer with a specifc timeout duration
    pub fn push(&mut self, timer_id: T, timeout_duration: Duration) {
        self.push_timer(PendingTimer::new(timer_id, timeout_duration));
    }

    /// Method to push a pending timer with a specifc timeout
    pub fn push_at(&mut self, timer_id: T, timeout: Instant) {
        self.push_timer(PendingTimer::new_at(timer_id, timeout));
    }

    /// Method to wait for the first timer
    /// If there is no pending timer (`is_empty` == `true`) the method return immediatelly. It doesn't block until a timer is pending
    ///
    /// ```
    /// use std::time::Duration;
    /// use prosa::event::pending::Timers;
    ///
    /// async fn processing() {
    ///     let mut pending_timer: Timers<u64> = Default::default();
    ///     let mut timer_id: Option<u64> = pending_timer.pull().await;
    ///     assert!(timer_id.is_none());
    ///     pending_timer.push(1, Duration::from_millis(200));
    ///     timer_id = pending_timer.pull().await;
    ///     assert!(timer_id.is_some());
    /// }
    /// ```
    pub async fn pull(&mut self) -> Option<T> {
        if let Some(timer) = self.timers.last() {
            if !timer.is_expired() {
                timer.sleep().await;
            }

            self.timers.pop().map(|t| t.get_timer_id())
        } else {
            None
        }
    }

    /// Retains only the elements specified by the predicate.
    ///
    /// ```
    /// use std::time::Duration;
    /// use prosa::event::pending::Timers;
    ///
    /// async fn processing() {
    ///     let mut pending_timer: Timers<u64> = Default::default();
    ///     pending_timer.push(1, Duration::from_secs(1));
    ///     pending_timer.push(2, Duration::from_secs(2));
    ///     pending_timer.push(3, Duration::from_secs(3));
    ///     pending_timer.push(4, Duration::from_secs(4));
    ///     assert_eq!(4, pending_timer.len());
    ///     pending_timer.retain(|x| x % 2 == 0);
    ///     assert_eq!(2, pending_timer.len());
    /// }
    /// ```
    pub fn retain<F>(&mut self, mut f: F)
    where
        F: FnMut(T) -> bool,
    {
        self.timers.retain(|t| f(t.timer_id));
    }
}

/// ProSA pending message to keep track of the message and trigger a timeout if a message is expire
/// This object is not thread safe, you must use it within the same Tokio thread
///
/// ```
/// use std::time::Duration;
/// use prosa::event::pending::PendingMsgs;
/// use tokio::sync::mpsc::Receiver;
/// use prosa::core::msg::{Msg, RequestMsg, InternalMsg};
/// use prosa_utils::msg::simple_string_tvf::SimpleStringTvf;
///
/// async fn processing(mut queue: Receiver<InternalMsg<SimpleStringTvf>>) {
///     let mut pending_msg: PendingMsgs<RequestMsg<SimpleStringTvf>, SimpleStringTvf> = Default::default();
///     tokio::select! {
///         Some(msg) = queue.recv() => {
///             match msg {
///                 InternalMsg::Request(msg) => {
///                     // Push in the pending message, the message will wait a timeout of 200ms
///                     pending_msg.push(msg, Duration::from_millis(200));
///                 },
///                 InternalMsg::Response(msg) => {
///                     let original_request: Option<RequestMsg<SimpleStringTvf>> = pending_msg.pull_msg(msg.get_id());
///                     println!("Receive a response: {:?}, from original request {:?}", msg, original_request);
///                 },
///                 _ => {},
///             }
///         },
///         Some(msg) = pending_msg.pull(), if !pending_msg.is_empty() => {
///             println!("Timeout message {:?}", msg);
///             // Do your processing
///         },
///     }
/// }
/// ```
#[derive(Debug)]
pub struct PendingMsgs<T, M>
where
    T: Msg<M>,
    M: Sized + Clone + Tvf,
{
    keys: IntHashMap<u64, Key>,
    queue: DelayQueue<(u64, T)>,
    phantom: PhantomData<M>,
}

impl<T, M> PendingMsgs<T, M>
where
    T: Msg<M>,
    M: Sized + Clone + Tvf,
{
    /// Returns the number of pending messages, also referred to as its ‘length’.
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    /// Returns the capacity of the internal message map.
    pub fn capacity(&self) -> usize {
        self.keys.capacity()
    }

    /// Returns true if there is no pending message
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// Method to create a new pending message list with a specific capacity
    pub fn with_capacity(capacity: usize) -> Self
    where
        T: Msg<M>,
        M: Sized + Clone + Tvf,
    {
        PendingMsgs {
            keys: IntHashMap::with_capacity_and_hasher(capacity, BuildIntHasher::default()),
            queue: DelayQueue::with_capacity(capacity),
            phantom: PhantomData,
        }
    }

    /// Method to push a pending message
    ///
    /// # Panics
    ///
    /// Panics if `timeout` is greater than 2^36 - 1 milliseconds (about 795 days).
    pub fn push(&mut self, msg: T, timeout: Duration) {
        self.push_with_id(msg.get_id(), msg, timeout);
    }

    /// Method to push a pending message with a custom id
    /// A message already pending with the same id is replaced, along with its timeout
    ///
    /// # Panics
    ///
    /// Panics if `timeout` is greater than 2^36 - 1 milliseconds (about 795 days).
    pub fn push_with_id(&mut self, id: u64, msg: T, timeout: Duration) {
        let key = self.queue.insert((id, msg), timeout);
        if let Some(old_key) = self.keys.insert(id, key) {
            self.queue.remove(&old_key);
        }
    }

    /// Method to pull a pending message to process it
    pub fn pull_msg(&mut self, msg_id: u64) -> Option<T> {
        let key = self.keys.remove(&msg_id)?;
        Some(self.queue.remove(&key).into_inner().1)
    }

    /// Method to wait for expired message (timeout)
    /// If there is no pending message (`is_empty` == `true`) the method return immediatelly. It doesn't block until a message is pending
    ///
    /// ```
    /// use std::time::Duration;
    /// use tokio::sync::mpsc::Sender;
    /// use prosa::event::pending::PendingMsgs;
    /// use prosa::core::msg::{Msg, RequestMsg, InternalMsg};
    /// use prosa_utils::msg::simple_string_tvf::SimpleStringTvf;
    ///
    /// async fn processing(tvf: SimpleStringTvf, queue: Sender<InternalMsg<SimpleStringTvf>>) {
    ///     let mut pending_msg: PendingMsgs<RequestMsg<SimpleStringTvf>, SimpleStringTvf> = Default::default();
    ///     let mut msg: Option<RequestMsg<SimpleStringTvf>> = pending_msg.pull().await;
    ///     assert!(msg.is_none());
    ///     pending_msg.push(RequestMsg::new(String::from("service"), tvf, queue), Duration::from_millis(200));
    ///     tokio::select! {
    ///         Some(msg) = pending_msg.pull(), if !pending_msg.is_empty() => {
    ///             println!("Timeout message {:?}", msg);
    ///         }
    ///     }
    /// }
    /// ```
    pub async fn pull(&mut self) -> Option<T> {
        let (id, msg) = poll_fn(|cx| self.queue.poll_expired(cx))
            .await?
            .into_inner();
        self.keys.remove(&id);
        Some(msg)
    }
}

impl<T, M> Default for PendingMsgs<T, M>
where
    T: Msg<M>,
    M: Sized + Clone + Tvf,
{
    fn default() -> Self {
        PendingMsgs::<T, M> {
            keys: Default::default(),
            queue: DelayQueue::new(),
            phantom: PhantomData,
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate self as prosa;

    use std::time::Duration;

    use prosa_macros::{proc, settings};
    use prosa_utils::msg::{simple_string_tvf::SimpleStringTvf, tvf::Tvf};
    use serde::Serialize;
    use tokio::time::timeout;

    use crate::core::{
        error::BusError,
        main::{MainProc, MainRunnable},
        msg::{InternalMsg, Msg, RequestMsg},
        proc::{ProcBusParam, ProcConfig},
    };

    use super::{PendingMsgs, Timers};

    #[proc]
    pub(crate) struct TestProc {}

    #[proc]
    impl TestProc<SimpleStringTvf> {
        async fn timers_run(&mut self) -> Result<(), BusError> {
            // Add proc and its service
            self.proc.add_proc().await?;
            self.proc
                .add_service_proc(vec![String::from("TEST")])
                .await?;

            let mut pending_timer: Timers<u64> = Default::default();
            loop {
                tokio::select! {
                    Some(msg) = self.internal_rx_queue.recv() => {
                        match msg {
                            InternalMsg::Request(_) => {
                                assert_eq!(0, pending_timer.len());
                                pending_timer.push(1, Duration::from_millis(100));
                                assert_eq!(1, pending_timer.len());
                            },
                            InternalMsg::Service(table) => {
                                if let Some(service) = table.get_proc_service("TEST") {
                                    service.proc_queue.send(InternalMsg::Request(RequestMsg::new(String::from("TEST"), Default::default(), self.proc.get_service_queue().clone()))).await.expect("Internal msg should be send");
                                }
                            },
                            _ => return Err(BusError::ProcComm(self.get_proc_id(), 0, String::from("Wrong message"))),
                        }
                    },
                    Some(timer_id) = pending_timer.pull(), if !pending_timer.is_empty() => {
                        assert_eq!(0, pending_timer.len());
                        assert_eq!(1, timer_id);
                        self.proc.remove_proc(None).await?;
                        return Ok(())
                    },
                }
            }
        }

        async fn pending_msgs_run(&mut self) -> Result<(), BusError> {
            // Add proc and its service
            self.proc.add_proc().await?;
            self.proc
                .add_service_proc(vec![String::from("TEST")])
                .await?;

            let mut pending_msg: PendingMsgs<RequestMsg<SimpleStringTvf>, SimpleStringTvf> =
                Default::default();
            loop {
                tokio::select! {
                    Some(msg) = self.internal_rx_queue.recv() => {
                        match msg {
                            InternalMsg::Request(msg) => {
                                assert_eq!(0, pending_msg.len());
                                pending_msg.push(msg, Duration::from_millis(100));
                                assert_eq!(1, pending_msg.len());
                            },
                            InternalMsg::Service(table) => {
                                if let Some(service) = table.get_proc_service("TEST") {
                                    let mut msg: SimpleStringTvf = Default::default();
                                    msg.put_string(1, "good");
                                    service.proc_queue.send(InternalMsg::Request(RequestMsg::new(String::from("TEST"), msg, self.proc.get_service_queue().clone()))).await.expect("Internal msg should be send");
                                }
                            },
                            _ => return Err(BusError::ProcComm(self.get_proc_id(), 0, String::from("Wrong message"))),
                        }
                    },
                    Some(msg) = pending_msg.pull(), if !pending_msg.is_empty() => {
                        assert_eq!(0, pending_msg.len());
                        assert_eq!(String::from("good"), msg.get_data()?.get_string(1)?.into_owned());
                        self.proc.remove_proc(None).await?;
                        return Ok(())
                    },
                }
            }
        }

        pub(crate) async fn timers_timeout_run(&mut self) -> Result<(), BusError> {
            if timeout(Duration::from_millis(200), self.timers_run())
                .await
                .is_err()
            {
                Err(BusError::InternalQueue(String::from(
                    "Timer is not working",
                )))
            } else {
                Ok(())
            }
        }

        pub(crate) async fn pending_msgs_timeout_run(&mut self) -> Result<(), BusError> {
            if timeout(Duration::from_millis(200), self.pending_msgs_run())
                .await
                .is_err()
            {
                Err(BusError::InternalQueue(String::from(
                    "pending msgs is not working",
                )))
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn test_with_capacity() {
        let capacity = 10;
        let pending_msg: PendingMsgs<RequestMsg<SimpleStringTvf>, SimpleStringTvf> =
            PendingMsgs::with_capacity(capacity);
        assert_eq!(pending_msg.len(), 0);
        assert!(pending_msg.is_empty());
        assert!(pending_msg.capacity() >= capacity);

        let pending_timer: Timers<u64> = Timers::with_capacity(capacity);
        assert_eq!(pending_timer.len(), 0);
        assert!(pending_timer.is_empty());
        assert!(pending_timer.capacity() >= capacity);
    }

    #[tokio::test]
    async fn test_timers_order() {
        let mut pending_timer: Timers<u64> = Default::default();
        pending_timer.push(3, Duration::from_millis(30));
        pending_timer.push(1, Duration::from_millis(10));
        pending_timer.push(2, Duration::from_millis(20));
        assert_eq!(Some(1), pending_timer.pull().await);
        assert_eq!(Some(2), pending_timer.pull().await);
        assert_eq!(Some(3), pending_timer.pull().await);
        assert_eq!(None, pending_timer.pull().await);
    }

    #[tokio::test(start_paused = true)]
    async fn test_pending_msgs_replace_and_pull() {
        let (tx, _rx) = tokio::sync::mpsc::channel(1);
        let mut pending_msg: PendingMsgs<RequestMsg<SimpleStringTvf>, SimpleStringTvf> =
            Default::default();
        let msg = |v: &str| {
            let mut data = SimpleStringTvf::default();
            data.put_string(1, v);
            RequestMsg::new(String::from("TEST"), data, tx.clone())
        };

        // Answered message doesn't expire
        let answered = msg("answered");
        let answered_id = answered.get_id();
        pending_msg.push(answered, Duration::from_millis(10));
        assert!(pending_msg.pull_msg(answered_id).is_some());
        assert!(pending_msg.pull_msg(answered_id).is_none());
        assert!(pending_msg.is_empty());

        // Pushing the same id replaces the message and its timeout
        pending_msg.push_with_id(1, msg("old"), Duration::from_millis(10));
        pending_msg.push_with_id(1, msg("new"), Duration::from_millis(50));
        assert_eq!(1, pending_msg.len());
        assert!(
            timeout(Duration::from_millis(30), pending_msg.pull())
                .await
                .is_err()
        );
        let expired = pending_msg.pull().await.expect("message should expire");
        assert_eq!(
            Ok(String::from("new")),
            expired
                .get_data()
                .and_then(|d| Ok(d.get_string(1)?.into_owned()))
        );
        assert!(pending_msg.is_empty());
        assert!(pending_msg.pull().await.is_none());
    }

    #[tokio::test]
    async fn test_pending() {
        /// Dummy settings
        #[settings]
        #[derive(Default, Debug, Serialize)]
        struct DummySettings {}

        // Create bus and main processor
        let (bus, main) = MainProc::<SimpleStringTvf>::create(&DummySettings::default(), Some(2));

        // Launch the main task
        let main_task = tokio::spawn(main.run());

        // Launch the test processor
        assert_eq!(
            Ok(()),
            TestProc::<SimpleStringTvf>::create_raw(1, "test1".to_string(), bus.clone())
                .timers_timeout_run()
                .await
        );

        assert_eq!(
            Ok(()),
            TestProc::<SimpleStringTvf>::create_raw(2, "test2".to_string(), bus.clone())
                .pending_msgs_timeout_run()
                .await
        );

        bus.stop("ProSA unit test end".into())
            .await
            .expect("ProSA should stop");
        main_task.await.expect("Main task should end correctly");
    }
}
