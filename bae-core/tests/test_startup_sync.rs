#![cfg(feature = "test-utils")]

use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use bae_core::app::bootstrap;
use bae_core::config::CloudProvider;
use bae_core::library::create_library;
use coven::{
    CloudHomeError, CloudKitAcceptedShareRecord, CloudKitAtomicCreateBatch, CloudKitOps,
    CloudKitProviderIdentity, CloudKitRecordCreate, CloudKitRecordVersion, CloudKitScope,
    CloudKitShare, UuidProvider,
};
use serial_test::serial;
use tempfile::TempDir;

fn fake_home() -> TempDir {
    let home = TempDir::new().expect("create test home");
    std::env::set_var("HOME", home.path());
    bae_core::config::install_test_keyring();
    home
}

fn write_cloudkit_library() -> String {
    let mut config = create_library(
        bae_core::library_name::LibraryName::parse("Test Library")
            .expect("valid test library name"),
        &UuidProvider,
    )
    .expect("create test library");
    config.cloud_home.provider = Some(CloudProvider::CloudKit);
    config.cloud_home.storage = coven::HomeStorage::Browsable;
    config.save_to_config_yaml().expect("save test config");
    config.store_id.clone()
}

struct TestApp {
    services: bae_core::library::AppServices,
    _ui_event_bus: bae_core::ui::UiEventBus,
    runtime: tokio::runtime::Runtime,
}

impl TestApp {
    fn start(
        services: bae_core::library::AppServices,
        ui_event_bus: bae_core::ui::UiEventBus,
        runtime: tokio::runtime::Runtime,
    ) -> Result<Self, bae_core::app::BootstrapError> {
        Ok(Self {
            services,
            _ui_event_bus: ui_event_bus,
            runtime,
        })
    }
}

struct PendingCloudKit {
    entered: mpsc::SyncSender<()>,
    release: Mutex<mpsc::Receiver<()>>,
}

impl PendingCloudKit {
    fn new(entered: mpsc::SyncSender<()>, release: mpsc::Receiver<()>) -> Self {
        Self {
            entered,
            release: Mutex::new(release),
        }
    }

    fn unexpected<T>(&self, operation: &str) -> Result<T, CloudHomeError> {
        panic!("unexpected CloudKit operation while provider identity is pending: {operation}")
    }
}

impl CloudKitOps for PendingCloudKit {
    fn provider_identity(
        &self,
        _scope: &CloudKitScope,
    ) -> Result<CloudKitProviderIdentity, CloudHomeError> {
        self.entered
            .send(())
            .expect("test still waits for CloudKit attachment");
        self.release
            .lock()
            .expect("release receiver lock")
            .recv()
            .expect("test releases CloudKit attachment");
        Err(CloudHomeError::Transport(
            "test provider unavailable".to_string(),
        ))
    }

    fn accepted_read_write_share(
        &self,
        _scope: &CloudKitScope,
    ) -> Result<CloudKitAcceptedShareRecord, CloudHomeError> {
        self.unexpected("accepted_read_write_share")
    }

    fn write_record(
        &self,
        _scope: &CloudKitScope,
        _key: &str,
        _data: Vec<u8>,
    ) -> Result<(), CloudHomeError> {
        self.unexpected("write_record")
    }

    fn read_record(&self, _scope: &CloudKitScope, _key: &str) -> Result<Vec<u8>, CloudHomeError> {
        self.unexpected("read_record")
    }

    fn list_records(
        &self,
        _scope: &CloudKitScope,
        _prefix: &str,
    ) -> Result<Vec<String>, CloudHomeError> {
        self.unexpected("list_records")
    }

    fn delete_record(&self, _scope: &CloudKitScope, _key: &str) -> Result<(), CloudHomeError> {
        self.unexpected("delete_record")
    }

    fn record_exists(&self, _scope: &CloudKitScope, _key: &str) -> Result<bool, CloudHomeError> {
        self.unexpected("record_exists")
    }

    fn read_versioned_record(
        &self,
        _scope: &CloudKitScope,
        _key: &str,
    ) -> Result<coven::CloudVersionedObject, CloudHomeError> {
        self.unexpected("read_versioned_record")
    }

    fn begin_atomic_create(
        &self,
        _scope: &CloudKitScope,
    ) -> Result<CloudKitAtomicCreateBatch, CloudHomeError> {
        self.unexpected("begin_atomic_create")
    }

    fn stage_atomic_create_record(
        &self,
        _scope: &CloudKitScope,
        _batch: &CloudKitAtomicCreateBatch,
        _record: CloudKitRecordCreate,
    ) -> Result<(), CloudHomeError> {
        self.unexpected("stage_atomic_create_record")
    }

    fn commit_atomic_create(
        &self,
        _scope: &CloudKitScope,
        _batch: &CloudKitAtomicCreateBatch,
    ) -> Result<Vec<CloudKitRecordVersion>, CloudHomeError> {
        self.unexpected("commit_atomic_create")
    }

    fn discard_atomic_create(
        &self,
        _scope: &CloudKitScope,
        _batch: &CloudKitAtomicCreateBatch,
    ) -> Result<(), CloudHomeError> {
        self.unexpected("discard_atomic_create")
    }

    fn delete_record_versions(
        &self,
        _scope: &CloudKitScope,
        _records: &[CloudKitRecordVersion],
    ) -> Result<(), CloudHomeError> {
        self.unexpected("delete_record_versions")
    }

    fn share_for_member(
        &self,
        _member_pubkey: &str,
    ) -> Result<Option<CloudKitShare>, CloudHomeError> {
        self.unexpected("share_for_member")
    }

    fn grant_share(&self, _member_pubkey: &str) -> Result<CloudKitShare, CloudHomeError> {
        self.unexpected("grant_share")
    }

    fn revoke_share(&self, _member_pubkey: &str) -> Result<(), CloudHomeError> {
        self.unexpected("revoke_share")
    }

    fn accept_share(&self, _share_url: &str) -> Result<CloudKitShare, CloudHomeError> {
        self.unexpected("accept_share")
    }
}

#[test]
#[serial]
fn local_startup_returns_while_cloud_attachment_is_pending() {
    let _home = fake_home();
    let library_id = write_cloudkit_library();
    let (entered_tx, entered_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    let cloudkit = Arc::new(PendingCloudKit::new(entered_tx, release_rx));
    let (app_tx, app_rx) = mpsc::sync_channel(1);

    let bootstrap_thread = std::thread::spawn(move || {
        let result = bootstrap(
            library_id,
            200,
            true,
            bae_core::diagnostics::Diagnostics::noop(),
            Some(cloudkit),
            TestApp::start,
        );
        app_tx.send(result).expect("test receives bootstrap result");
    });

    if entered_rx.recv_timeout(Duration::from_secs(10)).is_err() {
        match app_rx.try_recv() {
            Ok(Ok(app)) => panic!(
                "bootstrap returned before probing CloudKit: {:?}",
                app.services.get_sync_status().error
            ),
            Ok(Err(error)) => panic!("bootstrap failed before probing CloudKit: {error}"),
            Err(error) => panic!("startup did not reach CloudKit attachment: {error}"),
        }
    }
    let early = app_rx.recv_timeout(Duration::from_millis(250));
    let returned_while_pending = early.is_ok();
    release_tx
        .send(())
        .expect("release the pending CloudKit operation");
    let app = match early {
        Ok(result) => result.expect("local startup succeeds"),
        Err(mpsc::RecvTimeoutError::Timeout) => app_rx
            .recv_timeout(Duration::from_secs(10))
            .expect("bootstrap returns after CloudKit is released")
            .expect("local startup succeeds after provider failure"),
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            panic!("bootstrap result sender disconnected")
        }
    };
    bootstrap_thread.join().expect("bootstrap thread joins");

    assert!(
        returned_while_pending,
        "local services must return before cloud attachment completes"
    );

    let mut status = app.services.subscribe_sync_status_values();
    app.runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                if status.borrow_and_update().error.is_some() {
                    return;
                }
                status
                    .changed()
                    .await
                    .expect("sync-status sender remains owned by the app");
            }
        })
        .await
        .expect("provider attachment failure reaches sync status");
    });
}
