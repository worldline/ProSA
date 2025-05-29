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
//! use serde::{Deserialize, Serialize};
//! use prosa_utils::msg::tvf::Tvf;
//! use prosa::core::proc::{proc_settings, proc, Proc, ProcBusParam};
//! use prosa::core::adaptor::Adaptor;
//! use prosa::core::msg::{Msg, InternalMsg};
//! use prosa::core::error::ProcError;
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
//!     fn new(proc: &MyProc<M>) -> Result<Self, Box<dyn ProcError + Send + Sync>> where Self: Sized;
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
//!     fn new(proc: &MyProc<M>) -> Result<Self, Box<dyn ProcError + Send + Sync>> {
//!         // Init your adaptor from processor parameters
//!         Ok(Self {})
//!     }
//! }
//!
//! #[proc_settings]
//! #[derive(Default, Debug, Deserialize, Serialize)]
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
//!     async fn internal_run(&mut self, name: String) -> Result<(), Box<dyn ProcError + Send + Sync>> {
//!         // Initiate an adaptor
//!         let mut adaptor = A::new(self)?;
//!
//!         // Declare the processor
//!         self.proc.add_proc().await?;
//!
//!         // Retrieve param from the processor settings `MyProcSettings`
//!         let _param = self.proc.settings.param;
//!
//!         loop {
//!             if let Some(msg) = self.internal_rx_queue.recv().await {
//!                 match msg {
//!                     InternalMsg::Request(msg) => {
//!                         // TODO process the request
//!                     }
//!                     InternalMsg::Response(msg) => {
//!                        // TODO process the response
//!                     },
//!                     InternalMsg::Error(err) => {
//!                        // TODO process the error
//!                     },
//!                     InternalMsg::Command(_) => todo!(),
//!                     InternalMsg::Config => todo!(),
//!                     InternalMsg::Service(table) => self.service = table,
//!                     InternalMsg::Shutdown => {
//!                         adaptor.terminate();
//!                         self.proc.remove_proc(None).await?;
//!                         return Ok(());
//!                     }
//!                 }
//!             }
//!         }
//!     }
//! }
//! ```

use super::adaptor::Adaptor;
use super::error::{BusError, ProcError};
use super::{main::Main, msg::InternalMsg, service::ProcService};
use config::{Config, ConfigError, File};
use glob::glob;
use log::{error, info, warn};
use prosa_utils::msg::tvf::Tvf;
use std::borrow::Cow;
use std::fmt::Debug;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tokio::{runtime, spawn};

// Export proc macro
pub use prosa_macros::proc;

/// Implement the trait [`ProcSettings`].
pub use prosa_macros::proc_settings;

/// Trait to define ProSA processor settings
///
/// ```
/// use serde::Deserialize;
/// use prosa::core::proc::proc_settings;
///
/// #[proc_settings]
/// #[derive(Debug, Deserialize)]
/// pub struct MySettings {
///     my_param: String,
/// }
///
/// #[proc_settings]
/// impl Default for MySettings {
///     fn default() -> Self {
///         MySettings {
///             my_param: "default param".into(),
///         }
///     }
/// }
/// ```
pub trait ProcSettings {
    /// Getter of the processor's adaptor configuration path
    fn get_adaptor_config_path(&self) -> Option<&String>;

    /// Getter of the restart delay that must be apply to the processor if an error is trigger.
    /// Return the duration to be add to every restart, and the max duration wait between restarts in seconds.
    fn get_proc_restart_delay(&self) -> (Duration, u32);

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

    /// Provide the ProSA name based on ProSA settings
    fn name(&self) -> &str;
}

impl Debug for dyn ProcBusParam {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Processor[{}] {}", self.get_proc_id(), self.name())
    }
}

/// Trait to define all processor handle functions
pub trait ProcEpilogue {
    /// Getter to know timer for processor restart in case of error
    fn get_proc_restart_delay(&self) -> (std::time::Duration, u32);

    /// Method to remove the processor with a signal queue to the main task
    ///
    /// Once the processor is removed, all its associated service will be remove
    fn remove_proc(
        &self,
        err: Option<Box<dyn ProcError + Send + Sync>>,
    ) -> impl std::future::Future<Output = Result<(), BusError>> + Send;

    /// Indicates whether ProSA is stopping
    /// Prevents the rebooting of processors
    fn is_stopping(&self) -> bool;
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

    fn name(&self) -> &str {
        self.main.name()
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
    pub async fn remove_proc(
        &self,
        err: Option<Box<dyn ProcError + Send + Sync>>,
    ) -> Result<(), BusError> {
        self.main.remove_proc(self.id, err).await?;
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
    pub async fn remove_proc_queue(&self, queue_id: u32) -> Result<(), BusError> {
        self.main.remove_proc_queue(self.id, queue_id).await?;
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
    pub async fn remove_service_proc(&self, names: Vec<String>) -> Result<(), BusError> {
        self.main
            .remove_service_proc(names, self.get_proc_id())
            .await?;
        Ok(())
    }

    /// Method to remove a service from the main bus. The processor will no longuer receive those corresponding messages
    pub async fn remove_service(&self, names: Vec<String>, queue_id: u32) -> Result<(), BusError> {
        self.main
            .remove_service(names, self.get_proc_id(), queue_id)
            .await?;
        Ok(())
    }

    /// Indicates whether ProSA is stopping
    pub fn is_stopping(&self) -> bool {
        self.main.is_stopping()
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

macro_rules! proc_run {
    ( $self:ident, $proc_name:ident ) => {
        info!(
            "Run processor {} on {} threads",
            $proc_name,
            $self.get_proc_threads()
        );

        let proc_restart_delay = $self.get_proc_restart_delay();
        let mut wait_time = proc_restart_delay.0;
        loop {
            if let Err(proc_err) = $self.internal_run($proc_name.clone()).await {
                // Stop the processor immediately if ProSA is shutting down
                if $self.is_stopping() {
                    // Remove the proc from main
                    let _ = $self.remove_proc(None).await;
                    return;
                }

                let recovery_duration = proc_err.recovery_duration();

                // Log and restart if needed
                if proc_err.recoverable() {
                    warn!(
                        "Processor {} encounter an error `{}`. Will restart after {}ms",
                        $proc_name,
                        proc_err,
                        (wait_time + recovery_duration).as_millis()
                    );

                    // Notify the main task of the error
                    if $self.remove_proc(Some(proc_err)).await.is_err() {
                        return;
                    }
                } else {
                    error!(
                        "Processor {} encounter a fatal error `{}`",
                        $proc_name, proc_err
                    );

                    // Notify the main task of the error
                    let _ = $self.remove_proc(Some(proc_err)).await;
                    return;
                }

                // Wait a graceful time before restarting the processor
                sleep(wait_time + recovery_duration).await;
            } else {
                // Remove the proc from main
                let _ = $self.remove_proc(None).await;
                return;
            }

            // Don't wait more than the restart delay parameter
            if wait_time.as_secs() < proc_restart_delay.1 as u64 {
                wait_time += proc_restart_delay.0;
                wait_time *= 2;
            }
        }
    };
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
pub trait Proc<A>: ProcEpilogue
where
    A: Adaptor,
{
    /// Main loop of the processor to implement
    fn internal_run(
        &mut self,
        name: String,
    ) -> impl std::future::Future<Output = Result<(), Box<dyn ProcError + Send + Sync>>> + Send;

    /// Get the number of processor threads the Processors's `Runtime` will use.
    /// Must be implemented by the processor if more than one thread is to be used
    ///
    /// You can specify values:
    /// - `0` to spawn your processor on the main runtime
    /// - `1` to run your processor on a single thread
    /// - \> `1` to run your processor on a multiple thread Tokio runtime
    ///
    /// By default, the processor will run on a single thread.
    fn get_proc_threads(&self) -> usize {
        1
    }

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
        match self.get_proc_threads() {
            // Spawn the processor on the current Tokio runtime
            0 => {
                spawn(async move {
                    proc_run!(self, proc_name);
                });
            }
            // Start a Tokio runtime on a single thread [default]
            1 => {
                std::thread::Builder::new()
                    .name(proc_name.clone())
                    .spawn(move || {
                        runtime::Builder::new_current_thread()
                            .enable_all()
                            .thread_name(proc_name.clone())
                            .build()
                            .unwrap()
                            .block_on(async {
                                proc_run!(self, proc_name);
                            })
                    })
                    .unwrap();
            }
            // Start a Tokio runtime on multiple threads
            n => {
                std::thread::Builder::new()
                    .name(proc_name.clone())
                    .spawn(move || {
                        runtime::Builder::new_multi_thread()
                            .worker_threads(n)
                            .enable_all()
                            .thread_name(proc_name.clone())
                            .build()
                            .unwrap()
                            .block_on(async {
                                proc_run!(self, proc_name);
                            })
                    })
                    .unwrap();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prosa_macros::proc_settings;
    use serde::Serialize;

    extern crate self as prosa;

    #[test]
    fn test_proc_settings() {
        #[proc_settings]
        #[derive(Debug, Serialize)]
        struct TestProcSettings {
            name: String,
        }

        #[proc_settings]
        impl Default for TestProcSettings {
            fn default() -> Self {
                let _test_settings = TestProcSettings {
                    name: "test".into(),
                };

                TestProcSettings {
                    name: "test".into(),
                }
            }
        }

        let test_proc_settings = TestProcSettings::default();
        assert_eq!("test", test_proc_settings.name);
    }
}
