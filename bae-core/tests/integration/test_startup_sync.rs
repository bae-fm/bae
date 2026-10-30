use std::sync::{mpsc, Arc};
use std::time::Duration;

use bae_core::app::{bootstrap, BootstrapError};
use bae_core::config::AppDir;
use coven::{Clock, Storage};
use tempfile::TempDir;

struct TestApp {
    services: bae_core::library::AppServices,
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
    fn sleep(
        &self,
        duration: Duration,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
        let sleep = self.time.sleep(duration);
        self.sleeping.notify_one();
        sleep
    }
}

impl TestApp {
    fn start(
        services: bae_core::library::AppServices,
        runtime: tokio::runtime::Runtime,
    ) -> Result<Self, BootstrapError> {
        Ok(Self { services, runtime })
    }
}

#[test]
fn local_startup_returns_while_storage_is_pending_and_reports_its_failure() {
    let home = TempDir::new().unwrap();
    let app_dir = AppDir::under_home(home.path()).unwrap();
    let layout = app_dir.store_layout();
    let installation = coven::TestCoven::new();
    let clock = Arc::new(StorageClock {
        time: coven::FixedClock::new(coven::SystemClock.now()),
        sleeping: tokio::sync::Notify::new(),
    });
    let storage = Arc::new(
        coven::MemoryStorage::builder()
            .clock(clock.clone())
            .build()
            .unwrap(),
    );
    let setup = bae_test_support::multi_thread_runtime();
    let library_id = setup.block_on(async {
        let handle = bae_core::library::create_library(
            installation
                .builder(layout.clone())
                .storage_connector(storage.clone()),
            bae_core::library_name::LibraryName::parse("Test Library").unwrap(),
        )
        .await
        .unwrap();
        handle
            .setup_s3_storage(
                storage.config(),
                "Device",
                "key".into(),
                coven::SecretText::new("secret".into()),
            )
            .await
            .unwrap();
        let mut status = handle.subscribe_sync_status();
        tokio::time::timeout(
            Duration::from_secs(20),
            status.wait_for(|status| matches!(status, coven::SyncStatus::Synced { .. })),
        )
        .await
        .unwrap()
        .unwrap();
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
    let builder = installation
        .builder(layout)
        .storage_connector(storage.clone());
    let (app_tx, app_rx) = mpsc::sync_channel(1);
    let startup = std::thread::spawn(move || {
        app_tx
            .send(bootstrap(
                app_dir,
                library_id,
                200,
                true,
                bae_core::playback::AudioFocus::Unshared,
                bae_core::diagnostics::Diagnostics::noop(),
                builder,
                TestApp::start,
            ))
            .unwrap();
    });
    setup.block_on(async {
        tokio::time::timeout(Duration::from_secs(20), clock.sleeping.notified())
            .await
            .unwrap();
    });
    let app = app_rx
        .recv_timeout(Duration::from_secs(20))
        .expect("startup returns while the provider's clock is held")
        .unwrap();
    startup.join().unwrap();
    let mut status = app.services.subscribe_sync_status_values();
    app.runtime.block_on(async {
        storage.set_online(false);
        storage.set_faults(coven::Faults::none()).await;
        clock.time.set(clock.now() + Duration::from_secs(60));
        let observed = tokio::time::timeout(
            Duration::from_secs(20),
            status.wait_for(|status| {
                status.error.is_some()
                    || matches!(
                        status.status,
                        coven::SyncStatus::Failed { .. } | coven::SyncStatus::Offline { .. }
                    )
            }),
        )
        .await
        .map(|result| result.map(|_| ()));
        assert!(
            matches!(observed, Ok(Ok(()))),
            "startup failure must reach its observer: {:?}",
            status.borrow()
        );
        app.services.close().await.unwrap();
    });
}
