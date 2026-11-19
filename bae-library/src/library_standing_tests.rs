use super::{closed_library_standing, StandingState};
use bae_app_dir::AppDir;
use bae_diagnostics::Diagnostics;
use bae_sync_status::SyncStatusWord;
use coven::Storage;
use std::sync::Arc;
use tempfile::TempDir;

/// A library with S3 storage set up, synced, then closed.
async fn synced_closed_library(
    installation: &coven::Coven,
    sources: &crate::test_libraries::TestSources,
    name: &str,
) -> coven::CovenHandle {
    let storage = Arc::new(
        coven::MemoryStorage::builder().clock(Arc::new(coven::SystemClock)).build().unwrap(),
    );
    let handle = bae_store::test_devices::create_test_store(
        sources.builder(installation).storage_connector(storage.clone()),
        crate::SCHEMA,
        name,
        "Test Device",
    )
    .await
    .unwrap();
    handle
        .setup_s3_storage(
            storage.config(),
            "key".into(),
            coven::SecretText::new("secret".into()),
            &|_| (),
        )
        .await
        .unwrap();
    handle.test_sync_status().wait_for(|s| s.state == coven::SyncState::Synced).await;
    handle
}

/// A library that isn't open reports when it last synced and whether it holds
/// changes it never uploaded, as coven recorded them before it closed.
#[tokio::test]
async fn a_closed_library_reports_its_last_sync_and_its_unsent_changes() {
    let temp = TempDir::new().unwrap();
    let installation =
        coven::Coven::in_memory(AppDir::under_home(temp.path()).unwrap().coven_app_dir());
    let sources = crate::test_libraries::TestSources::system();

    let synced = synced_closed_library(&installation, &sources, "Synced").await;
    let synced_id = synced.store_id();
    synced.close().await.unwrap();
    drop(synced);
    let status =
        closed_library_standing(&installation, synced_id, &Diagnostics::noop()).await.unwrap();
    assert!(status.last_synced.is_some(), "{status:?}");
    assert!(
        matches!(&status.state, StandingState::Sync(shown) if shown.status == SyncStatusWord::Synced),
        "{status:?}"
    );

    let unsent = synced_closed_library(&installation, &sources, "Unsent").await;
    let unsent_id = unsent.store_id();
    unsent.stop_sync().await.unwrap();
    let records = bae_records::Records::new(
        bae_store::test_devices::store_under(unsent.clone(), sources.clock().clone()),
        sources.ids().clone(),
    );
    let album = crate::test_libraries::test_album();
    records.insert_album(&album).await.unwrap();
    crate::test_libraries::insert_release(
        &records,
        &crate::test_libraries::test_release(&album.id),
    )
    .await;
    unsent.close().await.unwrap();
    drop(unsent);
    let status =
        closed_library_standing(&installation, unsent_id, &Diagnostics::noop()).await.unwrap();
    assert!(status.last_synced.is_some(), "{status:?}");
    assert_eq!(status.state, StandingState::UnsentChanges);
}

/// A library with no storage set up has nowhere to upload the writes coven
/// queues, the creator's profile among them, so once closed it stands as
/// sync's state words it, as it did while open.
#[tokio::test]
async fn a_closed_library_without_storage_holds_no_unsent_changes() {
    let temp = TempDir::new().unwrap();
    let installation =
        coven::Coven::in_memory(AppDir::under_home(temp.path()).unwrap().coven_app_dir());
    let sources = crate::test_libraries::TestSources::system();
    let handle = bae_store::test_devices::create_test_store(
        sources.builder(&installation),
        crate::SCHEMA,
        "Kept Here",
        "Test Device",
    )
    .await
    .unwrap();
    let id = handle.store_id();
    assert!(!handle.unsent_work().await.unwrap().is_empty(), "creation queues a write");
    handle.close().await.unwrap();
    drop(handle);

    let status = closed_library_standing(&installation, id, &Diagnostics::noop()).await.unwrap();

    assert!(
        matches!(&status.state, StandingState::Sync(shown) if shown.status == SyncStatusWord::NotSynced),
        "{status:?}"
    );
}

/// With nothing to upload a library stands as sync's state words it, and with
/// the time sync last finished.
#[test]
fn a_library_with_nothing_unsent_stands_as_sync_words_it() {
    let synced_at = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000);
    let sync = bae_sync_status::SyncStatusSnapshot::closed(
        coven::SyncStatus { state: coven::SyncState::Synced, last_synced: Some(synced_at) },
        None,
    );

    let standing = super::LibraryStanding::of(&sync, coven::UnsentWork { writes: 0, files: 0 });

    assert_eq!(standing.last_synced, Some(synced_at));
    assert!(
        matches!(&standing.state, StandingState::Sync(shown) if shown.status == SyncStatusWord::Synced),
        "{standing:?}"
    );
}
