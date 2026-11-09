use super::{closed_library_standing, StandingState};
use bae_app_dir::AppDir;
use bae_diagnostics::Diagnostics;
use bae_sync_status::SyncStatusWord;
use coven::Storage;
use std::sync::Arc;
use tempfile::TempDir;

/// A library with S3 storage set up, synced, then closed.
async fn synced_closed_library(installation: &coven::Coven, name: &str) -> coven::CovenHandle {
    let storage = Arc::new(
        coven::MemoryStorage::builder().clock(Arc::new(coven::SystemClock)).build().unwrap(),
    );
    let handle = bae_store::configure_store(installation.builder(), crate::SCHEMA)
        .storage_connector(storage.clone())
        .create_store(name)
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

    let synced = synced_closed_library(&installation, "Synced").await;
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

    let unsent = synced_closed_library(&installation, "Unsent").await;
    let unsent_id = unsent.store_id();
    unsent.stop_sync().await.unwrap();
    let records = bae_records::Records::new(
        bae_store::test_devices::store_under(unsent.clone(), Arc::new(coven::SystemClock)),
        Arc::new(coven::UuidIds),
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
