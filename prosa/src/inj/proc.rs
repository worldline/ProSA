use std::time::Duration;

use opentelemetry::{KeyValue, metrics::Histogram};
use prosa_macros::{proc, proc_settings};
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::{
    core::{
        adaptor::Adaptor,
        error::ProcError,
        msg::{InternalMsg, Msg, RequestMsg},
        proc::{Proc, ProcBusParam as _},
        service::ServiceError,
    },
    event::speed::Regulator,
};

use super::adaptor::InjAdaptor;

extern crate self as prosa;

/// Inj settings for service and speed parameters
#[proc_settings]
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct InjSettings {
    /// Service to inject to
    pub service_name: String,
    /// Max TPS speed
    #[serde(default = "InjSettings::default_max_speed")]
    pub max_speed: f64,
    /// Timeout for cooldown when a service don't respond well
    #[serde(default = "InjSettings::default_timeout_threshold")]
    pub timeout_threshold: Duration,
    /// Max parallel transaction running at the same time
    #[serde(default = "InjSettings::default_max_concurrents_send")]
    pub max_concurrents_send: u32,
    /// Number of value keep to calculate the injection speed
    #[serde(default = "InjSettings::default_speed_interval")]
    pub speed_interval: u16,
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

#[proc_settings]
impl Default for InjSettings {
    fn default() -> InjSettings {
        InjSettings {
            service_name: Default::default(),
            max_speed: InjSettings::default_max_speed(),
            timeout_threshold: InjSettings::default_timeout_threshold(),
            max_concurrents_send: InjSettings::default_max_concurrents_send(),
            speed_interval: InjSettings::default_speed_interval(),
        }
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
/// let (bus, main) = MainProc::<SimpleStringTvf>::create(&settings, Some(1));
///
/// // Launch an injector processor
/// let inj_settings = InjSettings::new("INJ_TEST".into());
/// let inj_proc = InjProc::<SimpleStringTvf>::create(1, "INJ_PROC".to_string(), bus.clone(), inj_settings);
/// Proc::<InjDummyAdaptor>::run(inj_proc);
///
/// // Wait on main task
/// //main.run().await;
/// ```
#[proc(settings = prosa::inj::proc::InjSettings)]
pub struct InjProc {}

#[proc]
impl InjProc {
    async fn process_internal<A>(
        &mut self,
        msg: InternalMsg<M>,
        adaptor: &mut A,
        regulator: &mut Regulator,
        next_transaction: &mut Option<M>,
        meter_trans_duration: &Histogram<f64>,
    ) -> Result<(), Box<dyn ProcError + Send + Sync>>
    where
        A: Adaptor + InjAdaptor<M> + std::marker::Send + std::marker::Sync,
    {
        match msg {
            InternalMsg::Request(msg) => panic!(
                "The inj processor {} receive a request {:?}",
                self.get_proc_id(),
                msg
            ),
            InternalMsg::Response(mut msg) => {
                let response_data = msg.take_data();
                let _enter_span = msg.enter_span();
                meter_trans_duration.record(
                    msg.elapsed().as_secs_f64(),
                    &[
                        KeyValue::new("proc", self.name().to_string()),
                        KeyValue::new("service", msg.get_service().clone()),
                        KeyValue::new("err_code", "0".to_string()),
                    ],
                );

                if let Some(response) = response_data {
                    debug!(name: "resp_inj_proc", target: "prosa::inj::proc", proc_name = self.name(), service = msg.get_service(), "{:?}", response);
                    adaptor.process_response(response, msg.get_service())?;

                    regulator.notify_receive_transaction(msg.elapsed());

                    // Build the next transaction
                    let _ = next_transaction.get_or_insert(adaptor.build_transaction());
                }
            }
            InternalMsg::Error(err_msg) => {
                let enter_span = err_msg.enter_span();
                meter_trans_duration.record(
                    err_msg.elapsed().as_secs_f64(),
                    &[
                        KeyValue::new("proc", self.name().to_string()),
                        KeyValue::new("service", err_msg.get_service().clone()),
                        KeyValue::new("err_code", err_msg.get_err().get_code().to_string()),
                    ],
                );

                debug!(name: "resp_err_inj_proc", target: "prosa::inj::proc", proc_name = self.name(), service = err_msg.get_service(), "{:?}", err_msg.get_err());

                match err_msg.get_err() {
                    ServiceError::Timeout(_, overhead) => {
                        regulator.add_tick_overhead(Duration::from_millis(*overhead));
                    }
                    ServiceError::UnableToReachService(_) => {
                        regulator.add_tick_overhead(self.settings.timeout_threshold);
                    }
                    _ => {
                        drop(enter_span);
                        return Err(Box::new(err_msg.into_err()));
                    }
                }

                regulator.notify_receive_transaction(err_msg.elapsed());

                // Build the next transaction
                let _ = next_transaction.get_or_insert(adaptor.build_transaction());
            }
            InternalMsg::Command(_) => todo!(),
            InternalMsg::Config => todo!(),
            InternalMsg::Service(table) => self.service = table,
            InternalMsg::Shutdown => {
                adaptor.terminate();
                self.proc.remove_proc(None).await?;
                return Ok(());
            }
        }

        Ok(())
    }
}

#[proc]
impl<A> Proc<A> for InjProc
where
    A: Adaptor + InjAdaptor<M> + std::marker::Send + std::marker::Sync,
{
    async fn internal_run(&mut self) -> Result<(), Box<dyn ProcError + Send + Sync>> {
        // Initiate an adaptor for the inj processor
        let mut adaptor = A::new(self)?;

        // meter
        let meter = self.proc.meter("prosa_inj");
        let meter_trans_duration = meter
            .f64_histogram("prosa_inj_request_duration")
            .with_description("inj transaction processing duration")
            .with_unit("seconds")
            .build();

        // Declare the processor
        self.proc.add_proc().await?;

        // Create a message regulator
        let mut regulator = self.settings.get_regulator();
        let mut next_transaction = Some(adaptor.build_transaction());

        // Wait for service table
        while !self.service.exist_proc_service(&self.settings.service_name) {
            if let Some(msg) = self.internal_rx_queue.recv().await {
                self.process_internal(
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
            .get_proc_service(&self.settings.service_name)
            .unwrap()
            .proc_queue
            .send(InternalMsg::Request(RequestMsg::new(
                self.settings.service_name.clone(),
                next_transaction.take().unwrap(),
                self.proc.get_service_queue(),
            )))
            .await?;
        regulator.notify_send_transaction();

        loop {
            tokio::select! {
                Some(msg) = self.internal_rx_queue.recv() => {
                    self.process_internal(msg, &mut adaptor, &mut regulator, &mut next_transaction, &meter_trans_duration).await?;
                }
                _ = regulator.tick(), if self.service.exist_proc_service(&self.settings.service_name) => {
                    if let Some(service) = self.service.get_proc_service(&self.settings.service_name) {
                        let trans = if let Some(transaction) = next_transaction.take() {
                            RequestMsg::new(self.settings.service_name.clone(), transaction, self.proc.get_service_queue())
                        } else {
                            RequestMsg::new(self.settings.service_name.clone(), adaptor.build_transaction(), self.proc.get_service_queue())
                        };

                        debug!(name: "inj_proc", target: "prosa::inj::proc", parent: trans.get_span(), proc_name = self.name(), service = self.settings.service_name, "{:?}", trans.get_data());
                        service.proc_queue.send(InternalMsg::Request(trans)).await?;

                        regulator.notify_send_transaction();
                    }
                },
            };
        }
    }
}
