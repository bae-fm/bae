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

/// The first value the Storage Manager's list gives for what holds the
/// upload queue back that `accept` takes, once the Activity panel's
/// Uploading line says it too.
async fn upload_wait_until(
    manager: &LibraryManager,
    accept: impl Fn(&Option<UploadQueueWait>) -> bool,
) -> Option<UploadQueueWait> {
    let mut queues = views(manager).subscribe_storage_queues();
    let wait = crate::followed::query_matching(&mut queues, |queues| {
        queues.uploads.as_ref().is_some_and(|uploads| accept(&uploads.wait))
    })
    .await
    .uploads
    .expect("a file is queued")
    .wait;
    crate::followed::value_matching(&mut views(manager).subscribe_sync_activity(), |activity| {
        activity.uploads.as_ref().is_some_and(|uploads| uploads.wait == wait)
    })
    .await;
    wait
}

/// The earliest time coven gives a queued file's retry to end, in
/// milliseconds since the epoch.
async fn earliest_retry_ms(manager: &LibraryManager) -> i64 {
    let queue = manager.store.uploads().await.unwrap();
    let retries = queue.files.iter().filter_map(|file| match file.phase {
        coven::UploadPhase::Waiting(coven::UploadWait::Retrying { at }) => Some(at),
        _ => None,
    });
    let at = retries.min().expect("a file waits for its retry");
    i64::try_from(at.duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()).unwrap()
}

/// The upload queue names the first of coven's waits that holds: the pause,
/// then the network, then a metered network, then a file's retry at the
/// earliest time coven gives. Sync that isn't running reads as paused.
#[tokio::test]
async fn the_upload_queue_names_the_first_wait_that_holds() {
    use UploadQueueWait::*;
    let (test, temp_dir, _clock) = library_on_a_stopped_clock().await;
    let manager = test.library.manager();
    manager.store.set_uploads_paused(true).await.unwrap();
    insert_release_with_queued_uploads(
        manager,
        &temp_dir.path().join("held"),
        "Album Title",
        &[("a.flac", &vec![b'a'; 1000])],
    )
    .await;
    test.storage
        .set_faults(coven::Faults {
            refuse_file_uploads: 1_000,
            failure: coven::StorageFailure::Network,
            response: Some(500),
            ..coven::Faults::none()
        })
        .await;
    manager.store.set_uploads_paused(false).await.unwrap();
    let retrying = upload_wait_until(manager, |wait| matches!(wait, Some(Retrying { .. }))).await;
    assert_eq!(retrying, Some(Retrying { at_ms: earliest_retry_ms(manager).await }));

    manager.store.set_unmetered_only(true).await.unwrap();
    test.installation.set_network_metered(true);
    assert_eq!(
        upload_wait_until(manager, |wait| wait != &retrying).await,
        Some(Metered),
        "a metered network outranks the retry"
    );

    test.installation.lose_network();
    assert_eq!(
        upload_wait_until(manager, |wait| wait != &Some(Metered)).await,
        Some(Offline),
        "the network outranks the retry"
    );

    test.installation.join_network(false);
    assert!(matches!(
        upload_wait_until(manager, |wait| wait != &Some(Offline)).await,
        Some(Retrying { .. })
    ));
    manager.store.set_uploads_paused(true).await.unwrap();
    assert_eq!(
        upload_wait_until(manager, |_| true).await,
        Some(Paused),
        "the pause outranks the retry"
    );

    manager.store.set_uploads_paused(false).await.unwrap();
    manager.stop_sync().await.unwrap();
    assert_eq!(
        upload_wait_until(manager, |_| true).await,
        Some(Paused),
        "the queue reads as paused while sync isn't running"
    );
    assert!(!manager.store.uploads_paused().await.unwrap(), "the person didn't pause it");
    manager.close().await.unwrap();
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
    pending_records_until(manager.library.manager(), |records| !records.is_empty()).await;
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

/// The release's storage row stores the name of the file whose missing
/// original its move waits for, from the move's pending record, in the
/// write that records it, and stores none once Cancel ends the move and its
/// record.
#[tokio::test]
async fn a_waiting_moves_storage_row_stores_the_file_its_pending_record_names() {
    let (manager, temp, release, _file_id) = queued_upload_fixture("stored").await;
    let library = manager.library.manager();
    let stored = || super::tests::stored_move_files(library, &release.id);
    assert_eq!(stored().await, (None, None));
    std::fs::remove_file(temp.path().join("stored/a.flac")).unwrap();
    assert_eq!(library.drain_uploads_expecting_work().await.unwrap(), 0);
    library.start().unwrap();
    pending_records_until(library, |records| !records.is_empty()).await;
    assert_eq!(
        stored().await,
        (None, Some("a.flac".to_string())),
        "a missing original holds the move up without failing it"
    );
    library.cancel_release_transition(&release.id).await.unwrap();
    assert_eq!(stored().await, (None, None));
}
