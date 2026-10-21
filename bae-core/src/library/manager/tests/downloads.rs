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
/// in-memory state (enqueue, dedup, snapshot counts, cancel) is observable
/// deterministically without the download path racing the assertions.
#[tokio::test]
async fn download_queue_enqueue_dedups_and_cancels_while_paused() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let release_id = insert_pinnable_release(&manager).await;

    // Park the worker up front so nothing drains while we inspect state.
    manager.set_downloads_paused(true);

    manager.enqueue_pins(vec![release_id.clone()]).await;
    let snap = manager.download_queue();
    assert_eq!(snap.total().queued, 1);
    assert_eq!(snap.ops.len(), 1);
    assert_eq!(snap.ops[0].state, crate::library::DownloadState::Queued);
    assert!(snap.paused);

    // Re-enqueuing the same release is a no-op: still one entry.
    manager.enqueue_pins(vec![release_id.clone()]).await;
    assert_eq!(manager.download_queue().ops.len(), 1);

    // Cancel drops the entry.
    manager.cancel_download(&release_id);
    let snap = manager.download_queue();
    assert!(snap.ops.is_empty());
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

    manager.set_downloads_paused(true);
    manager.enqueue_pins(vec![release_id.clone()]).await;
    assert!(manager.download_queue().ops.is_empty());
}

/// A Local release has nothing to pin — it is already fully on disk —
/// so `enqueue_pins` skips it rather than queueing a download that would fail. The
/// album grid's bulk pin reaches this path with a mixed local/remote selection.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn download_queue_skips_local_release() {
    let (manager, temp_dir) = setup_test_manager().await;
    manager.set_downloads_paused(true);

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

    manager.enqueue_pins(vec![release.id.clone()]).await;
    assert!(
        manager.download_queue().ops.is_empty(),
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
    manager.enqueue_pins(vec![release_id.clone()]).await;

    // Let the worker pick it up, fail, and mark it Failed. Poll the snapshot
    // rather than sleeping a fixed interval.
    let failed = wait_for(|| {
        matches!(
            manager.download_queue().ops.first().map(|op| &op.state),
            Some(crate::library::DownloadState::Failed { .. })
        )
    })
    .await;
    assert!(
        failed,
        "the pin should land Failed while storage is offline"
    );
    assert_eq!(manager.download_queue().total().failed, 1);

    // Retry flips it back to Queued; while storage is offline it will fail again,
    // but the immediate post-retry state is Queued (or already re-failed).
    manager.retry_downloads();
    let snap = manager.download_queue();
    assert!(
        snap.ops.first().is_some_and(|op| matches!(
            op.state,
            crate::library::DownloadState::Queued
                | crate::library::DownloadState::Active { .. }
                | crate::library::DownloadState::Failed { .. }
        )),
        "after retry the release is still tracked"
    );

    // Cancelling clears it regardless of the in-flight retry.
    manager.cancel_download(&release_id);
    let cleared = wait_for(|| manager.download_queue().ops.is_empty()).await;
    assert!(cleared, "cancel removes the entry");
}

#[cfg(feature = "test-utils")]
#[tokio::test]
async fn download_queue_values_report_each_driven_file_progress() {
    use crate::storage::transfer::TransferProgress;

    let (manager, _temp_dir) = setup_test_manager().await;
    let release_id = "release-progress".to_string();
    let mut values = manager.subscribe_download_queue();
    values.borrow_and_update();
    assert!(manager.downloads.enqueue_all([crate::library::DownloadOp {
        release_id: release_id.clone(),
        created_at: 0,
        payload: (),
        state: crate::library::DownloadState::Queued,
    }]));
    let pending = tokio::spawn(std::future::pending::<()>());
    assert!(manager.downloads.activate(
        &release_id,
        pending.abort_handle(),
        crate::library::DownloadTransferProgress::new(&release_id, 0, 7).unwrap(),
    ));

    let active_progress = |snapshot: &crate::library::DownloadQueueContents| {
        let op = snapshot
            .ops
            .first()
            .expect("the active download remains queued");
        let crate::library::DownloadState::Active { progress } = &op.state else {
            panic!("the download is active")
        };
        progress.clone()
    };
    values.changed().await.expect("initial active value");
    assert_eq!(active_progress(&values.borrow_and_update()).bytes_done, 0);

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
    for bytes_done in [3, 7] {
        progress_tx
            .send(TransferProgress::Progress {
                progress: crate::library::DownloadTransferProgress::new(&release_id, bytes_done, 7)
                    .unwrap(),
            })
            .expect("the transfer driver is listening");
        values.changed().await.expect("file progress value");
        let progress = active_progress(&values.borrow_and_update());
        assert_eq!(progress.bytes_done, bytes_done);
        assert_eq!(progress.bytes_total, 7);
        assert_eq!(progress.fraction, bytes_done as f64 / 7.0);
    }
    progress_tx
        .send(TransferProgress::Complete {
            release_id: release_id.clone(),
        })
        .expect("the transfer driver is listening");
    driver.await.unwrap().unwrap();
    pending.abort();
}
