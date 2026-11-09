//! What holds the upload queue back, as the Storage Manager's list says it
//! once, and what a release's row says of a missing original.

use super::*;
use crate::manager::tests::*;
use bae_records::UploadQueueWait;

/// The name of the file whose missing original the move of `release_id`
/// waits for, as the release's Storage Manager row says.
async fn row_missing(manager: &LibraryManager, release_id: &str) -> Option<String> {
    views(manager)
        .get_storage_page(
            &bae_views::StorageSortCriterion {
                field: bae_views::StorageSortField::AlbumTitle,
                direction: bae_store::SortDirection::Ascending,
            },
            bae_views::StorageFilter::All,
            0,
            50,
        )
        .await
        .unwrap()
        .rows
        .into_iter()
        .find(|row| row.release.id == release_id)
        .and_then(|row| row.release.upload)
        .expect("the release has files waiting")
        .missing
}

/// The upload queue says what holds it back, once for the queue and on no
/// release: the pause before the network.
#[tokio::test]
async fn the_upload_queue_says_it_is_paused() {
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    connect_test_cloud(&test).await;
    manager.store.set_uploads_paused(true).await.unwrap();
    let release = insert_release_with_queued_uploads(
        manager,
        &temp_dir.path().join("paused"),
        "Album Title",
        &[("a.flac", b"bytes-a")],
    )
    .await;
    let upload_wait =
        || async { listed_queues(manager).await.uploads.expect("a file is queued").wait };
    assert_eq!(upload_wait().await, Some(UploadQueueWait::Paused));
    assert_eq!(row_missing(manager, &release.id).await, None);

    test.installation.lose_network();
    assert_eq!(
        upload_wait().await,
        Some(UploadQueueWait::Paused),
        "the pause outranks the network"
    );
}

/// A release whose original went missing says which file its move waits
/// for, whatever the network and the pause do: nothing but restoring the
/// file lets the move go on.
#[tokio::test]
async fn an_uploads_row_names_only_a_missing_original() {
    let (manager, temp, release, _file_id) = queued_upload_fixture("named").await;
    std::fs::remove_file(temp.path().join("named/a.flac")).unwrap();
    assert_eq!(manager.library.manager().drain_uploads_expecting_work().await.unwrap(), 0);
    manager.library.manager().start().unwrap();
    let mut values = manager.library.manager().subscribe_sync_status();
    pending_list_until(&mut values, |items| !items.is_empty()).await;
    let named = Some("a.flac".to_string());
    assert_eq!(row_missing(manager.library.manager(), &release.id).await, named);
    manager.installation.lose_network();
    assert_eq!(
        row_missing(manager.library.manager(), &release.id).await,
        named,
        "the network does not hide it"
    );
    manager.library.manager().store.set_uploads_paused(true).await.unwrap();
    assert_eq!(
        row_missing(manager.library.manager(), &release.id).await,
        named,
        "the pause does not hide it"
    );
}
