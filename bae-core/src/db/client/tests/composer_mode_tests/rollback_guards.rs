use super::*;

#[tokio::test]
async fn failed_import_rollback_refuses_a_deletion_plan_that_changed_before_write() {
    let (db, _tmp) = super::temp_db().await;
    let now = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);

    let artist = test_artist(ARTIST_A, "Artist Name A", now);
    db.insert_artist(&artist).await.unwrap();
    let album = test_album(ALBUM_A, "Album Title A", &artist.id, now);
    let release = DbRelease::new_test(&album.id, REL_A);
    db.insert_album_with_release_and_tracks(&album, &release, &[], &[])
        .await
        .unwrap();

    let concurrent_db = db.clone();
    let sibling = DbRelease::new_test(&album.id, REL_B);
    let error = db
        .fail_import_and_delete_release_after_planning_for_test(REL_A, move || async move {
            concurrent_db.insert_release(&sibling).await.unwrap();
        })
        .await
        .expect_err("a changed rollback plan must abort");

    assert!(error.to_string().contains("changed after planning"));
    assert!(db.find_album_by_id(ALBUM_A).await.unwrap().is_some());
    assert!(db.find_release_by_id(REL_A).await.unwrap().is_some());
    assert!(db.find_release_by_id(REL_B).await.unwrap().is_some());
}

/// A failed remote import's cover and artist-image blobs live only in coven's
/// on-device store, since the release never went remote. The rollback deletes
/// the release and unreferenced artist rows atomically. Their earlier inserts
/// remain replay inputs until baseline adoption, so coven retains the exact
/// bytes those inputs need even after the live rows are gone.
#[tokio::test]
async fn fail_import_and_delete_release_retains_replay_owned_image_blobs() {
    let (db, tmp) = super::temp_db().await;
    let now = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);

    // Every row here is named for its own id, so the assertions below can read
    // a subject straight off the name.
    let artist = |id: &str| test_artist(id, id, now);
    db.insert_artist(&artist(ARTIST_EXCLUSIVE)).await.unwrap();
    db.insert_artist(&artist(ARTIST_SHARED)).await.unwrap();

    let album = |id: &str, artist_id: &str| test_album(id, id, artist_id, now);
    let release = |id: &str, album_id: &str| test_release(id, album_id, now);
    let track = |id: &str, release_id: &str| test_track(id, release_id, id, now);

    // A prior surviving album references artist-shared, so the failed
    // import below must keep artist-shared and its image.
    db.insert_album_with_release_and_tracks(
        &album(ALBUM_PRIOR, ARTIST_SHARED),
        &release(RELEASE_PRIOR, ALBUM_PRIOR),
        &[track(TRACK_PRIOR, RELEASE_PRIOR)],
        &[],
    )
    .await
    .unwrap();

    let album_a = album(ALBUM_A, ARTIST_EXCLUSIVE);
    let release_a = release(RELEASE_A, ALBUM_A);
    let (track_files, file_a) = standalone_track_file(
        &tmp,
        track(TRACK_A, RELEASE_A),
        FILE_A,
        "Track A.flac",
        now,
    )
    .await;
    // The failed release also credits artist-shared, so both artists are
    // rollback candidates; only artist-exclusive should be deleted.
    let album_artists = vec![DbAlbumArtist {
        id: AA_SHARED.to_string(),
        album_id: ALBUM_A.to_string(),
        artist_id: ARTIST_SHARED.to_string(),
        position: 1,
        created_at: now,
    }];
    let image = |id: &str, image_type: LibraryImageType| DbLibraryImage {
        id: id.to_string(),
        blob_id: format!("{id}-blob"),
        image_type,
        content_type: ContentType::Jpeg,
        file_size: 3,
        width: None,
        height: None,
        source: "local".to_string(),
        source_url: None,
        cloud_path: None,
        content_hash: crate::util::fs::hash_bytes(&[1u8, 2, 3]),
        created_at: now,
    };
    let cover = image(RELEASE_A, LibraryImageType::Cover);
    let img_exclusive = image(ARTIST_EXCLUSIVE, LibraryImageType::Artist);
    let img_shared = image(ARTIST_SHARED, LibraryImageType::Artist);
    let bytes = [1u8, 2, 3];

    commit_import(
        &db,
        &release_a,
        Commit {
            album: Some(&album_a),
            track_files: &track_files,
            rows: crate::db::ImportRows {
                album_artists: &album_artists,
                ..Default::default()
            },
            files: vec![file_a],
            library_image: Some((&cover, &bytes)),
            artist_images: &[(&img_exclusive, &bytes), (&img_shared, &bytes)],
            primary_release_id: Some((&album_a.id, &release_a.id)),
            ..Default::default()
        },
    )
    .await;

    // The bytes coven holds for each host-provided image, before the rollback.
    let store_dir = coven::StoreDir::new(tmp.path());
    let blob_path = |namespace: &str, blob_id: &str| {
        store_dir
            .local_blob_path(namespace, blob_id)
            .expect("a valid blob path")
    };
    // The fixture's `image` helper derives each blob id from its subject id,
    // so the stored paths are named the same way.
    let cover_blob = blob_path(crate::sync::COVERS_NAMESPACE, &format!("{RELEASE_A}-blob"));
    let exclusive_blob = blob_path(
        crate::sync::ARTIST_IMAGES_NAMESPACE,
        &format!("{ARTIST_EXCLUSIVE}-blob"),
    );
    let shared_blob = blob_path(
        crate::sync::ARTIST_IMAGES_NAMESPACE,
        &format!("{ARTIST_SHARED}-blob"),
    );
    for path in [&cover_blob, &exclusive_blob, &shared_blob] {
        assert!(path.exists(), "finalize stored {}", path.display());
    }

    db.fail_import_and_delete_release(RELEASE_A).await.unwrap();

    // The deleted rows remain in coven's local replay journal until a baseline
    // consumes the matching insert/delete history. Their blob leases keep the
    // exact input bytes available across restart and replay; baseline adoption
    // releases those leases and lets coven complete the recorded cleanup.
    assert!(
        cover_blob.exists(),
        "the failed release's replay input retains its cover blob"
    );
    assert!(
        exclusive_blob.exists(),
        "the swept artist's replay input retains its image blob"
    );
    assert!(
        shared_blob.exists(),
        "the surviving artist still has its image blob"
    );
    assert_eq!(
        db.local_blob_cleanup_intent_count_for_test(
            crate::sync::COVERS_NAMESPACE,
            &format!("{RELEASE_A}-blob"),
        )
        .await
        .unwrap(),
        1,
        "the failed release records eventual cover cleanup"
    );
    assert_eq!(
        db.local_blob_cleanup_intent_count_for_test(
            crate::sync::ARTIST_IMAGES_NAMESPACE,
            &format!("{ARTIST_EXCLUSIVE}-blob"),
        )
        .await
        .unwrap(),
        1,
        "the swept artist records eventual image cleanup"
    );
    assert_eq!(
        db.local_blob_cleanup_intent_count_for_test(
            crate::sync::ARTIST_IMAGES_NAMESPACE,
            &format!("{ARTIST_SHARED}-blob"),
        )
        .await
        .unwrap(),
        0,
        "the surviving artist records no image cleanup"
    );

    // The shared artist and its image row survive; the exclusive one is gone.
    assert!(db.find_artist_by_id(ARTIST_SHARED).await.unwrap().is_some());
    assert!(db
        .find_artist_by_id(ARTIST_EXCLUSIVE)
        .await
        .unwrap()
        .is_none());
}
