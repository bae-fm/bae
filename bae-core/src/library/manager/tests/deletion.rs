#[tokio::test]
async fn work_detail_release_rows_are_display_ready() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.pressing.facts = bae_types::pressing::made_of(bae_types::pressing::Medium::Cd, 1);
    let track = crate::db::DbTrack::new_test(&release.id, TRACK_A, "Track Title", 1);
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
        Catalog::MusicBrainz,
        TRACK_WORK_A.to_string(),
        now,
    );

    manager.database.insert_album(&album).await.unwrap();
    insert_release(&manager, &release).await;
    manager.database.insert_track(&track).await.unwrap();
    manager
        .database
        .insert_composition_fixture_rows(std::slice::from_ref(&work), &[track_work])
        .await
        .unwrap();
    store_test_cover(&manager, &release.id, b"cover").await;

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
    assert_eq!(
        row.name,
        crate::album_detail::ReleaseName::Described {
            year: Some(2024),
            media: bae_types::pressing::made_of(bae_types::pressing::Medium::Cd, 1).media,
        }
    );
    assert_eq!(
        row.media,
        bae_types::pressing::made_of(bae_types::pressing::Medium::Cd, 1).media
    );
    let cover = row.cover.as_ref().expect("work release cover");
    assert_eq!(cover.id, release.id);
    assert!(!cover.version.is_empty());
    assert_eq!(cover.image_type, crate::db::LibraryImageType::Cover);
}

#[tokio::test]
async fn test_delete_release_with_single_release_empties_album() {
    let (manager, _temp_dir, album, release) = manager_with_release().await;

    manager.delete_release(&release.id).await.unwrap();

    let album_result = manager.find_album_detail(&album.id).await.unwrap();
    assert!(album_result.is_none());
    let releases = album_releases(&manager, &album.id).await;
    assert!(releases.is_empty());
}

#[tokio::test]
async fn test_delete_release_with_multiple_releases_preserves_album() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release1 = create_test_release(&album.id);
    let release2 = create_test_release(&album.id);

    manager.database.insert_album(&album).await.unwrap();
    insert_release(&manager, &release1).await;
    insert_release(&manager, &release2).await;

    manager.delete_release(&release1.id).await.unwrap();

    let album_result = find_album(&manager, &album.id).await;
    assert!(album_result.is_some());
    let releases = album_releases(&manager, &album.id).await;
    assert_eq!(releases.len(), 1);
    assert_eq!(releases[0].id, release2.id);
}

/// Deleted rows disappear immediately, offline too; the uploads their files
/// were queued for go with them, and the person's originals stay.
#[tokio::test]
async fn deletion_abandons_queued_file_uploads_and_keeps_the_originals() {
    for delete_album in [false, true] {
        let (manager, temp) = setup_test_manager().await;
        let storage = connect_test_cloud(&manager).await;
        let source = temp.path().join("source");
        let release = insert_release_with_queued_uploads(&manager, &source, "Album", &[("track.flac", b"audio")]).await;
        store_test_cover(&manager, &release.id, b"cover").await;
        storage.set_online(false);
        if delete_album { manager.delete_album(&release.album_id).await.unwrap(); }
        else { manager.delete_release(&release.id).await.unwrap(); }
        assert!(find_release(&manager, &release.id).await.is_none());
        assert_eq!(std::fs::read(source.join("track.flac")).unwrap(), b"audio");
        let snapshot = manager.outbox_state().await.unwrap();
        assert!(snapshot.queue.upload_groups.is_empty());
        assert!(snapshot.queue.other_files.is_empty());
    }
}

/// A closed store refuses deletion; reopening it proves no rows were changed.
#[tokio::test]
async fn deletion_refuses_an_unavailable_database_without_losing_rows() {
    for delete_album in [false, true] {
        let (manager, _temp, album, release) = manager_with_release().await;
        store_test_cover(&manager, &release.id, b"cover").await;
        let before = manager.database.every_row_for_test().await.unwrap();
        let id = manager.database.store_id();
        manager.database.close().await.unwrap();
        let error = if delete_album {
            manager.delete_album(&album.id).await.unwrap_err()
        } else {
            manager.delete_release(&release.id).await.unwrap_err()
        };
        assert!(matches!(error, LibraryError::Database(_)), "{error}");
        let handle = crate::library::configure_store(
            manager.installation.builder(manager.app_dir.store_layout())
        ).open(id).await.unwrap();
        let reopened = Database::from_handle(handle, manager.clock.clone());
        assert_eq!(reopened.every_row_for_test().await.unwrap(), before);
        reopened.close().await.unwrap();
    }
}

/// A track's display carries the cover reference the UI caches its art under,
/// so it has to carry the `covers` row's version, not just the release id — and
/// it has to be absent when the release has no cover at all, rather than naming
/// a row that isn't there.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn track_display_carries_the_cover_version() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    let track = crate::db::DbTrack::new_test(&release.id, TRACK_A, "Track Title", 1);
    let track_artist = crate::db::DbTrackArtist::new(
        &track.id,
        &bae_test_support::test_uuid("e36744a5-1a36-460f-891c-e7e558034edf"),
        0,
        Utc::now(),
    );
    manager.database.insert_album(&album).await.unwrap();
    insert_release(&manager, &release).await;
    manager.database.insert_track(&track).await.unwrap();
    manager
        .database
        .insert_track_artist(&track_artist)
        .await
        .unwrap();

    // No cover row yet: nothing to reference.
    let display = manager.get_track_display(&track.id).await.unwrap();
    assert_eq!(display.cover_image, None);

    store_test_cover(&manager, &release.id, b"cover").await;
    let version = manager
        .database
        .cover_version(&release.id)
        .await
        .unwrap()
        .expect("the stored cover has a version");
    let display = manager.get_track_display(&track.id).await.unwrap();
    assert_eq!(
        display.cover_image,
        Some(crate::album_detail::ImageRef {
            id: release.id.clone(),
            version,
            image_type: LibraryImageType::Cover,
        })
    );
}

/// Deleting a release cascade-deletes its `covers` row (the FK on `covers.id`
/// to `releases`), including a cover whose file really did reach the cloud —
/// the delete plan reads the published cover's file reference before the row
/// goes, so a lookup failure there would take the delete down with it.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn delete_release_removes_its_cover_image() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud_with_sync_loop(&manager).await;

    // A genuinely Remote release, so storing its cover publishes the file and
    // the delete runs against a cover that names a real cloud object.
    let release1 = ReleaseRef::of(
        &manager,
        upload_release_under_sync_loop(
            &manager,
            &temp_dir.path().join("r1"),
            "Album One",
            false,
        )
        .await,
    )
    .await;
    // A sibling release so the album survives the single-release delete.
    let release2 = create_test_release(&release1.album_id);
    insert_release(&manager, &release2).await;

    // Give release1 a cover: a `covers` row plus its file in one coven batch.
    store_test_cover(&manager, &release1.id, b"image").await;

    wait_for_published_file(&manager, crate::sync::COVERS_NAMESPACE, &release1.id).await;

    manager.delete_release(&release1.id).await.unwrap();

    // Row removed (the `covers` FK to `releases` cascade-deletes it).
    assert!(manager
        .get_library_image(&release1.id, &LibraryImageType::Cover)
        .await
        .unwrap()
        .is_none());
}

/// delete_album removes each release's cover too (same helper, second wiring
/// site): the cover row is gone with its release.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn delete_album_removes_release_covers() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud_with_sync_loop(&manager).await;
    let release = ReleaseRef::of(
        &manager,
        upload_release_under_sync_loop(
            &manager,
            &temp_dir.path().join("r1"),
            "Album One",
            false,
        )
        .await,
    )
    .await;
    store_test_cover(&manager, &release.id, b"image").await;

    wait_for_published_file(&manager, crate::sync::COVERS_NAMESPACE, &release.id).await;

    manager.delete_album(&release.album_id).await.unwrap();

    assert!(manager
        .get_library_image(&release.id, &LibraryImageType::Cover)
        .await
        .unwrap()
        .is_none());
}

/// A delete is planned against the release's rows and files before its write
/// opens. A cover replaced in between is a change the plan never saw, so the
/// write checks the plan against the rows it deletes and refuses a stale one.
#[tokio::test]
async fn delete_release_refuses_a_plan_whose_cover_changed() {
    let (manager, _temp_dir, _album, release) = manager_with_release().await;
    store_test_cover(&manager, &release.id, b"cover").await;
    let plan = manager
        .database
        .plan_release_deletion(&release.id)
        .await
        .unwrap();
    store_test_cover(&manager, &release.id, b"replacement cover").await;

    let error = manager
        .database
        .delete_release_with_cleanup(plan)
        .await
        .expect_err("a stale delete plan is refused");

    assert!(error.to_string().contains("changed after planning"));
    assert!(find_release(&manager, &release.id).await.is_some());
}

#[tokio::test]
async fn test_delete_album_deletes_all_releases() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release1 = create_test_release(&album.id);
    let release2 = create_test_release(&album.id);

    manager.database.insert_album(&album).await.unwrap();
    insert_release(&manager, &release1).await;
    insert_release(&manager, &release2).await;

    manager.delete_album(&album.id).await.unwrap();

    let album_result = manager.find_album_detail(&album.id).await.unwrap();
    assert!(album_result.is_none());
    let releases = album_releases(&manager, &album.id).await;
    assert!(releases.is_empty());
}

/// Every deletion reaches the player, however many land before it reads them:
/// one it missed would leave deleted tracks in its queue.
#[tokio::test]
async fn every_track_deletion_reaches_a_reader_that_fell_behind() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let mut deletions = manager.subscribe_track_deletions();
    let deleted = 2_048;
    for index in 0..deleted {
        manager.tracks_deleted(vec![format!("track-{index}")]);
    }

    let mut heard = Vec::new();
    while let Ok(track_ids) = deletions.try_recv() {
        heard.extend(track_ids);
    }
    assert_eq!(heard.len(), deleted);
    assert_eq!(heard.first().map(String::as_str), Some("track-0"));
}
