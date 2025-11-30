use config::Config;
use prosa::core::adaptor::Adaptor;
use prosa::core::error::ProcError;
use prosa::core::main::{MainProc, MainRunnable};
use prosa::core::msg::{InternalMsg, Msg, RequestMsg};
use prosa::core::proc::{Proc, ProcBusParam, ProcConfig, proc};
use prosa::core::settings::Settings;
use prosa::core::settings::settings;
use prosa::event::pending::PendingMsgs;
use prosa::stub::adaptor::StubParotAdaptor;
use prosa::stub::proc::{StubProc, StubSettings};
use prosa_utils::config::tracing::TelemetryFilter;
use prosa_utils::msg::simple_string_tvf::SimpleStringTvf;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::time;
use tracing::metadata::LevelFilter;
use tracing::{debug, info, warn};

#[derive(Default, Adaptor)]
struct MyAdaptor {}

#[proc]
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
            .add_service_proc(vec![String::from("PROC_TEST")])
            .await?;
        let mut interval = time::interval(time::Duration::from_secs(4));
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
                        InternalMsg::Command(_) => todo!(),
                        InternalMsg::Config => todo!(),
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
                        service.proc_queue.send(InternalMsg::Request(RequestMsg::new(stub_service_name, tvf.clone(), self.proc.get_service_queue()))).await.unwrap();
                    }

                    let proc_service_name = String::from("PROC_TEST");
                    if let Some(service) = self.service.get_proc_service(&proc_service_name) {
                        debug!("The service is find: {:?}", service);
                        service.proc_queue.send(InternalMsg::Request(RequestMsg::new(proc_service_name, tvf, self.proc.get_service_queue()))).await.unwrap();
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
    // Can add parameters here
}

#[allow(clippy::needless_return)]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // load the configuration
    let config = Config::builder()
        .add_source(config::File::with_name("./my_prosa_settings.yml"))
        .add_source(config::Environment::with_prefix("PROSA"))
        .build()
        .unwrap();

    let my_settings = config.try_deserialize::<MySettings>()?;
    println!("My ProSA settings: {my_settings:?}");

    // traces
    let telemetry_filter = TelemetryFilter::new(LevelFilter::DEBUG);
    my_settings
        .get_observability()
        .tracing_init(&telemetry_filter)?;

    // Create bus and main processor
    let (bus, main) = MainProc::<SimpleStringTvf>::create(&my_settings, Some(3));

    // Launch a stub processor
    let stub_settings = StubSettings::new(vec![String::from("STUB_TEST")]);
    let stub_proc = StubProc::<SimpleStringTvf>::create(
        1,
        String::from("STUB_PROC"),
        bus.clone(),
        stub_settings,
    );
    Proc::<StubParotAdaptor>::run(stub_proc);

    // Launch the test processor
    let proc = MyProcClass::<SimpleStringTvf>::create_raw(2, String::from("proc_1"), bus.clone());
    Proc::<MyAdaptor>::run(proc);

    // Wait before launch the second processor
    std::thread::sleep(time::Duration::from_secs(2));

    // Launch the second test processor
    let proc2 = MyProcClass::<SimpleStringTvf>::create_raw(3, String::from("proc_2"), bus.clone());
    Proc::<MyAdaptor>::run(proc2);

    // Wait on main task
    main.run().await;

    Ok(())
}
