use super::proc::StubProc;
use crate::{
    core::{
        adaptor::{Adaptor, MaybeAsync},
        error::ProcError,
        msg::Tvf,
        proc::{ProcConfig, ProcSettings},
        service::ServiceError,
    },
    maybe_async,
};
extern crate self as prosa;
use crate::otel::metrics::Meter;
use serde::Deserialize;
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

const DEFAULT_STUB_ASYNC_PAROT_SLEEP_MS: u64 = 100;

fn default_stub_async_parot_sleep_ms() -> u64 {
    DEFAULT_STUB_ASYNC_PAROT_SLEEP_MS
}

#[derive(Debug, Deserialize)]
struct StubAsyncParotConfig {
    #[serde(default = "default_stub_async_parot_sleep_ms", alias = "sleep_millis")]
    sleep_ms: u64,
}

impl Default for StubAsyncParotConfig {
    fn default() -> Self {
        StubAsyncParotConfig {
            sleep_ms: DEFAULT_STUB_ASYNC_PAROT_SLEEP_MS,
        }
    }
}

/// Adaptator trait for the stub processor
///
/// Need to define the process_request method to know what to do with incoming requests
/// ```
/// use prosa::stub::proc::StubProc;
/// use prosa::core::adaptor::{Adaptor, MaybeAsync};
/// use prosa::stub::adaptor::StubAdaptor;
/// use prosa::core::error::ProcError;
/// use prosa::core::msg::Tvf;
/// use prosa::core::service::ServiceError;
///
/// #[derive(Adaptor)]
/// pub struct MyStubAdaptor { }
///
/// impl<M> StubAdaptor<M> for MyStubAdaptor
/// where
///     M: 'static
///         + std::marker::Send
///         + std::marker::Sync
///         + std::marker::Sized
///         + std::clone::Clone
///         + std::fmt::Debug
///         + Tvf
///         + std::default::Default,
/// {
///     fn new(_proc: &StubProc<M>) -> Result<Self, Box<dyn ProcError + Send + Sync>> {
///         Ok(Self {})
///     }
///
///     fn process_request(&self, service_name: &str, request: M) -> MaybeAsync<Result<M, ServiceError>> {
///         let mut msg = request.clone();
///         msg.put_string(1, format!("test service {}", service_name));
///         Ok(msg).into()
///     }
/// }
/// ```
///
/// You also have the possibility to do an async request processing for your stub adaptor:
/// ```
/// use prosa::stub::proc::StubProc;
/// use prosa::core::adaptor::{Adaptor, MaybeAsync};
/// use prosa::stub::adaptor::StubAdaptor;
/// use prosa::core::error::ProcError;
/// use prosa::core::msg::Tvf;
/// use prosa::core::service::ServiceError;
/// use prosa::maybe_async;
///
/// #[derive(Adaptor)]
/// pub struct MyAsyncStubAdaptor { }
///
/// impl<M> StubAdaptor<M> for MyAsyncStubAdaptor
/// where
///     M: 'static
///         + std::marker::Send
///         + std::marker::Sync
///         + std::marker::Sized
///         + std::clone::Clone
///         + std::fmt::Debug
///         + Tvf
///         + std::default::Default,
/// {
///     fn new(_proc: &StubProc<M>) -> Result<Self, Box<dyn ProcError + Send + Sync>> {
///         Ok(Self {})
///     }
///
///     fn process_request(&self, service_name: &str, request: M) -> MaybeAsync<Result<M, ServiceError>> {
///         let service_name = service_name.to_string();
///         maybe_async!(async move {
///             // You can do async things here
///             let mut msg = request.clone();
///             msg.put_string(1, format!("test service {}", service_name));
///             Ok(msg)
///         })
///     }
/// }
/// ```
pub trait StubAdaptor<M>
where
    M: 'static
        + std::marker::Send
        + std::marker::Sync
        + std::marker::Sized
        + std::clone::Clone
        + std::fmt::Debug
        + Tvf
        + std::default::Default,
{
    /// Method called when the processor spawns
    /// This method is called only once so the processing will be thread safe
    fn new(proc: &StubProc<M>) -> Result<Self, Box<dyn ProcError + Send + Sync>>
    where
        Self: Sized;

    /// Method to process incoming requests
    fn process_request(
        &self,
        service_name: &str,
        request: M,
    ) -> MaybeAsync<Result<M, ServiceError>>;
}

/// Parot adaptor for the stub processor. Use to respond to a request with the same message
#[derive(Adaptor)]
pub struct StubParotAdaptor {
    #[allow(unused)]
    meter: Meter,
}

impl<M> StubAdaptor<M> for StubParotAdaptor
where
    M: 'static
        + std::marker::Send
        + std::marker::Sync
        + std::marker::Sized
        + std::clone::Clone
        + std::fmt::Debug
        + Tvf
        + std::default::Default,
{
    fn new(proc: &StubProc<M>) -> Result<Self, Box<dyn ProcError + Send + Sync>> {
        Ok(Self {
            meter: proc.get_proc_param().meter("stub_adaptor"),
        })
    }

    fn process_request(
        &self,
        _service_name: &str,
        request: M,
    ) -> MaybeAsync<Result<M, ServiceError>> {
        Ok(request.clone()).into()
    }
}

/// Parot adaptor for the stub processor. Use to respond asynchronously after a configurable delay.
///
/// The adaptor configuration accepts `sleep_ms` (or `sleep_millis`) and defaults to 100ms.
pub struct StubAsyncParotAdaptor {
    sleep_ms: AtomicU64,
}

impl Adaptor for StubAsyncParotAdaptor {
    fn reload_config(&self, config: Option<&config::Config>) -> Result<(), config::ConfigError> {
        let config = if let Some(config) = config {
            config.clone().try_deserialize::<StubAsyncParotConfig>()?
        } else {
            StubAsyncParotConfig {
                sleep_ms: DEFAULT_STUB_ASYNC_PAROT_SLEEP_MS,
            }
        };
        self.sleep_ms.store(config.sleep_ms, Ordering::Relaxed);
        Ok(())
    }

    fn terminate(&self) {}
}

impl<M> StubAdaptor<M> for StubAsyncParotAdaptor
where
    M: 'static
        + std::marker::Send
        + std::marker::Sync
        + std::marker::Sized
        + std::clone::Clone
        + std::fmt::Debug
        + Tvf
        + std::default::Default,
{
    fn new(proc: &StubProc<M>) -> Result<Self, Box<dyn ProcError + Send + Sync>> {
        let sleep_ms = match proc.settings.get_adaptor_config::<StubAsyncParotConfig>() {
            Ok(config) => config.sleep_ms,
            Err(err) => {
                if proc.settings.get_adaptor_config_path().is_some() {
                    log::warn!(
                        "Can't load StubAsyncParotAdaptor configuration: {err}. Using default sleep of {DEFAULT_STUB_ASYNC_PAROT_SLEEP_MS}ms"
                    );
                }
                DEFAULT_STUB_ASYNC_PAROT_SLEEP_MS
            }
        };

        Ok(Self {
            sleep_ms: AtomicU64::new(sleep_ms),
        })
    }

    fn process_request(
        &self,
        _service_name: &str,
        request: M,
    ) -> MaybeAsync<Result<M, ServiceError>> {
        let sleep_duration = Duration::from_millis(self.sleep_ms.load(Ordering::Relaxed));
        maybe_async!(async move {
            tokio::time::sleep(sleep_duration).await;
            Ok(request)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_async_parot_reload_config_updates_sleep() -> Result<(), config::ConfigError> {
        let adaptor = StubAsyncParotAdaptor {
            sleep_ms: AtomicU64::new(DEFAULT_STUB_ASYNC_PAROT_SLEEP_MS),
        };
        let config = config::Config::builder()
            .set_override("sleep_ms", 25_u64)?
            .build()?;

        adaptor.reload_config(Some(&config))?;

        assert_eq!(25, adaptor.sleep_ms.load(Ordering::Relaxed));

        Ok(())
    }
}
