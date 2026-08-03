//! APN (Application Programming Node): service-call automatons.
//!
//! An APN lets a processor react to a service request by running an *automaton* that can sub-call
//! other services, branch on their results, and produce the final response — without writing a
//! manual state machine over the processor loop.
//!
//! An APN is launched with [`RequestMsg::apn`](crate::core::msg::RequestMsg::apn): it spawns the
//! automaton on a Tokio task, handing it an [`Apn`](crate::core::apn::Apn) handle plus the request's
//! service name and data. The automaton issues sub-calls with [`Apn::call`](crate::core::apn::Apn::call),
//! branches on their results, and returns the final `M`; the APN sends that result back to the original
//! caller on the request's response queue. The processor loop is **not blocked**.
//!
//! Because the automaton is spawned, it must be `Send + 'static`: it captures owned data and cannot
//! borrow the processor's state.
//!
//! An APN only processes service requests: the whole automaton runs under a timeout budget, and an
//! APN should never open sockets, wait on timers, or block for a long time. If you need any of those,
//! write a full ProSA processor instead.

use std::sync::Arc;
use std::time::Duration;

use tokio::spawn;
use tokio::sync::{mpsc, oneshot};
use tokio::time::Instant;

use super::{
    msg::{InternalMsg, Msg, RequestMsg, ResponseMsg, Tvf},
    service::{ServiceError, ServiceTable},
};

/// Handle used by an APN automaton to issue sub-calls to other services.
///
/// Each sub-call gets its own oneshot response channel, so a reply can never be mistaken for another
/// call's — no shared queue, no correlation needed. Sub-calls propagate the original request's trace
/// span, so their traces are nested under it.
///
/// An APN is launched with [`RequestMsg::apn`].
pub struct Apn<M>
where
    M: Sized + Clone + Tvf,
{
    service_table: Arc<ServiceTable<M>>,
    timeout: Duration,
    trace_id: Option<tracing::span::Id>,
}

impl<M> std::fmt::Debug for Apn<M>
where
    M: Sized + Clone + Tvf,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Apn")
            .field("timeout", &self.timeout)
            .field("trace_id", &self.trace_id)
            .finish()
    }
}

impl<M> Apn<M>
where
    M: Sized + Clone + Tvf,
{
    /// Create an APN from a service table snapshot and the overall automaton timeout budget.
    ///
    /// Usually you don't call this directly — use [`RequestMsg::apn`], which builds the APN and
    /// launches its automaton on a spawned task.
    pub(crate) fn new(
        service_table: Arc<ServiceTable<M>>,
        timeout: Duration,
        trace_id: Option<tracing::span::Id>,
    ) -> Apn<M> {
        Apn {
            service_table,
            timeout,
            trace_id,
        }
    }

    /// The trace span id this APN propagates to its sub-calls, or `None` if the original request
    /// carried no span. Sub-calls set it automatically; use this only if the automaton needs to build
    /// its own spans nested under the request.
    pub fn trace_id(&self) -> Option<&tracing::span::Id> {
        self.trace_id.as_ref()
    }

    /// Sub-call a service and await its response.
    ///
    /// This call is not individually timed out: it is bounded only by the APN's overall budget (the
    /// `timeout` given to [`RequestMsg::apn`], which caps the whole automaton). Use
    /// [`Apn::call_with_timeout`] to bound a single sub-call.
    ///
    /// It borrows `&self`, so several sub-calls can be issued concurrently (e.g. with
    /// [`tokio::join!`]); each one has its own response channel, so their replies never interfere.
    ///
    /// Returns the response on success, or a [`ServiceError`] if the service can't be reached or
    /// returns an error.
    pub async fn call(&self, service_name: &str, data: M) -> Result<ResponseMsg<M>, ServiceError> {
        let Some(proc_queue) = self
            .service_table
            .get_proc_service(service_name)
            .map(|proc_service| proc_service.proc_queue.clone())
        else {
            return Err(ServiceError::UnableToReachService(service_name.to_string()));
        };
        self.dispatch(&proc_queue, service_name, data, None).await
    }

    /// Sub-call a service and await its response, bounding this single call with an explicit timeout.
    ///
    /// Returns the response on success, or a [`ServiceError`] if the service can't be reached,
    /// doesn't respond within `timeout`, or returns an error.
    pub async fn call_with_timeout(
        &self,
        service_name: &str,
        data: M,
        timeout: Duration,
    ) -> Result<ResponseMsg<M>, ServiceError> {
        let Some(proc_queue) = self
            .service_table
            .get_proc_service(service_name)
            .map(|proc_service| proc_service.proc_queue.clone())
        else {
            return Err(ServiceError::UnableToReachService(service_name.to_string()));
        };
        self.dispatch(&proc_queue, service_name, data, Some(timeout))
            .await
    }

    /// Send a request to a processor queue and await its response on a dedicated oneshot channel.
    async fn dispatch(
        &self,
        proc_queue: &mpsc::Sender<InternalMsg<M>>,
        service_name: &str,
        data: M,
        timeout: Option<Duration>,
    ) -> Result<ResponseMsg<M>, ServiceError> {
        let (response_tx, response_rx) = oneshot::channel();
        let request = match &self.trace_id {
            Some(trace_id) => RequestMsg::new_with_trace_id(
                service_name.to_string(),
                data,
                response_tx,
                trace_id.clone(),
            ),
            None => RequestMsg::new(service_name.to_string(), data, response_tx),
        };

        if proc_queue
            .send(InternalMsg::Request(request))
            .await
            .is_err()
        {
            return Err(ServiceError::UnableToReachService(service_name.to_string()));
        }

        if let Some(timeout) = timeout {
            match tokio::time::timeout(timeout, response_rx).await {
                Ok(Ok(InternalMsg::Response(resp))) => Ok(resp),
                Ok(Ok(InternalMsg::Error(err))) => Err(err.into_err()),
                Ok(Ok(_)) => Err(ServiceError::ProtocolError(service_name.to_string())),
                Ok(Err(_recv)) => Err(ServiceError::UnableToReachService(service_name.to_string())),
                Err(_elapsed) => Err(ServiceError::Timeout(
                    service_name.to_string(),
                    timeout.as_millis() as u64,
                )),
            }
        } else {
            match response_rx.await {
                Ok(InternalMsg::Response(resp)) => Ok(resp),
                Ok(InternalMsg::Error(err)) => Err(err.into_err()),
                Ok(_) => Err(ServiceError::ProtocolError(service_name.to_string())),
                Err(_recv) => Err(ServiceError::UnableToReachService(service_name.to_string())),
            }
        }
    }
}

impl<M> RequestMsg<M>
where
    M: Sized
        + Clone
        + std::fmt::Debug
        + Tvf
        + Default
        + 'static
        + std::marker::Send
        + std::marker::Sync,
{
    /// Run an APN automaton for this request on a spawned Tokio task.
    ///
    /// The automaton is handed an [`Apn`] handle plus this request's **service name and data**. It
    /// issues sub-calls with [`Apn::call`], branches on their results, and returns the final `M`. That
    /// result is sent back to the original requestor on this request's response queue — you never call
    /// `return_to_sender` yourself. The request's trace span is available to the automaton through
    /// [`Apn::trace_id`].
    ///
    /// The automaton runs on its own task, so the processor loop is not blocked. Because it is
    /// spawned, the closure must be `Send + 'static`: it captures owned data only and cannot borrow
    /// the processor's state. The `timeout` is the overall budget for the whole automaton; keep it
    /// short.
    ///
    /// ```no_run
    /// use std::sync::Arc;
    /// use std::time::Duration;
    /// use prosa::core::msg::{Msg, RequestMsg, Tvf};
    /// use prosa::core::service::ServiceTable;
    /// use prosa_utils::msg::simple_string_tvf::SimpleStringTvf;
    ///
    /// fn handle(
    ///     request: RequestMsg<SimpleStringTvf>,
    ///     services: Arc<ServiceTable<SimpleStringTvf>>,
    /// ) {
    ///     request.apn(
    ///         services.clone(),
    ///         Duration::from_millis(500),
    ///         move |apn, _service, data| async move {
    ///             let mut resp = match data.get_unsigned(1).unwrap_or(0) {
    ///                 0 => apn.call("PAY", data).await?,
    ///                 _ => apn.call("REJECT", data).await?,
    ///             };
    ///             Ok(resp.take_data().unwrap_or_default())
    ///         },
    ///     );
    /// }
    /// ```
    ///
    /// Sub-calls can also run concurrently: [`Apn::call`] borrows `&self`, so fan out to distinct
    /// services with [`tokio::join!`] and await them together:
    ///
    /// ```no_run
    /// use std::sync::Arc;
    /// use std::time::Duration;
    /// use prosa::core::msg::{Msg, RequestMsg, Tvf};
    /// use prosa::core::service::ServiceTable;
    /// use prosa_utils::msg::simple_string_tvf::SimpleStringTvf;
    ///
    /// fn handle(
    ///     request: RequestMsg<SimpleStringTvf>,
    ///     services: Arc<ServiceTable<SimpleStringTvf>>,
    /// ) {
    ///     request.apn(
    ///         services.clone(),
    ///         Duration::from_millis(500),
    ///         move |apn, _service, data| async move {
    ///             // Fire both sub-calls, then await both.
    ///             let (pay, fraud) = tokio::join!(
    ///                 apn.call("PAY", data.clone()),
    ///                 apn.call("FRAUD", data),
    ///             );
    ///             let mut pay = pay?;
    ///             let _fraud = fraud?;
    ///             Ok(pay.take_data().unwrap_or_default())
    ///         },
    ///     );
    /// }
    /// ```
    pub fn apn<F, Fut>(
        mut self,
        service_table: Arc<ServiceTable<M>>,
        timeout: Duration,
        automaton: F,
    ) where
        F: FnOnce(Apn<M>, String, M) -> Fut + Send + 'static,
        Fut: Future<Output = Result<M, ServiceError>> + Send + 'static,
    {
        let apn = Apn::new(service_table, timeout, self.get_span().id());
        let service = self.get_service().clone();
        let data = self.take_data().unwrap_or_default();

        let deadline = Instant::now() + timeout;
        spawn(async move {
            let _ = match tokio::time::timeout_at(deadline, automaton(apn, service, data)).await {
                Ok(result) => self.return_result_to_sender(result),
                Err(_elapsed) => {
                    let service_name = self.get_service().to_string();
                    self.return_error_to_sender(
                        None,
                        ServiceError::Timeout(service_name, timeout.as_millis() as u64),
                    )
                }
            };
        });
    }
}

#[cfg(test)]
mod tests {
    extern crate self as prosa;

    use std::sync::Arc;
    use std::time::Duration;

    use prosa_macros::{proc, settings};
    use prosa_utils::msg::{simple_string_tvf::SimpleStringTvf, tvf::Tvf};
    use serde::Serialize;
    use tokio::sync::mpsc;
    use tokio::time::timeout;

    use super::Apn;
    use crate::core::{
        error::BusError,
        main::{Main, MainProc, MainRunnable},
        msg::{InternalMsg, Msg, RequestMsg},
        proc::{ProcBusParam, ProcConfig, ProcParam},
        service::{ProcService, ServiceError, ServiceTable},
    };
    use crate::stub::adaptor::StubParotAdaptor;
    use crate::stub::proc::{StubProc, StubSettings};

    /// Dummy settings for building throwaway `Main` handles in unit tests
    #[settings]
    #[derive(Default, Debug, Serialize)]
    struct DummySettings {}

    #[tokio::test]
    async fn apn_call_unreachable_service() {
        let apn: Apn<SimpleStringTvf> = Apn::new(
            Arc::new(ServiceTable::default()),
            Duration::from_millis(50),
            None,
        );
        let err = apn
            .call("NOPE", SimpleStringTvf::default())
            .await
            .expect_err("unreachable service should error");
        assert!(matches!(err, ServiceError::UnableToReachService(_)));
    }

    #[tokio::test]
    async fn apn_call_timeout() {
        // Build a service table pointing at a queue that no one ever reads: the request is
        // buffered but never answered, so the sub-call must time out.
        let (bus, _main): (Main<SimpleStringTvf>, MainProc<SimpleStringTvf>) =
            MainProc::create(&DummySettings::default(), None);
        let (queue_tx, _queue_rx) = mpsc::channel(8);
        let proc_param = ProcParam::new(1, "slow".to_string(), queue_tx.clone(), bus);
        let proc_service = ProcService::new(&proc_param, queue_tx, 0);

        let mut table = ServiceTable::default();
        table.add_service("SLOW", proc_service);

        let apn: Apn<SimpleStringTvf> = Apn::new(Arc::new(table), Duration::from_millis(30), None);
        let err = apn
            .call_with_timeout(
                "SLOW",
                SimpleStringTvf::default(),
                Duration::from_millis(30),
            )
            .await
            .expect_err("slow service should time out");
        assert!(matches!(err, ServiceError::Timeout(_, _)));

        // Keep the never-read receiver alive until the assertion is done.
        drop(_queue_rx);
    }

    #[proc]
    struct ApnTestProc {}

    #[proc]
    impl ApnTestProc<SimpleStringTvf> {
        async fn apn_run(&mut self) -> Result<(), BusError> {
            self.proc.add_proc().await?;
            self.proc
                .add_service_proc(vec![String::from("APN")])
                .await?;

            let mut sent = false;
            loop {
                if let Some(msg) = self.internal_rx_queue.recv().await {
                    match msg {
                        InternalMsg::Service(table) => {
                            self.service = table;
                            if !sent
                                && self.service.exist_proc_service("SUB1")
                                && self.service.exist_proc_service("SUB2")
                                && self.service.exist_proc_service("APN")
                            {
                                sent = true;
                                let mut data = SimpleStringTvf::default();
                                data.put_string(1, "start");
                                if let Some(service) = self.service.get_proc_service("APN") {
                                    service
                                        .proc_queue
                                        .send(InternalMsg::Request(RequestMsg::new(
                                            String::from("APN"),
                                            data,
                                            self.proc.get_service_queue(),
                                        )))
                                        .await
                                        .expect("APN request should be sent");
                                }
                            }
                        }
                        InternalMsg::Request(req) => {
                            // Run an APN for the request: the automaton chains SUB1 then SUB2.
                            req.apn(
                                self.service.clone(),
                                Duration::from_millis(500),
                                move |apn, _service, data| async move {
                                    let mut first = apn.call("SUB1", data).await?;
                                    let first_data = first.take_data().ok_or_else(|| {
                                        ServiceError::ProtocolError("SUB1".to_string())
                                    })?;
                                    let mut resp = apn.call("SUB2", first_data).await?;
                                    resp.take_data().ok_or_else(|| {
                                        ServiceError::ProtocolError("SUB2".to_string())
                                    })
                                },
                            );
                        }
                        InternalMsg::Response(resp) => {
                            assert_eq!("start", resp.get_data()?.get_string(1)?.into_owned());
                            self.proc.remove_proc(None).await?;
                            return Ok(());
                        }
                        InternalMsg::Error(err) => {
                            return Err(BusError::ProcComm(
                                self.get_proc_id(),
                                0,
                                format!("unexpected APN error: {:?}", err.get_err()),
                            ));
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    #[tokio::test]
    async fn apn_happy_path() {
        let (bus, main) = MainProc::<SimpleStringTvf>::create(&DummySettings::default(), Some(2));
        let main_task = tokio::spawn(main.run());

        // Stub offering the two sub-services the automaton chains through (parrot echoes data).
        let stub_proc = StubProc::<SimpleStringTvf>::create(
            1,
            String::from("stub"),
            bus.clone(),
            StubSettings::new(vec![String::from("SUB1"), String::from("SUB2")]),
        );
        crate::core::proc::Proc::<StubParotAdaptor>::run(stub_proc).expect("stub should run");

        let result = timeout(
            Duration::from_secs(5),
            ApnTestProc::<SimpleStringTvf>::create_raw(2, "apn_test".to_string(), bus.clone())
                .apn_run(),
        )
        .await
        .expect("APN test should not time out");
        assert_eq!(Ok(()), result);

        bus.stop("ProSA unit test end".into())
            .await
            .expect("ProSA should stop");
        main_task.await.expect("Main task should end correctly");
    }
}
