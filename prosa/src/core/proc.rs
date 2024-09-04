//!
//! <svg width="40" height="40">
#![doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/doc_assets/proc.svg"))]
//! </svg>
//!
//! Define ProSA processor to do a processing
//!
//! To create a ProSA processor:
//! ```
//! use std::error::Error;
//! use serde::Serialize;
//! use prosa_utils::msg::tvf::Tvf;
//! use prosa::core::proc::{proc_settings, proc, Proc, ProcBusParam};
//! use prosa::core::adaptor::Adaptor;
//! use prosa::core::msg::{Msg, InternalMsg};
//!
//! pub trait MyAdaptorTrait<M>
//! where
//!     M: 'static
//!     + std::marker::Send
//!     + std::marker::Sync
//!     + std::marker::Sized
//!     + std::clone::Clone
//!     + std::fmt::Debug
//!     + prosa_utils::msg::tvf::Tvf
//!     + std::default::Default,
//! {
//!     /// Method called when the processor spawns
//!     /// This method is called only once so the processing will be thread safe
//!     fn new(proc: &MyProc<M>) -> Result<Self, Box<dyn Error>> where Self: Sized;
//!     /// Method to process incomming requests
//!     fn process_request(&self, service_name: &str, request: &M) -> M;
//! }
//!
//! #[derive(Adaptor)]
//! pub struct MyAdaptor {
//!     // your adaptor vars here
//! }
//!
//! impl<M> MyAdaptorTrait<M> for MyAdaptor
//! where
//!     M: 'static
//!     + std::marker::Send
//!     + std::marker::Sync
//!     + std::marker::Sized
//!     + std::clone::Clone
//!     + std::fmt::Debug
//!     + prosa_utils::msg::tvf::Tvf
//!     + std::default::Default,
//! {
//!     fn new(proc: &MyProc<M>) -> Result<Self, Box<dyn Error>> {
//!         // Init your adaptor from processor parameters
//!         Ok(Self {})
//!     }
//!
//!     fn process_request(&self, service_name: &str, request: &M) -> M {
//!         // Do your processing
//!         request.clone()
//!     }
//! }
//!
//! #[proc_settings]
//! #[derive(Default, Debug, Serialize)]
//! pub struct MyProcSettings {
//!     param: String,
//!     // ...
//! }
//!
//! #[proc(settings = MyProcSettings)]
//! pub struct MyProc { /* Nothing in here */ }
//!
//! #[proc]
//! impl MyProc
//! {
//!     fn internal_func() {
//!         // You can declare function
//!     }
//! }
//! // or explicitly
//! //#[proc]
//! //impl<M> MyProc<M>
//! //where
//! //    M: 'static
//! //    + std::marker::Send
//! //    + std::marker::Sync
//! //    + std::marker::Sized
//! //    + std::clone::Clone
//! //    + std::fmt::Debug
//! //    + prosa_utils::msg::tvf::Tvf
//! //    + std::default::Default,
//! //{
//! //    fn internal_func() {
//! //        // You can declare function
//! //    }
//! //}
//!
//! // You must implement the trait Proc to define your processing
//! #[proc]
//! impl<A> Proc<A> for MyProc
//! where
//!     A: Adaptor + MyAdaptorTrait<M> + std::marker::Send + std::marker::Sync,
//! {
//!     async fn internal_run(&mut self, name: String) -> Result<(), Box<dyn std::error::Error>> {
//!         // Initiate an adaptor for the stub processor
//!         let mut adaptor = A::new(self)?;
//!
//!         // Declare the processor
//!         self.proc.add_proc().await?;
//!
//!         // Add all service to listen
//!         self.proc
//!             .add_service_proc(vec![String::from("DUMMY")])
//!             .await?;
//!
//!         loop {
//!             if let Some(msg) = self.internal_rx_queue.recv().await {
//!                 match msg {
//!                     InternalMsg::Request(msg) => {
//!                         // Send the request to your adaptor and get a TVF object in return to respond to the sender
//!                         let tvf = adaptor.process_request(msg.get_service(), msg.get_data());
//!                         msg.return_to_sender(tvf).await.unwrap()
//!                     }
//!                     InternalMsg::Response(msg) => panic!(
//!                         "The stub processor {} receive a response {:?}",
//!                         self.get_proc_id(),
//!                         msg
//!                     ),
//!                     InternalMsg::Error(err) => panic!(
//!                         "The stub processor {} receive an error {:?}",
//!                         self.get_proc_id(),
//!                         err
//!                     ),
//!                     InternalMsg::Command(_) => todo!(),
//!                     InternalMsg::Config => todo!(),
//!                     InternalMsg::Service(table) => self.service = table,
//!                     InternalMsg::Shutdown => {
//!                         adaptor.terminate();
//!                         self.proc.rm_proc().await?;
//!                         return Ok(());
//!                     }
//!                 }
//!             }
//!         }
//!     }
//! }
//! ```

use super::adaptor::Adaptor;
use super::main::BusError;
use super::{main::Main, msg::InternalMsg, service::ProcService};
use config::File;
use config::{Config, ConfigError};
use glob::glob;
use prosa_utils::msg::tvf::Tvf;
use std::borrow::Cow;
use std::fmt::Debug;
use tokio::runtime;
use tokio::sync::mpsc;

// Export proc macro
pub use prosa_macros::proc;

/// Implement the trait [`ProcSettings`].
pub use prosa_macros::proc_settings;

/// Trait to define ProSA processor settings
///
/// ```
/// use prosa::core::proc::proc_settings;
///
/// #[proc_settings]
/// #[derive(Default, Debug)]
/// pub struct MySettings {
///     my_param: Option<String>,
/// }
/// ```
pub trait ProcSettings {
    /// Getter of the processor's adaptor configuration path
    fn get_adaptor_config_path(&self) -> Option<&String>;

    /// Getter of the processor's adaptor configuration
    fn get_adaptor_config<C>(&self) -> Result<C, ::config::ConfigError>
    where
        C: serde::de::Deserialize<'static>,
    {
        if let Some(config_path) = &self.get_adaptor_config_path() {
            Config::builder()
                .add_source(
                    glob(config_path)
                        .unwrap()
                        .map(|path| File::from(path.unwrap()))
                        .collect::<Vec<_>>(),
                )
                .build()?
                .try_deserialize()
        } else {
            Err(ConfigError::NotFound(
                "No configuration set for processor's adaptor".to_string(),
            ))
        }
    }
}

/// Global parameter for a processor (main or specific)
pub trait ProcBusParam {
    /// Getter of the processor id
    fn get_proc_id(&self) -> u32;
}

impl Debug for dyn ProcBusParam {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Processor {}", self.get_proc_id())
    }
}

#[derive(Debug, Clone)]
/// Parameters embeded in a ProSA processor
pub struct ProcParam<M>
where
    M: Sized + Clone + Tvf,
{
    id: u32,
    queue: mpsc::Sender<InternalMsg<M>>,
    main: Main<M>,
}

impl<M> ProcBusParam for ProcParam<M>
where
    M: Sized + Clone + Tvf,
{
    fn get_proc_id(&self) -> u32 {
        self.id
    }
}

impl<M> ProcParam<M>
where
    M: Sized + Clone + Debug + Tvf + Default + 'static + std::marker::Send + std::marker::Sync,
{
    /// Method to create a processor parameter
    pub fn new(id: u32, queue: mpsc::Sender<InternalMsg<M>>, main: Main<M>) -> ProcParam<M> {
        ProcParam { id, queue, main }
    }

    /// Getter of the processor service queue to send internal messages
    pub fn get_service_queue(&self) -> mpsc::Sender<InternalMsg<M>> {
        self.queue.clone()
    }

    /// Method to declare the processor with a signal queue to the main task
    ///
    /// Should be called only once at the processor start
    /// After the declaration, the main task will send the service table to the processor
    pub async fn add_proc(&self) -> Result<(), BusError> {
        self.main
            .add_proc_queue(ProcService::new_proc(self, 0))
            .await?;
        Ok(())
    }

    /// Method to remove the processor with a signal queue to the main task
    ///
    /// Once the processor is removed, all its associated service will be remove
    pub async fn rm_proc(&self) -> Result<(), BusError> {
        self.main.rm_proc(self.id).await?;
        Ok(())
    }

    /// Method to declare the processor with multiple queue identify with a queue id to the main task
    ///
    /// Should be called as many as queue but queue id must be unique per processor
    /// After the declaration, the main task will send the service table to the processor
    pub async fn add_proc_queue(
        &self,
        queue: mpsc::Sender<InternalMsg<M>>,
        queue_id: u32,
    ) -> Result<(), BusError> {
        self.main
            .add_proc_queue(ProcService::new(self, queue, queue_id))
            .await?;
        Ok(())
    }

    /// Method to remove the processor queue identify with a queue id to the main task
    ///
    /// Once the processor queue is removed, all its associated service will be remove
    pub async fn rm_proc_queue(&self, queue_id: u32) -> Result<(), BusError> {
        self.main.rm_proc_queue(self.id, queue_id).await?;
        Ok(())
    }

    /// Method to declare a new service for a whole processor to the main bus to receive corresponding messages
    pub async fn add_service_proc(&self, names: Vec<String>) -> Result<(), BusError> {
        self.main
            .add_service_proc(names, self.get_proc_id())
            .await?;
        Ok(())
    }

    /// Method to declare a new service to the main bus to receive corresponding messages
    pub async fn add_service(&self, names: Vec<String>, queue_id: u32) -> Result<(), BusError> {
        self.main
            .add_service(names, self.get_proc_id(), queue_id)
            .await?;
        Ok(())
    }

    /// Method to remove a service for a whole processor from the main bus. The processor will no longuer receive those corresponding messages
    pub async fn rm_service_proc(&self, names: Vec<String>) -> Result<(), BusError> {
        self.main.rm_service_proc(names, self.get_proc_id()).await?;
        Ok(())
    }

    /// Method to remove a service from the main bus. The processor will no longuer receive those corresponding messages
    pub async fn rm_service(&self, names: Vec<String>, queue_id: u32) -> Result<(), BusError> {
        self.main
            .rm_service(names, self.get_proc_id(), queue_id)
            .await?;
        Ok(())
    }

    /// Provide the ProSA name based on ProSA settings
    pub fn name(&self) -> &String {
        self.main.name()
    }

    /// Provide the opentelemetry Meter based on ProSA settings
    pub fn meter(&self, name: impl Into<Cow<'static, str>>) -> opentelemetry::metrics::Meter {
        self.main.meter(name)
    }

    /// Provide the opentelemetry Logger based on ProSA settings
    pub fn logger(&self, name: impl Into<Cow<'static, str>>) -> opentelemetry_sdk::logs::Logger {
        self.main.logger(name)
    }

    /// Provide the opentelemetry Tracer based on ProSA settings
    pub fn tracer(&self, name: impl Into<Cow<'static, str>>) -> opentelemetry_sdk::trace::Tracer {
        self.main.tracer(name)
    }
}

/// Trait to define ProSA processor configuration
///
/// Define by the macro `proc`
pub trait ProcConfig<M>
where
    M: 'static
        + std::marker::Send
        + std::marker::Sync
        + std::marker::Sized
        + std::clone::Clone
        + std::fmt::Debug
        + prosa_utils::msg::tvf::Tvf
        + std::default::Default,
{
    /// Settings use for the ProSA processor
    type Settings;

    /// Method to create a processor out of it's configuration
    fn create(proc_id: u32, main: Main<M>, settings: Self::Settings) -> Self;

    /// Method to create a processor with not specific configuration
    fn create_raw(proc_id: u32, main: Main<M>) -> Self
    where
        Self: Sized,
        Self::Settings: Default,
    {
        Self::create(proc_id, main, Self::Settings::default())
    }

    /// Getter of the processor parameters
    fn get_proc_param(&self) -> &ProcParam<M>;
}

#[cfg_attr(doc, aquamarine::aquamarine)]
/// Generic trait to define ProSA processor
///
/// It regroup several composant:
/// ```mermaid
/// graph LR
///     bus([Internal service bus])
///     queue[(Processor queue)]
///     adaptor[Adaptor]
///     task[Task]
///     ext(External system)
///     bus <--> adaptor
///     task <--> ext
///     subgraph proc[ProSA Processor]
///     queue <--> task
///     adaptor <--> task
///     end
/// ```
pub trait Proc<A>
where
    A: Adaptor,
{
    /// Main loop of the processor
    fn internal_run(
        &mut self,
        name: String,
    ) -> impl std::future::Future<Output = Result<(), Box<dyn std::error::Error>>> + Send;

    /// Method to run the processor
    ///
    /// ```
    /// use prosa::core::proc::Proc;
    /// use prosa::core::adaptor::Adaptor;
    ///
    /// fn routine<A, P>(proc: P)
    /// where
    ///     A: Adaptor,
    ///     P: Proc<A> + std::marker::Send + 'static,
    /// {
    ///     Proc::<A>::run(proc, String::from("processor_name"));
    /// }
    /// ```
    fn run(mut self, proc_name: String)
    where
        Self: Sized + 'static + std::marker::Send,
    {
        std::thread::Builder::new()
            .name(proc_name.clone())
            .spawn(move || {
                let rt: runtime::Runtime = runtime::Builder::new_current_thread()
                    .enable_all()
                    .thread_name(proc_name.clone())
                    .build()
                    .unwrap();
                rt.block_on(self.internal_run(proc_name)).unwrap();
            })
            .unwrap();
    }
}
