use std::time::Duration;

use opentelemetry::{
    metrics::{Histogram, Unit},
    KeyValue,
};
use prosa_macros::{proc, proc_settings};
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::{
    core::{
        adaptor::Adaptor,
        msg::{InternalMsg, Msg, RequestMsg},
        proc::{Proc, ProcBusParam as _},
    },
    event::speed::Regulator,
};

use super::adaptor::InjAdaptor;

extern crate self as prosa;

/// Inj settings for service and speed parameters
#[proc_settings]
#[derive(Default, Debug, Deserialize, Serialize, Clone)]
pub struct InjSettings {
    /// Service to inject to
    service_name: String,
    /// Max TPS speed
    #[serde(default = "InjSettings::default_max_speed")]
    max_speed: f64,
    /// Timeout for cooldown when a service don't respond well
    #[serde(default = "InjSettings::default_timeout_threshold")]
    timeout_threshold: Duration,
    /// Max parallel transaction running at the same time
    #[serde(default = "InjSettings::default_max_concurrents_send")]
    max_concurrents_send: u32,
    /// Number of value keep to calculate the injection speed
    #[serde(default = "InjSettings::default_speed_interval")]
    speed_interval: u16,
}

impl InjSettings {
    fn default_max_speed() -> f64 {
        5.0
    }

    fn default_timeout_threshold() -> Duration {
        Duration::new(10, 0)
    }

    fn default_max_concurrents_send() -> u32 {
        1
    }

    fn default_speed_interval() -> u16 {
        15
    }

    /// Create a new Inj settings
    pub fn new(service_name: String) -> InjSettings {
        InjSettings {
            service_name,
            max_speed: InjSettings::default_max_speed(),
            timeout_threshold: InjSettings::default_timeout_threshold(),
            max_concurrents_send: InjSettings::default_max_concurrents_send(),
            speed_interval: InjSettings::default_speed_interval(),
            ..Default::default()
        }
    }

    /// Setter of the service name to send the transaction to
    pub fn set_service_name(&mut self, service_name: String) {
        self.service_name = service_name;
    }

    /// Getter of a regulator from the current settings
    pub fn get_regulator(&self) -> Regulator {
        Regulator::new(
            self.max_speed,
            self.timeout_threshold,
            self.max_concurrents_send,
            self.speed_interval,
        )
    }
}

/// Inj processor to inject transactions
///
/// ```
/// use prosa::core::main::{MainProc, MainRunnable};
/// use prosa::core::proc::{proc, Proc, ProcBusParam, ProcConfig};
/// use prosa::inj::adaptor::InjDummyAdaptor;
/// use prosa::inj::proc::{InjProc, InjSettings};
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
/// // Launch an injector processor
/// let inj_settings = InjSettings::new("INJ_TEST".into());
/// let inj_proc = InjProc::<SimpleStringTvf>::create(1, bus.clone(), inj_settings);
/// Proc::<InjDummyAdaptor>::run(inj_proc, String::from("INJ_PROC"));
///
/// // Wait on main task
/// //main_task.join().unwrap();
/// ```
#[proc(settings = prosa::inj::proc::InjSettings)]
pub struct InjProc {}

#[proc]
impl InjProc {
    async fn process_internal<A>(
        &mut self,
        name: &str,
        msg: InternalMsg<M>,
        adaptor: &mut A,
        regulator: &mut Regulator,
        next_transaction: &mut Option<M>,
        meter_trans_duration: &Histogram<f64>,
    ) -> Result<(), Box<dyn std::error::Error>>
    where
        A: Default + Adaptor + InjAdaptor<M> + std::marker::Send + std::marker::Sync,
    {
        match msg {
            InternalMsg::Request(msg) => panic!(
                "The inj processor {} receive a request {:?}",
                self.get_proc_id(),
                msg
            ),
            InternalMsg::Response(msg) => {
                let _enter_span = msg.enter_span();
                meter_trans_duration.record(
                    msg.elapsed().as_secs_f64(),
                    &[
                        KeyValue::new("proc", name.to_string()),
                        KeyValue::new("service", msg.get_service().clone()),
                    ],
                );

                debug!(name: "resp_inj_proc", target: "prosa::inj::proc", proc_name = name, service = msg.get_service(), response = format!("{:?}", msg.get_data()));
                adaptor.process_response(msg.get_data(), msg.get_service())?;

                regulator.notify_receive_transaction(msg.elapsed());

                // Build the next transaction
                let _ = next_transaction.get_or_insert(adaptor.build_transaction());
            }
            InternalMsg::Error(err) => panic!(
                "The inj processor {} receive an error {:?}",
                self.get_proc_id(),
                err
            ),
            InternalMsg::Command(_) => todo!(),
            InternalMsg::Config => todo!(),
            InternalMsg::Service(table) => self.service = table,
            InternalMsg::Shutdown => {
                adaptor.terminate();
                self.proc.rm_proc().await?;
                return Ok(());
            }
        }

        Ok(())
    }
}

#[proc]
impl<A> Proc<A> for InjProc
where
    A: Default + Adaptor + InjAdaptor<M> + std::marker::Send + std::marker::Sync,
{
    async fn internal_run(&mut self, name: String) -> Result<(), Box<dyn std::error::Error>> {
        // Initiate an adaptor for the inj processor
        let mut adaptor = A::default();
        adaptor.init(self)?;

        // meter
        let meter = self.proc.meter(name.clone());
        let meter_trans_duration = meter
            .f64_histogram("prosa_inj_request_duration")
            .with_description("inj transaction processing duration")
            .with_unit(Unit::new("seconds"))
            .init();

        // Declare the processor
        self.proc.add_proc().await?;

        // Create a message regulator
        let mut regulator = self.settings.get_regulator();
        let mut next_transaction = Some(adaptor.build_transaction());
        let mut msg_id: u64 = 0;

        // Wait for service table
        while !self.service.exist_proc_service(&self.settings.service_name) {
            if let Some(msg) = self.internal_rx_queue.recv().await {
                self.process_internal(
                    name.as_str(),
                    msg,
                    &mut adaptor,
                    &mut regulator,
                    &mut next_transaction,
                    &meter_trans_duration,
                )
                .await?;
            }
        }

        // Send first transaction
        self.service
            .get_proc_service(&self.settings.service_name, msg_id)
            .unwrap()
            .proc_queue
            .send(InternalMsg::Request(RequestMsg::new(
                msg_id,
                self.settings.service_name.clone(),
                next_transaction.take().unwrap(),
                self.proc.get_service_queue(),
            )))
            .await?;
        msg_id += 1;
        regulator.notify_send_transaction();

        loop {
            tokio::select! {
                Some(msg) = self.internal_rx_queue.recv() => {
                    self.process_internal(name.as_str(), msg, &mut adaptor, &mut regulator, &mut next_transaction, &meter_trans_duration).await?;
                }
                _ = regulator.tick() => {
                    if let Some(service) = self.service.get_proc_service(&self.settings.service_name, msg_id) {
                        let trans = if let Some(transaction) = next_transaction.take() {
                            RequestMsg::new(msg_id, self.settings.service_name.clone(), transaction, self.proc.get_service_queue())
                        } else {
                            RequestMsg::new(msg_id, self.settings.service_name.clone(), adaptor.build_transaction(), self.proc.get_service_queue())
                        };

                        debug!(name: "inj_proc", target: "prosa::inj::proc", parent: trans.get_span(), proc_name = name, service = self.settings.service_name, request = format!("{:?}", trans.get_data()));
                        service.proc_queue.send(InternalMsg::Request(trans)).await?;

                        msg_id += 1;
                        regulator.notify_send_transaction();
                    }
                },
            };
        }
    }
}
