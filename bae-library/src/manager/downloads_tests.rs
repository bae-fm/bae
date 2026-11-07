//! The download (pin) queue: what it queues and passes over, pausing,
//! cancelling and retrying, the bytes its downloads report, how the storage
//! list shows a release it holds, and the inspector's view of a pinning
//! release.

use super::*;
use crate::manager::tests::*;

/// Pausing before the first enqueue parks the worker, so the queue's
/// state (enqueue, dedup, snapshot counts, cancel) is observable
/// deterministically without the download path racing the assertions.
#[tokio::test]
async fn download_queue_enqueue_dedups_and_cancels_while_paused() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let release_id = insert_pinnable_release(&test).await;

    // Park the worker up front so nothing drains while we inspect state.
    manager.set_downloads_paused(true).await.unwrap();

    manager.enqueue_pins(vec![release_id.clone()]).await.unwrap();
    let snap = download_queue_now(manager).await;
    assert_eq!(snap.total.queued, 1);
    assert_eq!(snap.rows.len(), 1);
    assert_eq!(snap.rows[0].op.state, bae_transfers::DownloadPhase::Queued);
    assert!(snap.paused);

    // Re-enqueuing the same release is a no-op: still one entry.
    manager.enqueue_pins(vec![release_id.clone()]).await.unwrap();
    assert_eq!(download_queue_now(manager).await.rows.len(), 1);

    // Cancel drops the entry.
    manager.cancel_download(&release_id).await.unwrap();
    let snap = download_queue_now(manager).await;
    assert!(snap.rows.is_empty());
}

/// An already-pinned release is skipped at enqueue rather than re-downloaded.
#[tokio::test]
async fn download_queue_skips_already_pinned() {
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();

    // A genuinely pinned release: made Remote with pin, so its file lands in
    // coven's offline cache.
    connect_test_cloud(&test).await;
    let release_id =
        uploaded_release(manager, &temp_dir.path().join("pinned"), "Test Album", true).await;
    let summary = views(manager).find_release_detail(&release_id).await.unwrap().unwrap().summary;
    assert_eq!(
        summary.storage_state,
        bae_views::album_detail::ReleaseStorageState::InStorage { pinned: true },
        "the offline-cached file makes it read as pinned"
    );

    manager.set_downloads_paused(true).await.unwrap();
    manager.enqueue_pins(vec![release_id.clone()]).await.unwrap();
    assert!(download_queue_now(manager).await.rows.is_empty());
}

/// A pin request queues, in the order asked, each release it can, in one
/// write: an unknown id, a repeat and one already queued are passed over.
#[tokio::test]
async fn a_pin_request_queues_what_it_can_in_the_order_asked() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let first = insert_pinnable_release(&test).await;
    let source = manager.library_path().join("second-source");
    let second = uploaded_release(manager, &source, "Album Title", false).await;
    manager.set_downloads_paused(true).await.unwrap();

    manager
        .enqueue_pins(vec![
            second.clone(),
            "4b9f0e0e-7d3a-4f2a-9c51-1f0c2b3a4d5e".to_string(),
            first.clone(),
            second.clone(),
        ])
        .await
        .unwrap();

    assert_eq!(
        download_queue_now(manager)
            .await
            .rows
            .iter()
            .map(|row| row.op.release_id.clone())
            .collect::<Vec<_>>(),
        [second, first]
    );
}

/// A Local release has nothing to pin — it is already fully on disk —
/// so `enqueue_pins` skips it rather than queueing a download that would fail. The
/// album grid's bulk pin reaches this path with a mixed local/remote selection.
#[tokio::test]
async fn download_queue_skips_local_release() {
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    manager.set_downloads_paused(true).await.unwrap();

    let release = insert_local_release_with_files(
        manager,
        &temp_dir.path().join("local-source"),
        "Test Album",
        &[("a.flac", b"aaa")],
    )
    .await;
    let summary = views(manager).find_release_detail(&release.id).await.unwrap().unwrap().summary;
    assert_eq!(summary.storage_state, bae_views::album_detail::ReleaseStorageState::OnOneDevice);

    manager.enqueue_pins(vec![release.id.clone()]).await.unwrap();
    assert!(
        download_queue_now(manager).await.rows.is_empty(),
        "a local release is not enqueued for pinning"
    );
}

/// A pin that fails because storage is offline lands `Failed`
/// and stays in the queue; `retry_downloads` flips it back to `Queued`.
#[tokio::test]
async fn download_queue_failed_pin_retries() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let release_id = insert_pinnable_release(&test).await;

    test.storage.set_online(false);
    // The evicted file cannot be fetched while storage is offline.
    manager.enqueue_pins(vec![release_id.clone()]).await.unwrap();

    // Let the worker pick it up, fail, and mark it Failed.
    let failed = download_queue_until(manager, |queue| {
        matches!(
            queue.rows.first().map(|row| &row.op.state),
            Some(bae_transfers::DownloadPhase::Failed { .. })
        )
    })
    .await;
    assert_eq!(failed.total.failed, 1, "the pin lands Failed while storage is offline");

    // Retry flips it back to Queued; while storage is offline it will fail again,
    // but the immediate post-retry state is Queued (or already re-failed).
    manager.retry_downloads().await.unwrap();
    let snap = download_queue_now(manager).await;
    assert!(
        snap.rows.first().is_some_and(|row| matches!(
            row.op.state,
            bae_transfers::DownloadPhase::Queued
                | bae_transfers::DownloadPhase::Active { .. }
                | bae_transfers::DownloadPhase::Failed { .. }
        )),
        "after retry the release is still tracked"
    );

    // Cancelling clears it regardless of the in-flight retry.
    manager.cancel_download(&release_id).await.unwrap();
    assert!(download_queue_now(manager).await.rows.is_empty(), "cancel removes the entry");
}

/// The download queue takes no entry while the device has no usable network,
/// and goes on when the network returns.
#[tokio::test]
async fn a_pin_waits_while_the_network_is_down() {
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    connect_test_cloud(&test).await;
    let release_id =
        uploaded_release(manager, &temp_dir.path().join("offline-pin"), "Offline Pin", false).await;
    for file in release_files(manager, &release_id).await {
        manager.evict_file_for_test(file.file).await.unwrap();
    }
    manager.start().unwrap();

    test.installation.lose_network();
    manager.enqueue_pins(vec![release_id.clone()]).await.unwrap();
    // The worker has had every chance to take the entry.
    for _ in 0..200 {
        tokio::task::yield_now().await;
    }
    let queue = download_queue_now(manager).await;
    assert_eq!(
        queue.rows.iter().map(|row| &row.op.state).collect::<Vec<_>>(),
        vec![&bae_transfers::DownloadPhase::Queued],
        "the entry waits for the network"
    );

    test.installation.join_network(false);
    wait_for_kept_release(manager, &release_id).await;
    download_queue_until(manager, |queue| queue.rows.is_empty()).await;
}

/// Each pin waiting in the queue shows how many entries are ahead of it that
/// have not failed, and that the queue is paused; a failed entry is not
/// ahead of anything.
#[tokio::test]
async fn a_queued_pin_counts_the_entries_ahead_of_it() {
    use bae_views::album_detail::{DownloadEntryStep, StorageRowStatus};
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    connect_test_cloud(&test).await;
    let dir = |name: &str| temp_dir.path().join(name);
    let first = uploaded_release(manager, &dir("first"), "Album Title A", false).await;
    let second = uploaded_release(manager, &dir("second"), "Album Title B", false).await;
    let third = uploaded_release(manager, &dir("third"), "Album Title C", false).await;
    manager.set_downloads_paused(true).await.unwrap();
    manager.enqueue_pins(vec![first.clone(), second.clone(), third.clone()]).await.unwrap();

    let statuses = || async {
        let sort = bae_views::StorageSortCriterion {
            field: bae_views::StorageSortField::AlbumTitle,
            direction: bae_store::SortDirection::Ascending,
        };
        views(manager)
            .get_storage_page(&sort, bae_views::StorageFilter::All, 0, 10)
            .await
            .unwrap()
            .rows
            .into_iter()
            .map(|row| row.status)
            .collect::<Vec<_>>()
    };
    let waiting = |ahead| {
        StorageRowStatus::Pinning(DownloadEntryStep::Queued {
            ahead,
            wait: Some(bae_transfers::QueueWait::Paused),
        })
    };
    assert_eq!(statuses().await, vec![waiting(0), waiting(1), waiting(2)]);

    assert!(activate_download(manager, &first).await.unwrap());
    assert!(fail_download(manager, &first, "the network connection was lost".into())
        .await
        .unwrap());
    manager.set_downloads_paused(true).await.unwrap();
    assert_eq!(
        statuses().await,
        vec![
            StorageRowStatus::DownloadFailed {
                error: "the network connection was lost".to_string()
            },
            waiting(0),
            waiting(1),
        ],
        "a failed entry is neither ahead of the rest nor waiting"
    );

    // Without a network the wait is the network: it makes the pause moot.
    test.installation.lose_network();
    assert_eq!(
        statuses().await[1],
        StorageRowStatus::Pinning(DownloadEntryStep::Queued {
            ahead: 0,
            wait: Some(bae_transfers::QueueWait::Offline)
        })
    );
}

/// Pinning a release whose download failed queues that entry again, in its
/// place ahead of one queued after it, with its error gone.
#[tokio::test]
async fn pinning_a_failed_download_queues_it_again_in_its_place() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let failed = insert_pinnable_release(&test).await;
    let source = manager.library_path().join("second-source");
    let later = uploaded_release(manager, &source, "Album Title", false).await;

    test.storage.set_online(false);
    manager.enqueue_pins(vec![failed.clone()]).await.unwrap();
    download_queue_until(manager, |queue| {
        matches!(
            queue.rows.first().map(|row| &row.op.state),
            Some(bae_transfers::DownloadPhase::Failed { .. })
        )
    })
    .await;
    // The worker leaves both entries waiting from here.
    manager.set_downloads_paused(true).await.unwrap();
    manager.enqueue_pins(vec![later.clone()]).await.unwrap();

    manager.enqueue_pins(vec![failed.clone()]).await.unwrap();

    let queue = download_queue_now(manager).await;
    assert_eq!(
        queue
            .rows
            .iter()
            .map(|row| (row.op.release_id.clone(), row.op.state.clone()))
            .collect::<Vec<_>>(),
        [
            (failed, bae_transfers::DownloadPhase::Queued),
            (later, bae_transfers::DownloadPhase::Queued),
        ]
    );
    assert_eq!(queue.total.failed, 0);
}

/// Each step of bytes a pin drives reaches the active download's progress,
/// read apart from the queue's table.
#[tokio::test]
async fn download_transfers_report_each_driven_file_progress() {
    use crate::storage::transfer::TransferProgress;

    let (test, _temp_dir) = setup_test_manager().await;

    let manager = test.library.manager();
    let release_id = "release-progress".to_string();
    // The manager's own worker leaves the entry to this test.
    manager.set_downloads_paused(true).await.unwrap();
    assert!(manager
        .downloads
        .enqueue_all(vec![bae_transfers::QueuedPin {
            release_id: release_id.clone(),
            created_at: 0,
            payload: (),
        }])
        .await
        .unwrap());
    let pending = tokio::spawn(std::future::pending::<()>());
    assert!(manager
        .downloads
        .activate_for_test(
            &release_id,
            pending.abort_handle(),
            bae_transfers::DownloadTransferProgress::new(&release_id, 0, 7).unwrap(),
        )
        .await
        .unwrap());

    let mut transfers = manager.downloads.watch_transfers();
    let bytes_done = |transfers: &std::collections::BTreeMap<
        String,
        bae_transfers::DownloadTransferProgress,
    >| {
        transfers.get("release-progress").expect("the active download has progress").clone()
    };
    assert_eq!(bytes_done(&transfers.until(|_| true).await.unwrap()).bytes_done, 0);

    let (progress_tx, progress_rx) = tokio::sync::mpsc::unbounded_channel();
    let driver = {
        let manager = manager.clone();
        let release_id = release_id.clone();
        tokio::spawn(async move {
            manager
                .drive_transfer(
                    &release_id,
                    ReleaseStorageAction::Pin,
                    crate::storage::transfer::TransferProgressFeed::of_test_channel(progress_rx),
                )
                .await
        })
    };
    for done in [3, 7] {
        progress_tx
            .send(TransferProgress::Progress {
                progress: bae_transfers::DownloadTransferProgress::new(&release_id, done, 7)
                    .unwrap(),
            })
            .expect("the transfer driver is listening");
        let progress = bytes_done(
            &transfers
                .until(|transfers| bytes_done(transfers).bytes_done == done)
                .await
                .expect("file progress value"),
        );
        assert_eq!(progress.bytes_done, done);
        assert_eq!(progress.bytes_total, 7);
        assert_eq!(progress.fraction, done as f64 / 7.0);
    }
    progress_tx
        .send(TransferProgress::Complete { release_id: release_id.clone() })
        .expect("the transfer driver is listening");
    driver.await.unwrap().unwrap();
    pending.abort();
}

/// Bytes arriving for a download leave the sync activity as it was, so they
/// send it no new value; the download's start and its end each do.
#[tokio::test]
async fn download_bytes_send_no_new_sync_activity() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let release_id = "release-activity".to_string();
    let progress = |bytes_done| {
        bae_transfers::DownloadTransferProgress::new(&release_id, bytes_done, 7).unwrap()
    };
    let mut activity = views(test.library.manager()).subscribe_sync_activity();
    assert_eq!(crate::followed::value_matching(&mut activity, |_| true).await.downloads, None);

    // The manager's own worker leaves the entry to this test.
    manager.set_downloads_paused(true).await.unwrap();
    assert!(manager
        .downloads
        .enqueue_all(vec![bae_transfers::QueuedPin {
            release_id: release_id.clone(),
            created_at: 0,
            payload: (),
        }])
        .await
        .unwrap());
    let pending = tokio::spawn(std::future::pending::<()>());
    assert!(manager
        .downloads
        .activate_for_test(&release_id, pending.abort_handle(), progress(0))
        .await
        .unwrap());
    assert_eq!(
        crate::followed::value_matching(&mut activity, |_| true).await.downloads,
        Some(bae_views::DownloadsInFlight { active: 1, queued: 0 })
    );

    let next = activity.next();
    tokio::pin!(next);
    for bytes_done in [3, 7] {
        assert!(manager.downloads.set_active_progress(&release_id, progress(bytes_done)));
        let polled = tokio::task::unconstrained(std::future::poll_fn(|cx| {
            std::task::Poll::Ready(std::future::Future::poll(next.as_mut(), cx))
        }))
        .await;
        assert!(polled.is_pending(), "{bytes_done} bytes sent an activity");
    }
    manager.cancel_download(&release_id).await.unwrap();
    assert_eq!(next.await.unwrap().unwrap().downloads, None);
    pending.abort();
}

/// A pin the download queue couldn't finish waits on the pending list,
/// offering Retry, which pins the release again: the pin lands and the
/// record goes.
#[tokio::test]
async fn a_failed_pins_retry_pins_the_release_again() {
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let home = connect_test_cloud(&test).await;
    let release_id =
        uploaded_release(manager, &temp_dir.path().join("retry-pin"), "Retry Pin", false).await;
    for file in release_files(manager, &release_id).await {
        manager.evict_file_for_test(file.file).await.unwrap();
    }
    let mut values = manager.subscribe_sync_status();
    manager.start().unwrap();
    home.set_faults(coven::Faults {
        fail_next: 1_000,
        failure: coven::StorageFailure::Network,
        response: Some(500),
        ..coven::Faults::none()
    })
    .await;
    manager.enqueue_pins(vec![release_id.clone()]).await.unwrap();
    let retry = bae_sync_status::PendingAction::RetryDownload { release_id: release_id.clone() };
    pending_list_until(&mut values, |items| items.iter().any(|item| item.wait.action == retry))
        .await;

    download_queue_until(manager, |queue| {
        queue.rows.iter().any(|row| {
            row.op.release_id == release_id
                && matches!(row.op.state, bae_transfers::DownloadPhase::Failed { .. })
        })
    })
    .await;
    home.set_faults(coven::Faults::none()).await;
    manager.enqueue_pins(vec![release_id.clone()]).await.unwrap();
    wait_for_kept_release(manager, &release_id).await;
    pending_list_until(&mut values, |items| !items.iter().any(|item| item.wait.action == retry))
        .await;
    assert!(
        views(test.library.manager())
            .find_release_detail(&release_id)
            .await
            .unwrap()
            .expect("the library holds the release")
            .summary
            .storage_state
            == bae_views::album_detail::ReleaseStorageState::InStorage { pinned: true }
    );
}

/// Each value the inspector reads of a pinning release is one the library
/// held: the release still in the download queue, or out of it with every
/// file kept, never out of it with a file not yet kept. The download worker
/// keeps the files and then takes the release out of the queue, one write
/// after the other, so each pin passes through both.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_inspector_never_reads_a_pin_out_of_the_queue_before_its_files_are_kept() {
    use bae_views::album_detail::FilePin;
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    connect_test_cloud(&test).await;
    let cloud =
        uploaded_release(manager, &temp_dir.path().join("cloud"), "Album Title B", false).await;
    let kept = |value: &bae_views::DbStorageInspector| {
        value.files.iter().all(|file| file.pin == Some(FilePin::Kept))
    };
    let mut rows = bae_views::LibraryViews::new(manager.store.clone())
        .subscribe_storage_inspector(Some(cloud.clone()));

    // The release as it stands before any pin: unpinned, out of the queue.
    rows_until(&mut rows, |value| value.files.iter().all(|file| file.pin.is_none())).await;
    for round in 0..20 {
        manager.enqueue_pins(vec![cloud.clone()]).await.unwrap();
        // The read sends only what changes, so every value after the pin is
        // queued holds the rule; a download can finish before a value shows
        // the release in the queue, so none has to.
        tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
            loop {
                let event = rows.next().await.expect("the inspector's read runs");
                let value = event.result.unwrap().expect("the library holds the release");
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
        rows_until(&mut rows, |value| value.files.iter().all(|file| file.pin.is_none())).await;
    }
}

/// Follow the inspector's read of `release_id` until the inspector it
/// shows is one `accept` takes.
async fn inspector_until(
    manager: &LibraryManager,
    release_id: &str,
    accept: impl Fn(&bae_views::storage_inspector::StorageInspector) -> bool,
) -> bae_views::storage_inspector::StorageInspector {
    let shown = |rows: bae_views::DbStorageInspector| {
        bae_views::storage_inspector::StorageInspector::read(release_id, rows)
    };
    shown(inspector_rows_until(manager, release_id, |rows| accept(&shown(rows.clone()))).await)
}

/// A release's file says whether this device keeps it whole, and while the
/// release waits in the download queue the inspector shows each file's pin
/// and counts them; once the download has kept every file it leaves the
/// queue, and the inspector shows no pin but each file kept.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_inspector_shows_each_files_pin_while_the_release_pins() {
    use bae_views::album_detail::FilePin;
    use bae_views::storage_inspector::StorageInspectorTransfer;
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    connect_test_cloud(&test).await;
    let cloud =
        uploaded_release(manager, &temp_dir.path().join("cloud"), "Album Title B", false).await;
    inspector_rows_until(manager, &cloud, |rows| rows.files.iter().all(|file| file.pin.is_none()))
        .await;

    manager.set_downloads_paused(true).await.unwrap();
    manager.enqueue_pins(vec![cloud.clone()]).await.unwrap();
    let pinning = inspector_until(manager, &cloud, |value| !value.transfers.is_empty()).await;
    assert!(pinning.files.iter().all(|file| file.pin == Some(FilePin::Waiting)));
    let [StorageInspectorTransfer::Download { summary: Some(summary), .. }] =
        pinning.transfers.as_slice()
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
    let pinned = inspector_until(manager, &cloud, |value| value.transfers.is_empty()).await;
    assert!(pinned.files.iter().all(|file| file.pin.is_none()));
    inspector_rows_until(manager, &cloud, |rows| {
        rows.files.iter().all(|file| file.pin == Some(FilePin::Kept))
    })
    .await;
}

/// A release is pinning while the download queue holds it waiting or
/// downloading, in the write that changes the queue: a failure, a cancel, a
/// finish each return it to its resting place, and a retry, a refreshed
/// listing row or the library opening keep it pinning.
#[tokio::test]
async fn a_release_sorts_as_pinning_while_the_download_queue_holds_it() {
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    connect_test_cloud(&test).await;
    let dir = |name: &str| temp_dir.path().join(name);
    let first = uploaded_release(manager, &dir("first"), "Album Title A", false).await;
    let second = uploaded_release(manager, &dir("second"), "Album Title B", false).await;
    let ascending =
        || by_storage(manager, bae_views::StorageFilter::All, bae_store::SortDirection::Ascending);
    let resting = vec![first.clone(), second.clone()];
    let second_pinning = vec![second.clone(), first.clone()];
    // A failed download sorts first, as a pinning one does, ahead of the rest.
    let second_failed = vec![second.clone(), first.clone()];
    assert_eq!(ascending().await, resting);

    manager.set_downloads_paused(true).await.unwrap();
    manager.enqueue_pins(vec![second.clone()]).await.unwrap();
    assert_eq!(ascending().await, second_pinning, "queued");

    assert!(activate_download(manager, &second).await.unwrap());
    assert_eq!(ascending().await, second_pinning, "downloading");

    assert!(fail_download(manager, &second, "the network connection was lost".into())
        .await
        .unwrap());
    assert_eq!(ascending().await, second_failed, "failed");

    // Starting the entry resumed the queue; paused again, the worker leaves
    // the entry to this test.
    manager.set_downloads_paused(true).await.unwrap();
    manager.retry_downloads().await.unwrap();
    assert_eq!(ascending().await, second_pinning, "retried");

    // Rewriting the release's listing row, as renaming its album does, reads
    // the queue again.
    manager
        .records
        .save_release_metadata_for_test(&second, |rows| {
            rows.album.title = "Album Title B Renamed".to_string()
        })
        .await
        .unwrap();
    assert_eq!(ascending().await, second_pinning, "refreshed");

    manager.cancel_download(&second).await.unwrap();
    assert_eq!(ascending().await, resting, "cancelled");

    manager.set_downloads_paused(true).await.unwrap();
    manager.enqueue_pins(vec![second.clone()]).await.unwrap();
    assert!(activate_download(manager, &second).await.unwrap());
    assert!(finish_download(manager, &second).await.unwrap());
    assert_eq!(ascending().await, resting, "finished");

    manager.set_downloads_paused(true).await.unwrap();
    manager.enqueue_pins(vec![second.clone()]).await.unwrap();
    clear_transfers_as_opening(manager).await.unwrap();
    assert_eq!(ascending().await, second_pinning, "kept as the library opens");
}

/// The queues the Storage Manager's list value carries, as one read of it
/// finds them.
async fn listed_queues(manager: &LibraryManager) -> bae_views::StorageQueues {
    let mut browse = bae_views::LibraryViews::new(manager.store.clone()).subscribe_storage_browse(
        bae_views::StorageBrowseRequest {
            sort: bae_views::StorageSortCriterion {
                field: bae_views::StorageSortField::Storage,
                direction: bae_store::SortDirection::Ascending,
            },
            filter: bae_views::StorageFilter::All,
            search: String::new(),
            windows: Default::default(),
            selected: Default::default(),
        },
    );
    browse.next().await.expect("the browse query answers").result.unwrap().queues
}

/// The list's value carries each transfer queue while it holds work waiting
/// or under way, with whether it is paused and the release a serial queue
/// runs; a queue holding only a failed entry holds none.
#[tokio::test]
async fn the_storage_list_carries_the_queues_holding_work() {
    use bae_views::{ReleaseQueueRun, StorageQueues, UploadQueueRun};
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    connect_test_cloud(&test).await;
    let pinning =
        uploaded_release(manager, &temp_dir.path().join("pin"), "Album Title", false).await;
    let idle = StorageQueues { uploads: None, pins: None, outputs: None };
    assert_eq!(listed_queues(manager).await, idle);

    manager.set_sync_paused(true).await.unwrap();
    let uploading = insert_release_with_queued_uploads(
        manager,
        &temp_dir.path().join("upload"),
        "Album Title B",
        &[("track.flac", b"uploading")],
    )
    .await;
    manager.set_downloads_paused(true).await.unwrap();
    manager.enqueue_pins(vec![pinning.clone()]).await.unwrap();
    let uploads = Some(UploadQueueRun { paused: true });
    assert_eq!(
        listed_queues(manager).await,
        StorageQueues {
            uploads,
            pins: Some(ReleaseQueueRun { paused: true, active: None }),
            outputs: None,
        }
    );

    assert!(activate_download(manager, &pinning).await.unwrap());
    // Paused, the export stays queued: running, the worker would take it.
    manager.set_outputs_paused(true).await.unwrap();
    manager.enqueue_export(&uploading.id, std::path::PathBuf::from("/exports")).await.unwrap();
    let outputs = Some(ReleaseQueueRun { paused: true, active: None });
    assert_eq!(
        listed_queues(manager).await,
        StorageQueues {
            uploads,
            pins: Some(ReleaseQueueRun { paused: false, active: Some(pinning.clone()) }),
            outputs: outputs.clone(),
        }
    );

    assert!(fail_download(manager, &pinning, "the network connection was lost".into())
        .await
        .unwrap());
    assert_eq!(
        listed_queues(manager).await,
        StorageQueues { uploads, pins: None, outputs },
        "a failed pin is no work"
    );
}

/// Start the download queue's entry for `release_id`, as its worker does.
async fn activate_download(
    manager: &LibraryManager,
    release_id: &str,
) -> Result<bool, bae_store::DbError> {
    let release_id = release_id.to_string();
    manager
        .store
        .call(move |sql| bae_transfers::tables::worker_moves::activate_on::<()>(sql, &release_id))
        .await
}

/// Finish the download queue's active entry for `release_id`.
async fn finish_download(
    manager: &LibraryManager,
    release_id: &str,
) -> Result<bool, bae_store::DbError> {
    let release_id = release_id.to_string();
    manager
        .store
        .call(move |sql| bae_transfers::tables::worker_moves::finish_on::<()>(sql, &release_id))
        .await
}

/// Fail the download queue's entry for `release_id` with `error`.
async fn fail_download(
    manager: &LibraryManager,
    release_id: &str,
    error: String,
) -> Result<bool, bae_store::DbError> {
    let release_id = release_id.to_string();
    manager
        .store
        .call(move |sql| {
            bae_transfers::tables::worker_moves::fail_on::<()>(sql, &release_id, &error)
        })
        .await
}

/// Empty the transfer queues as opening the library does.
async fn clear_transfers_as_opening(manager: &LibraryManager) -> Result<(), bae_store::DbError> {
    manager.store.call(bae_transfers::tables::clear_transfers_on).await
}
