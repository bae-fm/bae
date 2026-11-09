//! A library's life on this device: releasing the store's cached pages, and
//! removal with what it would lose.

use crate::manager::tests::*;
use tempfile::TempDir;

/// Releasing the store's cached pages leaves the library readable and
/// writable: reads after it load what they need again.
#[tokio::test]
async fn release_memory_keeps_the_library_readable_and_writable() {
    let (manager, _temp_dir, album, release) = manager_with_release().await;
    assert!(find_release(manager.library.manager(), &release.id).await.is_some());

    manager.library.manager().store.release_memory().await.unwrap();

    assert_eq!(find_album(manager.library.manager(), &album.id).await.unwrap().id, album.id);
    let second = create_test_release(&album.id);
    insert_release(manager.library.manager(), &second).await;
    assert_eq!(album_releases(manager.library.manager(), &album.id).await.len(), 2);
}

/// Removing the active library closes its custody and removes both directories.
#[tokio::test]
async fn removal_releases_custody_and_preferences() {
    let (test, _temp) = setup_test_manager().await;
    let manager = test.library.manager();
    let id = manager.store.store_id().to_string();
    manager.ensure_mcp_token().await.unwrap();
    manager.set_mcp_config(bae_preferences::McpConfig::disabled_default()).await.unwrap();
    manager.app_dir.save_active_library(manager.store.store_id()).unwrap();
    manager.remove_library(&test.installation, &|_| ()).await.unwrap();
    assert!(manager.get_mcp_token().await.is_err());
    assert!(!manager.app_dir.registered_library(&id).exists());
    assert!(test.installation.stores().await.unwrap().is_empty());
    assert!(!manager.app_dir.active_library_pointer().exists());
}

/// What removal would lose is read without removing anything; the removal
/// then discards it.
#[tokio::test]
async fn removal_discards_the_unsent_work_it_reports() {
    let (test, _temp) = setup_test_manager().await;
    let manager = test.library.manager();
    connect_test_cloud(&test).await;
    let files = TempDir::new().unwrap();
    insert_release_with_queued_uploads(
        manager,
        files.path(),
        "Album Title",
        &[("01.flac", b"audio")],
    )
    .await;
    let id = manager.store.store_id().to_string();

    let work = manager.unsent_before_removal().await.unwrap();
    assert!(work.writes > 0, "{work:?}");
    assert_eq!(work.files, 1);
    assert_eq!(test.installation.stores().await.unwrap().len(), 1);

    manager.remove_library(&test.installation, &|_| ()).await.unwrap();
    assert!(test.installation.stores().await.unwrap().is_empty());
    assert!(!manager.app_dir.registered_library(&id).exists());
}

/// A library with no storage set up has nowhere it uploads to, so nothing
/// counts as unsent; removing it deletes its whole catalog.
#[tokio::test]
async fn a_library_without_storage_reports_nothing_unsent() {
    let (test, _temp) = setup_test_manager().await;
    let manager = test.library.manager();
    let work = manager.unsent_before_removal().await.unwrap();
    assert!(work.is_empty(), "{work:?}");
}

/// An open library stands by its live sync status: it reports unsent changes
/// once it commits a release whose files it hasn't uploaded, with when it last
/// synced.
#[tokio::test]
async fn an_open_library_stands_by_its_live_sync_status() {
    let (test, _temp) = setup_test_manager().await;
    let manager = test.library.manager();
    connect_test_cloud(&test).await;
    let files = TempDir::new().unwrap();
    insert_release_with_queued_uploads(
        manager,
        files.path(),
        "Album Title",
        &[("01.flac", b"audio")],
    )
    .await;

    let standing = manager.standing().await.unwrap();

    assert_eq!(standing.state, crate::StandingState::UnsentChanges);
    assert!(standing.last_synced.is_some(), "{standing:?}");
}
