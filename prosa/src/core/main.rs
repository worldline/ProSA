//!
//! <svg width="40" height="40">
#![doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/doc_assets/main.svg"))]
//! </svg>
//!
//! Define ProSA main processing to bring asynchronous handler for all processors.
//!
//! Main can be consider as a service bus that routing processor messages.

use super::msg::{InternalMainMsg, InternalMsg};
use super::proc::ProcBusParam;
use super::service::{ProcService, ServiceTable};
use super::settings::Settings;
use opentelemetry::logs::LoggerProvider as _;
use opentelemetry::metrics::MeterProvider;
use opentelemetry::trace::TracerProvider as _;
use prosa_utils::msg::tvf::{Tvf, TvfError};
use std::borrow::Cow;
use std::sync::Arc;
use std::{collections::HashMap, fmt::Debug};
use thiserror::Error;
use tokio::sync::mpsc;
use tokio::{
    runtime::{Builder, Runtime},
    signal,
};
use tracing::{debug, info, warn};

/// Trait to define a ProSA main processor that is runnable
pub trait MainRunnable<M>
where
    M: Sized + Clone + Tvf,
{
    /// Method to create and run the main task (must be called before processor creation)
    fn create<S: Settings>(settings: &S) -> (Main<M>, Self);

    /// Method call to run the main task (must be called before processor creation)
    fn run(self) -> std::thread::JoinHandle<()>;
}

/// Error define for ProSA bus error (for message exchange)
#[derive(Debug, Eq, Error, PartialEq)]
pub enum BusError {
    /// Error that indicate the queue can forward the internal main message
    #[error("The Queue can't send the internal main message {0}, proc_id={1}, reason={2}")]
    InternalMainQueueError(String, u32, String),
    /// Error that indicate the queue can forward the internal message
    #[error("The Queue can't send the internal message: {0}")]
    InternalQueueError(String),
    /// Error that indicate the queue can forward the internal message
    #[error("The Processor {0}/{1} can't be contacted: {2}")]
    ProcCommError(u32, u32, String),
    /// Error on the internal TVF message use for internal exchange
    #[error("The internal message is not correct: {0}")]
    InternalTvfError(#[from] TvfError),
}

impl<M> From<mpsc::error::SendError<InternalMsg<M>>> for BusError
where
    M: Sized + Clone + Tvf,
{
    fn from(error: mpsc::error::SendError<InternalMsg<M>>) -> Self {
        BusError::InternalQueueError(error.to_string())
    }
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
    meter_provider: opentelemetry_sdk::metrics::MeterProvider,
    logger_provider: opentelemetry_sdk::logs::LoggerProvider,
    tracer_provider: opentelemetry_sdk::trace::TracerProvider,
}

impl<M> ProcBusParam for Main<M>
where
    M: Sized + Clone + Tvf,
{
    fn get_proc_id(&self) -> u32 {
        0
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
        Main {
            internal_tx_queue,
            name: settings.get_prosa_name(),
            meter_provider: settings.get_observability().build_meter_provider(),
            logger_provider: settings.get_observability().build_logger_provider(),
            tracer_provider: settings.get_observability().build_tracer_provider(),
        }
    }

    /// Getter of the main bus
    pub fn get_bus_queue(&self) -> mpsc::Sender<InternalMainMsg<M>> {
        self.internal_tx_queue.clone()
    }

    /// Method to declare a new processor on the main bus
    pub async fn add_proc_queue(&self, proc: ProcService<M>) -> Result<(), BusError> {
        self.internal_tx_queue
            .send(InternalMainMsg::NewProcQueue(proc.clone()))
            .await
            .map_err(|e| {
                BusError::InternalMainQueueError(
                    "NEWPROCQUEUE".into(),
                    proc.get_proc_id(),
                    e.to_string(),
                )
            })
    }

    /// Method to remove an entire processor from the main bus
    pub async fn rm_proc(&self, proc_id: u32) -> Result<(), BusError> {
        self.internal_tx_queue
            .send(InternalMainMsg::DeleteProc(proc_id))
            .await
            .map_err(|e| BusError::InternalMainQueueError("DELPROC".into(), proc_id, e.to_string()))
    }

    /// Method to declare a new processor on the main bus
    pub async fn rm_proc_queue(&self, proc_id: u32, queue_id: u32) -> Result<(), BusError> {
        self.internal_tx_queue
            .send(InternalMainMsg::DeleteProcQueue(proc_id, queue_id))
            .await
            .map_err(|e| {
                BusError::InternalMainQueueError("DELPROCQUEUE".into(), proc_id, e.to_string())
            })
    }

    /// Method to declare a new service for a whole processor on the main bus
    pub async fn add_service_proc(&self, names: Vec<String>, proc_id: u32) -> Result<(), BusError> {
        self.internal_tx_queue
            .send(InternalMainMsg::NewProcService(names, proc_id))
            .await
            .map_err(|e| {
                BusError::InternalMainQueueError("NEWPROCSRV".into(), proc_id, e.to_string())
            })
    }

    /// Method to declare a new service for a processor queue on the main bus
    pub async fn add_service(
        &self,
        names: Vec<String>,
        proc_id: u32,
        queue_id: u32,
    ) -> Result<(), BusError> {
        self.internal_tx_queue
            .send(InternalMainMsg::NewService(names, proc_id, queue_id))
            .await
            .map_err(|e| BusError::InternalMainQueueError("NEWSRV".into(), proc_id, e.to_string()))
    }

    /// Method to remove a service for a whole processor from the main bus
    pub async fn rm_service_proc(&self, names: Vec<String>, proc_id: u32) -> Result<(), BusError> {
        self.internal_tx_queue
            .send(InternalMainMsg::DeleteProcService(names, proc_id))
            .await
            .map_err(|e| {
                BusError::InternalMainQueueError("DELPROCSRV".into(), proc_id, e.to_string())
            })
    }

    /// Method to remove a service from the main bus
    pub async fn rm_service(
        &self,
        names: Vec<String>,
        proc_id: u32,
        queue_id: u32,
    ) -> Result<(), BusError> {
        self.internal_tx_queue
            .send(InternalMainMsg::DeleteService(names, proc_id, queue_id))
            .await
            .map_err(|e| BusError::InternalMainQueueError("DELSRV".into(), proc_id, e.to_string()))
    }

    /// Method to stop all processors
    pub async fn stop(&self, reason: String) -> Result<(), BusError> {
        self.internal_tx_queue
            .send(InternalMainMsg::Shutdown(reason))
            .await
            .map_err(|e| BusError::InternalMainQueueError("SHUTDOWN".into(), 0, e.to_string()))
    }

    /// Provide the ProSA name based on ProSA settings
    pub fn name(&self) -> &String {
        &self.name
    }

    /// Provide the opentelemetry Meter based on ProSA settings
    pub fn meter(&self, name: impl Into<Cow<'static, str>>) -> opentelemetry::metrics::Meter {
        self.meter_provider.meter(name)
    }

    /// Provide the opentelemetry Logger based on ProSA settings
    pub fn logger(&self, name: impl Into<Cow<'static, str>>) -> opentelemetry_sdk::logs::Logger {
        self.logger_provider.logger(name)
    }

    /// Provide the opentelemetry Tracer based on ProSA settings
    pub fn tracer(&self, name: impl Into<Cow<'static, str>>) -> opentelemetry_sdk::trace::Tracer {
        self.tracer_provider.tracer(name)
    }
}

/// Main ProSA task processor
pub struct MainProc<M>
where
    M: Sized + Clone + Tvf,
{
    processors: HashMap<u32, HashMap<u32, ProcService<M>>>,
    services: Arc<ServiceTable<M>>,
    internal_rx_queue: mpsc::Receiver<InternalMainMsg<M>>,
}

impl<M> ProcBusParam for MainProc<M>
where
    M: Sized + Clone + Tvf,
{
    fn get_proc_id(&self) -> u32 {
        0
    }
}

/// Macro to notify processors for a change about service list
macro_rules! prosa_main_update_srv {
    ( $x:ident ) => {
        if !$x.notify_srv_proc().await {
            $x.notify_srv_proc().await;
        }
    };
}

impl<M> MainProc<M>
where
    M: Sized + Clone + Debug + Tvf + Default + 'static + std::marker::Send + std::marker::Sync,
{
    async fn remove_proc(&mut self, proc_id: u32) -> Option<HashMap<u32, ProcService<M>>> {
        if let Some(proc) = self.processors.remove(&proc_id) {
            let mut new_services = (*self.services).clone();
            new_services.rm_proc_services(proc_id);
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
                new_services
                    .rm_proc_queue_services(proc_queue.get_proc_id(), proc_queue.get_queue_id());
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
                    return Err(BusError::ProcCommError(
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
        if let Err(BusError::ProcCommError(proc_id, queue_id, _)) =
            self.notify_srv_proc_queue().await
        {
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

    async fn internal_run(&mut self) -> Result<(), BusError> {
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
                        },
                        InternalMainMsg::DeleteProc(proc_id) => {
                            if self.remove_proc(proc_id).await.is_some() {
                                prosa_main_update_srv!(self);
                            }
                        },
                        InternalMainMsg::DeleteProcQueue(proc_id, queue_id) => {
                            if self.remove_proc_queue(proc_id, queue_id).await.is_some() {
                                self.notify_srv_proc().await;
                            }
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
                                self.notify_srv_proc().await;
                            }
                        },
                        InternalMainMsg::NewService(names, proc_id, queue_id) => {
                            if let Some(proc) = self.processors.get(&proc_id) {
                                if let Some(proc_queue) = proc.get(&queue_id) {
                                    let mut new_services = (*self.services).clone();
                                    for name in names {
                                        new_services.add_service(&name, proc_queue.clone());
                                    }
                                    self.services = Arc::new(new_services);
                                    self.notify_srv_proc().await;
                                }
                            }
                        },
                        InternalMainMsg::DeleteProcService(names, proc_id) => {
                            let mut new_services = (*self.services).clone();
                            for name in names {
                                new_services.rm_service_proc(&name, proc_id);
                            }
                            self.services = Arc::new(new_services);
                            self.notify_srv_proc().await;
                        },
                        InternalMainMsg::DeleteService(names, proc_id, queue_id) => {
                            let mut new_services = (*self.services).clone();
                            for name in names {
                                new_services.rm_service(&name, proc_id, queue_id);
                            }
                            self.services = Arc::new(new_services);
                            self.notify_srv_proc().await;
                        },
                        InternalMainMsg::Command(cmd)=> {
                            info!("Wan't to execute the command {}", cmd);
                        },
                        InternalMainMsg::Shutdown(reason) => {
                            warn!("ProSA need to stop: {}", reason);
                            self.stop().await;

                            // The shutdown mecanism will be implemented later
                            return Ok(())
                        },
                    }
                },
                _ = signal::ctrl_c() => {
                    warn!("ProSA need to stop");
                    self.stop().await;

                    // The shutdown mecanism will be implemented later
                    return Ok(())
                },
            }
        }
    }
}

impl<M> MainRunnable<M> for MainProc<M>
where
    M: Sized + Clone + Debug + Tvf + Default + 'static + std::marker::Send + std::marker::Sync,
{
    fn create<S: Settings>(settings: &S) -> (Main<M>, MainProc<M>) {
        let (internal_tx_queue, internal_rx_queue) = mpsc::channel(2048);
        (
            Main::new(internal_tx_queue, settings),
            MainProc {
                processors: Default::default(),
                services: Arc::new(ServiceTable::default()),
                internal_rx_queue,
            },
        )
    }

    fn run(mut self) -> std::thread::JoinHandle<()> {
        std::thread::Builder::new()
            .name("main".into())
            .spawn(move || {
                let rt: Runtime = Builder::new_current_thread()
                    .enable_all()
                    .thread_name("main")
                    .build()
                    .unwrap();
                rt.block_on(self.internal_run()).unwrap();
            })
            .unwrap()
    }
}
