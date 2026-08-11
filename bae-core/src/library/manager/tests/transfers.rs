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
    manager.disconnect_cloud_provider().unwrap();

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

    // A registered unmanage token is fired by the unified cancel.
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
    assert_eq!(snap.total.active, 0);
    assert_eq!(snap.total.failed, 0);
    assert_eq!(snap.total.bytes_total, 1000);
    assert_eq!(snap.total.bytes_done, 0);
    assert_eq!(snap.upload_groups.len(), 1);
    let group = &snap.upload_groups[0];
    assert_eq!(group.display_title, "Test Album");
    assert_eq!(group.release_id.as_deref(), Some(release.id.as_str()));
    assert_eq!(group.files.len(), 1);
    assert_eq!(group.files[0].display_name, "a.flac");
    assert_eq!(group.progress.queued, 1);
    assert_eq!(group.progress.bytes_total, 1000);

    // In flight now: the in-memory map flips it to active, starting at zero
    // bytes done.
    manager.sync.set_upload_progress_for_test(&file_id, 0);
    let snap = manager.outbox_snapshot().await.unwrap();
    assert_eq!(snap.total.active, 1);
    assert_eq!(snap.total.queued, 0);
    assert_eq!(snap.upload_groups[0].progress.active, 1);
    assert_eq!(snap.total.bytes_done, 0);

    // Mid-upload progress advances the live byte count: the snapshot's
    // per-release and aggregate bytes_done climb without the file completing.
    manager.sync.set_upload_progress_for_test(&file_id, 400);
    let snap = manager.outbox_snapshot().await.unwrap();
    assert_eq!(snap.upload_groups[0].progress.active, 1);
    assert_eq!(snap.upload_groups[0].progress.bytes_done, 400);
    assert_eq!(snap.total.bytes_done, 400);
    assert_eq!(snap.total.bytes_total, 1000);
    manager.sync.clear_upload_progress_for_test(&file_id);

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

/// The real `ReleaseUploadObserver` drives the snapshot's live byte count:
/// `on_blob_upload_progress` advances an in-flight `Active` file's
/// `bytes_done` so the aggregate and per-release bars move mid-file.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn observer_progress_advances_snapshot_bytes_done() {
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

    // The observer shares the manager's in-flight map and throughput tracker,
    // exactly as production wires it in `build_sync_manager`.
    manager.observe_blob_upload_started_for_test(&file_id).await;
    let snap = manager.outbox_snapshot().await.unwrap();
    assert_eq!(snap.upload_groups[0].progress.active, 1);
    assert_eq!(snap.upload_groups[0].progress.bytes_done, 0);
    assert_eq!(snap.total.bytes_done, 0);

    // A mid-upload progress report advances the live count without the file
    // completing.
    manager
        .observe_blob_upload_progress_for_test(&file_id, 600, 1000)
        .await;
    let snap = manager.outbox_snapshot().await.unwrap();
    assert_eq!(snap.upload_groups[0].progress.active, 1);
    assert_eq!(snap.upload_groups[0].progress.bytes_done, 600);
    assert_eq!(snap.total.bytes_done, 600);
    // The rolling-window tracker saw the 600-byte delta, so the rate is
    // non-zero before the file even finishes.
    assert!(manager.sync.upload_bytes_per_second_for_test() > 0);

    // Completion clears the in-flight entry and tallies the file as done; the
    // queue entry is still there (this test drives only the observer, not
    // coven's drain), but with its only file shipped the release has nothing
    // left to render — the group leaves the snapshot.
    manager.observe_blob_uploaded_for_test(&file_id).await;
    let snap = manager.outbox_snapshot().await.unwrap();
    assert_eq!(snap.total.active, 0);
    assert_eq!(snap.total.queued, 0);
    assert!(snap.upload_groups.is_empty());
}

/// A file that finished uploading but whose queue entry hasn't been consumed
/// yet (coven reports completion first, then clears the entry inside the
/// post-upload commit) must read as done work — never as freshly queued. The
/// Storage Manager renders whatever the last emitted snapshot says, so a
/// completed upload re-deriving as "Queued" is a lie the UI can freeze on.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn completed_upload_with_lingering_entry_is_not_queued() {
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

    manager.observe_blob_upload_started_for_test(&file_id).await;
    manager
        .observe_blob_upload_progress_for_test(&file_id, 1000, 1000)
        .await;
    manager.observe_blob_uploaded_for_test(&file_id).await;

    // The queue entry is still present — only coven's commit consumes it — but
    // the upload finished: nothing pending anywhere, and the release (its only
    // file shipped) is no longer rendered at all.
    let snap = manager.outbox_snapshot().await.unwrap();
    assert_eq!(
        snap.total.queued, 0,
        "a completed upload must not re-derive as queued"
    );
    assert_eq!(snap.total.active, 0);
    assert_eq!(snap.total.failed, 0);
    assert!(snap.upload_groups.is_empty());
}

/// Insert a remote, not-pinned release with one file and return its id.
/// `remote: true` + no pinned cache copy makes it eligible for pinning.
async fn insert_pinnable_release(manager: &LibraryManager) -> String {
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    let file = DbFile {
        id: bae_test_support::test_uuid(&format!("{}-file", release.id)),
        release_id: release.id.clone(),
        original_filename: "a.flac".to_string(),
        file_size: 1000,
        content_type: crate::util::content_type::ContentType::Flac,
        cloud_path: None,
        content_hash: crate::util::fs::hash_bytes(b"fixture"),
        created_at: Utc::now(),
    };
    manager.database.insert_file(&file).await.unwrap();
    release.id
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
    let snap = manager.download_snapshot();
    assert_eq!(snap.total.queued, 1);
    assert_eq!(snap.ops.len(), 1);
    assert_eq!(snap.ops[0].title, "Test Album");
    assert_eq!(snap.ops[0].file_count, 1);
    assert_eq!(snap.ops[0].state, crate::library::DownloadState::Queued);
    assert!(snap.paused);

    // Re-enqueuing the same release is a no-op: still one entry.
    manager.enqueue_pins(vec![release_id.clone()]).await;
    assert_eq!(manager.download_snapshot().ops.len(), 1);

    // Cancel drops the entry.
    manager.cancel_download(&release_id);
    let snap = manager.download_snapshot();
    assert!(snap.ops.is_empty());
}

/// An already-pinned release is skipped at enqueue rather than re-downloaded.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn download_queue_skips_already_pinned() {
    let (manager, temp_dir) = setup_test_manager().await;
    manager.set_downloads_paused(true);

    // A genuinely pinned release: made Remote with pin, so its blob lands in
    // coven's offline cache.
    connect_test_cloud(&manager).await;
    let release_id = make_remote_release(
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
        "the offline-cached blob makes it read as pinned"
    );

    manager.enqueue_pins(vec![release_id.clone()]).await;
    assert!(manager.download_snapshot().ops.is_empty());
}

/// A local (unmanaged) release has nothing to pin — it is already fully on disk —
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
        manager.download_snapshot().ops.is_empty(),
        "a local release is not enqueued for pinning"
    );
}

/// A pin that fails (no cloud home for a cloud-only release) lands `Failed`
/// and stays in the queue; `retry_downloads` flips it back to `Queued`.
#[tokio::test]
async fn download_queue_failed_pin_retries() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let release_id = insert_pinnable_release(&manager).await;

    // No cloud home + no local copy ⇒ the pin can't read the file and fails.
    manager.enqueue_pins(vec![release_id.clone()]).await;

    // Let the worker pick it up, fail, and mark it Failed. Poll the snapshot
    // rather than sleeping a fixed interval.
    let failed = wait_for(|| {
        matches!(
            manager.download_snapshot().ops.first().map(|op| &op.state),
            Some(crate::library::DownloadState::Failed { .. })
        )
    })
    .await;
    assert!(failed, "the pin should land Failed without a cloud home");
    assert_eq!(manager.download_snapshot().total.failed, 1);

    // Retry flips it back to Queued; with no cloud home it'll fail again,
    // but the immediate post-retry state is Queued (or already re-failed).
    manager.retry_downloads();
    let snap = manager.download_snapshot();
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
    let cleared = wait_for(|| manager.download_snapshot().ops.is_empty()).await;
    assert!(cleared, "cancel removes the entry");
}

#[cfg(feature = "test-utils")]
#[tokio::test]
async fn download_queue_active_pin_reports_file_progress() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    let release_id = make_remote_release_with_files(
        &manager,
        &temp_dir.path().join("download-source"),
        "Test Album",
        &[("a.flac", b"aaa"), ("b.flac", b"bbbb")],
        false,
    )
    .await;
    let mut events = manager.subscribe_events();

    manager.enqueue_pins(vec![release_id.clone()]).await;

    // The release pins one blob at a time, so the pane sees the byte total climb:
    // 0, then the first file's 3 bytes, then both files' 7.
    let mut seen: Vec<u64> = Vec::new();
    for _ in 0..20 {
        let event = tokio::time::timeout(std::time::Duration::from_secs(2), events.recv())
            .await
            .expect("download queue event")
            .expect("event channel stays open");
        let LibraryEvent::DownloadQueueChanged { snapshot } = event else {
            continue;
        };
        for op in snapshot.ops {
            if op.release_id != release_id {
                continue;
            }
            if let crate::library::DownloadState::Active { progress } = op.state {
                assert_eq!(progress.bytes_total, 7, "the release's known byte total");
                assert_eq!(
                    progress.fraction,
                    progress.bytes_done as f64 / 7.0,
                    "the fraction tracks the bytes"
                );
                if seen.last() != Some(&progress.bytes_done) {
                    seen.push(progress.bytes_done);
                }
            }
        }
        if seen.contains(&7) {
            break;
        }
    }

    assert_eq!(
        seen,
        vec![0, 3, 7],
        "an active download reports each file's bytes as it lands, not just 0 and done",
    );
}

// ── Export queue ─────────────────────────────────────────────────
