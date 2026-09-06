fn create_test_album() -> DbAlbum {
    DbAlbum {
        id: Uuid::new_v4().to_string(),
        title: "Test Album".to_string(),
        artist_id: bae_test_support::test_uuid("e36744a5-1a36-460f-891c-e7e558034edf"),
        year: Some(2024),
        primary_release_id: None,
        is_compilation: false,
        created_at: Utc::now(),
    }
}
fn create_test_release(album_id: &str) -> DbRelease {
    DbRelease {
        id: Uuid::new_v4().to_string(),
        album_id: album_id.to_string(),
        release_name: None,
        pressing: Pressing {
            year: Some(2024),
            format: None,
            label: None,
            catalog_number: None,
            country: None,
            barcode: None,
        },
        disc_id: None,
        metadata_provenance: Some(crate::import::MetadataProvenance::FileTags),
        remote: true,
        source_folder_name: None,
        content_hash: None,
        album_loudness_lufs: None,
        album_peak_linear: None,
        created_at: Utc::now(),
    }
}

async fn create_candidate_draft(manager: &LibraryManager) -> (String, String) {
    let root = crate::import::folder_registry::host_root("/music");
    let path = std::path::Path::new(&root).join("candidate");
    let path = path.to_string_lossy().into_owned();
    let files = crate::import::folder_scanner::CategorizedFiles {
        files: vec![crate::import::folder_scanner::CandidateFile {
            proposed_audio: true,
            file: crate::import::folder_scanner::ScannedFile::new(
                std::path::Path::new(&path).join("01.flac"),
                "01.flac".to_string(),
                1_000,
                1,
            )
            .with_test_flac_audio(),
            role: crate::import::folder_scanner::FileRole::Audio,
        }],
    };
    let content_hash = files.content_hash();
    let candidate = crate::import::folder_scanner::FolderCandidate {
        path: std::path::PathBuf::from(&path),
        file_root: std::path::PathBuf::from(&path),
        name: "Candidate".to_string(),
        files,
        watched_folder_path: root.clone(),
        scope: crate::import::folder_scanner::ReleaseFileScope::Recursive,
        file_edit_revision: 0,
        display_path: "Candidate".to_string(),
        resolved_boundaries: Vec::new(),
        combine_ancestor_key: None,
    };
    manager
        .database
        .add_watched_import_folder(&root)
        .await
        .unwrap();
    let generation = manager.database.begin_folder_scan(&root).await.unwrap();
    manager
        .database
        .save_folder_scan_item(
            &root,
            generation,
            &crate::import::folder_scanner::ScanItem::Valid(candidate),
        )
        .await
        .unwrap()
        .expect("the active scan stores the candidate draft");
    let track_id = manager
        .database
        .load_import_candidate_pane_rows(&content_hash)
        .await
        .unwrap()
        .draft
        .tracks
        .into_iter()
        .next()
        .expect("the audio file creates one draft track")
        .edit
        .id;
    (content_hash, track_id)
}

#[tokio::test]
async fn work_detail_release_rows_are_display_ready() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.pressing.format = Some("CD".to_string());
    let track = crate::db::DbTrack::new_test(&release.id, TRACK_A, "Track Title", Some(1));
    let now = Utc::now();
    let work = DbWork {
        id: WORK_A.to_string(),
        title: "Work Title".to_string(),
        disambiguation: None,
        work_type: Some("work".to_string()),
        musicbrainz_work_id: "mb-work-a".to_string(),
        created_at: now,
    };
    let track_work = DbTrackWork::new(
        &track.id,
        &work.id,
        0,
        MetadataSource::MusicBrainz,
        TRACK_WORK_A.to_string(),
        now,
    );
    let cover = DbLibraryImage {
        id: release.id.clone(),
        blob_id: format!("{}-cover-blob", release.id),
        image_type: LibraryImageType::Cover,
        content_type: ContentType::Jpeg,
        file_size: 100,
        width: None,
        height: None,
        source: "local".to_string(),
        source_url: None,
        cloud_path: None,
        content_hash: crate::util::fs::hash_bytes(b"fixture"),
        created_at: now,
    };

    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    manager.database.insert_track(&track).await.unwrap();
    manager
        .database
        .insert_composition_fixture_rows(std::slice::from_ref(&work), &[track_work], &[cover])
        .await
        .unwrap();

    let detail = manager
        .get_work_detail(&work.id)
        .await
        .unwrap()
        .expect("work detail");

    assert_eq!(detail.releases.len(), 1);
    let row = &detail.releases[0];
    assert_eq!(row.release_id, release.id);
    assert_eq!(row.album_id, album.id);
    assert_eq!(row.album_title, album.title);
    assert_eq!(row.display_name, "2024 CD");
    assert_eq!(row.format.as_deref(), Some("CD"));
    let cover = row.cover.as_ref().expect("work release cover");
    assert_eq!(cover.id, release.id);
    assert!(!cover.version.is_empty());
    assert_eq!(cover.image_type, crate::db::LibraryImageType::Cover);
}

#[tokio::test]
async fn test_delete_release_with_single_release_deletes_album() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);

    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    manager.delete_release(&release.id).await.unwrap();

    let album_result = manager.database.find_album_by_id(&album.id).await.unwrap();
    assert!(album_result.is_none());
    let releases = manager
        .database
        .get_releases_for_album(&album.id)
        .await
        .unwrap();
    assert!(releases.is_empty());
}

#[tokio::test]
async fn failed_import_rollback_preserves_an_artist_selected_by_candidate_edits() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    let artist = manager
        .database
        .find_artist_by_id(&album.artist_id)
        .await
        .unwrap()
        .expect("the fixture artist exists");
    let (candidate_hash, candidate_track_id) = create_candidate_draft(&manager).await;
    manager
        .database
        .save_import_candidate_failure(
            &candidate_hash,
            0,
            &crate::import::ImportFailure::error_only("not imported", manager.clock.now()),
        )
        .await
        .unwrap();
    manager.preparations.set_album_artists(
            &candidate_hash,
            &[crate::import::ArtistAssignment::existing(
                artist.clone().into(),
            )],
        )
        .await
        .unwrap();
    manager.preparations.set_track_edit(
            &candidate_hash,
            &crate::import::CandidateTrackEdit::edited(crate::import::RawTrackEdit {
                id: candidate_track_id,
                title: "Track Title".to_string(),
                artist_assignments: crate::import::TrackArtistAssignments::Explicit(vec![
                    crate::import::ArtistAssignment::existing(artist.into()),
                ]),
                side: 1,
                track_number: Some(1),
                file: None,
            }),
        )
        .await
        .unwrap();

    manager
        .database
        .fail_import_and_delete_release(&release.id)
        .await
        .unwrap();

    assert!(manager
        .database
        .find_artist_by_id(&album.artist_id)
        .await
        .unwrap()
        .is_some());
}
#[tokio::test]
async fn test_delete_release_with_multiple_releases_preserves_album() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release1 = create_test_release(&album.id);
    let release2 = create_test_release(&album.id);

    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release1).await.unwrap();
    manager.database.insert_release(&release2).await.unwrap();

    manager.delete_release(&release1.id).await.unwrap();

    let album_result = manager.database.find_album_by_id(&album.id).await.unwrap();
    assert!(album_result.is_some());
    let releases = manager
        .database
        .get_releases_for_album(&album.id)
        .await
        .unwrap();
    assert_eq!(releases.len(), 1);
    assert_eq!(releases[0].id, release2.id);
}

#[tokio::test]
async fn delete_releases_with_content_hash_removes_only_matching() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let mut matching1 = create_test_release(&album.id);
    matching1.content_hash = Some("hash-shared".to_string());
    let mut matching2 = create_test_release(&album.id);
    matching2.content_hash = Some("hash-shared".to_string());
    let mut other = create_test_release(&album.id);
    other.content_hash = Some("hash-other".to_string());

    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&matching1).await.unwrap();
    manager.database.insert_release(&matching2).await.unwrap();
    manager.database.insert_release(&other).await.unwrap();

    manager
        .delete_releases_with_content_hash("hash-shared")
        .await
        .unwrap();

    // Both releases carrying the re-imported folder's hash are gone; the
    // unrelated release survives. This is the overwrite the import worker
    // performs before inserting a re-import of the same folder tree.
    let remaining = manager
        .database
        .get_releases_for_album(&album.id)
        .await
        .unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].id, other.id);
}

/// Deleting one release of a multi-release album must tombstone its remote
/// cloud blobs — delete_release has to queue the cloud-outbox deletes like
/// delete_album/make-Local, or the remote blobs leak in the cloud (nothing else
/// processes the release once its rows are gone).
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn delete_release_tombstones_remote_cloud_blobs() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;

    // A release whose one file really did reach the cloud — the tombstone is
    // owed to a cloud object that exists, which is the whole point.
    let release1 =
        make_remote_release(&manager, &temp_dir.path().join("r1"), "Album One", false).await;
    let file_id = manager
        .database
        .get_files_for_release(&release1)
        .await
        .unwrap()[0]
        .id
        .clone();
    // A sibling release in the same album, so delete_release takes the
    // album-survives branch.
    let album_id = manager
        .database
        .find_release_by_id(&release1)
        .await
        .unwrap()
        .expect("the release exists")
        .album_id;
    let mut release2 = create_test_release(&album_id);
    release2.remote = false;
    manager.database.insert_release(&release2).await.unwrap();

    manager.delete_release(&release1).await.unwrap();

    // delete_release awaits the deletion queueing, so by now the cloud object's
    // tombstone is enqueued.
    assert!(
        has_queued_delete(&manager, crate::sync::RELEASE_FILES_NAMESPACE, &file_id).await,
        "deleting a remote release tombstones its cloud blob"
    );
}

#[cfg(feature = "test-utils")]
#[tokio::test]
async fn delete_release_cancels_in_flight_make_remote() {
    let (manager, temp_dir) = setup_test_manager().await;
    let home = connect_test_cloud(&manager).await;
    let release = insert_partially_uploaded_make_remote_release(&manager, temp_dir.path()).await;
    let deleted_before = home.exact_delete_count();

    manager.delete_release(&release.id).await.unwrap();

    assert!(manager
        .database
        .find_release_by_id(&release.id)
        .await
        .unwrap()
        .is_none());
    // The delete records the unwind rather than performing it: the object
    // already in the cloud has to be taken back out, and only a drain can do
    // that. Until it runs the queue is intact and says what it is doing.
    assert_eq!(
        manager
            .database
            .make_remote_progress_for_release(&release.id)
            .await
            .unwrap(),
        Some(coven::MakeRemoteProgress::Cancelling),
        "the deleted release's transition is being unwound"
    );
    manager.drain_uploads_for_test().await.unwrap();

    assert!(
        manager
            .database
            .queued_upload_count_for_test()
            .await
            .unwrap()
            == 0,
        "the drain took the deleted release's uploads out of the queue"
    );
    assert!(manager
        .database
        .make_remote_progress_for_release(&release.id)
        .await
        .unwrap()
        .is_none());
    // The object that already reached the cloud is removed outright, not left
    // as a queued tombstone: the make-Remote never published, so nothing else
    // can reference it and the cancel's own unwind deletes it.
    assert!(
        home.exact_delete_count() > deleted_before,
        "the uploaded object is deleted from the cloud"
    );
    assert!(
        manager
            .database
            .queued_delete_count_for_test()
            .await
            .unwrap()
            == 0,
        "an unpublished object needs no tombstone"
    );
}

#[cfg(feature = "test-utils")]
#[tokio::test]
/// Deleting is local work. A cloud that refuses the object's removal does not
/// keep the release: the removal stays owed and waits for a drain that can
/// carry it out, the way any pending transfer does.
async fn delete_release_survives_a_cloud_that_refuses_the_cleanup() {
    let (manager, temp_dir) = setup_test_manager().await;
    let home = connect_test_cloud(&manager).await;
    let release = insert_partially_uploaded_make_remote_release(&manager, temp_dir.path()).await;
    home.fail_exact_delete_on_call(1);

    manager
        .delete_release(&release.id)
        .await
        .expect("the delete does not wait on the cloud");

    assert!(manager
        .database
        .find_release_by_id(&release.id)
        .await
        .unwrap()
        .is_none());
    assert_eq!(
        manager
            .database
            .make_remote_progress_for_release(&release.id)
            .await
            .unwrap(),
        Some(coven::MakeRemoteProgress::Cancelling),
        "and the unwind is still owed"
    );
}

#[cfg(feature = "test-utils")]
#[tokio::test]
async fn delete_album_cancels_in_flight_make_remote() {
    let (manager, temp_dir) = setup_test_manager().await;
    let home = connect_test_cloud(&manager).await;
    let release = insert_partially_uploaded_make_remote_release(&manager, temp_dir.path()).await;
    let deleted_before = home.exact_delete_count();

    manager.delete_album(&release.album_id).await.unwrap();

    assert!(manager
        .database
        .find_release_by_id(&release.id)
        .await
        .unwrap()
        .is_none());
    assert_eq!(
        manager
            .database
            .make_remote_progress_for_release(&release.id)
            .await
            .unwrap(),
        Some(coven::MakeRemoteProgress::Cancelling),
        "each deleted release's transition is being unwound"
    );
    manager.drain_uploads_for_test().await.unwrap();

    assert!(
        manager
            .database
            .queued_upload_count_for_test()
            .await
            .unwrap()
            == 0,
        "the drain took the deleted album's uploads out of the queue"
    );
    assert!(manager
        .database
        .make_remote_progress_for_release(&release.id)
        .await
        .unwrap()
        .is_none());
    // The object that already reached the cloud is removed outright, not left
    // as a queued tombstone: the make-Remote never published, so nothing else
    // can reference it and the cancel's own unwind deletes it.
    assert!(
        home.exact_delete_count() > deleted_before,
        "the uploaded object is deleted from the cloud"
    );
    assert!(
        manager
            .database
            .queued_delete_count_for_test()
            .await
            .unwrap()
            == 0,
        "an unpublished object needs no tombstone"
    );
}

#[cfg(feature = "test-utils")]
#[tokio::test]
/// See the release case: a cloud that refuses the removal does not keep the
/// album. The removal stays owed.
async fn delete_album_survives_a_cloud_that_refuses_the_cleanup() {
    let (manager, temp_dir) = setup_test_manager().await;
    let home = connect_test_cloud(&manager).await;
    let release = insert_partially_uploaded_make_remote_release(&manager, temp_dir.path()).await;
    home.fail_exact_delete_on_call(1);

    manager
        .delete_album(&release.album_id)
        .await
        .expect("the delete does not wait on the cloud");

    assert!(manager
        .database
        .find_release_by_id(&release.id)
        .await
        .unwrap()
        .is_none());
    assert_eq!(
        manager
            .database
            .make_remote_progress_for_release(&release.id)
            .await
            .unwrap(),
        Some(coven::MakeRemoteProgress::Cancelling),
        "and the unwind is still owed"
    );
}

#[tokio::test]
async fn delete_release_fails_before_rows_are_deleted_when_file_cleanup_lookup_fails() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    let file = DbFile::new(
        &release.id,
        "track1.flac",
        5,
        crate::util::content_type::ContentType::Flac,
        Uuid::new_v4().to_string(),
        Utc::now(),
    );
    manager.add_file(&file).await.unwrap();

    rename_table_for_test(&manager, "release_files", "release_files_unavailable").await;

    let error = manager.delete_release(&release.id).await.unwrap_err();
    assert!(matches!(error, LibraryError::Database(_)));
    assert!(manager
        .database
        .find_release_by_id(&release.id)
        .await
        .unwrap()
        .is_some());
}

/// The row deletes and the blob cleanup share one transaction, so a cleanup step
/// coven refuses takes the row deletes down with it — the release survives rather
/// than leaving the library short a release whose blob bookkeeping still stands.
///
/// Clearing an external registration names the blob table it belongs to, and
/// coven refuses a name that declares no blob. That refusal lands mid-transaction,
/// after the deletes have been staged, which is the point.
#[tokio::test]
async fn delete_release_rolls_back_when_an_external_ref_clear_is_refused() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    let file = DbFile::new(
        &release.id,
        "track1.flac",
        5,
        crate::util::content_type::ContentType::Flac,
        Uuid::new_v4().to_string(),
        Utc::now(),
    );
    manager.add_file(&file).await.unwrap();

    manager
        .database
        .delete_release_with_cleanup(
            &release.id,
            &album.id,
            DeleteCleanupPlan {
                blobs_to_tombstone: Vec::new(),
                external_refs_to_clear: vec![("no_such_blob_table".to_string(), file.id.clone())],
            },
        )
        .await
        .expect_err("clearing a ref on an undeclared blob table is refused");

    assert!(manager
        .database
        .find_release_by_id(&release.id)
        .await
        .unwrap()
        .is_some());
}

/// The tombstone half of the rollback above. A blob with no committed cloud
/// object has nothing to remove and coven refuses to queue a tombstone for it, so
/// a cover that never reached the cloud is a cleanup step that fails inside the
/// delete transaction.
#[tokio::test]
async fn delete_release_rolls_back_when_a_blob_tombstone_is_refused() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    store_test_cover_image(&manager, &release.id).await;

    let cover_blob = manager
        .database
        .row_blob_ref(crate::sync::COVERS_NAMESPACE, &release.id)
        .await
        .unwrap();
    assert!(
        cover_blob.stored().is_none(),
        "no provider is connected, so the cover reached no cloud object"
    );

    manager
        .database
        .delete_release_with_cleanup(
            &release.id,
            &album.id,
            DeleteCleanupPlan {
                blobs_to_tombstone: vec![cover_blob],
                external_refs_to_clear: Vec::new(),
            },
        )
        .await
        .expect_err("tombstoning a blob with no cloud object is refused");

    assert!(manager
        .database
        .find_release_by_id(&release.id)
        .await
        .unwrap()
        .is_some());
}

#[tokio::test]
async fn delete_album_fails_before_rows_are_deleted_when_track_lookup_fails() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    rename_table_for_test(&manager, "tracks", "tracks_unavailable").await;

    let error = manager.delete_album(&album.id).await.unwrap_err();
    assert!(matches!(error, LibraryError::Database(_)));
    assert!(manager
        .database
        .find_album_by_id(&album.id)
        .await
        .unwrap()
        .is_some());
}

#[tokio::test]
async fn delete_album_fails_before_rows_are_deleted_when_file_cleanup_lookup_fails() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    rename_table_for_test(&manager, "release_files", "release_files_unavailable").await;

    let error = manager.delete_album(&album.id).await.unwrap_err();
    assert!(matches!(error, LibraryError::Database(_)));
    assert!(manager
        .database
        .find_album_by_id(&album.id)
        .await
        .unwrap()
        .is_some());
}

/// The playing track's cover reference is what the UI caches its art under, so
/// it has to carry the `covers` row's version, not just the release id — and it
/// has to be absent when the release has no cover at all, rather than naming a
/// row that isn't there.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn playback_track_info_carries_the_cover_version() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    let track = crate::db::DbTrack::new_test(&release.id, TRACK_A, "Track Title", Some(1));
    let track_artist = crate::db::DbTrackArtist::new(
        &track.id,
        &bae_test_support::test_uuid("e36744a5-1a36-460f-891c-e7e558034edf"),
        0,
        Uuid::new_v4().to_string(),
        Utc::now(),
    );
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    manager.database.insert_track(&track).await.unwrap();
    manager
        .database
        .insert_track_artist(&track_artist)
        .await
        .unwrap();

    // No cover row yet: nothing to reference.
    let info = manager.get_playback_track_info(&track.id).await.unwrap();
    assert_eq!(info.cover_image, None);

    store_test_cover_image(&manager, &release.id).await;
    let version = manager
        .database
        .cover_version(&release.id)
        .await
        .unwrap()
        .expect("the stored cover has a version");
    let info = manager.get_playback_track_info(&track.id).await.unwrap();
    assert_eq!(
        info.cover_image,
        Some(crate::album_detail::ImageRef {
            id: release.id.clone(),
            version,
            image_type: LibraryImageType::Cover,
        })
    );
}

#[tokio::test]
async fn playback_info_from_track_release_rejects_missing_album() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    let track = crate::db::DbTrack::new_test(&release.id, TRACK_A, "Track Title", Some(1));
    let track_artist = crate::db::DbTrackArtist::new(
        &track.id,
        &bae_test_support::test_uuid("e36744a5-1a36-460f-891c-e7e558034edf"),
        0,
        Uuid::new_v4().to_string(),
        Utc::now(),
    );
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    manager.database.insert_track(&track).await.unwrap();
    manager
        .database
        .insert_track_artist(&track_artist)
        .await
        .unwrap();

    let mut broken_release = release.clone();
    broken_release.album_id = "missing-album".to_string();

    let error = playback_info_from_track_release(&manager.database, &track, &broken_release)
        .await
        .unwrap_err();
    assert!(
        matches!(error, LibraryError::TrackMapping(message) if message.contains("missing-album"))
    );
}

#[tokio::test]
async fn delete_release_fails_before_rows_are_deleted_when_cover_lookup_fails() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    store_test_cover_image(&manager, &release.id).await;

    rename_table_for_test(&manager, "covers", "covers_unavailable").await;

    let error = manager.delete_release(&release.id).await.unwrap_err();
    assert!(matches!(error, LibraryError::Database(_)));
    assert!(manager
        .database
        .find_release_by_id(&release.id)
        .await
        .unwrap()
        .is_some());
}

/// Deleting a release cascade-deletes its `covers` row (the FK on `covers.id`
/// to `releases`), and the delete path cleans up the cover blob: a Remote
/// release's cover is tombstoned in the cloud and dropped from the cache.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn delete_release_removes_its_cover_image() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud_with_sync_loop(&manager).await;

    // A genuinely Remote release, so storing its cover publishes the blob and
    // there is a real cloud object for the delete to tombstone.
    let release1 = ReleaseRef::of(
        &manager,
        make_remote_release_under_sync_loop(
            &manager,
            &temp_dir.path().join("r1"),
            "Album One",
            false,
        )
        .await,
    )
    .await;
    // A sibling release so the album survives the single-release delete.
    let mut release2 = create_test_release(&release1.album_id);
    release2.remote = false;
    manager.database.insert_release(&release2).await.unwrap();

    // Give release1 a cover: a `covers` row plus its blob in one coven batch.
    manager
        .store_library_image_blob(
            &crate::db::DbLibraryImage {
                id: release1.id.clone(),
                blob_id: bae_test_support::test_uuid(&format!("{}-cover-blob", release1.id)),
                image_type: LibraryImageType::Cover,
                content_type: crate::util::content_type::ContentType::Jpeg,
                file_size: 5,
                width: None,
                height: None,
                source: "local".to_string(),
                source_url: None,
                cloud_path: None,
                content_hash: crate::util::fs::hash_bytes(b"image"),
                created_at: manager.clock.now(),
            },
            b"image",
        )
        .await
        .unwrap();

    wait_for_published_blob(&manager, crate::sync::COVERS_NAMESPACE, &release1.id).await;

    manager.delete_release(&release1.id).await.unwrap();

    // Row removed (the `covers` FK to `releases` cascade-deletes it).
    assert!(manager
        .get_library_image(&release1.id, &LibraryImageType::Cover)
        .await
        .unwrap()
        .is_none());

    // The cover blob's cloud object is tombstoned, named by the namespace and
    // blob id coven queues it under.
    assert!(
        has_queued_delete(
            &manager,
            crate::sync::COVERS_NAMESPACE,
            &bae_test_support::test_uuid(&format!("{}-cover-blob", release1.id)),
        )
        .await,
        "cover blob delete must be enqueued"
    );
}

/// delete_album removes each release's cover too (same helper, second wiring
/// site): the cover row is gone and its blob delete is enqueued.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn delete_album_removes_release_covers() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud_with_sync_loop(&manager).await;
    let release = ReleaseRef::of(
        &manager,
        make_remote_release_under_sync_loop(
            &manager,
            &temp_dir.path().join("r1"),
            "Album One",
            false,
        )
        .await,
    )
    .await;
    manager
        .store_library_image_blob(
            &crate::db::DbLibraryImage {
                id: release.id.clone(),
                blob_id: bae_test_support::test_uuid(&format!("{}-cover-blob", release.id)),
                image_type: LibraryImageType::Cover,
                content_type: crate::util::content_type::ContentType::Jpeg,
                file_size: 5,
                width: None,
                height: None,
                source: "local".to_string(),
                source_url: None,
                cloud_path: None,
                content_hash: crate::util::fs::hash_bytes(b"image"),
                created_at: manager.clock.now(),
            },
            b"image",
        )
        .await
        .unwrap();

    wait_for_published_blob(&manager, crate::sync::COVERS_NAMESPACE, &release.id).await;

    manager.delete_album(&release.album_id).await.unwrap();

    assert!(manager
        .get_library_image(&release.id, &LibraryImageType::Cover)
        .await
        .unwrap()
        .is_none());
    assert!(
        has_queued_delete(
            &manager,
            crate::sync::COVERS_NAMESPACE,
            &bae_test_support::test_uuid(&format!("{}-cover-blob", release.id)),
        )
        .await
    );
}

#[tokio::test]
async fn test_delete_album_deletes_all_releases() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release1 = create_test_release(&album.id);
    let release2 = create_test_release(&album.id);

    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release1).await.unwrap();
    manager.database.insert_release(&release2).await.unwrap();

    manager.delete_album(&album.id).await.unwrap();

    let album_result = manager.database.find_album_by_id(&album.id).await.unwrap();
    assert!(album_result.is_none());
    let releases = manager
        .database
        .get_releases_for_album(&album.id)
        .await
        .unwrap();
    assert!(releases.is_empty());
}
