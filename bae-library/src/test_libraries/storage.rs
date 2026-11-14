//! Libraries whose storage calls reach an isolated in-memory provider.

use crate::followed::value_matching;
use std::sync::Arc;

use crate::LibraryManager;
use bae_catalogs::providers::Providers;
use coven::{MemoryStorage, Storage};

/// [`storage_library`] opening the person's files within `files`, a budget
/// the test sizes and watches.
pub async fn storage_library_within(
    directory: &std::path::Path,
    providers: Providers,
    http: bae_util::http::Http,
    files: coven::FileBudget,
) -> (super::TestLibrary, Arc<MemoryStorage>) {
    let sources =
        super::TestSources::on_budget(&(Arc::new(coven::SystemClock) as coven::ClockRef), files);
    storage_library_on(directory, providers, http, None, sources).await
}

/// Create a store in its own installation, with an injected provider, before
/// setup.
pub async fn storage_library(
    directory: &std::path::Path,
    providers: Providers,
    http: bae_util::http::Http,
) -> (super::TestLibrary, Arc<MemoryStorage>) {
    storage_library_of(directory, providers, http, None).await
}

/// [`storage_library`] of a creator who typed `creator`, whose name it keeps
/// as `create_library` does.
pub async fn storage_library_of(
    directory: &std::path::Path,
    providers: Providers,
    http: bae_util::http::Http,
    creator: Option<&str>,
) -> (super::TestLibrary, Arc<MemoryStorage>) {
    storage_library_on(directory, providers, http, creator, super::TestSources::system()).await
}

/// [`storage_library_of`] on `sources`.
async fn storage_library_on(
    directory: &std::path::Path,
    providers: Providers,
    http: bae_util::http::Http,
    creator: Option<&str>,
    sources: super::TestSources,
) -> (super::TestLibrary, Arc<MemoryStorage>) {
    let app_dir = bae_app_dir::AppDir::under_home(directory).unwrap();
    let installation = sources.installation(app_dir.coven_app_dir());
    let storage =
        Arc::new(MemoryStorage::builder().clock(sources.clock().clone()).build().unwrap());
    let handle = bae_store::configure_store(sources.builder(&installation), crate::SCHEMA)
        .storage_connector(storage.clone())
        .create_store("Test Library", "Test Device", &[], |_, _| Ok(()))
        .await
        .unwrap();
    if let Some(creator) = creator {
        bae_people::People::new(bae_store::test_devices::store_under(
            handle.clone(),
            sources.clock().clone(),
        ))
        .keep_creator_name(&bae_store::EnteredName::parse(creator).unwrap())
        .await
        .unwrap();
    }
    let library =
        super::test_library_from_handle(handle, directory, providers, http, &sources).await;
    library.manager().start().unwrap();
    (library, storage)
}

/// Set up storage and wait for its first pass to end synced, then pause
/// uploads while the caller seeds its queue. Sync stays started: coven
/// uploads only while it is.
pub async fn connect_storage(
    manager: &LibraryManager,
    store: &bae_store::Store,
    storage: &MemoryStorage,
) {
    let coven::StorageConfig::S3 { bucket, region, endpoint, profile } = storage.config() else {
        panic!("the memory provider uses S3 configuration");
    };
    manager
        .save_s3_config(
            crate::sync::S3ConfigData {
                bucket,
                region,
                endpoint: endpoint.map(|url| url.to_string()),
                profile,
                access_key: "test-access".into(),
                secret_key: "test-secret".into(),
            },
            &|_| (),
        )
        .await
        .unwrap();
    let mut status = manager.subscribe_sync_status();
    value_matching(&mut status, |value| value.coven.state == coven::SyncState::Synced).await;
    store.set_uploads_paused(true).await.unwrap();
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
