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
    let release_id = uploaded_release_with_files(
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
    let release = insert_partially_uploaded_release(&manager, temp_dir.path()).await;

    let files = release_files(&manager, &release.id).await;
    let file = files
        .iter()
        .find(|f| f.original_filename == "b.flac")
        .expect("the un-uploaded file whose source was removed");
    let reason = playback_error_reason_for_file(&manager, file).await;
    assert!(
        matches!(reason, PlaybackErrorReason::OriginalMissing),
        "got {reason:?}"
    );
    assert!(manager.pending().await.unwrap().iter().any(|record|
        crate::library::pending_action(record, None) == crate::library::PendingAction::RestoreFile));
}

/// A shared local release with no path on this device says it isn't uploaded:
/// nobody started an upload, so there is nothing to wait for.
#[tokio::test]
async fn an_unattached_file_on_another_device_is_not_uploaded() {
    use crate::ui::PlaybackErrorReason;
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    manager.database.insert_album(&album).await.unwrap();
    let release = insert_local_release_without_local_files(&manager, &album.id).await;

    let files = release_files(&manager, &release.id).await;
    let file = files.first().expect("the release has a file");
    let reason = playback_error_reason_for_file(&manager, file).await;
    assert!(
        matches!(reason, PlaybackErrorReason::NotUploaded),
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

#[tokio::test]
async fn cancel_release_transition_fires_a_registered_transfer_token() {
    let (manager, _temp_dir) = setup_test_manager().await;

    // A registered audio detachment token is fired by the unified cancel.
    let token = crate::library::CancellationToken::new();
    let _registration = manager.transitions.track(
        REL_X,
        ReleaseStorageAction::DetachAudio,
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
    assert!(release_files(&manager, &release.id).await.iter().all(|file| file.stored));
    assert_eq!(std::fs::read(temp.path().join("completed/a.flac")).unwrap().len(), 1000);
}

// Needs the test-utils mock cloud home with its sync loop: audio detachment is only
// offered a release whose upload has been accepted, so the transition has
// to run all the way through a cycle and retire its intent first.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn unmanage_cancelled_before_copy_leaves_release_remote() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud_with_sync_loop(&manager).await;
    // Really Remote, through the real transition: audio detachment resolves the
    // release's current locality from coven, which a fabricated `remote` column
    // with no cloud objects behind it cannot answer.
    let release_id = upload_release_under_sync_loop(
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
        .detach_release_audio_into(&release_id, dest.to_str().unwrap(), &token)
        .await
        .expect_err("a cancelled audio detachment must not report completion");

    assert!(release_files(&manager, &release_id).await.iter().all(|file| file.stored),
        "cancellation preserves every attachment");
    assert!(!dest.exists());
}

/// A missing original cannot become a playable local release; cancellation
/// fails before changing any attachment or local path.
#[tokio::test]
async fn a_missing_upload_source_is_pending_and_cannot_be_cancelled() {
    let (manager, temp, release, file_id) = queued_upload_fixture("queued").await;
    assert_eq!(manager.count_pending_uploads_for_release(&release.id).await.unwrap(), 1);
    std::fs::remove_file(temp.path().join("queued/a.flac")).unwrap();
    assert_eq!(manager.drain_uploads_expecting_work().await.unwrap(), 0);
    assert!(!manager.pending().await.unwrap().is_empty());
    manager.cancel_release_upload(&release.id).await.expect_err("the missing original prevents restoring local playback");
    assert!(
        manager.database.find_file_by_id(&file_id).await.unwrap().unwrap().file.is_some(),
        "the file stays moving"
    );
    assert_eq!(manager.database.queued_upload_count_for_test().await.unwrap(), 1);
}

#[tokio::test]
async fn cancelling_all_release_uploads_restores_their_originals() {
    let (manager, temp, first, _) = queued_upload_fixture("first").await;
    let second = insert_release_with_queued_uploads(&manager, &temp.path().join("second"), "Album", &[("b.flac", b"audio")]).await;
    manager.cancel_all_release_uploads().await.unwrap();
    // A file no row references any more goes, so its upload leaves the
    // queue.
    assert_eq!(manager.database.queued_upload_count_for_test().await.unwrap(), 0);
    assert!(manager.pending().await.unwrap().is_empty());
    for release in [first, second] {
        for file in release_files(&manager, &release.id).await {
            assert!(!file.stored);
            assert!(file.file.is_none());
            assert!(manager.database.external_file(&file.id).await.unwrap().is_some(), "the original plays again");
        }
    }
}

/// Deleting a release deletes the coven file of each of its files: its upload
/// leaves the queue, since no row references the file any more.
#[tokio::test]
async fn deleting_a_release_retires_its_queued_uploads() {
    let (manager, temp, release, _) = queued_upload_fixture("queued").await;
    manager.delete_release(&release.id).await.unwrap();
    assert_eq!(manager.database.queued_upload_count_for_test().await.unwrap(), 0);
    assert!(manager.pending().await.unwrap().is_empty());
    assert_eq!(std::fs::read(temp.path().join("queued/a.flac")).unwrap().len(), 1000);
}

/// A change that deletes the row of a file still uploading, or takes the row
/// off its file, deletes the file with its last reference: its upload leaves
/// the queue and nothing is left pending.
#[tokio::test]
async fn an_upload_whose_row_no_longer_references_it_leaves_the_queue() {
    for change in [
        "DELETE FROM release_files WHERE id = ?1",
        "UPDATE release_files SET file = NULL WHERE id = ?1",
    ] {
        let (manager, _temp, _release, file_id) = queued_upload_fixture("changed").await;
        manager
            .database
            .execute_for_test(change, vec![file_id])
            .await
            .unwrap();
        assert_eq!(
            manager.database.queued_upload_count_for_test().await.unwrap(),
            0,
            "{change}"
        );
        assert!(manager.pending().await.unwrap().is_empty(), "{change}");
    }
}

/// An original that changes before its upload fails its release's move: the
/// pending list names the file and offers Cancel Upload, which ends the move
/// and makes the release local here again.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn a_changed_original_fails_the_move_until_its_upload_is_cancelled() {
    let (manager, temp, release, _file_id) = queued_upload_fixture("changed").await;
    std::fs::write(temp.path().join("changed/a.flac"), b"changed after the move").unwrap();
    manager.drain_uploads_expecting_work().await.unwrap();
    manager.start().unwrap();
    let mut values = manager.subscribe_sync_status_values();
    let listed = pending_list_until(&mut values, |items| {
        items
            .iter()
            .any(|item| matches!(item.wait.action, crate::library::PendingAction::CancelUpload { .. }))
    })
    .await;
    let item = listed
        .iter()
        .find(|item| matches!(item.wait.action, crate::library::PendingAction::CancelUpload { .. }))
        .unwrap();
    assert_eq!(
        item.subject,
        PendingSubjectName::ReleaseFile {
            file_name: "a.flac".into(),
            album_title: "Test Album".into(),
        }
    );
    let crate::library::PendingAction::CancelUpload { release_id, id } = item.wait.action.clone()
    else {
        unreachable!("found by its action");
    };
    assert_eq!(release_id, release.id);
    manager
        .cancel_failed_release_move(&release_id, id)
        .await
        .unwrap();
    for file in release_files(&manager, &release.id).await {
        assert!(file.file.is_none(), "the release is local again");
    }
    pending_list_until(&mut values, |items| {
        !items
            .iter()
            .any(|item| matches!(item.wait.action, crate::library::PendingAction::CancelUpload { .. }))
    })
    .await;
}

/// An upload whose original vanished is listed in the sync status, named by
/// its file and album, with restoring the file as what the person can do.
#[tokio::test]
async fn an_upload_missing_its_original_is_listed_by_name() {
    let (manager, temp, _release, _file_id) = queued_upload_fixture("listed").await;
    std::fs::remove_file(temp.path().join("listed/a.flac")).unwrap();
    assert_eq!(manager.drain_uploads_expecting_work().await.unwrap(), 0);
    manager.start().unwrap();
    let mut values = manager.subscribe_sync_status_values();
    let listed = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        values.wait_for(|status| matches!(&status.pending, Some(Ok(items)) if !items.is_empty())),
    )
    .await
    .expect("the pending upload is listed")
    .unwrap()
    .clone();
    let Some(Ok(items)) = listed.pending else {
        unreachable!("waited for a listed record");
    };
    let item = items
        .iter()
        .find(|item| matches!(item.subject, PendingSubjectName::ReleaseFile { .. }))
        .expect("the upload is named by its file");
    assert_eq!(
        item.subject,
        PendingSubjectName::ReleaseFile {
            file_name: "a.flac".into(),
            album_title: "Test Album".into(),
        }
    );
    assert_eq!(item.wait.action, crate::library::PendingAction::RestoreFile);
}

/// A release's cover waits to upload beside its files, carried by the
/// `covers` row whose key is the release id; it is no file of the release's
/// upload.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn a_releases_cover_is_no_file_of_its_upload() {
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
    let cover = DbLibraryImage::cover(&release.id, "local", None, manager.clock.now().into());
    manager
        .store_library_image(&cover, cover_bytes)
        .await
        .unwrap();
    manager.upload_release(&release.id, false).await.unwrap();

    assert_eq!(
        manager.count_pending_uploads_for_release(&release.id).await.unwrap(),
        1,
        "the release's upload is its own file"
    );
    assert_eq!(
        manager
            .database
            .queued_upload_count_in_table_for_test("covers")
            .await
            .unwrap(),
        1,
        "the cover waits beside it"
    );
}

/// An upload command has queued every release it admitted before it
/// returns, so the release's row reads its upload as the command's caller
/// finishes.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn an_upload_queues_each_release_before_returning() {
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

    let outcome = manager
        .upload_releases(
            &[release.id.clone(), second_release.id.clone()])
        .await
        .unwrap();
    let UploadReleasesOutcome::Complete { receipt } = outcome else {
        panic!("both releases must be admitted");
    };
    assert_eq!(
        receipt.release_ids,
        vec![release.id.clone(), second_release.id.clone()]
    );
    for release in [release, second_release] {
        assert!(has_pending_uploads(&manager, &release.id).await);
    }
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
            ReleaseStorageAction::Upload,
        )
        .expect("hold the target in another foreground command");

    let outcome = manager
        .upload_releases(
            &[
                first_new_release.id.clone(),
                active_release.id.clone(),
                last_new_release.id.clone(),
            ])
        .await
        .expect("the batch returns every release's admission outcome");
    let UploadReleasesOutcome::Partial {
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
        assert_eq!(attached_file_count(&manager, &release.id).await, 1);
    }
}

#[cfg(feature = "test-utils")]
#[tokio::test]
async fn an_upload_batch_refuses_duplicate_release_ids_without_admission() {
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
        .upload_releases(&[release.id.clone(), release.id.clone()])
        .await
        .expect_err("a duplicate target refuses the whole batch");

    assert!(!has_pending_uploads(&manager, &release.id).await);
    assert_eq!(attached_file_count(&manager, &release.id).await, 0);
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
    assert!(manager.is_sync_paused().await.unwrap());
    manager.set_sync_paused(false).await.unwrap();
    manager.retry_outbox_now().await.unwrap();
    assert_eq!(manager.database.queued_upload_count_for_test().await.unwrap(), 0);
}

#[tokio::test]
async fn retry_outbox_surfaces_a_provider_failure_and_can_be_retried() {
    let (manager, _temp, _release, _) = queued_upload_fixture("retry").await;
    manager.storage.set_online(false);
    manager.set_sync_paused(false).await.unwrap();
    manager.retry_outbox_now().await.unwrap();
    assert!(!manager.pending().await.unwrap().is_empty());
    assert_eq!(manager.database.queued_upload_count_for_test().await.unwrap(), 1);
    manager.storage.set_online(true);
    manager.retry_outbox_now().await.unwrap();
    assert_eq!(manager.database.queued_upload_count_for_test().await.unwrap(), 0);
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

/// A release moved to the cloud to be kept here goes through no download
/// queue entry: once its move finishes, coven keeps each file whole on this
/// device itself, and the release is pinned.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn a_release_kept_here_is_pinned_once_its_move_finishes() {
    let (manager, temp) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    let release = insert_local_release_with_files(
        &manager,
        &temp.path().join("kept"),
        "Album",
        &[("audio.flac", b"audio")],
    )
    .await;
    manager.upload_release(&release.id, true).await.unwrap();
    assert_eq!(attached_file_count(&manager, &release.id).await, 1);
    assert!(download_queue_now(&manager).await.rows.is_empty());
    assert_eq!(manager.drain_uploads_expecting_work().await.unwrap(), 1);
    wait_for_kept_release(&manager, &release.id).await;
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

/// Each device says whether it can play a release: the importing device plays
/// its own originals, another device says the audio isn't uploaded, and an
/// original removed on the importing device reads as missing, at play time
/// as well.
#[tokio::test]
async fn a_release_says_whether_this_device_can_play_it() {
    use crate::album_detail::ReleasePlayability;
    use crate::ui::PlaybackErrorReason;
    let (manager, temp_dir) = setup_test_manager().await;
    let originals = temp_dir.path().join("originals");
    let here = insert_local_release_with_files(
        &manager,
        &originals,
        "Album Title",
        &[("track.flac", b"track-bytes")],
    )
    .await;
    let playability = |release_id: String| {
        let manager = &manager;
        async move {
            manager
                .find_release_detail(&release_id)
                .await
                .unwrap()
                .expect("the release exists")
                .playability
        }
    };
    assert_eq!(
        playability(here.id.clone()).await,
        ReleasePlayability::Playable
    );

    let album = create_test_album();
    manager.database.insert_album(&album).await.unwrap();
    let elsewhere = insert_local_release_without_local_files(&manager, &album.id).await;
    assert_eq!(
        playability(elsewhere.id.clone()).await,
        ReleasePlayability::NotUploaded
    );

    std::fs::remove_file(originals.join("track.flac")).unwrap();
    assert_eq!(
        playability(here.id.clone()).await,
        ReleasePlayability::OriginalMissing
    );
    let file = release_files(&manager, &here.id).await.remove(0);
    let reason = playback_error_reason_for_file(&manager, &file).await;
    assert!(
        matches!(reason, PlaybackErrorReason::OriginalMissing),
        "got {reason:?}"
    );
}
