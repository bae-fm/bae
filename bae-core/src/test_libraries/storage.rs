//! Libraries whose storage calls reach an isolated in-memory provider.

use crate::followed::value_matching;
use std::sync::Arc;

use crate::library::LibraryManager;
use bae_catalogs::providers::Providers;
use coven::{MemoryStorage, Storage};

/// Create a store with its own keychain and an injected provider, before setup.
pub async fn storage_library(
    directory: &std::path::Path,
    providers: Providers,
) -> (super::TestLibrary, Arc<MemoryStorage>) {
    let installation = coven::TestCoven::new();
    let storage =
        Arc::new(MemoryStorage::builder().clock(Arc::new(coven::SystemClock)).build().unwrap());
    let handle = bae_db::configure_store(
        installation.builder(bae_app_dir::AppDir::under_home(directory).unwrap().store_layout()),
    )
    .storage_connector(storage.clone())
    .create_store("Test Library")
    .await
    .unwrap();
    let library = super::test_library_from_handle(handle, directory, providers).await;
    library.start().unwrap();
    (library, storage)
}

/// Set up storage, then pause uploads while the caller seeds its queue.
pub async fn connect_storage(manager: &LibraryManager, storage: &MemoryStorage) {
    let coven::StorageConfig::S3 { bucket, region, endpoint, prefix, profile } = storage.config()
    else {
        panic!("the memory provider uses S3 configuration");
    };
    manager
        .save_s3_config(
            crate::sync::S3ConfigData {
                bucket,
                region,
                endpoint: endpoint.map(|url| url.to_string()),
                key_prefix: prefix,
                profile,
                access_key: "test-access".into(),
                secret_key: "test-secret".into(),
            },
            "Test Device",
            &|_| (),
        )
        .await
        .unwrap();
    let mut status = manager.subscribe_sync_status();
    value_matching(&mut status, |value| matches!(value.status, coven::SyncStatus::Synced { .. }))
        .await;
    manager.stop_sync().await.unwrap();
    value_matching(&mut status, |value| matches!(value.status, coven::SyncStatus::Stopped)).await;
    manager.unlock_store_key().await.unwrap();
    manager.set_sync_paused(true).await.unwrap();
}

/// Remove the provider object read for this file, then discard the cached copy.
pub async fn remove_uploaded_file(
    manager: &LibraryManager,
    storage: &MemoryStorage,
    file_id: &str,
) {
    let reference = manager.release_file_ref_for_test(file_id).await.unwrap();
    manager.evict_file_for_test(reference.id()).await.unwrap();
    let before = storage.reads().await.len();
    manager.materialize_release_file_for_test(file_id).await.unwrap();
    let reads = storage.reads().await;
    let objects: std::collections::BTreeSet<_> =
        reads[before..].iter().map(|(path, _, _)| path.clone()).collect();
    assert_eq!(objects.len(), 1, "the file read names exactly one provider object");
    for object in objects {
        storage.delete(&object).await.unwrap();
    }
    manager.evict_file_for_test(reference.id()).await.unwrap();
}

/// This release's storage state and whether it is pinned on this device.
pub async fn release_storage(
    database: &bae_db::Database,
    release_id: &str,
) -> (crate::album_detail::ReleaseStorageState, bool) {
    let summary = crate::library::views::LibraryViews::new(database.clone())
        .find_release_detail(release_id)
        .await
        .unwrap()
        .unwrap()
        .summary;
    (summary.storage_state, summary.pinned)
}
