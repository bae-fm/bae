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

    let files = manager
        .database
        .get_files_for_release(&release_id)
        .await
        .unwrap();
    let file = files.first().expect("the release has a file");
    let reason = playback_error_reason_for_file(&manager, file).await;
    assert!(
        matches!(reason, PlaybackErrorReason::SyncDisconnected),
        "got {reason:?}"
    );
}

/// A Local track whose source file was removed while its cloud upload is still
/// queued reports `UploadPending` — wait for the upload — because a
/// queued upload for the file explains the missing source. coven
/// raises `ExternalMissing`; the outbox check keys it.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn pending_upload_with_missing_source_reports_upload_pending() {
    use crate::ui::PlaybackErrorReason;
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    let release = insert_partially_uploaded_make_remote_release(&manager, temp_dir.path()).await;

    let files = manager
        .database
        .get_files_for_release(&release.id)
        .await
        .unwrap();
    let file = files
        .iter()
        .find(|f| f.original_filename == "b.flac")
        .expect("the un-uploaded file whose source was removed");
    let reason = playback_error_reason_for_file(&manager, file).await;
    assert!(
        matches!(reason, PlaybackErrorReason::UploadPending),
        "got {reason:?}"
    );
}

/// A Local track whose source file is gone with no queued upload stays a
/// diagnostic — the "files missing / moved" state, not `UploadPending`. This
/// pins the discriminator: `ExternalMissing` alone is not upload-pending.
#[tokio::test]
async fn missing_source_without_pending_upload_stays_diagnostic() {
    use crate::ui::PlaybackErrorReason;
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    manager.database.insert_album(&album).await.unwrap();
    let release = insert_local_release_without_local_files(&manager, &album.id).await;

    let files = manager
        .database
        .get_files_for_release(&release.id)
        .await
        .unwrap();
    let file = files.first().expect("the release has a file");
    let reason = playback_error_reason_for_file(&manager, file).await;
    assert!(
        matches!(reason, PlaybackErrorReason::Diagnostic { .. }),
        "got {reason:?}"
    );
}

/// The read layer surfaces a local release even when no local copy
/// resolves on this device — there is no availability filter to hide one.
/// The substrate gate (coven's `gated_by_descendants`) prunes such a
/// release's album from a *peer's* sync entirely, so a receiver never
/// materializes an orphan album; nothing is hidden on read here.
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
async fn storage_page_sort_by_artist_names() {
    let (manager, _temp_dir) = setup_test_manager().await;

    // Two artists with distinct sort-orderings; ArtistNames sort triggers
    // the `needs_artist_sort_join` branch.
    for (artist_id, artist_name) in &[
        ("ba5b6a6c-bc8c-4015-8b3c-03e78dfe28e5", "Zulu"),
        ("f2ad46f1-3a5e-4bb5-807f-5a314ae94f25", "Alpha"),
    ] {
        let artist = DbArtist {
            id: artist_id.to_string(),
            name: artist_name.to_string(),
            sort_name: None,
            discogs_artist_id: None,
            musicbrainz_artist_id: None,
            created_at: Utc::now(),
        };
        manager.database.insert_artist(&artist).await.unwrap();
        let mut album = create_test_album();
        album.title = format!("Album by {artist_name}");
        album.artist_id = artist_id.to_string();
        let release = create_test_release(&album.id);
        manager.database.insert_album(&album).await.unwrap();
        manager.database.insert_release(&release).await.unwrap();
    }

    let asc = crate::db::StorageSortCriterion {
        field: crate::db::StorageSortField::ArtistNames,
        direction: crate::db::SortDirection::Ascending,
    };
    let page = manager
        .get_storage_page(&asc, crate::db::StorageFilter::All, 0, 10)
        .await
        .unwrap();
    let names: Vec<_> = page.rows.iter().map(|r| r.album.title.clone()).collect();
    assert_eq!(names, vec!["Album by Alpha", "Album by Zulu"]);

    let desc = crate::db::StorageSortCriterion {
        field: crate::db::StorageSortField::ArtistNames,
        direction: crate::db::SortDirection::Descending,
    };
    let page = manager
        .get_storage_page(&desc, crate::db::StorageFilter::All, 0, 10)
        .await
        .unwrap();
    let names: Vec<_> = page.rows.iter().map(|r| r.album.title.clone()).collect();
    assert_eq!(names, vec!["Album by Zulu", "Album by Alpha"]);
}

#[tokio::test]
async fn storage_page_sort_by_format_nulls_last() {
    let (manager, _temp_dir) = setup_test_manager().await;

    for (title, format) in &[
        ("Album No Format", None),
        ("Album CD", Some("CD")),
        ("Album Vinyl", Some("Vinyl")),
    ] {
        let mut album = create_test_album();
        album.title = title.to_string();
        let mut release = create_test_release(&album.id);
        release.pressing.format = format.map(str::to_string);
        manager.database.insert_album(&album).await.unwrap();
        manager.database.insert_release(&release).await.unwrap();
    }

    let asc = crate::db::StorageSortCriterion {
        field: crate::db::StorageSortField::Format,
        direction: crate::db::SortDirection::Ascending,
    };
    let page = manager
        .get_storage_page(&asc, crate::db::StorageFilter::All, 0, 10)
        .await
        .unwrap();
    let titles: Vec<_> = page.rows.iter().map(|r| r.album.title.clone()).collect();
    // NULL format sorts last in both directions.
    assert_eq!(titles, vec!["Album CD", "Album Vinyl", "Album No Format"]);
}

#[tokio::test]
async fn storage_page_sort_by_file_count() {
    let (manager, _temp_dir) = setup_test_manager().await;

    // Three releases, each with a distinct number of files.
    for (title, file_count) in &[("Album A", 1usize), ("Album B", 3), ("Album C", 2)] {
        let mut album = create_test_album();
        album.title = title.to_string();
        let release = create_test_release(&album.id);
        manager.database.insert_album(&album).await.unwrap();
        manager.database.insert_release(&release).await.unwrap();
        for i in 0..*file_count {
            let file = DbFile {
                id: bae_test_support::test_uuid(&format!("{}-file-{i}", release.id)),
                release_id: release.id.clone(),
                original_filename: format!("{i}.flac"),
                file_size: 1000,
                content_type: crate::util::content_type::ContentType::Flac,
                cloud_path: None,
                content_hash: crate::util::fs::hash_bytes(b"fixture"),
                created_at: Utc::now(),
            };
            manager.database.insert_file(&file).await.unwrap();
        }
    }

    let desc = crate::db::StorageSortCriterion {
        field: crate::db::StorageSortField::FileCount,
        direction: crate::db::SortDirection::Descending,
    };
    let page = manager
        .get_storage_page(&desc, crate::db::StorageFilter::All, 0, 10)
        .await
        .unwrap();
    let titles: Vec<_> = page.rows.iter().map(|r| r.album.title.clone()).collect();
    assert_eq!(titles, vec!["Album B", "Album C", "Album A"]);
}

#[tokio::test]
async fn storage_page_sort_by_total_size() {
    let (manager, _temp_dir) = setup_test_manager().await;

    for (title, file_size) in &[("Small", 100i64), ("Big", 10_000), ("Medium", 1_000)] {
        let mut album = create_test_album();
        album.title = title.to_string();
        let release = create_test_release(&album.id);
        manager.database.insert_album(&album).await.unwrap();
        manager.database.insert_release(&release).await.unwrap();
        let file = DbFile {
            id: bae_test_support::test_uuid(&format!("{}-file", release.id)),
            release_id: release.id.clone(),
            original_filename: "a.flac".to_string(),
            file_size: *file_size,
            content_type: crate::util::content_type::ContentType::Flac,
            cloud_path: None,
            content_hash: crate::util::fs::hash_bytes(b"fixture"),
            created_at: Utc::now(),
        };
        manager.database.insert_file(&file).await.unwrap();
    }

    let asc = crate::db::StorageSortCriterion {
        field: crate::db::StorageSortField::TotalSize,
        direction: crate::db::SortDirection::Ascending,
    };
    let page = manager
        .get_storage_page(&asc, crate::db::StorageFilter::All, 0, 10)
        .await
        .unwrap();
    let titles: Vec<_> = page.rows.iter().map(|r| r.album.title.clone()).collect();
    assert_eq!(titles, vec!["Small", "Medium", "Big"]);
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
    manager
        .database
        .insert_release(&release_quiet)
        .await
        .unwrap();

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
    manager
        .transfer_cancels
        .lock()
        .unwrap()
        .insert(REL_X.to_string(), token.clone());
    manager.cancel_release_transition(REL_X).await.unwrap();
    assert!(token.is_cancelled(), "transfer token fired");

    // Nothing in progress for an unknown release → no-op, no error.
    manager.cancel_release_transition(REL_NONE).await.unwrap();
}

// Needs the test-utils mock cloud home: a Remote release implies a connected
// home, which the make-Local read storage is built over (the cancel fires
// before any blob is read, so the home is never actually called).
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn unmanage_cancelled_before_copy_leaves_release_remote() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    // Really Remote, through the real transition: make-Local resolves the
    // release's current locality from coven, which a fabricated `remote` column
    // with no cloud objects behind it cannot answer.
    let release_id =
        make_remote_release(&manager, &temp_dir.path().join("r1"), "Album One", false).await;

    // A token cancelled before the materialize loop runs: coven aborts at the
    // first check, before reading/writing any blob, and never flips state. A
    // cancelled make-Local is a clean stop (Ok), not a failure.
    let token = crate::library::CancellationToken::new();
    token.cancel();
    let dest = temp_dir.path().join("out");
    manager
        .coven_make_local(&release_id, dest.to_str().unwrap(), &token)
        .await
        .expect("a cancelled make-Local ends cleanly");

    let after = manager
        .get_release_by_id(&release_id)
        .await
        .unwrap()
        .unwrap();
    assert!(
        after.remote,
        "cancelled make-Local leaves the release remote"
    );
}

/// The snapshot over a real make-Remote: queued, in flight, mid-file progress,
/// a genuine upload failure, and the cancel that empties the queue.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn outbox_snapshot_tracks_queued_active_failed_and_cancel() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    let source_dir = temp_dir.path().join("queued");
    let release = insert_release_with_queued_uploads(
        &manager,
        &source_dir,
        "Test Album",
        &[("a.flac", &vec![b'a'; 1000])],
    )
    .await;
    let file_id = manager
        .database
        .get_files_for_release(&release.id)
        .await
        .unwrap()[0]
        .id
        .clone();

    // Freshly queued: per-release count is 1 queued, joined to the album title.
    let snap = manager.outbox_snapshot().await.unwrap();
    assert_eq!(snap.total.queued, 1);
    assert_eq!(snap.total.uploading, 0);
    assert_eq!(snap.total.failed, 0);
    assert_eq!(snap.total.preparation_bytes_total, 1000);
    assert_eq!(snap.total.upload_bytes_done, 0);
    assert_eq!(snap.upload_groups.len(), 1);
    let group = &snap.upload_groups[0];
    assert_eq!(group.display_title, "Test Album");
    assert_eq!(group.release_id, release.id);
    assert_eq!(group.files.len(), 1);
    assert_eq!(
        group.files[0].label,
        crate::library::UploadFileLabel::Filename("a.flac".to_string())
    );
    assert_eq!(group.progress.queued, 1);
    assert_eq!(group.progress.preparation_bytes_total, 1000);

    // Source preparation starts before coven has produced the encrypted object.
    manager
        .observe_blob_preparation_started_for_test(&file_id)
        .await;
    let snap = manager.outbox_snapshot().await.unwrap();
    assert_eq!(snap.total.preparing, 1);
    assert_eq!(snap.total.queued, 0);
    assert_eq!(snap.upload_groups[0].progress.preparing, 1);
    assert_eq!(snap.total.preparation_bytes_done, 0);

    // Mid-preparation progress advances at the source stream's buffer cadence.
    manager
        .observe_blob_preparation_progress_for_test(&file_id, 400, 1000)
        .await;
    let snap = manager.outbox_snapshot().await.unwrap();
    assert_eq!(snap.upload_groups[0].progress.preparing, 1);
    assert_eq!(snap.upload_groups[0].progress.preparation_bytes_done, 400);
    assert_eq!(snap.total.preparation_bytes_done, 400);
    assert_eq!(snap.total.upload_bytes_done, 0);
    assert_eq!(snap.total.preparation_bytes_total, 1000);
    manager.sync.clear_transient_upload_for_test(&file_id);

    // A real failure: the user's file is gone, so the drain cannot seal it. The
    // entry stays queued with coven's own attempt count and error on it.
    std::fs::remove_file(source_dir.join("a.flac")).unwrap();
    assert_eq!(
        manager.drain_uploads_expecting_work().await.unwrap(),
        0,
        "the entry was attempted and sealed nothing"
    );
    let snap = manager.outbox_snapshot().await.unwrap();
    assert_eq!(snap.total.failed, 1);
    assert_eq!(snap.total.queued, 0);
    assert_eq!(snap.upload_groups[0].progress.failed, 1);
    let failure = manager
        .database
        .first_queued_upload_failure_for_test()
        .await
        .unwrap()
        .expect("the failed upload remains queued");
    assert_eq!(failure.0, 1);
    assert!(failure.1, "coven records why the attempt failed");

    // Cancelling the release's make-Remote clears the queue; the snapshot empties.
    manager.cancel_release_upload(&release.id).await.unwrap();
    let snap = manager.outbox_snapshot().await.unwrap();
    assert!(snap.upload_groups.is_empty());
    assert_eq!(snap.total.failed, 0);
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
        manager.clock.now(),
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
        cover_upload.file_id,
        format!("{}:{cover_blob_id}", crate::sync::COVERS_NAMESPACE)
    );
    assert_eq!(cover_upload.source_bytes_total, cover_bytes.len() as u64);
}

/// A successful make-Remote command must publish the canonical durable queue
/// snapshot before it returns. The foreground transfer and Import status both
/// finish immediately after this call; without this ordering the UI briefly
/// falls back to Local/Imported while the live query catches up.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn make_remote_publishes_its_durable_queue_before_returning() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    assert!(!manager.is_sync_ready(), "the test exercises queueing without a sync loop");
    let release = insert_local_release_with_files(
        &manager,
        &temp_dir.path().join("publish-queue"),
        "Album Title",
        &[("track.flac", b"track-bytes")],
    )
    .await;
    let mut values = manager.subscribe_outbox_values();
    assert!(values.borrow_and_update().is_none());

    let queued_revision = manager
        .make_release_remote(&release.id, false)
        .await
        .unwrap();

    let snapshot = values
        .borrow_and_update()
        .clone()
        .expect("make-Remote publishes an outbox value")
        .expect("the durable queue projects");
    assert_eq!(snapshot.revision, queued_revision);
    assert!(snapshot
        .upload_groups
        .iter()
        .any(|group| group.release_id == release.id));
}

/// Queue identity and display context have different owners. A title edit does
/// not change coven's private outbox rows, but the retained outbox value must
/// still re-enrich the same release instead of freezing the title captured at
/// enqueue time.
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

    manager
        .database
        .rename_album_for_test(&release.album_id, "Renamed Album")
        .await
        .unwrap();

    let renamed = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            values.changed().await.expect("outbox value stream");
            let current = values.borrow_and_update();
            let snapshot = current
                .as_ref()
                .expect("outbox subscription published a value")
                .as_ref()
                .unwrap_or_else(|error| panic!("outbox projection failed: {error}"));
            if snapshot.upload_groups[0].display_title == "Renamed Album" {
                break snapshot.upload_groups[0].display_title.clone();
            }
        }
    })
    .await
    .expect("album title changes wake outbox enrichment");
    assert_eq!(renamed, "Renamed Album");

    let file = manager
        .database
        .get_files_for_release(&release.id)
        .await
        .unwrap()
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

/// Storage Manager retry bypasses an automatic retry delay, but a paused retry
/// neither attempts the upload nor changes that delay.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn retry_outbox_now_respects_pause_and_bypasses_backoff() {
    let (manager, temp_dir) = setup_test_manager().await;
    let home = connect_test_cloud(&manager).await;
    insert_release_with_queued_uploads(
        &manager,
        &temp_dir.path().join("retry-queue"),
        "Album Title",
        &[("track.flac", b"track-bytes")],
    )
    .await;

    home.fail_exact_create_before_call(1);
    assert_eq!(manager.drain_uploads_expecting_work().await.unwrap(), 0);
    assert_eq!(home.exact_create_count(), 1);
    assert!(matches!(
        manager.drain_uploads_for_test().await.unwrap(),
        coven::DrainOutcome::AllInBackoff
    ));

    manager.set_sync_paused(true).await;
    let paused = manager
        .retry_outbox_now()
        .await
        .expect_err("a paused retry must report that it did not run");
    assert!(matches!(paused, LibraryError::Storage(_)));
    assert_eq!(home.exact_create_count(), 1);
    manager.set_sync_paused(false).await;
    assert!(matches!(
        manager.drain_uploads_for_test().await.unwrap(),
        coven::DrainOutcome::AllInBackoff
    ));

    manager.retry_outbox_now().await.unwrap();
    assert_eq!(home.exact_create_count(), 2);
}

/// Test managers that connect a cloud must install the same upload observer as
/// the app; otherwise a paused queue drains because coven cannot see bae's
/// pause state.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn connected_test_manager_uses_the_production_upload_observer() {
    let (manager, temp_dir) = setup_test_manager().await;
    let home = connect_test_cloud(&manager).await;
    insert_release_with_queued_uploads(
        &manager,
        &temp_dir.path().join("paused-queue"),
        "Album Title",
        &[("track.flac", b"track-bytes")],
    )
    .await;

    let creates_before_pause = home.exact_create_count();
    manager.set_sync_paused(true).await;
    let outcome = manager.database.retry_uploads_now().await.unwrap();

    assert!(matches!(outcome, coven::DrainOutcome::Paused));
    assert_eq!(home.exact_create_count(), creates_before_pause);
}

/// The real observer drives source-preparation byte progress into the canonical
/// snapshot at coven's buffer cadence. Provider callback ordering is tested at
/// the observer itself; the snapshot's provider-phase join is a pure projection
/// test with coven's matching durable phase.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn preparation_observer_advances_snapshot_bytes_done() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    let release = insert_release_with_queued_uploads(
        &manager,
        &temp_dir.path().join("queued"),
        "Test Album",
        &[("a.flac", &vec![b'a'; 1000])],
    )
    .await;
    let file_id = manager
        .database
        .get_files_for_release(&release.id)
        .await
        .unwrap()[0]
        .id
        .clone();

    manager
        .observe_blob_preparation_started_for_test(&file_id)
        .await;
    manager
        .observe_blob_preparation_progress_for_test(&file_id, 300, 1000)
        .await;
    let snap = manager.outbox_snapshot().await.unwrap();
    assert_eq!(snap.upload_groups[0].progress.preparing, 1);
    assert_eq!(snap.total.preparation_bytes_done, 300);
    assert_eq!(snap.total.upload_bytes_done, 0);
}

/// One coalesced coven callback is one outbox value. Publishing it twice would
/// double the database reads and revision churn at every progress tick.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn one_upload_observer_callback_publishes_one_outbox_revision() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    let release = insert_release_with_queued_uploads(
        &manager,
        &temp_dir.path().join("queued"),
        "Test Album",
        &[("a.flac", &vec![b'a'; 1000])],
    )
    .await;
    let file_id = manager
        .database
        .get_files_for_release(&release.id)
        .await
        .unwrap()[0]
        .id
        .clone();
    let before = manager.outbox_snapshot().await.unwrap().revision;

    manager
        .observe_blob_preparation_started_for_test(&file_id)
        .await;

    let after = manager.outbox_snapshot().await.unwrap().revision;
    assert_eq!(after, before + 1);
}

/// Source preparation progress is ordered after preparation-start, which
/// establishes the exact plaintext denominator for that attempt.
#[cfg(feature = "test-utils")]
#[tokio::test]
#[should_panic(expected = "preparation progress arrived without a preparation-start state")]
async fn preparation_progress_requires_preparation_start() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    let release = insert_release_with_queued_uploads(
        &manager,
        &temp_dir.path().join("queued"),
        "Test Album",
        &[("a.flac", &vec![b'a'; 1000])],
    )
    .await;
    let file_id = manager
        .database
        .get_files_for_release(&release.id)
        .await
        .unwrap()[0]
        .id
        .clone();

    manager
        .observe_blob_preparation_progress_for_test(&file_id, 300, 1000)
        .await;
}

/// Preparation measures the exact plaintext source declared by the row.
#[cfg(feature = "test-utils")]
#[tokio::test]
#[should_panic(expected = "preparation progress changed its exact plaintext total")]
async fn preparation_progress_must_match_source_total() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    let release = insert_release_with_queued_uploads(
        &manager,
        &temp_dir.path().join("queued"),
        "Test Album",
        &[("a.flac", &vec![b'a'; 1000])],
    )
    .await;
    let file_id = manager
        .database
        .get_files_for_release(&release.id)
        .await
        .unwrap()[0]
        .id
        .clone();

    manager
        .observe_blob_preparation_started_for_test(&file_id)
        .await;
    manager
        .observe_blob_preparation_progress_for_test(&file_id, 300, 999)
        .await;
}
