//! Why a release's waiting uploads are not moving, as its Storage Manager row
//! says it.

use super::*;
use crate::manager::tests::*;
use bae_records::UploadWait;

/// Why the files of `release_id` that wait in the upload queue are not
/// moving, as the release's Storage Manager row says.
async fn row_wait(manager: &LibraryManager, release_id: &str) -> Option<UploadWait> {
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
        .wait
}

/// A release with files waiting to upload says why they do not move: the
/// network when there is none, else the pause; and nothing once uploads run.
#[tokio::test]
async fn an_uploads_row_says_it_waits_while_uploads_are_paused() {
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
    assert_eq!(row_wait(manager, &release.id).await, Some(UploadWait::Paused));

    test.installation.lose_network();
    assert_eq!(
        row_wait(manager, &release.id).await,
        Some(UploadWait::Offline),
        "the network outranks the pause"
    );

    test.installation.join_network(false);
    manager.store.set_uploads_paused(false).await.unwrap();
    assert_eq!(row_wait(manager, &release.id).await, None);
}

/// A release whose original went missing says which file its move waits
/// for, over the network and the pause: nothing but restoring the file lets
/// the move go on.
#[tokio::test]
async fn an_uploads_row_names_the_original_it_waits_for() {
    let (manager, temp, release, _file_id) = queued_upload_fixture("named").await;
    std::fs::remove_file(temp.path().join("named/a.flac")).unwrap();
    assert_eq!(manager.library.manager().drain_uploads_expecting_work().await.unwrap(), 0);
    manager.library.manager().start().unwrap();
    let mut values = manager.library.manager().subscribe_sync_status();
    pending_list_until(&mut values, |items| !items.is_empty()).await;
    let named = Some(UploadWait::Original { file_name: "a.flac".to_string() });
    assert_eq!(row_wait(manager.library.manager(), &release.id).await, named);
    manager.installation.lose_network();
    assert_eq!(
        row_wait(manager.library.manager(), &release.id).await,
        named,
        "the network does not hide it"
    );
}
