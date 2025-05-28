# Error

When you do production grade application, error handling is realy important.
You don't want your application to shutdown on a non handle error, without useful information to know what happened.

If there is one advice that we learn implemting ProSA is to use only if needed the `.unwrap()` method and favorise handling every error correctly.
By sending them or to proper react from them.


## Service Error

When you deal with ProSA internal transaction, you need to pay attention to [ServiceError](https://docs.rs/prosa/latest/prosa/core/service/enum.ServiceError.html).
This error provide base error that a SOA[^soa] need to handle.
From it you find:
- `UnableToReachService` indicate that the service is not available. You should stop sending transaction to it, and send service test until it's available.
- `Timeout` is an error about your processing time. To garante a real time system (response in a certain amount of time), you need to propagate this information to indicate the source that you wasn't able to process it's transaction in time.
- `ProtocolError` indicate a protocol issue on the source request. In that case you need to check your api versionnning.


## Processor error

As you can see when you implent a processor, [internal_run](https://docs.rs/prosa/latest/prosa/core/proc/trait.Proc.html#tymethod.internal_run) method return a ProcError.

This error follow the same principle as the [std::error::Error](https://doc.rust-lang.org/std/error/trait.Error.html).
It's a trait that you need to implement to be usable for your processor.

By default, it implement already some of default error type.

But when you implement your processor, a good practice is to define your own specific error with [thiserror](https://docs.rs/thiserror/latest/thiserror/).
Following that, you can implement the [prosa::proc::ProcError](https://docs.rs/prosa/latest/prosa/core/proc/trait.ProcError.html) trait.


## Processor restart

Processor have an internal feature to automatically restart if the ProcError is recoverable.
It mean that the error is temporary and processor can be restart.

The processor will take more time on every error occurence until a max.

[^soa]: Service Oriented Architecure. There is a lot of good lecture about Microservice resilience that is useful when you want to implement properly a service.
