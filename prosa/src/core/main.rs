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
    settings::{ProsaConfig, Settings},
};
use crate::otel::metrics::{Meter, MeterProvider as _};
use crate::otel::trace::TracerProvider as _;
use crate::otel::{InstrumentationScope, KeyValue};
use crate::tracing::{debug, info, warn};
use prosa_utils::hash::{BuildIntHasher, IntHashMap, IntHashSet};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::{borrow::Cow, fmt::Debug};
use tokio::{signal, sync::mpsc};

/// Trait to define a ProSA main processor that is runnable
pub trait MainRunnable<M>
where
    M: Sized + Clone + Tvf,
{
    /// Method to create and run the main task (must be called before processor creation)
    /// The processor capacity is an indicator to preallocate resources (can be None if not known)
    fn create<S: Settings>(settings: &S, proc_capacity: Option<usize>) -> (Main<M>, Self);

    /// Method call to run the main task (should be called before processor creation)
    fn run(self) -> impl std::future::Future<Output = ()> + Send;
}

/// Main ProSA task to handle every task spawn in the ProSA
/// Use an internal ProSA service bus
/// Must be run only one time in the ProSA
///
/// This is the core strucutre of ProSA.
#[doc = simple_mermaid::mermaid!("diagrams/main_bus.mmd")]
#[derive(Clone, Debug)]
pub struct Main<M>
where
    M: Sized + Clone + Tvf,
{
    internal_tx_queue: mpsc::Sender<InternalMainMsg<M>>,
    name: String,
    scope_attributes: Vec<KeyValue>,
    #[cfg(feature = "prometheus")]
    prometheus_registry: prometheus::Registry,
    meter_provider: opentelemetry_sdk::metrics::SdkMeterProvider,
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
        #[cfg(feature = "prometheus")]
        {
            let prometheus_registry = prometheus::Registry::new();
            let meter_provider = settings
                .get_observability()
                .build_meter_provider(&prometheus_registry);

            Main {
                internal_tx_queue,
                name: settings.get_prosa_name(),
                scope_attributes: settings.get_observability().get_scope_attributes(),
                prometheus_registry,
                meter_provider,
                tracer_provider: settings.get_observability().build_tracer_provider(),
                stop: Arc::new(AtomicBool::new(false)),
            }
        }

        #[cfg(not(feature = "prometheus"))]
        {
            let meter_provider = settings.get_observability().build_meter_provider();

            Main {
                internal_tx_queue,
                name: settings.get_prosa_name(),
                scope_attributes: settings.get_observability().get_scope_attributes(),
                meter_provider,
                tracer_provider: settings.get_observability().build_tracer_provider(),
                stop: Arc::new(AtomicBool::new(false)),
            }
        }
    }

    /// Getter of the main bus
    pub fn get_bus_queue(&self) -> mpsc::Sender<InternalMainMsg<M>> {
        self.internal_tx_queue.clone()
    }

    /// Getter of the Prometheus registry
    #[cfg(feature = "prometheus")]
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
            .send(InternalMainMsg::NewProcQueue(proc))
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

    /// Method to notify processors that the configuration changed
    pub async fn update_config(
        &self,
        config: Arc<ProsaConfig>,
    ) -> Result<(), SendError<InternalMainMsg<M>>> {
        Ok(self
            .internal_tx_queue
            .send(InternalMainMsg::Config(config))
            .await?)
    }

    /// Provide the ProSA name based on ProSA settings
    pub fn name(&self) -> &String {
        &self.name
    }

    /// Provide the opentelemetry Meter based on ProSA settings
    pub fn meter(&self, name: &'static str) -> opentelemetry::metrics::Meter {
        let scope = InstrumentationScope::builder(name)
            .with_version(env!("CARGO_PKG_VERSION"))
            .with_attributes(self.scope_attributes.clone())
            .build();
        self.meter_provider.meter_with_scope(scope)
    }

    /// Provide the opentelemetry Tracer based on ProSA settings
    pub fn tracer(&self, name: impl Into<Cow<'static, str>>) -> opentelemetry_sdk::trace::Tracer {
        self.tracer_provider.tracer(name)
    }
}

type ProcQueueMap<M> = IntHashMap<u32, ProcService<M>>;
type ProcessorMap<M> = IntHashMap<u32, ProcQueueMap<M>>;

/// Main ProSA task processor
pub struct MainProc<M>
where
    M: Sized + Clone + Tvf,
{
    name: String,
    processors: ProcessorMap<M>,
    services: Arc<ServiceTable<M>>,
    config: Option<Arc<ProsaConfig>>,
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
    async fn remove_proc(&mut self, proc_id: u32) -> Option<ProcQueueMap<M>> {
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
    async fn notify_srv_proc_queue(&self) -> Vec<BusError> {
        let mut errors = Vec::new();
        for proc in self.processors.values() {
            for proc_service in proc.values() {
                if let Err(e) = proc_service
                    .proc_queue
                    .send(InternalMsg::Service(self.services.clone()))
                    .await
                {
                    errors.push(BusError::ProcComm(
                        proc_service.get_proc_id(),
                        proc_service.get_queue_id(),
                        e.to_string(),
                    ));
                }
            }
        }

        errors
    }

    /// Method to notify processors whose configuration has changed
    async fn notify_config_proc_queue(&mut self, config: Arc<ProsaConfig>) -> Vec<BusError> {
        let current = self.config.as_deref();
        let mut errors = Vec::new();
        for proc in self.processors.values() {
            for proc_service in proc.values() {
                if current.is_none_or(|current_config| {
                    current_config.has_proc_changed(&config, &proc_service.get_proc_config_key())
                }) && let Err(e) = proc_service
                    .proc_queue
                    .send(InternalMsg::Config(config.clone()))
                    .await
                {
                    errors.push(BusError::ProcComm(
                        proc_service.get_proc_id(),
                        proc_service.get_queue_id(),
                        e.to_string(),
                    ));
                }
            }
        }

        self.config = Some(config);

        errors
    }

    /// Method to notify one processor queue with the current configuration
    async fn notify_config_proc_service(
        &self,
        proc_service: &ProcService<M>,
    ) -> Result<(), BusError> {
        if let Some(config) = &self.config {
            proc_service
                .proc_queue
                .send(InternalMsg::Config(config.clone()))
                .await
                .map_err(|e| {
                    BusError::ProcComm(
                        proc_service.get_proc_id(),
                        proc_service.get_queue_id(),
                        e.to_string(),
                    )
                })?;
        }

        Ok(())
    }

    /// Method to notify all processor that the service table have changed
    async fn notify_srv_proc(&mut self) {
        for error in self.notify_srv_proc_queue().await {
            // The processor doesn't exist anymore so remove it
            if let BusError::ProcComm(proc_id, queue_id, _) = error {
                if queue_id > 0 {
                    self.remove_proc_queue(proc_id, queue_id).await;
                } else {
                    warn!("Processor {proc_id} stopped during service table reload");
                    self.remove_proc(proc_id).await;
                }
            }
        }
    }

    /// Method to shutdown all processors (return `true` if all processor are off, `false` otherwise)
    async fn stop(&mut self) -> bool {
        self.stop.store(true, Ordering::Relaxed);
        let mut is_stopped = true;
        for proc in self.processors.values() {
            for proc_service in proc.values() {
                if let Err(e) = proc_service.proc_queue.send(InternalMsg::Shutdown).await {
                    debug!(
                        "Processor service {:?} seems to have already stopped: {}",
                        proc_service, e
                    );
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
    fn create<S: Settings>(settings: &S, proc_capacity: Option<usize>) -> (Main<M>, MainProc<M>) {
        fn inner<M>(
            main: Main<M>,
            processors: ProcessorMap<M>,
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
                    processors,
                    services: Arc::new(ServiceTable::default()),
                    config: None,
                    internal_rx_queue,
                    meter,
                    stop,
                },
            )
        }
        let (internal_tx_queue, internal_rx_queue) = mpsc::channel(2048);
        let processors = if let Some(capacity) = proc_capacity {
            IntHashMap::with_capacity_and_hasher(capacity, BuildIntHasher::default())
        } else {
            IntHashMap::with_hasher(BuildIntHasher::default())
        };
        inner(
            Main::new(internal_tx_queue, settings),
            processors,
            internal_rx_queue,
        )
    }

    async fn run(mut self) {
        #[cfg(feature = "system-metrics")]
        {
            // Monitor RAM usage
            self.meter
                .u64_observable_gauge("prosa_main_ram")
                .with_description("RAM consumed by ProSA")
                .with_unit("bytes")
                .with_callback(move |observer| {
                    if let Some(usage) = memory_stats::memory_stats() {
                        observer.observe(
                            usage.physical_mem as u64,
                            &[KeyValue::new("type", "physical")],
                        );
                        observer.observe(
                            usage.virtual_mem as u64,
                            &[KeyValue::new("type", "virtual")],
                        );
                    }
                })
                .build();
        }

        // Monitor services
        let (service_update, new_service) = tokio::sync::watch::channel(self.services.clone());
        self.meter
            .u64_observable_gauge("prosa_services")
            .with_description("Services declared to the main task")
            .with_callback(move |observer| {
                new_service.borrow().observe_metrics(observer);
            })
            .build();

        let mut proc_names = IntHashMap::with_hasher(BuildIntHasher::default());

        // Monitor processors objects
        let mut crashed_proc = IntHashSet::with_hasher(BuildIntHasher::default());
        let mut restarted_proc = IntHashMap::with_hasher(BuildIntHasher::default());
        let processors_meter = self
            .meter
            .i64_gauge("prosa_processors")
            .with_description("Processors declared to the main task")
            .build();

        /// Macro to record a change to the processors
        macro_rules! prosa_main_record_proc {
            ( ) => {
                for (id, name) in proc_names.iter() {
                    if crashed_proc.contains(id) {
                        // The processor is crashed
                        processors_meter.record(
                            -2,
                            &[
                                KeyValue::new("type", "node"),
                                KeyValue::new("id", *id as i64),
                                KeyValue::new("title", name.to_string()),
                            ],
                        );
                    } else if let Some(proc_service) = self.processors.get(id) {
                        // The processor is running
                        let nb_restarted = *restarted_proc.get(id).unwrap_or(&0);
                        processors_meter.record(
                            proc_service.len() as i64,
                            &[
                                KeyValue::new("type", "queues"),
                                KeyValue::new("id", *id as i64),
                                KeyValue::new("title", name.to_string()),
                            ],
                        );
                        processors_meter.record(
                            nb_restarted as i64,
                            &[
                                KeyValue::new("type", "node"),
                                KeyValue::new("id", *id as i64),
                                KeyValue::new("title", name.to_string()),
                            ],
                        );
                    } else {
                        // The processor is not running
                        processors_meter.record(
                            -1,
                            &[
                                KeyValue::new("type", "node"),
                                KeyValue::new("id", *id as i64),
                                KeyValue::new("title", name.to_string()),
                            ],
                        );
                    }
                }
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
                            let proc_config = proc.clone();
                            if let Some(proc_service) = self.processors.get_mut(&proc_id) {
                                proc_service.insert(queue_id, proc);
                            } else {
                                proc_names.insert(proc_id, proc.name().to_string());
                                self.processors.insert(proc_id, [(queue_id, proc)]
                                .into_iter()
                                .collect::<IntHashMap<_, _>>());
                            }

                            // Ask to the processor to load the service table
                            if proc_queue.send(InternalMsg::Service(self.services.clone())).await.is_err() {
                                if let Some(proc_service) = self.processors.get_mut(&proc_id) {
                                    let _ = proc_service.remove(&queue_id);
                                } else {
                                    warn!("Processor {proc_id} stopped while loading the service table");
                                    let _ = self.processors.remove(&proc_id);
                                }
                            }

                            if self.notify_config_proc_service(&proc_config).await.is_err() {
                                if let Some(proc_service) = self.processors.get_mut(&proc_id) {
                                    let _ = proc_service.remove(&queue_id);
                                } else {
                                    warn!("Processor {proc_id} stopped while loading configuration");
                                    let _ = self.processors.remove(&proc_id);
                                }
                            }

                            prosa_main_record_proc!();
                        },
                        InternalMainMsg::DeleteProc(proc_id, proc_err) => {
                            if self.remove_proc(proc_id).await.is_some() {
                                self.notify_srv_proc().await;
                            }

                            if let Some(err) = proc_err {
                                if err.recoverable() {
                                    if let Some(restarted) = restarted_proc.get_mut(&proc_id) {
                                        *restarted += 1;
                                    } else {
                                        restarted_proc.insert(proc_id, 1);
                                    }
                                } else {
                                    crashed_proc.insert(proc_id);
                                }
                            }

                            prosa_main_record_proc!();
                        },
                        InternalMainMsg::DeleteProcQueue(proc_id, queue_id) => {
                            if self.remove_proc_queue(proc_id, queue_id).await.is_some() {
                                self.notify_srv_proc().await;
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
                                let _ = service_update.send(self.services.clone());
                                self.notify_srv_proc().await;
                            }
                        },
                        InternalMainMsg::NewService(names, proc_id, queue_id) => {
                            if let Some(proc_queue) = self.processors.get(&proc_id).and_then(|p| p.get(&queue_id)) {
                                let mut new_services = (*self.services).clone();
                                for name in &names {
                                    new_services.add_service(name, proc_queue.clone());
                                }
                                self.services = Arc::new(new_services);
                                let _ = service_update.send(self.services.clone());
                                self.notify_srv_proc().await;
                            }
                        },
                        InternalMainMsg::DeleteProcService(names, proc_id) => {
                            let mut new_services = (*self.services).clone();
                            for name in names {
                                new_services.remove_service_proc(&name, proc_id);
                            }
                            self.services = Arc::new(new_services);
                            let _ = service_update.send(self.services.clone());
                            self.notify_srv_proc().await;
                        },
                        InternalMainMsg::DeleteService(names, proc_id, queue_id) => {
                            let mut new_services = (*self.services).clone();
                            for name in names {
                                new_services.remove_service(&name, proc_id, queue_id);
                            }
                            self.services = Arc::new(new_services);
                            let _ = service_update.send(self.services.clone());
                            self.notify_srv_proc().await;
                        },
                        InternalMainMsg::Config(config) => {
                            info!("Reloading ProSA configuration");
                            for error in self.notify_config_proc_queue(config).await {
                                if let BusError::ProcComm(proc_id, queue_id, _) = error {
                                    if queue_id > 0 {
                                        self.remove_proc_queue(proc_id, queue_id).await;
                                    } else {
                                        warn!("Processor {proc_id} stopped during configuration reload");
                                        self.remove_proc(proc_id).await;
                                    }
                                }
                            }
                        },
                        InternalMainMsg::Shutdown(reason) => {
                            warn!("ProSA is stopping: {}", reason);
                            self.stop().await;

                            // The shutdown mecanism will be implemented later
                            return;
                        },
                    }
                },
                _ = signal::ctrl_c() => {
                    warn!("ProSA is stopping");
                    self.stop().await;

                    // The shutdown mecanism will be implemented later
                    return;
                },
            }
        }
    }
}
