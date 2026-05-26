# Processor Pattern Reference

Complete annotated code templates for creating a ProSA processor with settings, error type, and Cargo.toml metadata.

## 1. Error Type

Always create a dedicated error type first. Use `thiserror` for ergonomic error definitions.

```rust
use thiserror::Error;
use prosa::core::error::ProcError;
use prosa::core::service::ServiceError;
use std::time::Duration;

#[derive(Debug, Error)]
pub enum MyProcError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Protocol error: {0}")]
    Protocol(String),
    #[error("Configuration error: {0}")]
    Config(String),
}

// Determine which errors allow processor restart
impl ProcError for MyProcError {
    fn recoverable(&self) -> bool {
        match self {
            MyProcError::Io(_) => true,       // transient, can retry
            MyProcError::Protocol(_) => true,  // remote issue, can retry
            MyProcError::Config(_) => false,   // fatal, cannot restart
        }
    }

    fn recovery_duration(&self) -> Duration {
        Duration::from_secs(1) // wait before restart
    }
}

// Convert to ServiceError for returning errors to callers
impl From<MyProcError> for ServiceError {
    fn from(e: MyProcError) -> Self {
        match e {
            MyProcError::Io(e) => ServiceError::UnableToReachService(e.to_string()),
            MyProcError::Protocol(e) => ServiceError::ProtocolError(e),
            MyProcError::Config(e) => ServiceError::UnableToReachService(e),
        }
    }
}
```

## 2. Settings

Use `#[proc_settings]` macro. It automatically adds `adaptor_config_path`, `proc_restart_duration_period`, and `proc_max_restart_period` fields.

```rust
use prosa::core::proc::proc_settings;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[proc_settings]
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct MyProcSettings {
    /// Services to listen on
    pub service_names: Vec<String>,
    /// Connection timeout
    #[serde(default = "MyProcSettings::default_timeout")]
    pub timeout: Duration,
}

impl MyProcSettings {
    fn default_timeout() -> Duration {
        Duration::from_secs(5)
    }

    pub fn new(service_names: Vec<String>) -> Self {
        Self {
            service_names,
            timeout: Self::default_timeout(),
            ..Default::default()
        }
    }
}

// Use #[proc_settings] on Default impl to include macro-added fields
#[proc_settings]
impl Default for MyProcSettings {
    fn default() -> Self {
        Self {
            service_names: Vec::new(),
            timeout: Self::default_timeout(),
        }
    }
}
```

## 3. Processor Struct and Implementation

The `#[proc]` macro adds these fields automatically: `proc` (bus access), `service` (service table), `internal_rx_queue` (message receiver), and `settings` (if specified).

**Important**: You cannot add custom fields to the proc struct. Declare variables inside `internal_run()` instead.

```rust
use prosa::core::adaptor::Adaptor;
use prosa::core::error::ProcError;
use prosa::core::msg::{InternalMsg, Msg, RequestMsg};
use prosa::core::proc::{Proc, ProcBusParam, proc};
use tracing::{debug, info, warn};

// The `extern crate self as prosa;` line is needed when writing processors
// inside the prosa crate itself. When writing processors in external crates
// that depend on `prosa`, this line is NOT needed.
// extern crate self as prosa;

#[proc(settings = MyProcSettings)]
pub struct MyProc {}

// Optional: add helper methods
#[proc]
impl MyProc {
    fn process_request_data(&self, data: &M) -> Result<M, MyProcError> {
        // Transform request data
        let mut response = data.clone();
        response.put_string(1, "processed");
        Ok(response)
    }
}

// The Proc trait implementation — this is the processor's main loop
#[proc]
impl<A> Proc<A> for MyProc
where
    A: Adaptor + MyAdaptorTrait<M> + std::marker::Send + std::marker::Sync,
{
    async fn internal_run(&mut self) -> Result<(), Box<dyn ProcError + Send + Sync>> {
        // 1. Initialize the adaptor
        let adaptor = A::new(self)?;

        // 2. Register the processor with the bus (MUST be before the main loop)
        self.proc.add_proc().await?;

        // 3. Register services to listen on
        self.proc
            .add_service_proc(self.settings.service_names.clone())
            .await?;

        // 4. Main event loop
        loop {
            if let Some(msg) = self.internal_rx_queue.recv().await {
                match msg {
                    InternalMsg::Request(mut msg) => {
                        let _enter = msg.enter_span();
                        debug!(proc_name = self.name(), service = msg.get_service(), "Request received");

                        // Process the request and return response
                        match msg.get_data() {
                            Ok(data) => {
                                let response = adaptor.process_request(msg.get_service(), data.clone());
                                let _ = msg.return_result_to_sender(response);
                            }
                            Err(e) => {
                                warn!("Failed to get request data: {e}");
                            }
                        }
                    }
                    InternalMsg::Response(msg) => {
                        let _enter = msg.enter_span();
                        debug!(proc_name = self.name(), "Response received");
                        // Handle responses to requests we sent
                    }
                    InternalMsg::Error(err) => {
                        let _enter = err.enter_span();
                        warn!(proc_name = self.name(), "Error received: {:?}", err.get_err());
                    }
                    InternalMsg::Command(_cmd) => {
                        // Handle commands (e.g., status, reload)
                    }
                    InternalMsg::Config => {
                        // Handle configuration reload
                    }
                    InternalMsg::Service(table) => {
                        self.service = table;
                    }
                    InternalMsg::Shutdown => {
                        adaptor.terminate();
                        self.proc.remove_proc(None).await?;
                        return Ok(());
                    }
                }
            }
        }
    }
}
```

## 4. Cargo.toml Metadata

Declare the processor for `cargo-prosa` discovery:

```toml
[package.metadata.prosa.my_proc]
proc = "my_crate::proc::MyProc"
settings = "my_crate::proc::MyProcSettings"
adaptor = ["my_crate::adaptor::MyDefaultAdaptor"]
```

## 5. Main Settings (for the binary entrypoint)

Use `#[settings]` (not `#[proc_settings]`) for the global application settings. It adds `name` and `observability` fields.

```rust
use prosa::core::settings::settings;
use serde::{Deserialize, Serialize};

#[settings]
#[derive(Default, Debug, Deserialize, Serialize)]
pub struct MyAppSettings {
    // Global application settings (not processor-specific)
}
```

## 6. Launching the Processor

```rust
use prosa::core::main::{MainProc, MainRunnable};
use prosa::core::proc::{Proc, ProcConfig};

// Create bus and main processor
let (bus, main) = MainProc::<SimpleStringTvf>::create(&settings, Some(3));

// Launch processor with settings
let proc_settings = MyProcSettings::new(vec!["SERVICE_NAME".into()]);
let proc = MyProc::<SimpleStringTvf>::create(1, "my_proc".to_string(), bus.clone(), proc_settings);
Proc::<MyDefaultAdaptor>::run(proc)?;

// Launch processor without settings (using create_raw)
let proc = MyProc::<SimpleStringTvf>::create_raw(2, "my_proc_2".to_string(), bus.clone());
Proc::<MyDefaultAdaptor>::run(proc)?;

// Wait on main task
main.run().await;
```
