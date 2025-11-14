use std::{
    sync::{Arc, atomic},
    time::{Duration, SystemTime},
};

use super::{
    error::{BusError, ProcError},
    queue::{InternalMsgQueue, SendError},
    service::{ProcService, ServiceError, ServiceTable},
};
use tracing::{Level, Span, event, info_span, span};

/// Expose Tvf trait
pub use prosa_utils::msg::tvf::Tvf;

/// Internal ProSA message that define all message type that can be received by the main ProSA processor
#[derive(Debug)]
pub enum InternalMainMsg<M>
where
    M: Sized + Clone + Tvf,
{
    /// Message to register a new spawned processor queue
    NewProcQueue(ProcService<M>),
    /// Message to indicate that a the processor stopped (and the error if there is any), delete all processor queues
    DeleteProc(u32, Option<Box<dyn ProcError + Send + Sync>>),
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
/// Trait that define a ProSA Msg use to send transactions
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
    fn enter_span(&self) -> span::Entered<'_>;
    /// Return the elapsed time corresponding to the processing time (duration since the request creation)
    fn elapsed(&self) -> Duration;
    /// Getter of the message content. Return an error if the data have been taken
    fn get_data(&self) -> Result<&M, BusError>;
    /// Getter of the mutable message content. Return an error if the data have been taken
    fn get_data_mut(&mut self) -> Result<&mut M, BusError>;
    /// Takes the data out of the message.
    ///
    /// ```
    /// use prosa::core::msg::{Msg, Tvf};
    ///
    /// fn process_msg<T, M>(mut msg: T)
    /// where
    ///     M: Sized + Clone + Tvf,
    ///     T: Msg<M>,
    /// {
    ///     if msg.get_data().is_ok() {
    ///         let data = msg.take_data();
    ///         assert!(data.is_some());
    ///         assert!(msg.get_data().is_err());
    ///     } else {
    ///         assert!(msg.take_data().is_none());
    ///     }
    /// }
    /// ```
    fn take_data(&mut self) -> Option<M>;
    /// Takes the value out of the message, but only if the predicate evaluates to true on a mutable reference to the data.
    /// This method operates similar to [`Msg<M>::take_data`] but conditional.
    ///
    /// ```
    /// use prosa::core::msg::{Msg, Tvf};
    ///
    /// fn process_msg<T, M>(mut msg: T)
    /// where
    ///     M: Sized + Clone + Tvf,
    ///     T: Msg<M>,
    /// {
    ///     if msg.get_data().is_ok() {
    ///         let data = msg.take_data_if(|data| data.contains(42));
    ///         // Return the data only if its contain a field 42.
    ///     }
    /// }
    /// ```
    fn take_data_if<P>(&mut self, predicate: P) -> Option<M>
    where
        P: FnOnce(&mut M) -> bool;
}

pub(crate) static ATOMIC_INTERNAL_MSG_ID: atomic::AtomicU64 = atomic::AtomicU64::new(0);

/// ProSA request message that define a data message that need to be process by a processor
#[derive(Debug)]
pub struct RequestMsg<M>
where
    M: Sized + Clone + Tvf,
{
    id: u64,
    service: String,
    span: Span,
    data: Option<M>,
    begin_time: SystemTime,
    response_queue: InternalMsgQueue<M>,
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

    fn enter_span(&self) -> span::Entered<'_> {
        self.span.enter()
    }

    fn elapsed(&self) -> Duration {
        self.begin_time.elapsed().unwrap_or(Duration::new(0, 0))
    }

    fn get_data(&self) -> Result<&M, BusError> {
        self.data.as_ref().ok_or(BusError::NoData)
    }

    fn get_data_mut(&mut self) -> Result<&mut M, BusError> {
        self.data.as_mut().ok_or(BusError::NoData)
    }

    fn take_data(&mut self) -> Option<M> {
        self.data.take()
    }

    fn take_data_if<P>(&mut self, predicate: P) -> Option<M>
    where
        P: FnOnce(&mut M) -> bool,
    {
        self.data.take_if(predicate)
    }
}

impl<M> RequestMsg<M>
where
    M: Sized + Clone + Tvf,
{
    /// Method to create a new RequestMessage
    pub fn new(service: String, data: M, response_queue: impl Into<InternalMsgQueue<M>>) -> Self {
        let begin_time = SystemTime::now();
        let span = info_span!("prosa::Msg", service = service);
        RequestMsg {
            id: ATOMIC_INTERNAL_MSG_ID.fetch_add(1, atomic::Ordering::Relaxed),
            service,
            data: Some(data),
            begin_time,
            span,
            response_queue: response_queue.into(),
        }
    }

    /// Method to create a new RequestMessage with a specific trace Id
    pub fn new_with_trace_id(
        service: String,
        data: M,
        response_queue: impl Into<InternalMsgQueue<M>>,
        trace_id: tracing::span::Id,
    ) -> Self {
        let begin_time = SystemTime::now();
        let span = info_span!(parent: trace_id, "prosa::Msg", service = service);
        RequestMsg {
            id: ATOMIC_INTERNAL_MSG_ID.fetch_add(1, atomic::Ordering::Relaxed),
            service,
            data: Some(data),
            begin_time,
            span,
            response_queue: response_queue.into(),
        }
    }

    /// Method to return the response to the called processor
    pub fn return_to_sender(mut self, resp: M) -> Result<(), SendError<M>> {
        let response_queue = self.response_queue.take();
        response_queue
            .send(InternalMsg::Response(ResponseMsg::from_request(self, resp)))
            .map_err(|e| {
                e.map(|i| {
                    if let InternalMsg::Response(mut resp) = i {
                        resp.take_data().unwrap()
                    } else {
                        panic!("Expected InternalMsg::Response")
                    }
                })
            })
    }

    /// Method to return an error to the called processor
    /// You can specify a return data
    pub fn return_error_to_sender(
        mut self,
        data: Option<M>,
        err: ServiceError,
    ) -> Result<(), SendError<Option<M>>> {
        let response_queue = self.response_queue.take();
        response_queue
            .send(InternalMsg::Error(ErrorMsg::from_request(self, data, err)))
            .map_err(|e| {
                e.map(|i| {
                    if let InternalMsg::Error(mut err) = i {
                        err.take_data()
                    } else {
                        panic!("Expected InternalMsg::Error")
                    }
                })
            })
    }

    /// Method to return a result to the called processor
    /// If Ok, return the response data, else return the error
    pub fn return_result_to_sender(
        self,
        result: Result<M, ServiceError>,
    ) -> Result<(), SendError<Option<M>>> {
        match result {
            Ok(resp) => self.return_to_sender(resp).map_err(|e| e.map(|m| Some(m))),
            Err(err) => self.return_error_to_sender(None, err),
        }
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
    data: Option<M>,
}

impl<M> ResponseMsg<M>
where
    M: Sized + Clone + Tvf,
{
    /// Method to create a `ResponseMsg` from a [`RequestMsg`]
    pub fn from_request(request: RequestMsg<M>, resp_data: M) -> Self {
        ResponseMsg {
            id: request.id,
            service: request.service,
            span: request.span,
            response_time: request.begin_time,
            data: Some(resp_data),
        }
    }
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

    fn enter_span(&self) -> span::Entered<'_> {
        self.span.enter()
    }

    fn elapsed(&self) -> Duration {
        self.response_time.elapsed().unwrap_or(Duration::new(0, 0))
    }

    fn get_data(&self) -> Result<&M, BusError> {
        self.data.as_ref().ok_or(BusError::NoData)
    }

    fn get_data_mut(&mut self) -> Result<&mut M, BusError> {
        self.data.as_mut().ok_or(BusError::NoData)
    }

    fn take_data(&mut self) -> Option<M> {
        self.data.take()
    }

    fn take_data_if<P>(&mut self, predicate: P) -> Option<M>
    where
        P: FnOnce(&mut M) -> bool,
    {
        self.data.take_if(predicate)
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
    data: Option<M>,
    err: ServiceError,
}

impl<M> ErrorMsg<M>
where
    M: Sized + Clone + Tvf,
{
    /// Method to create an `ErrorMsg` from a [`RequestMsg`] (if the request encounter an error)
    pub fn from_request(request: RequestMsg<M>, data: Option<M>, err: ServiceError) -> Self {
        ErrorMsg {
            id: request.id,
            service: request.service,
            span: request.span,
            error_time: request.begin_time,
            data: data.or(request.data),
            err,
        }
    }

    /// Getter of the service error
    pub fn get_err(&self) -> &ServiceError {
        &self.err
    }

    /// Consume the message and return the service error
    pub fn into_err(self) -> ServiceError {
        self.err
    }
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

    fn enter_span(&self) -> span::Entered<'_> {
        let enter = self.span.enter();
        event!(Level::WARN, "{}", self.err);
        enter
    }

    fn elapsed(&self) -> Duration {
        self.error_time.elapsed().unwrap_or(Duration::new(0, 0))
    }

    fn get_data(&self) -> Result<&M, BusError> {
        self.data.as_ref().ok_or(BusError::NoData)
    }

    fn get_data_mut(&mut self) -> Result<&mut M, BusError> {
        self.data.as_mut().ok_or(BusError::NoData)
    }

    fn take_data(&mut self) -> Option<M> {
        self.data.take()
    }

    fn take_data_if<P>(&mut self, predicate: P) -> Option<M>
    where
        P: FnOnce(&mut M) -> bool,
    {
        self.data.take_if(predicate)
    }
}
