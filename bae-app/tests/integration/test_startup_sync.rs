use std::sync::{mpsc, Arc};
use std::time::Duration;

use bae_app::{bootstrap, BootstrapError};
use bae_app_dir::AppDir;
use bae_util::test_channels::HANG_GUARD;
use coven::{Clock, Storage};
use tempfile::TempDir;

struct TestApp {
    services: bae_app::AppServices,
    runtime: tokio::runtime::Runtime,
}

struct StorageClock {
    time: coven::FixedClock,
    sleeping: tokio::sync::Notify,
}

impl Clock for StorageClock {
    fn now(&self) -> std::time::SystemTime {
        self.time.now()
    }
    fn elapsed(&self) -> Duration {
        self.time.elapsed()
    }
    /// The memory provider's injected delay waits on this deadline, which
    /// the test ends by moving the clock once it is armed.
    fn deadline(
        &self,
        duration: Duration,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
        let deadline = self.time.deadline(duration);
        self.sleeping.notify_one();
        deadline
    }
    fn sleep_until(
        &self,
        at: std::time::SystemTime,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<(), coven::WallTimerError>> + Send>,
    > {
        self.time.sleep_until(at)
    }
}

impl TestApp {
    fn start(
        services: bae_app::AppServices,
        runtime: tokio::runtime::Runtime,
    ) -> Result<Self, BootstrapError> {
        Ok(Self { services, runtime })
    }
}

#[test]
fn local_startup_returns_while_storage_is_pending_and_reports_its_failure() {
    let home = TempDir::new().unwrap();
    let app_dir = AppDir::under_home(home.path()).unwrap();
    let installation = coven::Coven::in_memory(app_dir.coven_app_dir());
    let clock = Arc::new(StorageClock {
        time: coven::FixedClock::new(coven::SystemClock.now()),
        sleeping: tokio::sync::Notify::new(),
    });
    let storage = Arc::new(coven::MemoryStorage::builder().clock(clock.clone()).build().unwrap());
    let setup = bae_library::test_libraries::multi_thread_runtime();
    let library_id = setup.block_on(async {
        let handle = bae_library::create_library(
            installation.builder().storage_connector(storage.clone()),
            bae_store::EnteredName::parse("Test Library").unwrap(),
            &bae_diagnostics::Diagnostics::noop(),
        )
        .await
        .unwrap();
        handle
            .setup_s3_storage(
                storage.config(),
                "Device",
                "key".into(),
                coven::SecretText::new("secret".into()),
                &|_| (),
            )
            .await
            .unwrap();
        handle.test_sync_status().wait_for(|status| status.state == coven::SyncState::Synced).await;
        let id = handle.store_id().to_string();
        handle.close().await.unwrap();
        storage
            .set_faults(coven::Faults {
                delay: Duration::from_secs(60),
                fail_next: 1,
                failure: coven::StorageFailure::Network,
                ..coven::Faults::none()
            })
            .await;
        id
    });
    let builder = installation.builder().storage_connector(storage.clone());
    let (app_tx, app_rx) = mpsc::sync_channel(1);
    let startup = std::thread::spawn(move || {
        app_tx
            .send(bootstrap(
                app_dir,
                library_id,
                200,
                true,
                bae_app::PlaybackOutput::Device(Box::new(
                    bae_playback::audio_output::FailingAudioDevice,
                )),
                bae_diagnostics::Diagnostics::noop(),
                builder,
                |_| (),
                TestApp::start,
            ))
            .unwrap();
    });
    setup.block_on(async {
        tokio::time::timeout(HANG_GUARD, clock.sleeping.notified()).await.unwrap();
    });
    let app = app_rx
        .recv_timeout(HANG_GUARD)
        .expect("startup returns while the provider's clock is held")
        .unwrap();
    startup.join().unwrap();
    let status = app.services.subscribe_sync_status();
    app.runtime.block_on(async {
        storage.set_online(false);
        storage.set_faults(coven::Faults::none()).await;
        clock.time.set(clock.now() + Duration::from_secs(60));
        // The startup failure reaches its observer, or this panics.
        bae_library::followed::pulled_matching(&status, |status| {
            status.error.is_some()
                || matches!(
                    status.shown_pass(),
                    Some(coven::LastPass::Failed { .. } | coven::LastPass::Offline { .. })
                )
        })
        .await;
        app.services.close().await.unwrap();
    });
}
