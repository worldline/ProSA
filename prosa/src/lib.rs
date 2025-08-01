#![doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/COPYRIGHT"))]
//!
//! [![github]](https://github.com/worldline/ProSA)&ensp;[![crates-io]](https://crates.io/crates/prosa)&ensp;[![docs-rs]](crate)&ensp;[![mdbook]](https://worldline.github.io/ProSA/)
//!
#![doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/doc_assets/header_badges.md"))]
//!
//! ProSA base library that define standard modules and include procedural macros
#![warn(missing_docs)]
#![deny(unreachable_pub)]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]

pub mod core;

pub mod event;

pub mod io;

pub mod inj;
pub mod stub;

#[cfg(test)]
mod tests {
    use std::{
        sync::atomic::{AtomicU32, Ordering},
        time,
    };

    extern crate self as prosa;

    use prosa::core::{
        main::{MainProc, MainRunnable as _},
        proc::{Proc, ProcConfig as _},
    };
    use prosa::inj::{
        adaptor::InjDummyAdaptor,
        proc::{InjProc, InjSettings},
    };
    use prosa::stub::{
        adaptor::StubAdaptor,
        proc::{StubProc, StubSettings},
    };
    use prosa_macros::{Adaptor, settings};
    use prosa_utils::msg::simple_string_tvf::SimpleStringTvf;
    use serde::Serialize;

    use crate::core::{adaptor::MaybeAsync, error::ProcError, service::ServiceError};

    const SERVICE_TEST: &str = "PROSA_TEST";
    const WAIT_TIME: time::Duration = time::Duration::from_secs(5);
    static COUNTER: AtomicU32 = AtomicU32::new(0);

    /// Dummy settings
    #[settings]
    #[derive(Default, Debug, Serialize)]
    struct TestSettings {
        stub: StubSettings,
        inj: InjSettings,
    }

    impl TestSettings {
        fn new(service_name: &str) -> Self {
            let stub = StubSettings::new(vec![service_name.into()]);
            let inj = InjSettings::new(service_name.into());
            TestSettings {
                stub,
                inj,
                ..Default::default()
            }
        }
    }

    #[derive(Adaptor)]
    struct TestStubAdaptor {}

    impl<M> StubAdaptor<M> for TestStubAdaptor
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
        fn new(_proc: &StubProc<M>) -> Result<Self, Box<dyn ProcError + Send + Sync>> {
            Ok(Self {})
        }

        fn process_request(
            &self,
            _service_name: &str,
            request: M,
        ) -> MaybeAsync<Result<M, ServiceError>> {
            assert!(!request.is_empty());
            COUNTER.fetch_add(1, Ordering::SeqCst);
            Ok(request.clone()).into()
        }
    }

    /// Test a ProSA with an injector processor sending transactions to a stub processor
    #[allow(clippy::needless_return)]
    #[tokio::test]
    async fn prosa() {
        let test_settings = TestSettings::new(SERVICE_TEST);

        // Create bus and main processor
        let (bus, main) = MainProc::<SimpleStringTvf>::create(&test_settings);

        // Launch the main task
        let main_task = tokio::spawn(main.run());

        // Launch a stub processor
        let stub_proc = StubProc::<SimpleStringTvf>::create(1, bus.clone(), test_settings.stub);
        Proc::<TestStubAdaptor>::run(stub_proc, String::from("STUB_PROC"));

        // Launch an inj processor
        let inj_proc = InjProc::<SimpleStringTvf>::create(2, bus.clone(), test_settings.inj);
        Proc::<InjDummyAdaptor>::run(inj_proc, String::from("INJ_PROC"));

        // Wait before stopping prosa
        tokio::time::sleep(WAIT_TIME).await;
        bus.stop("ProSA unit test end".into()).await.unwrap();

        // Wait on main task to end (should be immediate with the previous stop)
        main_task.await.unwrap();

        // Check exchanges messages
        let nb_trans = COUNTER.load(Ordering::SeqCst) as u64;
        let estimated_trans = WAIT_TIME.as_secs() * 5;
        assert!(nb_trans > (estimated_trans - 2) && nb_trans < (estimated_trans + 2));
        // Should have a coherent number of transaction with the regulator
    }
}
