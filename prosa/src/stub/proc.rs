use prosa_macros::proc_settings;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::core::adaptor::Adaptor;
use crate::core::msg::{InternalMsg, Msg};
use crate::core::proc::{proc, Proc, ProcBusParam};

use super::adaptor::StubAdaptor;

extern crate self as prosa;

/// Stub settings to list all services tu stub
#[proc_settings]
#[derive(Default, Debug, Deserialize, Serialize, Clone)]
pub struct StubSettings {
    service_names: Vec<String>,
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
/// // Launch the main task
/// let main_task = main.run();
///
/// // Launch a stub processor
/// let stub_settings = StubSettings::new(vec![String::from("STUB_TEST")]);
/// let stub_proc = StubProc::<SimpleStringTvf>::create(1, bus.clone(), stub_settings);
/// Proc::<StubParotAdaptor>::run(stub_proc, String::from("STUB_PROC"));
///
/// // Wait on main task
/// //main_task.join().unwrap();
/// ```
#[proc(settings = prosa::stub::proc::StubSettings)]
pub struct StubProc {}

#[proc]
impl<A> Proc<A> for StubProc
where
    A: Default + Adaptor + StubAdaptor<M> + std::marker::Send + std::marker::Sync,
{
    async fn internal_run(&mut self, name: String) -> Result<(), Box<dyn std::error::Error>> {
        // Initiate an adaptor for the stub processor
        let mut adaptor = A::default();
        adaptor.init(self)?;

        // Declare the processor
        self.proc.add_proc().await?;

        // Add all service to listen
        self.proc
            .add_service_proc(self.settings.service_names.clone())
            .await?;

        loop {
            if let Some(msg) = self.internal_rx_queue.recv().await {
                match msg {
                    InternalMsg::REQUEST(msg) => {
                        let resp_data = adaptor.process_request(msg.get_service(), msg.get_data());
                        debug!(name: "stub_proc", target: "prosa::stub::proc", parent: msg.get_span(), proc_name = name, stub_service = msg.get_service(), stub_req = format!("{:?}", msg.get_data()).to_string(), stub_resp = format!("{:?}", resp_data));
                        msg.return_to_sender(resp_data).await.unwrap()
                    }
                    InternalMsg::RESPONSE(msg) => panic!(
                        "The stub processor {} receive a response {:?}",
                        self.get_proc_id(),
                        msg
                    ),
                    InternalMsg::ERROR(err) => panic!(
                        "The stub processor {} receive an error {:?}",
                        self.get_proc_id(),
                        err
                    ),
                    InternalMsg::COMMAND(_) => todo!(),
                    InternalMsg::CONFIG => todo!(),
                    InternalMsg::SERVICE(table) => self.service = table,
                    InternalMsg::SHUTDOWN => {
                        adaptor.terminate();
                        self.proc.rm_proc().await?;
                        return Ok(());
                    }
                }
            }
        }
    }
}
