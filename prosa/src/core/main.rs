//!
//! <svg width="40" height="40">
#![doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/doc_assets/main.svg"))]
//! </svg>
//!
//! Define ProSA main processing to bring asynchronous handler for all processors.
//!
//! Main can be consider as a service bus that routing processor messages.

use crate::core::queue::SendError;

use super::{
    error::{BusError, ProcError},
    msg::{InternalMainMsg, InternalMsg, Tvf},
    proc::ProcBusParam,
    service::{ProcService, ServiceTable},
    settings::Settings,
};
use opentelemetry::logs::LoggerProvider as _;
use opentelemetry::metrics::{Meter, MeterProvider as _};
use opentelemetry::trace::TracerProvider as _;
use opentelemetry::{InstrumentationScope, KeyValue};
use opentelemetry_appender_log::OpenTelemetryLogBridge;
use std::borrow::Cow;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::{collections::HashMap, fmt::Debug};
use tokio::{signal, sync::mpsc};
use tracing::{debug, info, warn};

/// Trait to define a ProSA main processor that is runnable
pub trait MainRunnable<M>
where
    M: Sized + Clone + Tvf,
{
    /// Method to create and run the main task (must be called before processor creation)
    fn create<S: Settings>(settings: &S) -> (Main<M>, Self);

    /// Method call to run the main task (should be called before processor creation)
    fn run(self) -> impl std::future::Future<Output = ()> + Send;
}

#[cfg_attr(doc, aquamarine::aquamarine)]
/// Main ProSA task to handle every task spawn in the ProSA
/// Use an internal ProSA service bus
/// Must be run only one time in the ProSA
///
/// This is the core strucutre of ProSA.
/// ```mermaid
/// graph LR
///     table>Service table]
///     main_queue[(Main queue)]
///     main_task[Main task]
///     proc_queue[(Processor queue)]
///     proc_task[Processor task]
///     proc_io[Processor IOs]
///
///     table <--> main_task
///     table --> proc_task
///     proc_task --> main_queue
///
///     subgraph main[ProSA Main processor]
///     main_queue --> main_task
///     end
///
///     subgraph proc[ProSA Processor]
///     proc_queue --> proc_task
///     proc_task <--> proc_io
///     end
/// ```
#[derive(Clone, Debug)]
pub struct Main<M>
where
    M: Sized + Clone + Tvf,
{
    internal_tx_queue: mpsc::Sender<InternalMainMsg<M>>,
    name: String,
    prometheus_registry: prometheus::Registry,
    meter_provider: opentelemetry_sdk::metrics::SdkMeterProvider,
    logger_provider: opentelemetry_sdk::logs::SdkLoggerProvider,
    tracer_provider: opentelemetry_sdk::trace::SdkTracerProvider,
    stop: Arc<AtomicBool>,
}

impl<M> ProcBusParam for Main<M>
where
    M: Sized + Clone + Tvf,
{
    fn get_proc_id(&self) -> u32 {
        0
    }

    fn name(&self) -> &str {
        self.name.as_str()
    }
}

impl<M> Main<M>
where
    M: Sized + Clone + Debug + Tvf + Default + 'static + std::marker::Send + std::marker::Sync,
{
    /// Method to instanciate a ProSA main task
    /// Must be called only one time
    pub fn new<S: Settings>(
        internal_tx_queue: mpsc::Sender<InternalMainMsg<M>>,
        settings: &S,
    ) -> Main<M> {
        let prometheus_registry = prometheus::Registry::new();
        let meter_provider = settings
            .get_observability()
            .build_meter_provider(&prometheus_registry);
        let logger_provider = settings.get_observability().build_logger_provider();
        let otel_log_appender = OpenTelemetryLogBridge::new(&logger_provider);
        let _ = log::set_boxed_logger(Box::new(otel_log_appender));
        log::set_max_level(settings.get_observability().get_logger_level().into());

        Main {
            internal_tx_queue,
            name: settings.get_prosa_name(),
            prometheus_registry,
            meter_provider,
            logger_provider,
            tracer_provider: settings.get_observability().build_tracer_provider(),
            stop: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Getter of the main bus
    pub fn get_bus_queue(&self) -> mpsc::Sender<InternalMainMsg<M>> {
        self.internal_tx_queue.clone()
    }

    /// Getter of the Prometheus registry
    pub fn get_prometheus_registry(&self) -> &prometheus::Registry {
        &self.prometheus_registry
    }

    /// Method to declare a new processor on the main bus
    pub async fn add_proc_queue(
        &self,
        proc: ProcService<M>,
    ) -> Result<(), SendError<InternalMainMsg<M>>> {
        Ok(self
            .internal_tx_queue
            .send(InternalMainMsg::NewProcQueue(proc.clone()))
            .await?)
    }

    /// Method to remove an entire processor from the main bus
    pub async fn remove_proc(
        &self,
        proc_id: u32,
        proc_err: Option<Box<dyn ProcError + Send + Sync>>,
    ) -> Result<(), SendError<InternalMainMsg<M>>> {
        Ok(self
            .internal_tx_queue
            .send(InternalMainMsg::DeleteProc(proc_id, proc_err))
            .await?)
    }

    /// Method to declare a new processor on the main bus
    pub async fn remove_proc_queue(
        &self,
        proc_id: u32,
        queue_id: u32,
    ) -> Result<(), SendError<InternalMainMsg<M>>> {
        Ok(self
            .internal_tx_queue
            .send(InternalMainMsg::DeleteProcQueue(proc_id, queue_id))
            .await?)
    }

    /// Method to declare a new service for a whole processor on the main bus
    pub async fn add_service_proc(
        &self,
        names: Vec<String>,
        proc_id: u32,
    ) -> Result<(), SendError<InternalMainMsg<M>>> {
        Ok(self
            .internal_tx_queue
            .send(InternalMainMsg::NewProcService(names, proc_id))
            .await?)
    }

    /// Method to declare a new service for a processor queue on the main bus
    pub async fn add_service(
        &self,
        names: Vec<String>,
        proc_id: u32,
        queue_id: u32,
    ) -> Result<(), SendError<InternalMainMsg<M>>> {
        Ok(self
            .internal_tx_queue
            .send(InternalMainMsg::NewService(names, proc_id, queue_id))
            .await?)
    }

    /// Method to remove a service for a whole processor from the main bus
    pub async fn remove_service_proc(
        &self,
        names: Vec<String>,
        proc_id: u32,
    ) -> Result<(), SendError<InternalMainMsg<M>>> {
        Ok(self
            .internal_tx_queue
            .send(InternalMainMsg::DeleteProcService(names, proc_id))
            .await?)
    }

    /// Method to remove a service from the main bus
    pub async fn remove_service(
        &self,
        names: Vec<String>,
        proc_id: u32,
        queue_id: u32,
    ) -> Result<(), SendError<InternalMainMsg<M>>> {
        Ok(self
            .internal_tx_queue
            .send(InternalMainMsg::DeleteService(names, proc_id, queue_id))
            .await?)
    }

    /// Indicates whether ProSA is stopping
    pub fn is_stopping(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    /// Method to stop all processors
    pub async fn stop(&self, reason: String) -> Result<(), SendError<InternalMainMsg<M>>> {
        self.stop.store(true, Ordering::Relaxed);
        Ok(self
            .internal_tx_queue
            .send(InternalMainMsg::Shutdown(reason))
            .await?)
    }

    /// Provide the ProSA name based on ProSA settings
    pub fn name(&self) -> &String {
        &self.name
    }

    /// Provide the opentelemetry Meter based on ProSA settings
    pub fn meter(&self, name: &'static str) -> opentelemetry::metrics::Meter {
        self.meter_provider.meter(name)
    }

    /// Provide the opentelemetry Logger based on ProSA settings
    pub fn logger(&self, name: impl Into<Cow<'static, str>>) -> opentelemetry_sdk::logs::SdkLogger {
        self.logger_provider.logger(name)
    }

    /// Provide the opentelemetry Tracer based on ProSA settings
    pub fn tracer(&self, name: impl Into<Cow<'static, str>>) -> opentelemetry_sdk::trace::Tracer {
        let scope = InstrumentationScope::builder(name)
            .with_version(env!("CARGO_PKG_VERSION"))
            .with_attributes([KeyValue::new("prosa_name", self.name.clone())])
            .build();
        self.tracer_provider.tracer_with_scope(scope)
    }
}

/// Main ProSA task processor
pub struct MainProc<M>
where
    M: Sized + Clone + Tvf,
{
    name: String,
    processors: HashMap<u32, HashMap<u32, ProcService<M>>>,
    services: Arc<ServiceTable<M>>,
    internal_rx_queue: mpsc::Receiver<InternalMainMsg<M>>,
    meter: Meter,
    stop: Arc<AtomicBool>,
}

impl<M> ProcBusParam for MainProc<M>
where
    M: Sized + Clone + Tvf,
{
    fn get_proc_id(&self) -> u32 {
        0
    }

    fn name(&self) -> &str {
        self.name.as_str()
    }
}

impl<M> MainProc<M>
where
    M: Sized + Clone + Debug + Tvf + Default + 'static + std::marker::Send + std::marker::Sync,
{
    /// Getter of the number of processors' queues
    fn get_proc_queue_len(&self) -> usize {
        let mut proc_queue_len = 0;
        for proc_queue in self.processors.values() {
            proc_queue_len += proc_queue.len();
        }

        proc_queue_len
    }

    async fn remove_proc(&mut self, proc_id: u32) -> Option<HashMap<u32, ProcService<M>>> {
        if let Some(proc) = self.processors.remove(&proc_id) {
            let mut new_services = (*self.services).clone();
            new_services.remove_proc_services(proc_id);
            self.services = Arc::new(new_services);
            Some(proc)
        } else {
            None
        }
    }

    async fn remove_proc_queue(&mut self, proc_id: u32, queue_id: u32) -> Option<ProcService<M>> {
        if let Some(proc_service) = self.processors.get_mut(&proc_id) {
            if let Some(proc_queue) = proc_service.remove(&queue_id) {
                let mut new_services = (*self.services).clone();
                new_services.remove_proc_queue_services(
                    proc_queue.get_proc_id(),
                    proc_queue.get_queue_id(),
                );
                self.services = Arc::new(new_services);
                Some(proc_queue)
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Method to notify all processor that the service table have changed
    async fn notify_srv_proc_queue(&self) -> Result<(), BusError> {
        for proc in self.processors.values() {
            for proc_service in proc.values() {
                if let Err(e) = proc_service
                    .proc_queue
                    .send(InternalMsg::Service(self.services.clone()))
                    .await
                {
                    // FIXME match the error. If it's a capacity error, don't drop the processor do something else
                    return Err(BusError::ProcComm(
                        proc_service.get_proc_id(),
                        proc_service.get_queue_id(),
                        e.to_string(),
                    ));
                }
            }
        }

        Ok(())
    }

    /// Method to notify all processor that the service table have changed
    async fn notify_srv_proc(&mut self) -> bool {
        if let Err(BusError::ProcComm(proc_id, queue_id, _)) = self.notify_srv_proc_queue().await {
            // The processor doesn't exist anymore so remove it
            if queue_id > 0 {
                self.remove_proc_queue(proc_id, queue_id).await;
            } else {
                self.remove_proc(proc_id).await;
            }

            false
        } else {
            true
        }
    }

    /// Method to shutdown all processors (return `true` if all processor are off, `false` otherwise)
    async fn stop(&mut self) -> bool {
        self.stop.store(true, Ordering::Relaxed);
        let mut is_stopped = true;
        for proc in self.processors.values() {
            for proc_service in proc.values() {
                if let Err(e) = proc_service.proc_queue.send(InternalMsg::Shutdown).await {
                    debug!("The {:?} seems already stopped: {}", proc_service, e);
                } else {
                    is_stopped = false;
                }
            }
        }

        is_stopped
    }
}

impl<M> MainRunnable<M> for MainProc<M>
where
    M: Sized + Clone + Debug + Tvf + Default + 'static + std::marker::Send + std::marker::Sync,
{
    fn create<S: Settings>(settings: &S) -> (Main<M>, MainProc<M>) {
        fn inner<M>(
            main: Main<M>,
            internal_rx_queue: mpsc::Receiver<InternalMainMsg<M>>,
        ) -> (Main<M>, MainProc<M>)
        where
            M: Sized
                + Clone
                + Debug
                + Tvf
                + Default
                + 'static
                + std::marker::Send
                + std::marker::Sync,
        {
            let name = main.name().clone();
            let meter = main.meter("prosa_main_task_meter");
            let stop = main.stop.clone();
            (
                main,
                MainProc {
                    name,
                    processors: Default::default(),
                    services: Arc::new(ServiceTable::default()),
                    internal_rx_queue,
                    meter,
                    stop,
                },
            )
        }
        let (internal_tx_queue, internal_rx_queue) = mpsc::channel(2048);
        inner(Main::new(internal_tx_queue, settings), internal_rx_queue)
    }

    async fn run(mut self) {
        // Monitor RAM usage
        let prosa_name = self.name.clone();
        self.meter
            .u64_observable_gauge("prosa_main_ram")
            .with_description("RAM consumed by ProSA")
            .with_unit("bytes")
            .with_callback(move |observer| {
                if let Some(usage) = memory_stats::memory_stats() {
                    observer.observe(
                        usage.physical_mem as u64,
                        &[
                            KeyValue::new("prosa_name", prosa_name.clone()),
                            KeyValue::new("type", "physical"),
                        ],
                    );
                    observer.observe(
                        usage.virtual_mem as u64,
                        &[
                            KeyValue::new("prosa_name", prosa_name.clone()),
                            KeyValue::new("type", "virtual"),
                        ],
                    );
                }
            })
            .build();

        // Monitor services
        let services_meter = self
            .meter
            .u64_gauge("prosa_main_services")
            .with_description("Services declared to the main task")
            .build();
        // Monitor processors objects
        let mut crashed_proc = 0;
        let mut restarted_proc = 0;
        let processors_meter = self
            .meter
            .u64_gauge("prosa_processors")
            .with_description("Processors declared to the main task")
            .build();

        let prosa_name = self.name.clone();

        /// Macro to notify processors for a change about service list
        macro_rules! prosa_main_update_srv {
            ( ) => {
                if !self.notify_srv_proc().await {
                    self.notify_srv_proc().await;
                }
            };
        }

        /// Macro to record a change to the services
        macro_rules! prosa_main_record_services {
            ( ) => {
                services_meter.record(
                    self.services.len() as u64,
                    &[KeyValue::new("prosa_name", prosa_name.clone())],
                );
            };
        }

        /// Macro to record a change to the processors
        macro_rules! prosa_main_record_proc {
            ( ) => {
                processors_meter.record(
                    self.processors.len() as u64,
                    &[
                        KeyValue::new("prosa_name", prosa_name.clone()),
                        KeyValue::new("type", "tasks"),
                    ],
                );
                processors_meter.record(
                    self.get_proc_queue_len() as u64,
                    &[
                        KeyValue::new("prosa_name", prosa_name.clone()),
                        KeyValue::new("type", "queues"),
                    ],
                );
                processors_meter.record(
                    crashed_proc,
                    &[
                        KeyValue::new("prosa_name", prosa_name.clone()),
                        KeyValue::new("type", "crashed"),
                    ],
                );
                processors_meter.record(
                    restarted_proc,
                    &[
                        KeyValue::new("prosa_name", prosa_name.clone()),
                        KeyValue::new("type", "restarted"),
                    ],
                );
            };
        }

        loop {
            tokio::select! {
                Some(msg) = self.internal_rx_queue.recv() => {
                    match msg {
                        InternalMainMsg::NewProcQueue(proc) => {
                            let proc_id = proc.get_proc_id();
                            let queue_id = proc.get_queue_id();
                            let proc_queue = proc.proc_queue.clone();
                            if let Some(proc_service) = self.processors.get_mut(&proc_id) {
                                proc_service.insert(queue_id, proc);
                            } else {
                                self.processors.insert(proc_id, HashMap::from([
                                    (queue_id, proc),
                                ]));
                            }

                            // Ask to the processor to load the service table
                            if proc_queue.send(InternalMsg::Service(self.services.clone())).await.is_err() {
                                if let Some(proc_service) = self.processors.get_mut(&proc_id) {
                                    let _ = proc_service.remove(&queue_id);
                                } else {
                                    let _ = self.processors.remove(&proc_id);
                                }
                            }

                            prosa_main_record_proc!();
                        },
                        InternalMainMsg::DeleteProc(proc_id, proc_err) => {
                            if self.remove_proc(proc_id).await.is_some() {
                                prosa_main_update_srv!();
                            }

                            if let Some(err) = proc_err {
                                if err.recoverable() {
                                    restarted_proc += 1;
                                } else {
                                    crashed_proc += 1;
                                }
                            }

                            prosa_main_record_proc!();
                        },
                        InternalMainMsg::DeleteProcQueue(proc_id, queue_id) => {
                            if self.remove_proc_queue(proc_id, queue_id).await.is_some() {
                                prosa_main_update_srv!();
                            }

                            prosa_main_record_proc!();
                        },
                        InternalMainMsg::NewProcService(names, proc_id) => {
                            if let Some(proc_service) = self.processors.get(&proc_id) {
                                let mut new_services = (*self.services).clone();
                                for proc_queue in proc_service.values() {
                                    for name in &names {
                                        new_services.add_service(name, proc_queue.clone());
                                    }
                                }
                                self.services = Arc::new(new_services);
                                prosa_main_record_services!();
                                prosa_main_update_srv!();
                            }
                        },
                        InternalMainMsg::NewService(names, proc_id, queue_id) => {
                            if let Some(proc_queue) = self.processors.get(&proc_id).and_then(|p| p.get(&queue_id)) {
                                let mut new_services = (*self.services).clone();
                                for name in names {
                                    new_services.add_service(&name, proc_queue.clone());
                                }
                                self.services = Arc::new(new_services);
                                prosa_main_record_services!();
                                prosa_main_update_srv!();
                            }
                        },
                        InternalMainMsg::DeleteProcService(names, proc_id) => {
                            let mut new_services = (*self.services).clone();
                            for name in names {
                                new_services.remove_service_proc(&name, proc_id);
                            }
                            self.services = Arc::new(new_services);
                            prosa_main_record_services!();
                            prosa_main_update_srv!();
                        },
                        InternalMainMsg::DeleteService(names, proc_id, queue_id) => {
                            let mut new_services = (*self.services).clone();
                            for name in names {
                                new_services.remove_service(&name, proc_id, queue_id);
                            }
                            self.services = Arc::new(new_services);
                            prosa_main_record_services!();
                            prosa_main_update_srv!();
                        },
                        InternalMainMsg::Command(cmd)=> {
                            info!("Wan't to execute the command {}", cmd);
                        },
                        InternalMainMsg::Shutdown(reason) => {
                            warn!("ProSA need to stop: {}", reason);
                            self.stop().await;

                            // The shutdown mecanism will be implemented later
                            return;
                        },
                    }
                },
                _ = signal::ctrl_c() => {
                    warn!("ProSA need to stop");
                    self.stop().await;

                    // The shutdown mecanism will be implemented later
                    return;
                },
            }
        }
    }
}
