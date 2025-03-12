use std::{cmp::Ordering, collections::HashMap, marker::PhantomData, ops::Add, time::Duration};

use prosa_utils::msg::tvf::Tvf;
use tokio::time::{Instant, Sleep, sleep_until};

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

    /// Returns true if there is no pending timer
    pub fn is_empty(&self) -> bool {
        self.timers.is_empty()
    }

    /// Method to push a pending timer
    pub fn push(&mut self, timer_id: T, timeout: Duration) {
        let timer = PendingTimer::new(timer_id, timeout);
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

    /// Method to pop the inner Pending timer of the timer list
    fn pop(&mut self) -> Option<PendingTimer<T>> {
        self.timers.pop()
    }

    /// Method to get a reference on the last pending timer or None if the list is empty
    fn last(&self) -> Option<&PendingTimer<T>> {
        self.timers.last()
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
    pending_messages: HashMap<u64, T>,
    timers: Timers<u64>,
    phantom: PhantomData<M>,
}

impl<T, M> PendingMsgs<T, M>
where
    T: Msg<M>,
    M: Sized + Clone + Tvf,
{
    /// Returns the number of pending messages, also referred to as its ‘length’.
    pub fn len(&self) -> usize {
        self.pending_messages.len()
    }

    /// Returns true if there is no pending message
    pub fn is_empty(&self) -> bool {
        self.pending_messages.is_empty()
    }

    /// Method to push a pending message
    pub fn push(&mut self, msg: T, timeout: Duration) {
        self.push_with_id(msg.get_id(), msg, timeout);
    }

    /// Method to push a pending message with a custom id
    pub fn push_with_id(&mut self, id: u64, msg: T, timeout: Duration) {
        self.timers.push(id, timeout);
        self.pending_messages.insert(id, msg);
    }

    /// Method to pull a pending message to process it
    pub fn pull_msg(&mut self, msg_id: u64) -> Option<T> {
        if let Some(msg) = self.pending_messages.remove(&msg_id) {
            return Some(msg);
        }

        None
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
    ///     pending_msg.push(RequestMsg::new(1, String::from("service"), tvf, queue), Duration::from_millis(200));
    ///     msg = pending_msg.pull().await;
    ///     assert!(msg.is_some());
    ///     println!("Timeout message {:?}", msg);
    /// }
    /// ```
    pub async fn pull(&mut self) -> Option<T> {
        while let Some(timer) = self.timers.last() {
            if self.pending_messages.contains_key(&timer.get_timer_id()) {
                if !timer.is_expired() {
                    timer.sleep().await;
                }

                if let Some(time) = self.timers.pop() {
                    return self.pull_msg(time.get_timer_id());
                } else {
                    return None;
                }
            } else {
                self.timers.pop();
            }
        }

        None
    }
}

impl<T, M> Default for PendingMsgs<T, M>
where
    T: Msg<M>,
    M: Sized + Clone + Tvf,
{
    fn default() -> Self {
        PendingMsgs::<T, M> {
            pending_messages: Default::default(),
            timers: Default::default(),
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
    use tokio::{
        runtime::{Builder, Runtime},
        time::timeout,
    };

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
                                if let Some(service) = table.get_proc_service(&String::from("TEST"), 1) {
                                    service.proc_queue.send(InternalMsg::Request(RequestMsg::new(1, String::from("TEST"), Default::default(), self.proc.get_service_queue().clone()))).await.unwrap();
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
                                if let Some(service) = table.get_proc_service(&String::from("TEST"), 1) {
                                    let mut msg: SimpleStringTvf = Default::default();
                                    msg.put_string(1, "good");
                                    service.proc_queue.send(InternalMsg::Request(RequestMsg::new(1, String::from("TEST"), msg, self.proc.get_service_queue().clone()))).await.unwrap();
                                }
                            },
                            _ => return Err(BusError::ProcComm(self.get_proc_id(), 0, String::from("Wrong message"))),
                        }
                    },
                    Some(msg) = pending_msg.pull(), if !pending_msg.is_empty() => {
                        assert_eq!(0, pending_msg.len());
                        assert_eq!(String::from("good"), msg.get_data().get_string(1)?.into_owned());
                        self.proc.remove_proc(None).await?;
                        return Ok(())
                    },
                }
            }
        }

        async fn timers_timeout_run(&mut self) -> Result<(), BusError> {
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

        async fn pending_msgs_timeout_run(&mut self) -> Result<(), BusError> {
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

        pub(crate) fn run_timers(mut self) -> Result<(), BusError> {
            let rt: Runtime = Builder::new_current_thread()
                .enable_all()
                .thread_name("timer_thread")
                .build()
                .unwrap();
            rt.block_on(self.timers_timeout_run())
        }

        pub(crate) fn run_pending_msgs(mut self) -> Result<(), BusError> {
            let rt: Runtime = Builder::new_current_thread()
                .enable_all()
                .thread_name("pending_msg_thread")
                .build()
                .unwrap();
            rt.block_on(self.pending_msgs_timeout_run())
        }
    }

    #[test]
    fn test_pending() {
        /// Dummy settings
        #[settings]
        #[derive(Default, Debug, Serialize)]
        struct DummySettings {}

        // Create bus and main processor
        let (bus, main) = MainProc::<SimpleStringTvf>::create(&DummySettings::default());

        // Launch the main task
        let _main_task = main.run();

        // Launch the test processor
        assert_eq!(
            Ok(()),
            TestProc::<SimpleStringTvf>::create_raw(1, bus.clone()).run_timers()
        );

        assert_eq!(
            Ok(()),
            TestProc::<SimpleStringTvf>::create_raw(2, bus).run_pending_msgs()
        );
    }
}
