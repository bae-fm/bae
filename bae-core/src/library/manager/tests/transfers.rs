/// The opener the queued-upload tests share: a manager on a connected test
/// cloud holding one "Test Album" release whose single 1000-byte `a.flac` is
/// queued for upload. Returns the release and the id of that queued file;
/// `folder` names the source directory under the temp dir.
#[cfg(feature = "test-utils")]
async fn queued_upload_fixture(folder: &str) -> (TestManager, TempDir, DbRelease, String) {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    let release = insert_release_with_queued_uploads(
        &manager,
        &temp_dir.path().join(folder),
        "Test Album",
        &[("a.flac", &vec![b'a'; 1000])],
    )
    .await;
    let file_id = release_files(&manager, &release.id).await[0].id.clone();
    (manager, temp_dir, release, file_id)
}

/// A Remote track whose bytes must come from the cloud, read with no provider
/// connected, reports `SyncDisconnected` — the reconnect-sync state — not a
/// generic diagnostic. coven raises `NoCloudHome` for the cloud miss; the
/// classifier keys it.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn remote_read_with_sync_disconnected_reports_sync_disconnected() {
    use crate::ui::PlaybackErrorReason;
    let (manager, dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    let release_id = make_remote_release_with_files(
        &manager,
        dir.path(),
        "Album Title",
        &[("track.flac", b"track-bytes")],
        false,
    )
    .await;
    manager.disconnect_cloud_provider().await.unwrap();

    let files = release_files(&manager, &release_id).await;
    let file = files.first().expect("the release has a file");
    let reason = playback_error_reason_for_file(&manager, file).await;
    assert!(
        matches!(reason, PlaybackErrorReason::SyncDisconnected),
        "got {reason:?}"
    );
}

/// An upload whose original vanished cannot finish by waiting. Playback reports
/// the source failure, and the pending list offers restoring the file.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn pending_upload_with_missing_source_reports_the_missing_original() {
    use crate::ui::PlaybackErrorReason;
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    let release = insert_partially_uploaded_make_remote_release(&manager, temp_dir.path()).await;

    let files = release_files(&manager, &release.id).await;
    let file = files
        .iter()
        .find(|f| f.original_filename == "b.flac")
        .expect("the un-uploaded file whose source was removed");
    let reason = playback_error_reason_for_file(&manager, file).await;
    assert!(
        matches!(reason, PlaybackErrorReason::Diagnostic { .. }),
        "got {reason:?}"
    );
    assert!(manager.pending().await.unwrap().iter().any(|record|
        crate::library::pending_action(record) == crate::library::PendingAction::RestoreFile));
}

/// A shared local release with no path on this device waits for its importer
/// to attach the files.
#[tokio::test]
async fn an_unattached_file_on_another_device_waits_for_upload() {
    use crate::ui::PlaybackErrorReason;
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    manager.database.insert_album(&album).await.unwrap();
    let release = insert_local_release_without_local_files(&manager, &album.id).await;

    let files = release_files(&manager, &release.id).await;
    let file = files.first().expect("the release has a file");
    let reason = playback_error_reason_for_file(&manager, file).await;
    assert!(
        matches!(reason, PlaybackErrorReason::UploadPending),
        "got {reason:?}"
    );
}

/// The read layer surfaces a local release even when no local copy
/// resolves on this device — there is no availability filter to hide one.
/// Every release is shared, including one whose files have not been uploaded.
#[tokio::test]
async fn surfaces_local_release_with_no_local_files() {
    let (manager, _temp_dir) = setup_test_manager().await;

    let album = create_test_album();
    manager.database.insert_album(&album).await.unwrap();
    let release = insert_local_release_without_local_files(&manager, &album.id).await;

    // Grid and count include the album.
    let page = manager.get_album_page(&[], 0, 10).await.unwrap();
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].release_ids, vec![release.id.clone()]);
    assert_eq!(manager.get_album_count().await.unwrap(), 1);

    // Album detail carries the release.
    let detail = manager
        .find_album_detail(&album.id)
        .await
        .unwrap()
        .expect("album surfaces");
    let detail_ids: Vec<_> = detail
        .releases
        .iter()
        .map(|r| r.summary.id.clone())
        .collect();
    assert_eq!(detail_ids, vec![release.id.clone()]);

    // The release-level resolver returns it.
    assert!(manager
        .find_release_detail(&release.id)
        .await
        .unwrap()
        .is_some());
}


/// The Uploading filter is coven's upload queue, not a bae column: only the
/// release whose make-Remote is still enqueued appears under it.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn storage_page_uploading_filter_matches_the_upload_queue() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;

    let album_quiet = create_test_album();
    let release_quiet = create_test_release(&album_quiet.id);
    manager.database.insert_album(&album_quiet).await.unwrap();
    insert_release(&manager, &release_quiet).await;

    let uploading = insert_release_with_queued_uploads(
        &manager,
        &temp_dir.path().join("uploading"),
        "Uploading Album",
        &[("a.flac", b"a-bytes")],
    )
    .await;

    let sort = crate::db::StorageSortCriterion {
        field: crate::db::StorageSortField::AlbumTitle,
        direction: crate::db::SortDirection::Ascending,
    };
    let page = manager
        .get_storage_page(&sort, crate::db::StorageFilter::Uploading, 0, 10)
        .await
        .unwrap();
    assert_eq!(page.total_count, 1);
    assert_eq!(page.rows.len(), 1);
    assert_eq!(page.rows[0].release.id, uploading.id);
    assert_eq!(
        manager
            .get_storage_count(crate::db::StorageFilter::Uploading)
            .await
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn cancel_release_transition_fires_a_registered_transfer_token() {
    let (manager, _temp_dir) = setup_test_manager().await;

    // A registered make-Local token is fired by the unified cancel.
    let token = crate::library::CancellationToken::new();
    let _registration = manager.transitions.track(
        REL_X,
        ReleaseStorageAction::MakeLocal,
        Some(token.clone()),
    );
    manager.cancel_release_transition(REL_X).await.unwrap();
    assert!(token.is_cancelled(), "transfer token fired");

    // Nothing in progress for an unknown release → no-op, no error.
    manager.cancel_release_transition(REL_NONE).await.unwrap();
}

#[tokio::test]
async fn cancelling_a_completed_upload_preserves_the_attachment() {
    let (manager, temp, release, _) = queued_upload_fixture("completed").await;
    manager.drain_uploads_expecting_work().await.unwrap();
    manager.cancel_release_transition(&release.id).await.unwrap();
    assert!(release_files(&manager, &release.id).await.iter().all(|file| file.attached));
    assert_eq!(std::fs::read(temp.path().join("completed/a.flac")).unwrap().len(), 1000);
}

// Needs the test-utils mock cloud home with its sync loop: make-Local is only
// offered a release whose make-Remote has been accepted, so the transition has
// to run all the way through a cycle and retire its intent first.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn unmanage_cancelled_before_copy_leaves_release_remote() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud_with_sync_loop(&manager).await;
    // Really Remote, through the real transition: make-Local resolves the
    // release's current locality from coven, which a fabricated `remote` column
    // with no cloud objects behind it cannot answer.
    let release_id = make_remote_release_under_sync_loop(
        &manager,
        &temp_dir.path().join("r1"),
        "Album One",
        false,
    )
    .await;

    // A token cancelled before the materialize loop runs: coven aborts at the
    // first check, before reading or writing a file, and reports cancellation.
    let token = crate::library::CancellationToken::new();
    token.cancel();
    let dest = temp_dir.path().join("out");
    manager
        .coven_make_local(&release_id, dest.to_str().unwrap(), &token)
        .await
        .expect_err("a cancelled make-Local must not report completion");

    assert!(release_files(&manager, &release_id).await.iter().all(|file| file.attached),
        "cancellation preserves every attachment");
    assert!(!dest.exists());
}

/// A missing original cannot become a playable local release; cancellation
/// fails before changing any attachment or local path.
#[tokio::test]
async fn a_missing_upload_source_is_pending_and_cannot_be_cancelled() {
    let (manager, temp, release, file_id) = queued_upload_fixture("queued").await;
    let snapshot = manager.outbox_snapshot().await.unwrap();
    assert_eq!(snapshot.total.queued, 1);
    assert_eq!(snapshot.upload_groups[0].files[0].upload.file.plaintext_size(), 1000);
    assert_eq!(snapshot.upload_groups[0].files[0].label, crate::library::UploadFileLabel::Filename("a.flac".into()));
    std::fs::remove_file(temp.path().join("queued/a.flac")).unwrap();
    assert_eq!(manager.drain_uploads_expecting_work().await.unwrap(), 0);
    assert!(!manager.pending().await.unwrap().is_empty());
    manager.cancel_release_upload(&release.id).await.expect_err("the missing original prevents restoring local playback");
    assert!(manager.database.find_file_by_id(&file_id).await.unwrap().unwrap().attached);
    assert_eq!(manager.outbox_snapshot().await.unwrap().total.queued, 1);
}

#[tokio::test]
async fn cancelling_all_release_uploads_restores_their_originals() {
    let (manager, temp, first, _) = queued_upload_fixture("first").await;
    let second = insert_release_with_queued_uploads(&manager, &temp.path().join("second"), "Album", &[("b.flac", b"audio")]).await;
    manager.cancel_all_release_uploads().await.unwrap();
    assert_eq!(manager.outbox_snapshot().await.unwrap().total.queued, 2);
    for release in [first, second] {
        assert!(release_files(&manager, &release.id).await.iter().all(|file| !file.attached));
    }
}

#[tokio::test]
async fn deleting_a_release_keeps_its_committed_upload_visible_until_it_finishes() {
    let (manager, _temp, release, _) = queued_upload_fixture("queued").await;
    manager.delete_release(&release.id).await.unwrap();
    let snapshot = manager.outbox_snapshot().await.unwrap();
    assert_eq!(snapshot.total.queued, 1);
    assert_eq!(snapshot.other_files.len(), 1);
    assert!(snapshot.upload_groups.is_empty());
    assert_eq!(manager.drain_uploads_expecting_work().await.unwrap(), 1);
    let snapshot = manager.outbox_snapshot().await.unwrap();
    assert_eq!(snapshot.total.queued, 0);
    assert!(snapshot.other_files.is_empty());
}

/// A cover upload is carried by the `covers` row whose primary key is the
/// release id, while its immutable cloud blob has a distinct id. The queue must
/// identify and size the blob itself rather than mistaking the release id for
/// an audio-file id.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn outbox_snapshot_identifies_the_cover_blob() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    let release = insert_local_release_with_files(
        &manager,
        &temp_dir.path().join("cover-upload"),
        "Album Title",
        &[("track.flac", b"track-bytes")],
    )
    .await;
    let cover_bytes = b"cover-bytes";
    let cover_blob_id = Uuid::new_v4().to_string();
    let cover = DbLibraryImage::cover(
        &release.id,
        &cover_blob_id,
        "local",
        None,
        cover_bytes,
        manager.clock.now().into(),
    );
    manager
        .store_library_image_blob(&cover, cover_bytes)
        .await
        .unwrap();
    manager.coven_make_remote(&release.id, false).await.unwrap();

    let snapshot = manager.outbox_snapshot().await.unwrap();
    let group = snapshot
        .upload_groups
        .iter()
        .find(|group| group.release_id == release.id)
        .expect("the release upload group");
    let cover_upload = group
        .files
        .iter()
        .find(|file| file.label == crate::library::UploadFileLabel::Cover)
        .expect("the cover is a typed upload row");
    assert_eq!(
        cover_upload.upload.file,
        manager.database.file_ref("covers", &release.id).await.unwrap()
    );
    assert_eq!(cover_upload.upload.file.plaintext_size(), cover_bytes.len() as u64);
}

/// A successful make-Remote command must publish the canonical durable queue
/// snapshot before it returns. The foreground transfer and Import status both
/// finish immediately after this call; without this ordering the UI briefly
/// falls back to Local/Imported while the live query catches up.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn make_remote_publishes_its_durable_queue_before_returning() {
    let (manager, temp_dir) = setup_test_manager().await;
    // A caller-driven connection: no sync loop drains the queue behind the test.
    connect_test_cloud(&manager).await;
    let release = insert_local_release_with_files(
        &manager,
        &temp_dir.path().join("publish-queue"),
        "Album Title",
        &[("track.flac", b"track-bytes")],
    )
    .await;
    let second_release = insert_local_release_with_files(
        &manager,
        &temp_dir.path().join("publish-queue-second"),
        "Second Album Title",
        &[("second.flac", b"second-track-bytes")],
    )
    .await;
    let mut values = manager.subscribe_outbox_values();
    assert!(values.borrow_and_update().is_none());

    let outcome = manager
        .make_releases_remote(
            &[release.id.clone(), second_release.id.clone()])
        .await
        .unwrap();
    let MakeReleasesRemoteOutcome::Complete { receipt } = outcome else {
        panic!("both releases must be admitted");
    };
    assert_eq!(
        receipt.release_ids,
        vec![release.id.clone(), second_release.id.clone()]
    );

    let snapshot = values
        .borrow_and_update()
        .clone()
        .expect("make-Remote publishes an outbox value")
        .expect("the durable queue projects");
    assert_eq!(snapshot.revision, receipt.outbox_revision);
    assert!(snapshot
        .upload_groups
        .iter()
        .any(|group| group.release_id == release.id));
    assert!(snapshot
        .upload_groups
        .iter()
        .any(|group| group.release_id == second_release.id));
}

#[cfg(feature = "test-utils")]
#[tokio::test]
async fn an_active_target_does_not_block_sibling_cloud_admissions() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    let first_new_release = insert_local_release_with_files(
        &manager,
        &temp_dir.path().join("first-new-target"),
        "First Album",
        &[("first.flac", b"first-bytes")],
    )
    .await;
    let active_release = insert_local_release_with_files(
        &manager,
        &temp_dir.path().join("active-target"),
        "Second Album",
        &[("second.flac", b"second-bytes")],
    )
    .await;
    let last_new_release = insert_local_release_with_files(
        &manager,
        &temp_dir.path().join("last-new-target"),
        "Third Album",
        &[("third.flac", b"third-bytes")],
    )
    .await;
    let _active_command = manager
        .admit_transfer_values(
            std::slice::from_ref(&active_release.id),
            ReleaseStorageAction::MakeRemote,
        )
        .expect("hold the target in another foreground command");

    let outcome = manager
        .make_releases_remote(
            &[
                first_new_release.id.clone(),
                active_release.id.clone(),
                last_new_release.id.clone(),
            ])
        .await
        .expect("the batch returns every release's admission outcome");
    let MakeReleasesRemoteOutcome::Partial {
        receipt: Some(receipt),
        failure,
    } = outcome
    else {
        panic!("two releases must be admitted around one refusal");
    };
    assert_eq!(
        receipt.release_ids,
        vec![first_new_release.id.clone(), last_new_release.id.clone()]
    );
    assert_eq!(failure.release_ids, vec![active_release.id.clone()]);
    let crate::ui::UiError::Diagnostic { category, detail } = failure.error;
    assert_eq!(category, crate::ui::UiErrorCategory::Internal);
    assert!(detail.contains(&active_release.id));

    for release in [first_new_release, last_new_release] {
        assert!(has_pending_uploads(&manager, &release.id).await);
        assert!(release_files(&manager, &release.id).await.iter().all(|file| file.attached));
    }
}

#[cfg(feature = "test-utils")]
#[tokio::test]
async fn a_move_to_cloud_batch_refuses_duplicate_release_ids_without_admission() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    let release = insert_local_release_with_files(
        &manager,
        &temp_dir.path().join("duplicate-target"),
        "Album Title",
        &[("track.flac", b"track-bytes")],
    )
    .await;

    manager
        .make_releases_remote(&[release.id.clone(), release.id.clone()])
        .await
        .expect_err("a duplicate target refuses the whole batch");

    assert!(!has_pending_uploads(&manager, &release.id).await);
    assert!(release_files(&manager, &release.id).await.iter().all(|file| !file.attached));
}

/// The queue names each queued file by its row's current original filename,
/// and a rename of that file wakes it.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn outbox_subscription_reacts_to_release_display_context_changes() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    manager.start();
    let mut values = manager.subscribe_outbox_values();
    let release = insert_release_with_queued_uploads(
        &manager,
        &temp_dir.path().join("reactive-outbox-context"),
        "Album Title",
        &[("track.flac", b"track-bytes")],
    )
    .await;

    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            values.changed().await.expect("outbox value stream");
            let current = values.borrow_and_update();
            let snapshot = current
                .as_ref()
                .expect("outbox subscription published a value")
                .as_ref()
                .unwrap_or_else(|error| panic!("outbox projection failed: {error}"));
            if snapshot
                .upload_groups
                .iter()
                .any(|group| group.release_id == release.id)
            {
                break;
            }
        }
    })
    .await
    .expect("durable enqueue reaches the outbox subscription");

    let file = release_files(&manager, &release.id)
        .await
        .into_iter()
        .next()
        .expect("release file");
    manager
        .database
        .set_original_filename_for_test(&file.id, "renamed-track.flac")
        .await
        .unwrap();

    let renamed_file = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            values.changed().await.expect("outbox value stream");
            let current = values.borrow_and_update();
            let snapshot = current
                .as_ref()
                .expect("outbox subscription published a value")
                .as_ref()
                .unwrap_or_else(|error| panic!("outbox projection failed: {error}"));
            let label = &snapshot.upload_groups[0].files[0].label;
            if label
                == &crate::library::UploadFileLabel::Filename(
                    "renamed-track.flac".to_string(),
                )
            {
                break label.clone();
            }
        }
    })
    .await
    .expect("original filename changes wake outbox enrichment");
    assert_eq!(
        renamed_file,
        crate::library::UploadFileLabel::Filename("renamed-track.flac".to_string())
    );
}

/// A user retry without a connected cloud must report the refusal to its
/// caller; returning success would leave the failed queue unchanged while the
/// Storage Manager claims the retry ran.
#[tokio::test]
async fn retry_outbox_now_surfaces_a_missing_cloud_connection() {
    let (manager, _temp_dir) = setup_test_manager().await;

    let error = manager
        .retry_outbox_now()
        .await
        .expect_err("retrying without a cloud connection must fail");

    assert!(matches!(error, LibraryError::Sync(_)), "got {error:?}");
}

#[tokio::test]
async fn upload_pause_survives_retry_and_resuming_allows_progress() {
    let (manager, _temp, _release, _) = queued_upload_fixture("paused").await;
    manager.set_sync_paused(true).await.unwrap();
    let before = manager.storage.request_count();
    assert!(matches!(manager.database.retry_uploads_now().await.unwrap(), coven::DrainOutcome::Paused));
    assert_eq!(manager.storage.request_count(), before);
    assert_eq!(manager.outbox_snapshot().await.unwrap().pause_state, crate::library::OutboxPauseState::Paused);
    manager.set_sync_paused(false).await.unwrap();
    manager.retry_outbox_now().await.unwrap();
    assert_eq!(manager.outbox_snapshot().await.unwrap().total.queued, 0);
}

#[tokio::test]
async fn retry_outbox_surfaces_a_provider_failure_and_can_be_retried() {
    let (manager, _temp, _release, _) = queued_upload_fixture("retry").await;
    manager.storage.set_online(false);
    manager.set_sync_paused(false).await.unwrap();
    manager.retry_outbox_now().await.unwrap();
    assert!(!manager.pending().await.unwrap().is_empty());
    assert_eq!(manager.outbox_snapshot().await.unwrap().total.queued, 1);
    manager.storage.set_online(true);
    manager.retry_outbox_now().await.unwrap();
    assert_eq!(manager.outbox_snapshot().await.unwrap().total.queued, 0);
}

/// The concurrency settings reach the open store when they are written, not
/// at the next open: the limit the next upload drain and pin read is the one
/// just set.
#[tokio::test]
async fn transfer_concurrency_settings_apply_to_the_open_store() {
    let (manager, _temp) = setup_test_manager().await;
    let before = manager.transfer_limits();

    manager.set_max_concurrent_uploads(5).await.unwrap();
    manager.set_max_concurrent_downloads(2).await.unwrap();

    let after = manager.transfer_limits();
    assert_eq!(after.uploads.get(), 5);
    assert_eq!(after.downloads.get(), 2);
    assert_ne!((before.uploads, before.downloads), (after.uploads, after.downloads));
    assert_eq!(manager.get_config().prefs.max_concurrent_uploads.get(), 5);
    assert_eq!(manager.get_config().prefs.max_concurrent_downloads.get(), 2);

    assert!(manager.set_max_concurrent_uploads(0).await.is_err());
    assert_eq!(
        manager.transfer_limits().uploads.get(),
        5,
        "a rejected value changes nothing"
    );
}

#[tokio::test]
async fn attachment_commits_before_its_offline_copy_runs_in_downloads() {
    let (manager, temp) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    manager.set_downloads_paused(true);
    let release = insert_local_release_with_files(
        &manager,
        &temp.path().join("pin-after-attachment"),
        "Album",
        &[("audio.flac", b"audio")],
    )
    .await;
    manager.coven_make_remote(&release.id, true).await.unwrap();
    assert!(release_files(&manager, &release.id)
        .await
        .iter()
        .all(|file| file.attached));
    let queue = manager.download_queue();
    assert_eq!(queue.ops.len(), 1);
    assert_eq!(queue.ops[0].release_id, release.id);
    assert!(matches!(
        queue.ops[0].state,
        crate::library::DownloadState::Queued
    ));
    manager.set_downloads_paused(false);
    let mut changes = manager.subscribe_download_queue();
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        changes.wait_for(|queue| queue.ops.is_empty()),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(
        manager
            .find_release_storage_summary(&release.id)
            .await
            .unwrap()
            .unwrap()
            .pinned
    );
}

#[tokio::test]
async fn close_waits_for_a_file_operations_rollback_and_refuses_new_work() {
    let (manager, _home) = setup_test_manager().await;
    let (entered, reached) = tokio::sync::oneshot::channel();
    let (release, released) = tokio::sync::oneshot::channel();
    let database = manager.database.clone();
    let (admitted, admission) = tokio::sync::oneshot::channel();
    let working = manager.clone();
    let operation = tokio::spawn(async move {
        working
            .run_file_operation("rollback test", move |closing| async move {
                admitted.send(()).unwrap();
                closing.cancelled().await;
                entered.send(()).unwrap();
                released.await.unwrap();
                database.store_name().await?;
                Ok(())
            })
            .await
    });
    admission.await.unwrap();
    let closing = manager.clone();
    let mut close = tokio::spawn(async move { closing.close().await });
    reached.await.unwrap();
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(50), &mut close)
            .await
            .is_err(),
        "close must wait for rollback before closing the database"
    );
    assert!(manager
        .run_file_operation("late operation", |_| async { Ok(()) })
        .await
        .is_err());
    release.send(()).unwrap();
    operation.await.unwrap().unwrap();
    close.await.unwrap().unwrap();
}
