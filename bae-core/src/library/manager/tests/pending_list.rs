// The pending list the sync status shows: coven's pending records named by
// the rows they are about, each with the error behind it while this process
// holds one.
//
// `include!`d into `manager::tests`.

/// Follow the pending list as the running library does, and wait for the
/// first list `matches` accepts.
#[cfg(feature = "test-utils")]
async fn pending_list_until(
    values: &mut tokio::sync::watch::Receiver<crate::library::SyncStatusSnapshot>,
    matches: impl Fn(&[PendingItem]) -> bool,
) -> Vec<PendingItem> {
    let reached = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        values.wait_for(|status| {
            matches!(&status.pending, Some(Ok(items)) if matches(items))
        }),
    )
    .await
    .map(|value| match &value.unwrap().pending {
        Some(Ok(items)) => items.clone(),
        other => unreachable!("waited for a read list, got {other:?}"),
    });
    reached.unwrap_or_else(|_| {
        let shown = values.borrow().pending.clone().map(|list| {
            list.map(|items| {
                items
                    .iter()
                    .map(|item| (item.subject.clone(), item.wait.reason.clone(), cause_text(item)))
                    .collect::<Vec<_>>()
            })
        });
        panic!("the pending list never reached the expected value: {shown:?}")
    })
}

/// The text of the error behind `item`, with every cause below it.
#[cfg(feature = "test-utils")]
fn cause_text(item: &PendingItem) -> Option<String> {
    item.wait.cause.as_ref().map(|cause| {
        let cause: &(dyn std::error::Error + 'static) = cause.as_ref();
        bae_util::error_text::CauseChain(cause).to_string()
    })
}

/// A file this device is uploading is named on the pending list by its file
/// name and its album, not by coven's ids.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn a_waiting_upload_is_named_by_its_file_and_album() {
    let (manager, temp, _release, _file_id) = queued_upload_fixture("named").await;
    let mut values = manager.subscribe_sync_status_values();
    manager.start().unwrap();
    // The upload can't read a source that moved, so it waits on the file.
    std::fs::remove_file(temp.path().join("named/a.flac")).unwrap();
    assert_eq!(manager.drain_uploads_expecting_work().await.unwrap(), 0);

    pending_list_until(&mut values, |items| {
        items.iter().any(|item| {
            item.subject
                == crate::library::PendingSubjectName::ReleaseFile {
                    file_name: "a.flac".into(),
                    album_title: "Test Album".into(),
                }
        })
    })
    .await;
}

/// A retry that fails for the same reason keeps its record as it was, and the
/// list still shows the error the retry met, not the first attempt's.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn a_retry_that_fails_again_shows_its_own_error() {
    let (manager, _temp, _release, _file_id) = queued_upload_fixture("retried").await;
    let mut values = manager.subscribe_sync_status_values();
    manager.start().unwrap();
    let refuse = |status: u16| coven::Faults {
        fail_next: 1,
        failure: coven::StorageFailure::Network,
        response: Some(status),
        ..coven::Faults::none()
    };

    manager.storage.set_faults(refuse(500)).await;
    assert_eq!(manager.drain_uploads_expecting_work().await.unwrap(), 0);
    let first = pending_list_until(&mut values, |items| {
        items.iter().any(|item| cause_text(item).is_some_and(|text| text.contains("500")))
    })
    .await;

    manager.storage.set_faults(refuse(503)).await;
    assert_eq!(manager.drain_uploads_expecting_work().await.unwrap(), 0);
    let again = pending_list_until(&mut values, |items| {
        items.iter().any(|item| cause_text(item).is_some_and(|text| text.contains("503")))
    })
    .await;
    assert_eq!(
        first.iter().map(|item| (&item.subject, &item.wait.reason)).collect::<Vec<_>>(),
        again.iter().map(|item| (&item.subject, &item.wait.reason)).collect::<Vec<_>>(),
        "the record stays as it was"
    );
}

/// The pending list keeps the order its records arose in, whatever coven's
/// ids for them: a move whose original went missing first stays first after
/// a later one's does, though the later record sorts first.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn the_pending_list_keeps_the_order_its_records_arose_in() {
    let (manager, temp, first, _file_id) = queued_upload_fixture("first").await;
    let mut values = manager.subscribe_sync_status_values();
    manager.start().unwrap();
    // The move whose original at `path` is missing, from its record.
    let missing_move = |path: std::path::PathBuf| {
        let manager = manager.library.clone();
        async move {
            manager
                .database
                .pending()
                .await
                .unwrap()
                .into_iter()
                .find_map(|record| match (record.subject, record.reason) {
                    (
                        coven::PendingSubject::Move { id },
                        coven::PendingReason::Original { path: missing, .. },
                    ) if missing == path => Some(id),
                    _ => None,
                })
                .expect("the move's record names its missing original")
        }
    };
    // A move's record as coven stores it begins with its subject, so two
    // records of one reason sort as their subjects' stored text does.
    let stored = |id: coven::MoveId| serde_json::to_vec(&coven::PendingSubject::Move { id }).unwrap();
    let first_path = temp.path().join("first/a.flac");
    std::fs::remove_file(&first_path).unwrap();
    assert_eq!(manager.drain_uploads_expecting_work().await.unwrap(), 0);
    let first_move = missing_move(first_path).await;

    // This device numbers its moves in the order it makes them, each write
    // staging files for the cloud one. Covers staged now are moves of their
    // own, until the next move's stored subject sorts before the first's, so
    // an order by record would put the second failure first.
    let mut next = first_move.0 + 1;
    while stored(coven::MoveId(next)) >= stored(first_move) {
        store_test_cover(&manager, &first.id, format!("cover {next}").as_bytes()).await;
        next += 1;
    }
    let second = insert_release_with_queued_uploads(
        &manager,
        &temp.path().join("second"),
        "Album Title Second",
        &[("b.flac", b"second-bytes")],
    )
    .await;
    let second_path = temp.path().join("second/b.flac");
    std::fs::remove_file(&second_path).unwrap();
    manager.drain_uploads_expecting_work().await.unwrap();
    assert!(
        stored(missing_move(second_path).await) < stored(first_move),
        "the second failure's record sorts first"
    );

    let album = |title: &str| PendingSubjectName::ReleaseFile {
        file_name: if title == "Test Album" { "a.flac" } else { "b.flac" }.into(),
        album_title: title.into(),
    };
    let listed = pending_list_until(&mut values, |items| {
        items.iter().filter(|item| matches!(item.subject, PendingSubjectName::ReleaseFile { .. })).count() == 2
    })
    .await;
    assert_eq!(
        listed
            .iter()
            .map(|item| item.subject.clone())
            .filter(|subject| matches!(subject, PendingSubjectName::ReleaseFile { .. }))
            .collect::<Vec<_>>(),
        vec![album("Test Album"), album("Album Title Second")],
        "{second:?} failed after the first"
    );
}

/// A pin the download queue couldn't finish waits on the pending list,
/// offering Retry, which pins the release again: the pin lands and the
/// record goes.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn a_failed_pins_retry_pins_the_release_again() {
    let (manager, temp_dir) = setup_test_manager().await;
    let home = connect_test_cloud(&manager).await;
    let release_id = uploaded_release(
        &manager,
        &temp_dir.path().join("retry-pin"),
        "Retry Pin",
        false,
    )
    .await;
    for file in release_files(&manager, &release_id).await {
        manager
            .evict_file_for_test(&manager.release_file_ref(&file.id).await.unwrap())
            .await
            .unwrap();
    }
    let mut values = manager.subscribe_sync_status_values();
    manager.start().unwrap();
    home.set_faults(coven::Faults {
        fail_next: 1_000,
        failure: coven::StorageFailure::Network,
        response: Some(500),
        ..coven::Faults::none()
    })
    .await;
    manager.enqueue_pins(vec![release_id.clone()]).await.unwrap();
    let retry = crate::library::PendingAction::RetryDownload {
        release_id: release_id.clone(),
    };
    pending_list_until(&mut values, |items| {
        items.iter().any(|item| item.wait.action == retry)
    })
    .await;

    download_queue_until(&manager, |queue| {
        queue.rows.iter().any(|row| {
            row.op.release_id == release_id
                && matches!(row.op.state, crate::library::DownloadPhase::Failed { .. })
        })
    })
    .await;
    home.set_faults(coven::Faults::none()).await;
    manager.enqueue_pins(vec![release_id.clone()]).await.unwrap();
    wait_for_kept_release(&manager, &release_id).await;
    pending_list_until(&mut values, |items| {
        !items.iter().any(|item| item.wait.action == retry)
    })
    .await;
    assert!(
        manager
            .library
            .find_release_storage_summary(&release_id)
            .await
            .unwrap()
            .expect("the library holds the release")
            .pinned
    );
}

/// Follow the inspector until it shows a value `accept` takes.
#[cfg(feature = "test-utils")]
async fn inspector_until(
    inspector: &crate::library::DetailSubscription<crate::library::storage_inspector::StorageInspector>,
    accept: impl Fn(&crate::library::storage_inspector::StorageInspector) -> bool,
) -> crate::library::storage_inspector::StorageInspector {
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let snapshot = inspector.next().await.expect("the inspector reads");
            if let Some(value) = snapshot.value.filter(|value| accept(value)) {
                break value;
            }
        }
    })
    .await
    .expect("the inspector shows the expected value")
}

/// The first rows the inspector's read gives for the release `release_id`
/// that `accept` takes: its files with each one's pin as coven's tables hold
/// it, whether or not the inspector shows it.
#[cfg(feature = "test-utils")]
async fn inspector_rows_until(
    manager: &LibraryManager,
    release_id: &str,
    accept: impl Fn(&crate::db::DbStorageInspector) -> bool,
) -> crate::db::DbStorageInspector {
    let mut rows = manager
        .database
        .subscribe_storage_inspector(Some(release_id.to_string()));
    rows_until(&mut rows, accept).await
}

/// The first rows `rows` gives from now that `accept` takes.
#[cfg(feature = "test-utils")]
async fn rows_until(
    rows: &mut coven::ReconfigurableLiveQuery<Option<String>, Option<crate::db::DbStorageInspector>>,
    accept: impl Fn(&crate::db::DbStorageInspector) -> bool,
) -> crate::db::DbStorageInspector {
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let event = rows.next().await.expect("the inspector's read runs");
            if let Some(value) = event.result.unwrap().filter(|value| accept(value)) {
                break value;
            }
        }
    })
    .await
    .expect("the inspector's read gives the expected rows")
}

/// The Storage Manager inspector shows each of its release's files beside
/// its upload while one waits, and the error a failed upload met while this
/// process holds it.
#[cfg(feature = "test-utils")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_inspector_shows_each_files_upload_and_its_error() {
    let (manager, temp, release, file_id) = queued_upload_fixture("inspected").await;
    let services = crate::library::AppServices::for_test(manager.library.clone())
        .await
        .unwrap();
    let inspector = services.subscribe_storage_inspector();
    inspector.set(Some(release.id.clone())).unwrap();
    use crate::library::storage_inspector::StorageInspectorTransfer;
    let waiting = inspector_until(&inspector, |value| {
        value.files.iter().any(|row| row.file.id == file_id && row.upload.is_some())
    })
    .await;
    assert!(waiting.transfers.iter().any(|transfer| matches!(
        transfer,
        StorageInspectorTransfer::Upload { .. }
    )));

    // The upload can't read a source that moved, so it waits on the file
    // with the error it met.
    std::fs::remove_file(temp.path().join("inspected/a.flac")).unwrap();
    assert_eq!(manager.drain_uploads_expecting_work().await.unwrap(), 0);
    inspector_until(&inspector, |value| {
        value.files.iter().any(|row| {
            row.file.id == file_id
                && row
                    .upload
                    .as_ref()
                    .and_then(|upload| upload.pending.as_ref())
                    .is_some_and(|wait| wait.cause.is_some())
        })
    })
    .await;
}

/// A release's file says whether this device keeps it whole, and while the
/// release waits in the download queue the inspector shows each file's pin
/// and counts them; once the download has kept every file it leaves the
/// queue, and the inspector shows no pin but each file kept.
#[cfg(feature = "test-utils")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_inspector_shows_each_files_pin_while_the_release_pins() {
    use crate::album_detail::FilePin;
    use crate::library::storage_inspector::StorageInspectorTransfer;
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    let cloud = uploaded_release(&manager, &temp_dir.path().join("cloud"), "Album Title B", false).await;
    inspector_rows_until(&manager, &cloud, |rows| {
        rows.files.iter().all(|file| file.pin.is_none())
    })
    .await;

    let services = crate::library::AppServices::for_test(manager.library.clone())
        .await
        .unwrap();
    let inspector = services.subscribe_storage_inspector();
    inspector.set(Some(cloud.clone())).unwrap();
    manager.set_downloads_paused(true).await.unwrap();
    manager.enqueue_pins(vec![cloud.clone()]).await.unwrap();
    let pinning = inspector_until(&inspector, |value| !value.transfers.is_empty()).await;
    assert!(pinning.files.iter().all(|file| file.pin == Some(FilePin::Waiting)));
    let [StorageInspectorTransfer::Download {
        summary: Some(summary),
        ..
    }] = pinning.transfers.as_slice()
    else {
        panic!("the pinning release's download counts its files");
    };
    assert_eq!(
        summary
            .iter()
            .map(|count| (count.label.key.as_str(), count.label.count))
            .collect::<Vec<_>>(),
        vec![("core.queue.waiting", 1)],
        "no file is kept yet, so none counts as pinned"
    );

    manager.set_downloads_paused(false).await.unwrap();
    let pinned = inspector_until(&inspector, |value| value.transfers.is_empty()).await;
    assert!(pinned.files.iter().all(|file| file.pin.is_none()));
    inspector_rows_until(&manager, &cloud, |rows| {
        rows.files.iter().all(|file| file.pin == Some(FilePin::Kept))
    })
    .await;
}

/// Each value the inspector reads of a pinning release is one the library
/// held: the release still in the download queue, or out of it with every
/// file kept, never out of it with a file not yet kept. The download worker
/// keeps the files and then takes the release out of the queue, one write
/// after the other, so each pin passes through both.
#[cfg(feature = "test-utils")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_inspector_never_reads_a_pin_out_of_the_queue_before_its_files_are_kept() {
    use crate::album_detail::FilePin;
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    let cloud = uploaded_release(&manager, &temp_dir.path().join("cloud"), "Album Title B", false).await;
    let kept = |value: &crate::db::DbStorageInspector| {
        value
            .files
            .iter()
            .all(|file| file.pin == Some(FilePin::Kept))
    };
    let mut rows = manager
        .database
        .subscribe_storage_inspector(Some(cloud.clone()));

    // The release as it stands before any pin: unpinned, out of the queue.
    rows_until(&mut rows, |value| {
        value.files.iter().all(|file| file.pin.is_none())
    })
    .await;
    for round in 0..20 {
        manager.enqueue_pins(vec![cloud.clone()]).await.unwrap();
        // The read sends only what changes, so every value after the pin is
        // queued holds the rule; a download can finish before a value shows
        // the release in the queue, so none has to.
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let event = rows.next().await.expect("the inspector's read runs");
                let value = event
                    .result
                    .unwrap()
                    .expect("the library holds the release");
                let queued = value.entries.download.is_some();
                assert!(
                    queued || kept(&value),
                    "round {round}: out of the download queue with a file not kept: {value:?}"
                );
                if !queued && kept(&value) {
                    break;
                }
            }
        })
        .await
        .expect("the pin finishes");

        manager.unpin_release(&cloud).await.unwrap();
        rows_until(&mut rows, |value| {
            value.files.iter().all(|file| file.pin.is_none())
        })
        .await;
    }
}
