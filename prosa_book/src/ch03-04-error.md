# Error

When making production grade application, error handling is really important.
It is out of the question for the application to crash on an unhandled error.
And even in such occurrence, it is mandatory to have logs about the root cause of such crash.

If there is one advice that we learn implementing ProSA is to avoid using any method that can result in a panic (such as `.unwrap()`) and prefer handling every error correctly.
Errors should be forwarded to the caller, transformed into an other error type using the `From` trait or handled properly when encountered.

```rust,noplayground
use thiserror::Error;
use prosa::core::service::ServiceError;

#[derive(Debug, Error)]
/// ProSA specific processor error
pub enum ProcSpecificError {
    /// IO error
    #[error("Proc IO error `{0}`")]
    Io(#[from] std::io::Error),
    /// Other error
    #[error("Proc other error `{0}`")]
    Other(String),
}

impl From<ProcSpecificError> for ServiceError {
    fn from(e: ProcSpecificError) -> Self {
        match e {
            ProcSpecificError::Io(io_error) => {
                ServiceError::UnableToReachService(io_error.to_string())
            }
            ProcSpecificError::Other(error) => ServiceError::UnableToReachService(error),
        }
    }
}

impl ProcError for ProcSpecificError {
    fn recoverable(&self) -> bool {
        match self {
            ProcSpecificError::Io(_error) => false,
            ProcSpecificError::Other(_error) => false,
        }
    }
}
```

## Service Error

When you deal with ProSA internal transaction, you need to pay attention to [ServiceError](https://docs.rs/prosa/latest/prosa/core/service/enum.ServiceError.html).
This type is the base error type that a SOA[^soa] need to handle.
In it, you'll find:
- `UnableToReachService` indicate that the service is not available. You should stop sending transaction to it, and send service test until it's available.
- `Timeout` is an error about your processing time. To guarantee real-time processing, you need to propagate this information to indicate the source that you were not able to process the transaction in time.
- `ProtocolError` indicate a protocol issue on the source request. In that case you need to check your API version.

## Processor error

As you can see when you implement a processor, [internal_run](https://docs.rs/prosa/latest/prosa/core/proc/trait.Proc.html#tymethod.internal_run) method return a ProcError.

This error follow the same principle as the [std::error::Error](https://doc.rust-lang.org/std/error/trait.Error.html).
It's a trait that you need to implement to be usable for your processor.

By default, it already implements some of the default error types.

But when you implement your processor, a good practice is to define your own specific error with [thiserror](https://docs.rs/thiserror/latest/thiserror/).
Following that, you can implement the [prosa::proc::ProcError](https://docs.rs/prosa/latest/prosa/core/proc/trait.ProcError.html) trait.

## Processor restart

Processors have an internal feature to automatically restart if the ProcError is recoverable.
It means that the error is transient and the processor can be restarted.

The processor will wait a bit and then try to reestablish the communication. On every error occurrence, the wait delay will increase until a maximum wait time is reached.

[^soa]: Service Oriented Architecure. There is a lot of good lecture about Microservice resilience that is useful when you want to implement properly a service.
