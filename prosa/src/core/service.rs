use crate::core::{error::ProcError, msg};

use super::{
    msg::InternalMsg,
    proc::{ProcBusParam, ProcParam},
};
use prosa_utils::msg::tvf::{Tvf, TvfError};
use std::{
    collections::HashMap,
    fmt::{self, Debug},
    sync::atomic,
};
use thiserror::Error;
use tokio::sync::mpsc;

/// Strucure that define the service table which contain information to how contact a processor for a given service name
#[derive(Debug, Default, Clone)]
pub struct ServiceTable<M>
where
    M: Sized + Clone + Tvf,
{
    table: HashMap<String, Vec<ProcService<M>>>,
}

impl<M> ServiceTable<M>
where
    M: Sized + Clone + Tvf,
{
    /// Getter to know if the service table is empty
    pub fn is_empty(&self) -> bool {
        self.table.is_empty()
    }

    /// Getter of the length of the service table (use for metrics)
    pub fn len(&self) -> usize {
        self.table.len()
    }

    /// Method to know if the service is available from a processor
    ///
    /// Call by the processor to know if a service is available (service test)
    pub fn exist_proc_service(&self, name: &str) -> bool {
        if let Some(services) = self.table.get(name) {
            !services.is_empty()
        } else {
            false
        }
    }

    /// Method to get a processor that respond to the service
    ///
    /// Call by the processor to send a transaction to a processor that give the corresponding service
    pub fn get_proc_service(&self, name: &str) -> Option<&ProcService<M>> {
        if let Some(services) = self.table.get(name) {
            match services.len() {
                2.. => services.get(
                    msg::ATOMIC_INTERNAL_MSG_ID.load(atomic::Ordering::Relaxed) as usize
                        % services.len(),
                ),
                1 => services.first(),
                _ => None,
            }
        } else {
            None
        }
    }

    /// Method to add a service to the table
    ///
    /// Can be call only by the main task to modify the service table
    pub fn add_service(&mut self, name: &str, proc_service: ProcService<M>) {
        if let Some(services) = self.table.get_mut(name) {
            if !services.iter().any(|s| s.proc_id == proc_service.proc_id) {
                services.push(proc_service);
            }
        } else {
            self.table.insert(name.to_string(), vec![proc_service]);
        }
    }

    /// Method to remove whole processor service from the table
    ///
    /// Can be call only by the main task to modify the service table
    pub fn remove_service_proc(&mut self, name: &str, proc_id: u32) {
        if let Some(services) = self.table.get_mut(name) {
            services.retain(|s| s.proc_id != proc_id);
        }
    }

    /// Method to remove a service from the table
    ///
    /// Can be call only by the main task to modify the service table
    pub fn remove_service(&mut self, name: &str, proc_id: u32, queue_id: u32) {
        if let Some(services) = self.table.get_mut(name) {
            services.retain(|s| s.proc_id != proc_id && s.queue_id != queue_id);
        }
    }

    /// Method to remove all services from a given processor from the table
    ///
    /// Can be call only by the main task to modify the service table
    pub fn remove_proc_services(&mut self, proc_id: u32) {
        // This will let service with empty processors
        for service in self.table.values_mut() {
            service.retain(|s| s.proc_id != proc_id);
        }

        // FIXME When the API will not be unstable anymore:
        /*self.table.drain_filter(|k, v| {
            v.retain(|&s| s.proc_id != proc_id);
            v.is_empty()
        });*/
    }

    /// Method to remove all services from a given processor queue from the table
    ///
    /// Can be call only by the main task to modify the service table
    pub fn remove_proc_queue_services(&mut self, proc_id: u32, queue_id: u32) {
        // This will let service with empty processors
        for service in self.table.values_mut() {
            service.retain(|s| s.proc_id != proc_id && s.queue_id != queue_id);
        }

        // FIXME When the API will not be unstable anymore:
        /*self.table.drain_filter(|k, v| {
            v.retain(|&s| s.proc_id != proc_id && s.queue_id != queue_id);
            v.is_empty()
        });*/
    }
}

impl<M> fmt::Display for ServiceTable<M>
where
    M: Sized + Clone + Tvf,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (name, services) in self.table.iter() {
            writeln!(f, "Service name: {name}")?;
            for service in services {
                writeln!(f, "\tProcessor ID: {}", service.proc_id)?;
            }
        }

        Ok(())
    }
}

/// Object to define a ProSA processor service
/// Use by the main processor to have every useful information on a ProSA processor.
#[derive(Debug, Clone)]
pub struct ProcService<M>
where
    M: Sized + Clone + Tvf,
{
    proc_id: u32,
    proc_name: String,
    queue_id: u32,
    /// Processor queue use to send transactionnal message to the processor
    pub proc_queue: mpsc::Sender<InternalMsg<M>>,
}

impl<M> ProcService<M>
where
    M: Sized + Clone + Debug + Tvf + Default + 'static + std::marker::Send + std::marker::Sync,
{
    /// Method to create a processor service with its processor ID and a message queue
    pub fn new(
        proc: &ProcParam<M>,
        proc_queue: mpsc::Sender<InternalMsg<M>>,
        queue_id: u32,
    ) -> ProcService<M> {
        ProcService {
            proc_id: proc.get_proc_id(),
            proc_name: proc.name().to_string(),
            queue_id,
            proc_queue,
        }
    }

    /// Method to create a processor service with its processor ID and its internal message queue
    pub fn new_proc(proc: &ProcParam<M>, queue_id: u32) -> ProcService<M> {
        ProcService {
            proc_id: proc.get_proc_id(),
            proc_name: proc.name().to_string(),
            queue_id,
            proc_queue: proc.get_service_queue(),
        }
    }

    /// Getter of the processor ID
    pub fn get_proc_id(&self) -> u32 {
        self.proc_id
    }

    /// Getter of the queue ID
    pub fn get_queue_id(&self) -> u32 {
        self.queue_id
    }
}

impl<M> ProcBusParam for ProcService<M>
where
    M: Sized + Clone + Tvf,
{
    fn get_proc_id(&self) -> u32 {
        self.proc_id
    }

    fn name(&self) -> &str {
        self.proc_name.as_str()
    }
}

impl<M> PartialEq for ProcService<M>
where
    M: Sized + Clone + Tvf,
{
    fn eq(&self, other: &Self) -> bool {
        self.proc_id == other.proc_id && self.queue_id == other.queue_id
    }
}

#[derive(Debug, Eq, Error, PartialEq)]
/// ProSA service error when the service can't respond correctly to a request
pub enum ServiceError {
    /// No error on the ProSA service
    #[error("No error on the service `{0}`")]
    NoError(String),
    /// The service is unavailable and can't be reach
    #[error("The service `{0}` can't be reach")]
    UnableToReachService(String),
    /// The service didn't respond in time
    #[error("The service `{0}` didn't respond before {1} ms")]
    Timeout(String, u64),
    /// The protocol is not correct on the service
    #[error("The service `{0}` made a protocol error")]
    ProtocolError(String),
}

impl ServiceError {
    /// Method to get the error code of the service error
    /// - 0: No error
    /// - 1: Unable to reach service
    /// - 2: Timeout
    /// - 3: Protocol error
    pub fn get_code(&self) -> u8 {
        match self {
            ServiceError::NoError(_) => 0,
            ServiceError::UnableToReachService(_) => 1,
            ServiceError::Timeout(_, _) => 2,
            ServiceError::ProtocolError(_) => 3,
        }
    }
}

/// A service error should not stop a processor
impl ProcError for ServiceError {
    fn recoverable(&self) -> bool {
        true
    }
}

impl From<TvfError> for ServiceError {
    fn from(err: TvfError) -> Self {
        match err {
            TvfError::FieldNotFound(id) => {
                ServiceError::ProtocolError(format!("on TVF field {id}"))
            }
            TvfError::TypeMismatch => ServiceError::ProtocolError(String::from("on TVF type")),
            TvfError::ConvertionError(str) => {
                ServiceError::ProtocolError(format!("on TVF convertion {str}"))
            }
            TvfError::SerializationError(str) => {
                ServiceError::ProtocolError(format!("on TVF serialization {str}"))
            }
        }
    }
}
