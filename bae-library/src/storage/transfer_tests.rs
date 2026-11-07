use super::*;
use bae_diagnostics::{
    AppDiagnosticMetadata, DatadogDiagnosticsConfig, DiagnosticEvent, Diagnostics,
    RecordingTransport,
};
use coven::Storage;
use std::sync::Arc;
use tempfile::TempDir;

/// A `LibraryManager` (over an empty test DB) whose diagnostics ship to a
/// recording transport, so an emitted event can be read back off the wire.
async fn manager_with_recording_transport(
) -> (LibraryManager, Diagnostics, Arc<RecordingTransport>, TempDir) {
    let (manager, diagnostics, transport, home, _storage, _store) =
        manager_with_recording_transport_over_storage().await;
    (manager, diagnostics, transport, home)
}

/// [`manager_with_recording_transport`], with the memory storage its store
/// can be set up on.
async fn manager_with_recording_transport_over_storage() -> (
    LibraryManager,
    Diagnostics,
    Arc<RecordingTransport>,
    TempDir,
    Arc<coven::MemoryStorage>,
    bae_store::Store,
) {
    let home = TempDir::new().unwrap();
    let storage = Arc::new(
        coven::MemoryStorage::builder()
            .clock(Arc::new(coven::SystemClock))
            .build()
            .expect("memory storage builds"),
    );
    let installation = coven::Coven::in_memory(
        bae_app_dir::AppDir::under_home(home.path()).unwrap().coven_app_dir(),
    );
    let handle = bae_store::configure_store(installation.builder(), crate::SCHEMA)
        .clock(Arc::new(coven::SystemClock))
        .storage_connector(storage.clone())
        .create_store("Test Library")
        .await
        .unwrap();
    let database = bae_store::test_devices::device_store(handle);
    let store = database.clone();

    let transport = Arc::new(RecordingTransport::new(vec![]));
    let config = DatadogDiagnosticsConfig {
        datadog_site: "datadoghq.com".to_string(),
        client_token: "client-token".to_string(),
        source: "test".to_string(),
        app: AppDiagnosticMetadata {
            service: "bae".to_string(),
            environment: "test".to_string(),
            app_version: "1.2.3".to_string(),
            edition: "bae".to_string(),
            git_commit: "abc123".to_string(),
        },
    };
    let diagnostics = Diagnostics::recording_to(
        config,
        Arc::new(coven::SystemClock),
        Arc::new(coven::SequentialIds::new()),
        transport.clone(),
    )
    .expect("diagnostics starts");

    let library = crate::test_libraries::TestLibrary::open_with(
        database,
        bae_app_dir::AppDir::under_home(home.path()).unwrap(),
        bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
        bae_util::http::Http::for_test(),
        Arc::new(coven::SystemClock),
        Arc::new(coven::UuidIds),
        diagnostics.clone(),
    )
    .await;
    let manager = LibraryManager::clone(library.manager());
    (manager, diagnostics, transport, home, storage, store)
}

async fn shipped_events(
    diagnostics: &Diagnostics,
    transport: &RecordingTransport,
) -> Vec<DiagnosticEvent> {
    diagnostics.flush().await.expect("flush succeeds");
    transport
        .requests()
        .iter()
        .flat_map(|request| serde_json::from_slice::<Vec<DiagnosticEvent>>(&request.body).unwrap())
        .collect()
}

/// Pinning a release that isn't in the library fails the transfer's
/// precondition check, which ships `storage_transfer_failed` — an Error-level
/// event — through the real diagnostics pipeline to the wire.
#[tokio::test]
async fn a_failed_transfer_ships_storage_transfer_failed() {
    let (manager, diagnostics, transport, _home) = manager_with_recording_transport().await;

    // No such release, so `ReleaseTransfer::start` errors before any file work.
    let entry = bae_transfers::tables::worker_moves::active_entry::<bae_transfers::DownloadRequest>(
        "missing-release",
    );
    let running = manager.start_pin_transfer(
        "missing-release".to_string(),
        entry,
        |mut progress: TransferProgressFeed| async move {
            while progress.recv().await.is_some() {}
            Ok(())
        },
    );
    running.finish().await.expect_err("a failed transfer finishes no queue entry");

    let events = shipped_events(&diagnostics, &transport).await;
    let failed = events
        .iter()
        .find(|e| e.name == "storage_transfer_failed")
        .expect("a failed transfer ships storage_transfer_failed");
    assert_eq!(failed.fields["action"], serde_json::json!("pin"));
    assert_eq!(failed.fields["release_id"], serde_json::json!("missing-release"));
}

#[tokio::test]
async fn a_failed_upload_batch_ships_storage_transfer_failed() {
    let (manager, diagnostics, transport, _home, storage, store) =
        manager_with_recording_transport_over_storage().await;
    store
        .setup_s3_storage(
            storage.config(),
            "Test Device",
            "test-key".into(),
            coven::SecretText::new("test-secret".into()),
            &|_| (),
        )
        .await
        .unwrap();

    let outcome = manager
        .upload_releases(&["missing-release".to_string()])
        .await
        .expect("a per-release refusal is returned as a batch outcome");
    let crate::UploadReleasesOutcome::Partial { receipt: None, failure } = outcome else {
        panic!("the missing release must be refused without a receipt");
    };
    assert_eq!(failure.release_ids, vec!["missing-release"]);

    let events = shipped_events(&diagnostics, &transport).await;
    let failed = events
        .iter()
        .find(|event| event.name == "storage_transfer_failed")
        .expect("a failed batch ships storage_transfer_failed");
    assert_eq!(failed.fields["action"], serde_json::json!("upload"));
    assert_eq!(failed.fields["release_id"], serde_json::json!("missing-release"));
}
