use prosa::core::adaptor::Adaptor;
use prosa::core::error::ProcError;
use prosa::core::main::{MainProc, MainRunnable};
use prosa::core::msg::{InternalMsg, Msg, RequestMsg};
use prosa::core::proc::{Proc, ProcBusParam, ProcConfig, proc};
use prosa::core::settings::{ProsaConfig, Settings, settings};
use prosa::core::settings::tracing::TelemetryFilter;
use prosa::event::pending::PendingMsgs;
use prosa::stub::adaptor::StubParotAdaptor;
use prosa::stub::proc::{StubProc, StubSettings};
use prosa::tracing::{debug, info, warn};
use prosa_macros::proc_settings;
use prosa_utils::msg::simple_string_tvf::SimpleStringTvf;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::time;
use tracing::metadata::LevelFilter;

#[derive(Default, Adaptor)]
struct MyAdaptor {}

#[proc_settings]
#[derive(Debug, Deserialize, Serialize, Clone)]
struct MyProcSettings {
    service_name: String,
    tick_secs: u64,
}

impl MyProcSettings {
    fn interval(&self) -> time::Interval {
        time::interval(time::Duration::from_secs(self.tick_secs.max(1)))
    }
}

#[proc_settings]
impl Default for MyProcSettings {
    fn default() -> Self {
        MyProcSettings {
            service_name: String::from("PROC_TEST"),
            tick_secs: 4,
        }
    }
}

#[proc(settings = MyProcSettings)]
struct MyProcClass {}

#[proc]
impl<A> Proc<A> for MyProcClass
where
    A: Default + Adaptor + std::marker::Send + std::marker::Sync,
{
    async fn internal_run(&mut self) -> Result<(), Box<dyn ProcError + Send + Sync>> {
        let adaptor = A::default();
        self.proc.add_proc().await?;
        self.proc
            .add_service_proc(vec![self.settings.service_name.clone()])
            .await?;
        let mut interval = self.settings.interval();
        let mut pending_msgs: PendingMsgs<RequestMsg<M>, M> = Default::default();
        loop {
            tokio::select! {
                Some(msg) = self.internal_rx_queue.recv() => {
                    match msg {
                        InternalMsg::Request(msg) => {
                            info!("Proc {} receive a request: {:?}", self.get_proc_id(), msg);

                            // Push in the pending message
                            pending_msgs.push(msg, Duration::from_millis(200));
                            //msg.return_to_sender(tvf).await.unwrap();
                        },
                        InternalMsg::Response(msg) => {
                            let _enter = msg.enter_span();
                            info!("Proc {} receive a response: {:?}", self.get_proc_id(), msg);
                        },
                        InternalMsg::Error(err) => {
                            let _enter = err.enter_span();
                            info!("Proc {} receive an error: {:?}", self.get_proc_id(), err);
                        },
                        InternalMsg::Config(config) => {
                            let settings = config.get_proc::<MyProcSettings>(self.proc.as_ref())?;

                            if self.settings.service_name != settings.service_name {
                                self.proc
                                    .remove_service_proc(vec![self.settings.service_name.clone()])
                                    .await?;
                                self.proc
                                    .add_service_proc(vec![settings.service_name.clone()])
                                    .await?;
                            }

                            if self.settings.tick_secs != settings.tick_secs {
                                interval = settings.interval();
                            }

                            info!("Proc {} reloaded settings: {:?}", self.get_proc_id(), settings);
                            self.settings = settings;
                        },
                        InternalMsg::Service(table) => {
                            debug!("New service table received:\n{}\n", table);
                            self.service = table;
                        },
                        InternalMsg::Shutdown => {
                            adaptor.terminate();
                            warn!("The processor will shut down");
                        },
                    }
                },
                _ = interval.tick() => {
                    debug!("Timer on my proc");

                    let mut tvf: M = Default::default();
                    tvf.put_string(1, String::from("test srv"));
                    tvf.put_string(2, String::from("request"));

                    let stub_service_name = String::from("STUB_TEST");
                    if let Some(service) = self.service.get_proc_service(&stub_service_name) {
                        debug!("The service is find: {:?}", service);
                        let _ = service.proc_queue.send(InternalMsg::Request(RequestMsg::new(stub_service_name, tvf.clone(), self.proc.get_service_queue()))).await;
                    }

                    if let Some(service) = self.service.get_proc_service(&self.settings.service_name) {
                        debug!("The service is find: {:?}", service);
                        let _ = service.proc_queue.send(InternalMsg::Request(RequestMsg::new(self.settings.service_name.clone(), tvf, self.proc.get_service_queue()))).await;
                    }
                },
                Some(msg) = pending_msgs.pull(), if !pending_msgs.is_empty() => {
                    debug!("Timeout message {:?}", msg);


                    let mut tvf: M = Default::default();
                    tvf.put_unsigned(1, 42u64);
                    tvf.put_string(2, "test");

                    // Return the message to the sender, but ignore error if the sender is not present anymore
                    let _ = msg.return_to_sender(tvf);
                },
            }
        }
    }
}

#[settings]
#[derive(Default, Debug, Deserialize, Serialize)]
struct MySettings {
    stub_proc: StubSettings,
    proc_1: MyProcSettings,
    proc_2: MyProcSettings,
}

#[allow(clippy::needless_return)]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config_path = "examples/my_prosa_settings.yml";

    // load the configuration
    let config = ProsaConfig::from_path(config_path)?;

    let my_settings = config.try_deserialize::<MySettings>()?;
    println!("My ProSA settings: {my_settings:?}");

    // traces
    let telemetry_filter = TelemetryFilter::new(LevelFilter::DEBUG);
    my_settings
        .get_observability()
        .tracing_init(&telemetry_filter)?;

    // Create bus and main processor
    let (bus, main) = MainProc::<SimpleStringTvf>::create(&my_settings, Some(3));
    bus.update_config(Arc::new(config.clone())).await?;

    let reload_bus = bus.clone();
    let reload_filter = telemetry_filter.clone();
    tokio::spawn(async move {
        prosa::core::settings::watch_config_reload::<MySettings, _, _, _>(
            String::from(config_path),
            config,
            ProsaConfig::from_path,
            move |settings, new_config| {
                let reload_bus = reload_bus.clone();
                let reload_filter = reload_filter.clone();
                async move {
                    reload_filter.set_level(settings.get_observability().get_level().into());

                    if let Err(err) = reload_bus.update_config(Arc::new(new_config)).await {
                        warn!("Can't notify processors of configuration reload: {err}");
                        false
                    } else {
                        true
                    }
                }
            },
        )
        .await;
    });

    // Launch a stub processor
    let stub_proc = StubProc::<SimpleStringTvf>::create(
        1,
        String::from("stub_proc"),
        bus.clone(),
        my_settings.stub_proc.clone(),
    );
    Proc::<StubParotAdaptor>::run(stub_proc)?;

    // Launch the test processor
    let proc = MyProcClass::<SimpleStringTvf>::create(
        2,
        String::from("proc_1"),
        bus.clone(),
        my_settings.proc_1.clone(),
    );
    Proc::<MyAdaptor>::run(proc)?;

    // Wait before launch the second processor
    std::thread::sleep(time::Duration::from_secs(2));

    // Launch the second test processor
    let proc2 = MyProcClass::<SimpleStringTvf>::create(
        3,
        String::from("proc_2"),
        bus.clone(),
        my_settings.proc_2.clone(),
    );
    Proc::<MyAdaptor>::run(proc2)?;

    // Wait on main task
    main.run().await;

    Ok(())
}
