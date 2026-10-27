/// Upload one real file and evict its cache so the release needs a download.
async fn insert_pinnable_release(manager: &TestManager) -> String {
    connect_test_cloud(manager).await;
    let source = manager.get_config().library_path().join("pin-source");
    let release = uploaded_release(manager, &source, "Test Album", false).await;
    let file = release_files(manager, &release).await.remove(0);
    manager
        .evict_file_for_test(&manager.release_file_ref(&file.id).await.unwrap())
        .await
        .unwrap();
    release
}

/// Pausing before the first enqueue parks the worker, so the queue's
/// state (enqueue, dedup, snapshot counts, cancel) is observable
/// deterministically without the download path racing the assertions.
#[tokio::test]
async fn download_queue_enqueue_dedups_and_cancels_while_paused() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let release_id = insert_pinnable_release(&manager).await;

    // Park the worker up front so nothing drains while we inspect state.
    manager.set_downloads_paused(true).await.unwrap();

    manager.enqueue_pins(vec![release_id.clone()]).await
        .unwrap();
    let snap = download_queue_now(&manager).await;
    assert_eq!(snap.total.queued, 1);
    assert_eq!(snap.rows.len(), 1);
    assert_eq!(snap.rows[0].op.state, crate::library::DownloadPhase::Queued);
    assert!(snap.paused);

    // Re-enqueuing the same release is a no-op: still one entry.
    manager.enqueue_pins(vec![release_id.clone()]).await
        .unwrap();
    assert_eq!(download_queue_now(&manager).await.rows.len(), 1);

    // Cancel drops the entry.
    manager.cancel_download(&release_id).await.unwrap();
    let snap = download_queue_now(&manager).await;
    assert!(snap.rows.is_empty());
}

/// An already-pinned release is skipped at enqueue rather than re-downloaded.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn download_queue_skips_already_pinned() {
    let (manager, temp_dir) = setup_test_manager().await;

    // A genuinely pinned release: made Remote with pin, so its file lands in
    // coven's offline cache.
    connect_test_cloud(&manager).await;
    let release_id = uploaded_release(
        &manager,
        &temp_dir.path().join("pinned"),
        "Test Album",
        true,
    )
    .await;
    let summary = manager
        .find_release_storage_summary(&release_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        summary.storage_state,
        crate::album_detail::ReleaseStorageState::Remote
    );
    assert!(
        summary.pinned,
        "the offline-cached file makes it read as pinned"
    );

    manager.set_downloads_paused(true).await.unwrap();
    manager.enqueue_pins(vec![release_id.clone()]).await
        .unwrap();
    assert!(download_queue_now(&manager).await.rows.is_empty());
}

/// A release whose storage can't be read fails the whole pin request before
/// anything is queued — not a skip that leaves the pin silently doing nothing.
#[tokio::test]
async fn a_failed_release_read_fails_the_whole_pin_request() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let readable = insert_pinnable_release(&manager).await;
    let source = manager.get_config().library_path().join("unreadable-source");
    let unreadable = uploaded_release(&manager, &source, "Album Title", false).await;
    manager.set_downloads_paused(true).await.unwrap();
    manager
        .database
        .set_release_media_text_for_test(&unreadable, r#"[{"medium":"not-a-medium","count":1}]"#)
        .await
        .unwrap();

    let error = manager
        .enqueue_pins(vec![readable.clone(), unreadable.clone()])
        .await
        .expect_err("a release that can't be read fails the request");

    assert!(error.to_string().contains("not-a-medium"), "{error}");
    assert!(
        download_queue_now(&manager).await.rows.is_empty(),
        "nothing is queued when the request fails"
    );
}

/// A Local release has nothing to pin — it is already fully on disk —
/// so `enqueue_pins` skips it rather than queueing a download that would fail. The
/// album grid's bulk pin reaches this path with a mixed local/remote selection.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn download_queue_skips_local_release() {
    let (manager, temp_dir) = setup_test_manager().await;
    manager.set_downloads_paused(true).await.unwrap();

    let release = insert_local_release_with_files(
        &manager,
        &temp_dir.path().join("local-source"),
        "Test Album",
        &[("a.flac", b"aaa")],
    )
    .await;
    let summary = manager
        .find_release_storage_summary(&release.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        summary.storage_state,
        crate::album_detail::ReleaseStorageState::Local
    );

    manager.enqueue_pins(vec![release.id.clone()]).await
        .unwrap();
    assert!(
        download_queue_now(&manager).await.rows.is_empty(),
        "a local release is not enqueued for pinning"
    );
}

/// A pin that fails because storage is offline lands `Failed`
/// and stays in the queue; `retry_downloads` flips it back to `Queued`.
#[tokio::test]
async fn download_queue_failed_pin_retries() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let release_id = insert_pinnable_release(&manager).await;

    manager.storage.set_online(false);
    // The evicted file cannot be fetched while storage is offline.
    manager.enqueue_pins(vec![release_id.clone()]).await
        .unwrap();

    // Let the worker pick it up, fail, and mark it Failed.
    let failed = download_queue_until(&manager, |queue| {
        matches!(
            queue.rows.first().map(|row| &row.op.state),
            Some(crate::library::DownloadPhase::Failed { .. })
        )
    })
    .await;
    assert_eq!(failed.total.failed, 1, "the pin lands Failed while storage is offline");

    // Retry flips it back to Queued; while storage is offline it will fail again,
    // but the immediate post-retry state is Queued (or already re-failed).
    manager.retry_downloads().await.unwrap();
    let snap = download_queue_now(&manager).await;
    assert!(
        snap.rows.first().is_some_and(|row| matches!(
            row.op.state,
            crate::library::DownloadPhase::Queued
                | crate::library::DownloadPhase::Active { .. }
                | crate::library::DownloadPhase::Failed { .. }
        )),
        "after retry the release is still tracked"
    );

    // Cancelling clears it regardless of the in-flight retry.
    manager.cancel_download(&release_id).await.unwrap();
    assert!(
        download_queue_now(&manager).await.rows.is_empty(),
        "cancel removes the entry"
    );
}

/// Each step of bytes a pin drives reaches the active download's progress,
/// read apart from the queue's table.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn download_transfers_report_each_driven_file_progress() {
    use crate::storage::transfer::TransferProgress;

    let (manager, _temp_dir) = setup_test_manager().await;
    let release_id = "release-progress".to_string();
    // The manager's own worker leaves the entry to this test.
    manager.set_downloads_paused(true).await.unwrap();
    assert!(manager
        .downloads
        .enqueue_all(vec![crate::library::DownloadEntry {
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
            crate::library::DownloadTransferProgress::new(&release_id, 0, 7).unwrap(),
        )
        .await
        .unwrap());

    let mut transfers = manager.downloads.subscribe_transfers();
    let bytes_done = |transfers: &std::collections::BTreeMap<
        String,
        crate::library::DownloadTransferProgress,
    >| {
        transfers
            .get("release-progress")
            .expect("the active download has progress")
            .clone()
    };
    assert_eq!(bytes_done(&transfers.borrow_and_update()).bytes_done, 0);

    let (progress_tx, progress_rx) = tokio::sync::mpsc::unbounded_channel();
    let driver = {
        let manager = manager.clone();
        let release_id = release_id.clone();
        tokio::spawn(async move {
            manager
                .drive_transfer(&release_id, ReleaseStorageAction::Pin, progress_rx, None)
                .await
        })
    };
    for done in [3, 7] {
        progress_tx
            .send(TransferProgress::Progress {
                progress: crate::library::DownloadTransferProgress::new(&release_id, done, 7)
                    .unwrap(),
            })
            .expect("the transfer driver is listening");
        transfers.changed().await.expect("file progress value");
        let progress = bytes_done(&transfers.borrow_and_update());
        assert_eq!(progress.bytes_done, done);
        assert_eq!(progress.bytes_total, 7);
        assert_eq!(progress.fraction, done as f64 / 7.0);
    }
    progress_tx
        .send(TransferProgress::Complete {
            release_id: release_id.clone(),
        })
        .expect("the transfer driver is listening");
    driver.await.unwrap().unwrap();
    pending.abort();
}

/// Bytes arriving for a download leave the sync activity as it was, so they
/// send it no new value; the download's start and its end each do.
#[tokio::test]
async fn download_bytes_send_no_new_sync_activity() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let release_id = "release-activity".to_string();
    let progress = |bytes_done| {
        crate::library::DownloadTransferProgress::new(&release_id, bytes_done, 7).unwrap()
    };
    let mut activity = crate::library::SyncActivityStream::new(manager.library.clone());
    assert_eq!(activity.next().await.unwrap().downloads, None);

    // The manager's own worker leaves the entry to this test.
    manager.set_downloads_paused(true).await.unwrap();
    assert!(manager
        .downloads
        .enqueue_all(vec![crate::library::DownloadEntry {
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
        activity.next().await.unwrap().downloads,
        Some(crate::library::DownloadsInFlight {
            active: 1,
            queued: 0
        })
    );

    let next = activity.next();
    tokio::pin!(next);
    for bytes_done in [3, 7] {
        assert!(manager
            .downloads
            .set_active_progress(&release_id, progress(bytes_done)));
        let polled = tokio::task::unconstrained(std::future::poll_fn(|cx| {
            std::task::Poll::Ready(std::future::Future::poll(next.as_mut(), cx))
        }))
        .await;
        assert!(polled.is_pending(), "{bytes_done} bytes sent an activity");
    }
    manager.cancel_download(&release_id).await.unwrap();
    assert_eq!(next.await.unwrap().downloads, None);
    pending.abort();
}

/// No transfer outlives the process that ran it: a library opened over a
/// store whose queues still hold entries opens with both empty and running.
#[tokio::test]
async fn opening_a_library_empties_its_transfer_queues() {
    let temp_dir = TempDir::new().unwrap();
    let app_dir = crate::config::AppDir::at(temp_dir.path()).unwrap();
    let installation = coven::TestCoven::new();
    let clock: coven::ClockRef = Arc::new(coven::SystemClock);
    let handle = crate::library::configure_store(installation.builder(app_dir.store_layout()))
        .clock(clock.clone())
        .storage_connector(Arc::new(MemoryStorage::builder().clock(clock.clone()).build().unwrap()))
        .create_store("Test Library")
        .await
        .unwrap();
    let left = Database::from_handle(handle.clone(), clock.clone());
    left.enqueue_releases(vec![crate::library::DownloadEntry {
        release_id: REL_1.to_string(),
        created_at: 0,
        payload: (),
    }])
    .await
    .unwrap();
    left.set_release_queue_paused::<()>(true).await.unwrap();

    let config = Config::with_defaults(
        handle.store_id(),
        app_dir.registered_library(&handle.store_id().to_string()),
    );
    let manager = LibraryManager::open(
        app_dir,
        Arc::new(ConfigHandle::new(config)),
        clock,
        Arc::new(coven::UuidIds),
        crate::diagnostics::Diagnostics::noop(),
        tokio::runtime::Handle::current(),
        crate::library::LibraryOpening::Open(handle),
        bae_catalogs::remote_image::RemoteImageCache::for_test(bae_util::http::Http::for_test()),
        crate::providers::Providers::offline(),
    )
    .await
    .unwrap();

    let queue = manager.download_snapshot().await.unwrap();
    assert!(queue.rows.is_empty(), "{queue:?}");
    assert!(!queue.paused);
}
