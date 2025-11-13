use std::sync::Arc;

use prosa_macros::proc_settings;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::core::adaptor::{Adaptor, MaybeAsync};
use crate::core::error::{BusError, ProcError};
use crate::core::msg::{InternalMsg, Msg};
use crate::core::proc::{Proc, ProcBusParam, proc};

use super::adaptor::StubAdaptor;

extern crate self as prosa;

/// Stub settings to list all services tu stub
#[proc_settings]
#[derive(Default, Debug, Deserialize, Serialize, Clone)]
pub struct StubSettings {
    /// Services to respond to
    pub service_names: Vec<String>,
}

impl StubSettings {
    /// Create a new Stub settings
    pub fn new(service_names: Vec<String>) -> StubSettings {
        StubSettings {
            service_names,
            ..Default::default()
        }
    }

    /// Method to add service name
    pub fn add_service_name(&mut self, service_name: String) {
        self.service_names.push(service_name);
    }
}

/// Stub processor to respond to a request
///
/// ```
/// use prosa::core::main::{MainProc, MainRunnable};
/// use prosa::core::proc::{proc, Proc, ProcBusParam, ProcConfig};
/// use prosa::stub::adaptor::StubParotAdaptor;
/// use prosa::stub::proc::{StubProc, StubSettings};
/// use prosa_utils::config::observability::Observability;
/// use prosa_utils::msg::simple_string_tvf::SimpleStringTvf;
/// use prosa::core::settings::settings;
/// use serde::Serialize;
///
/// // Main settings
/// #[settings]
/// #[derive(Default, Debug, Serialize)]
/// struct Settings {}
///
/// // Create bus and main processor
/// let settings = Settings::default();
/// let (bus, main) = MainProc::<SimpleStringTvf>::create(&settings);
///
/// // Launch a stub processor
/// let stub_settings = StubSettings::new(vec![String::from("STUB_TEST")]);
/// let stub_proc = StubProc::<SimpleStringTvf>::create(1, "STUB_PROC".to_string(), bus.clone(), stub_settings);
/// Proc::<StubParotAdaptor>::run(stub_proc);
///
/// // Wait on main task
/// //main_task.await;
/// ```
#[proc(settings = prosa::stub::proc::StubSettings)]
pub struct StubProc {}

#[proc]
impl<A> Proc<A> for StubProc
where
    A: 'static + Adaptor + StubAdaptor<M> + std::marker::Send + std::marker::Sync,
{
    async fn internal_run(&mut self) -> Result<(), Box<dyn ProcError + Send + Sync>> {
        // Initiate an adaptor for the stub processor
        let adaptor = Arc::new(A::new(self)?);

        // Declare the processor
        self.proc.add_proc().await?;

        // Add all service to listen
        self.proc
            .add_service_proc(self.settings.service_names.clone())
            .await?;

        loop {
            if let Some(msg) = self.internal_rx_queue.recv().await {
                match msg {
                    InternalMsg::Request(mut msg) => {
                        let request_data = msg.take_data().ok_or(BusError::NoData)?;
                        let enter_span = msg.enter_span();

                        debug!(name: "stub_proc_request", target: "prosa::stub::proc", parent: msg.get_span(), proc_name = self.name(), stub_service = msg.get_service(), "{:?}", msg.get_data());

                        match adaptor.process_request(msg.get_service(), request_data) {
                            MaybeAsync::Ready(Ok(resp)) => {
                                debug!(name: "stub_proc_response", target: "prosa::stub::proc", parent: msg.get_span(), stub_service = msg.get_service(), "{resp:?}");
                                drop(enter_span);
                                let _ = msg.return_to_sender(resp);
                            }
                            MaybeAsync::Ready(Err(err)) => {
                                debug!(name: "stub_proc_error", target: "prosa::stub::proc", parent: msg.get_span(), stub_service = msg.get_service(), "{err}");
                                drop(enter_span);
                                let _ = msg.return_error_to_sender(None, err);
                            }
                            MaybeAsync::Future(future_resp) => {
                                drop(enter_span);
                                tokio::spawn(async move {
                                    let enter_span = msg.enter_span();
                                    let resp_data = future_resp.await;
                                    match resp_data {
                                        Ok(data) => {
                                            debug!(name: "stub_proc_response", target: "prosa::stub::proc", parent: msg.get_span(), stub_service = msg.get_service(), "{data:?}");
                                            drop(enter_span);
                                            let _ = msg.return_to_sender(data);
                                        }
                                        Err(err) => {
                                            debug!(name: "stub_proc_error", target: "prosa::stub::proc", parent: msg.get_span(), stub_service = msg.get_service(), "{err}");
                                            drop(enter_span);
                                            let _ = msg.return_error_to_sender(None, err);
                                        }
                                    }
                                });
                            }
                        }
                    }
                    InternalMsg::Response(msg) => panic!(
                        "The stub processor {} receive a response {:?}",
                        self.get_proc_id(),
                        msg
                    ),
                    InternalMsg::Error(err) => panic!(
                        "The stub processor {} receive an error {:?}",
                        self.get_proc_id(),
                        err
                    ),
                    InternalMsg::Command(_) => todo!(),
                    InternalMsg::Config => todo!(),
                    InternalMsg::Service(table) => self.service = table,
                    InternalMsg::Shutdown => {
                        adaptor.terminate();
                        self.proc.remove_proc(None).await?;
                        return Ok(());
                    }
                }
            }
        }
    }
}
