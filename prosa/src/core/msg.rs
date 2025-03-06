use std::{
    sync::Arc,
    time::{Duration, SystemTime},
};

use prosa_utils::msg::tvf::Tvf;
use tokio::sync::mpsc;
use tracing::span;
use tracing::{Level, Span, event};

use super::service::{ProcService, ServiceError, ServiceTable};

/// Internal ProSA message that define all message type that can be received by the main ProSA processor
#[derive(Debug)]
pub enum InternalMainMsg<M>
where
    M: Sized + Clone + Tvf,
{
    /// Message to register a new spawned processor queue
    NewProcQueue(ProcService<M>),
    /// Message to indicate that a the processor stopped, delete all processor queues
    DeleteProc(u32),
    /// Message to indicate that a the processor queue stopped, delete the processor queue
    DeleteProcQueue(u32, u32),
    /// Message to declare new service(s) with their service name and the processor id (the processor should have been declared). Declare service(s) for the whole processor
    NewProcService(Vec<String>, u32),
    /// Message to declare new service(s) with their service name, the processor id (the processor should have been declared), and the queue id
    NewService(Vec<String>, u32, u32),
    /// Message to unregister a service for all the processor. Message that contain the service name and the processor id
    DeleteProcService(Vec<String>, u32),
    /// Message to unregister service(s) for a processor queue. Message that contain service(s) name(s), the processor id, and the queue id
    DeleteService(Vec<String>, u32, u32),
    /// Command to ask an action or a status to the main processor
    Command(String),
    /// Internal call for shutdown (with a reason)
    Shutdown(String),
}

/// Internal ProSA message that define all message type that can be received by a processor
#[derive(Debug)]
pub enum InternalMsg<M>
where
    M: Sized + Clone + Tvf,
{
    /// Request Data message to process
    Request(RequestMsg<M>),
    /// Response of a data request message
    Response(ResponseMsg<M>),
    /// Response of a data request message by an error
    Error(ErrorMsg<M>),
    /// Command to ask an actiion or a status to the processor
    Command(String),
    /// Message to ask the processor to reload its configuration
    Config,
    /// Message to ask the processor to reload its service table
    Service(Arc<ServiceTable<M>>),
    /// Message to ask the processor to shutdown
    Shutdown,
}

#[cfg_attr(doc, aquamarine::aquamarine)]
/// Trait that define a ProSAMsg use to send transactions
///
/// ```mermaid
/// sequenceDiagram
///     Client->>Service: RequestMsg
///     alt is ok
///         Service->>Client: ResponseMsg
///     else is error
///         Service->>Client: ErrorMsg
///     end
/// ```
pub trait Msg<M>
where
    M: Sized + Clone + Tvf,
{
    /// Getter of the message id
    fn get_id(&self) -> u64;
    /// Getter of the service name
    fn get_service(&self) -> &String;
    /// Getter of the span of the message (use for metrics)
    fn get_span(&self) -> &Span;
    /// Getter of the mutable span of the message (use to add informations for metrics)
    fn get_span_mut(&mut self) -> &mut Span;
    /// Enter the span and push metadata in it
    fn enter_span(&self) -> span::Entered;
    /// Return the elapsed time corresponding to the processing time (duration since the request creation)
    fn elapsed(&self) -> Duration;
    /// Getter of the message content
    fn get_data(&self) -> &M;
    /// Getter of the mutable message content
    fn get_data_mut(&mut self) -> &mut M;
}

/// ProSA request message that define a data message that need to be process by a processor
#[derive(Debug)]
pub struct RequestMsg<M>
where
    M: Sized + Clone + Tvf,
{
    id: u64,
    service: String,
    span: Span,
    data: M,
    begin_time: SystemTime,
    response_queue: mpsc::Sender<InternalMsg<M>>,
}

impl<M> Msg<M> for RequestMsg<M>
where
    M: Sized + Clone + Tvf,
{
    fn get_id(&self) -> u64 {
        self.id
    }

    fn get_service(&self) -> &String {
        &self.service
    }

    fn get_span(&self) -> &Span {
        &self.span
    }

    fn get_span_mut(&mut self) -> &mut Span {
        &mut self.span
    }

    fn enter_span(&self) -> span::Entered {
        self.span.enter()
    }

    fn elapsed(&self) -> Duration {
        self.begin_time.elapsed().unwrap_or(Duration::new(0, 0))
    }

    fn get_data(&self) -> &M {
        &self.data
    }

    fn get_data_mut(&mut self) -> &mut M {
        &mut self.data
    }
}

impl<M> RequestMsg<M>
where
    M: Sized + Clone + Tvf,
{
    /// Method to create a new RequestMessage
    pub fn new(
        id: u64,
        service: String,
        data: M,
        response_queue: mpsc::Sender<InternalMsg<M>>,
    ) -> Self {
        let begin_time = SystemTime::now();
        let span = span!(Level::INFO, "prosa::Msg", service = service);
        RequestMsg {
            id,
            service,
            data,
            begin_time,
            span,
            response_queue,
        }
    }

    /// Method to return the response to the called processor
    pub async fn return_to_sender(
        self,
        resp: M,
    ) -> Result<(), tokio::sync::mpsc::error::SendError<InternalMsg<M>>> {
        self.response_queue
            .send(InternalMsg::Response(ResponseMsg {
                id: self.id,
                service: self.service,
                span: self.span,
                response_time: self.begin_time,
                data: resp,
            }))
            .await
    }

    /// Method to return an error to the called processor
    /// You can specify a return data otherwise
    pub async fn return_error_to_sender(
        self,
        data: Option<M>,
        err: ServiceError,
    ) -> Result<(), tokio::sync::mpsc::error::SendError<InternalMsg<M>>> {
        self.response_queue
            .send(InternalMsg::Error(ErrorMsg {
                id: self.id,
                service: self.service,
                span: self.span,
                error_time: self.begin_time,
                data: data.unwrap_or(self.data),
                err,
            }))
            .await
    }
}

/// ProSA request message that define a data message that need to be process by a processor
#[derive(Debug)]
pub struct ResponseMsg<M>
where
    M: Sized + Clone + Tvf,
{
    id: u64,
    service: String,
    span: Span,
    response_time: SystemTime,
    data: M,
}

impl<M> Msg<M> for ResponseMsg<M>
where
    M: Sized + Clone + Tvf,
{
    fn get_id(&self) -> u64 {
        self.id
    }

    fn get_service(&self) -> &String {
        &self.service
    }

    fn get_span(&self) -> &Span {
        &self.span
    }

    fn get_span_mut(&mut self) -> &mut Span {
        &mut self.span
    }

    fn enter_span(&self) -> span::Entered {
        self.span.enter()
    }

    fn elapsed(&self) -> Duration {
        self.response_time.elapsed().unwrap_or(Duration::new(0, 0))
    }

    fn get_data(&self) -> &M {
        &self.data
    }

    fn get_data_mut(&mut self) -> &mut M {
        &mut self.data
    }
}

/// ProSA request message that define a data message that need to be process by a processor
#[derive(Debug)]
pub struct ErrorMsg<M>
where
    M: Sized + Clone + Tvf,
{
    id: u64,
    service: String,
    span: Span,
    error_time: SystemTime,
    data: M,
    err: ServiceError,
}

impl<M> Msg<M> for ErrorMsg<M>
where
    M: Sized + Clone + Tvf,
{
    fn get_id(&self) -> u64 {
        self.id
    }

    fn get_service(&self) -> &String {
        &self.service
    }

    fn get_span(&self) -> &Span {
        &self.span
    }

    fn get_span_mut(&mut self) -> &mut Span {
        &mut self.span
    }

    fn enter_span(&self) -> span::Entered {
        let enter = self.span.enter();
        event!(Level::ERROR, "{}", self.err);
        enter
    }

    fn elapsed(&self) -> Duration {
        self.error_time.elapsed().unwrap_or(Duration::new(0, 0))
    }

    fn get_data(&self) -> &M {
        &self.data
    }

    fn get_data_mut(&mut self) -> &mut M {
        &mut self.data
    }
}

impl<M> ErrorMsg<M>
where
    M: Sized + Clone + Tvf,
{
    /// Method to create a new ErrorMsg
    pub fn new(id: u64, service: String, span: Span, data: M, err: ServiceError) -> Self {
        ErrorMsg {
            id,
            service,
            span,
            error_time: SystemTime::now(),
            data,
            err,
        }
    }

    /// Getter of the service error
    pub fn get_err(&self) -> &ServiceError {
        &self.err
    }
}
