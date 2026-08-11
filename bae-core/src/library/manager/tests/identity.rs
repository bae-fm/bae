/// Two release ids whose lexical order is fixed, for the `r.id` tiebreaker.
const TIEBREAK_LO: &str = "066483e0-f9fc-4636-865d-08c069510b2e";
const TIEBREAK_HI: &str = "2153cb27-8335-4523-ae52-be2d6f577ba3";

#[tokio::test]
async fn storage_page_id_tiebreaker_stable_across_pages() {
    let (manager, _temp_dir) = setup_test_manager().await;

    // Two releases sharing album title + created_at — the ORDER BY clause
    // falls through to the `r.id` tiebreaker. The ids are canonical UUIDs
    // (coven takes no other shape on a synced row) chosen so LO sorts first.
    let now = Utc::now();
    let mut album = create_test_album();
    album.title = "Same Title".to_string();
    manager.database.insert_album(&album).await.unwrap();
    let mut release_a = create_test_release(&album.id);
    release_a.id = TIEBREAK_LO.to_string();
    release_a.created_at = now;
    let mut release_b = create_test_release(&album.id);
    release_b.id = TIEBREAK_HI.to_string();
    release_b.created_at = now;
    manager.database.insert_release(&release_a).await.unwrap();
    manager.database.insert_release(&release_b).await.unwrap();

    let sort = crate::db::StorageSortCriterion {
        field: crate::db::StorageSortField::AlbumTitle,
        direction: crate::db::SortDirection::Ascending,
    };
    let first_page = manager
        .get_storage_page(&sort, crate::db::StorageFilter::All, 0, 1)
        .await
        .unwrap();
    let second_page = manager
        .get_storage_page(&sort, crate::db::StorageFilter::All, 1, 1)
        .await
        .unwrap();

    assert_eq!(first_page.rows.len(), 1);
    assert_eq!(second_page.rows.len(), 1);
    assert_eq!(first_page.rows[0].release.id, TIEBREAK_LO);
    assert_eq!(second_page.rows[0].release.id, TIEBREAK_HI);
}

// ── set_identity ───────────────────────────────────────────────────

fn mb_identity(group: &str, release: Option<&str>) -> crate::import::ReleaseIdentity {
    crate::import::ReleaseIdentity {
        source: crate::import::MetadataSource::MusicBrainz,
        source_group_id: group.to_string(),
        source_release_id: release.map(|s| s.to_string()),
    }
}

fn discogs_identity(group: &str, release: Option<&str>) -> crate::import::ReleaseIdentity {
    crate::import::ReleaseIdentity {
        source: crate::import::MetadataSource::Discogs,
        source_group_id: group.to_string(),
        source_release_id: release.map(|s| s.to_string()),
    }
}

#[tokio::test]
async fn set_identity_to_unknown_moves_release_to_fresh_album() {
    let (manager, _temp_dir) = setup_test_manager().await;

    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.metadata_source = crate::db::ReleaseMetadataSource::MusicBrainz;
    release.metadata_source_release_id = Some("mb-rel-1".to_string());

    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    manager
        .database
        .insert_release_identities(&release.id, &[mb_identity("g1", Some("mb-rel-1"))])
        .await
        .unwrap();

    manager
        .set_identity(
            &release.id,
            vec![],
            crate::import::MetadataPointer::FileTags,
        )
        .await
        .unwrap();

    // The original album was a one-release album → deleted now.
    assert!(manager
        .database
        .find_album_by_id(&album.id)
        .await
        .unwrap()
        .is_none());

    // Release moved to a brand-new album, holds nothing else.
    let new_album_id = manager
        .database
        .find_album_id_for_release(&release.id)
        .await
        .unwrap()
        .unwrap();
    assert_ne!(new_album_id, album.id);
    let siblings = manager
        .database
        .get_releases_for_album(&new_album_id)
        .await
        .unwrap();
    assert_eq!(siblings.len(), 1);
    assert_eq!(siblings[0].id, release.id);

    // Identity rows wiped, metadata source flipped to file_tags.
    let identities = manager
        .database
        .get_release_identities(&release.id)
        .await
        .unwrap();
    assert!(identities.is_empty());
    let updated = manager
        .database
        .find_release_by_id(&release.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        updated.metadata_source,
        crate::db::ReleaseMetadataSource::FileTags
    );
    assert_eq!(updated.metadata_source_release_id, None);
}

#[tokio::test]
async fn set_identity_replaces_rows_when_new_identity_fits_current_album() {
    let (manager, _temp_dir) = setup_test_manager().await;

    // Album has two releases, both Approximate-MB on group g1.
    let album = create_test_album();
    let release1 = create_test_release(&album.id);
    let release2 = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release1).await.unwrap();
    manager.database.insert_release(&release2).await.unwrap();
    manager
        .database
        .insert_release_identities(&release1.id, &[mb_identity("g1", None)])
        .await
        .unwrap();
    manager
        .database
        .insert_release_identities(&release2.id, &[mb_identity("g1", None)])
        .await
        .unwrap();

    // Promote release1 from Approximate to Exact within g1. New row
    // still agrees with release2's group, so release1 stays put.
    manager
        .set_identity(
            &release1.id,
            vec![mb_identity("g1", Some("mb-rel-99"))],
            crate::import::MetadataPointer::External {
                source: crate::import::MetadataSource::MusicBrainz,
                release_id: "mb-rel-99".to_string(),
            },
        )
        .await
        .unwrap();

    let landing_album_id = manager
        .database
        .find_album_id_for_release(&release1.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(landing_album_id, album.id);

    let identities = manager
        .database
        .get_release_identities(&release1.id)
        .await
        .unwrap();
    assert_eq!(identities.len(), 1);
    assert_eq!(identities[0].source_group_id, "g1");
    assert_eq!(
        identities[0].source_release_id.as_deref(),
        Some("mb-rel-99")
    );

    let updated = manager
        .database
        .find_release_by_id(&release1.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        updated.metadata_source,
        crate::db::ReleaseMetadataSource::MusicBrainz
    );
    assert_eq!(
        updated.metadata_source_release_id.as_deref(),
        Some("mb-rel-99"),
    );

    // Source album still holds both releases.
    let siblings = manager
        .database
        .get_releases_for_album(&album.id)
        .await
        .unwrap();
    assert_eq!(siblings.len(), 2);
}

#[tokio::test]
async fn set_identity_creates_new_album_when_no_existing_album_fits() {
    let (manager, _temp_dir) = setup_test_manager().await;

    // Two albums, neither matching the new MB group g2.
    let album_a = create_test_album();
    let mut album_b = create_test_album();
    album_b.title = "Other Album".to_string();
    manager.database.insert_album(&album_a).await.unwrap();
    manager.database.insert_album(&album_b).await.unwrap();

    let release_alpha = create_test_release(&album_a.id);
    let release_beta = create_test_release(&album_a.id);
    let release_other = create_test_release(&album_b.id);
    manager
        .database
        .insert_release(&release_alpha)
        .await
        .unwrap();
    manager
        .database
        .insert_release(&release_beta)
        .await
        .unwrap();
    manager
        .database
        .insert_release(&release_other)
        .await
        .unwrap();
    manager
        .database
        .insert_release_identities(&release_alpha.id, &[mb_identity("g1", None)])
        .await
        .unwrap();
    manager
        .database
        .insert_release_identities(&release_beta.id, &[mb_identity("g1", None)])
        .await
        .unwrap();
    manager
        .database
        .insert_release_identities(&release_other.id, &[mb_identity("g3", None)])
        .await
        .unwrap();

    // release_alpha takes on a brand-new MB group (g2). Its current
    // album (album_a) holds release_beta on g1, so it can't stay.
    // No other album holds g2 either → fresh album.
    manager
        .set_identity(
            &release_alpha.id,
            vec![mb_identity("g2", None)],
            crate::import::MetadataPointer::External {
                source: crate::import::MetadataSource::MusicBrainz,
                release_id: "mb-rel-g2".to_string(),
            },
        )
        .await
        .unwrap();

    let landing_album_id = manager
        .database
        .find_album_id_for_release(&release_alpha.id)
        .await
        .unwrap()
        .unwrap();
    assert_ne!(landing_album_id, album_a.id);
    assert_ne!(landing_album_id, album_b.id);

    // Source album loses release_alpha but keeps release_beta.
    let source_siblings = manager
        .database
        .get_releases_for_album(&album_a.id)
        .await
        .unwrap();
    assert_eq!(source_siblings.len(), 1);
    assert_eq!(source_siblings[0].id, release_beta.id);
}

#[tokio::test]
async fn set_identity_moves_release_to_matching_album() {
    let (manager, _temp_dir) = setup_test_manager().await;

    // Source album (album_a) carries release_alpha solo at MB g1.
    // Target album (album_b) carries release_other at MB g2 — that
    // matches the new identity we'll set on release_alpha.
    let album_a = create_test_album();
    let mut album_b = create_test_album();
    album_b.title = "Other Album".to_string();
    manager.database.insert_album(&album_a).await.unwrap();
    manager.database.insert_album(&album_b).await.unwrap();

    let release_alpha = create_test_release(&album_a.id);
    let release_other = create_test_release(&album_b.id);
    manager
        .database
        .insert_release(&release_alpha)
        .await
        .unwrap();
    manager
        .database
        .insert_release(&release_other)
        .await
        .unwrap();
    manager
        .database
        .insert_release_identities(&release_alpha.id, &[mb_identity("g1", None)])
        .await
        .unwrap();
    manager
        .database
        .insert_release_identities(&release_other.id, &[mb_identity("g2", None)])
        .await
        .unwrap();

    manager
        .set_identity(
            &release_alpha.id,
            vec![mb_identity("g2", Some("mb-rel-pressing"))],
            crate::import::MetadataPointer::External {
                source: crate::import::MetadataSource::MusicBrainz,
                release_id: "mb-rel-pressing".to_string(),
            },
        )
        .await
        .unwrap();

    // release_alpha now lives in album_b alongside release_other.
    let landing_album_id = manager
        .database
        .find_album_id_for_release(&release_alpha.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(landing_album_id, album_b.id);
    let target_siblings = manager
        .database
        .get_releases_for_album(&album_b.id)
        .await
        .unwrap();
    assert_eq!(target_siblings.len(), 2);

    // album_a was a single-release album → deleted now.
    assert!(manager
        .database
        .find_album_by_id(&album_a.id)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn set_identity_keeps_vacated_album_when_other_releases_remain() {
    let (manager, _temp_dir) = setup_test_manager().await;

    // album_a holds two releases, both on MB g1. Move release_alpha
    // out by giving it a different group.
    let album_a = create_test_album();
    manager.database.insert_album(&album_a).await.unwrap();
    let release_alpha = create_test_release(&album_a.id);
    let release_beta = create_test_release(&album_a.id);
    manager
        .database
        .insert_release(&release_alpha)
        .await
        .unwrap();
    manager
        .database
        .insert_release(&release_beta)
        .await
        .unwrap();
    manager
        .database
        .insert_release_identities(&release_alpha.id, &[mb_identity("g1", None)])
        .await
        .unwrap();
    manager
        .database
        .insert_release_identities(&release_beta.id, &[mb_identity("g1", None)])
        .await
        .unwrap();

    manager
        .set_identity(
            &release_alpha.id,
            vec![mb_identity("g2", None)],
            crate::import::MetadataPointer::External {
                source: crate::import::MetadataSource::MusicBrainz,
                release_id: "mb-rel-g2".to_string(),
            },
        )
        .await
        .unwrap();

    // album_a still exists, holds release_beta only.
    let surviving = manager
        .database
        .find_album_by_id(&album_a.id)
        .await
        .unwrap();
    assert!(surviving.is_some());
    let surviving_releases = manager
        .database
        .get_releases_for_album(&album_a.id)
        .await
        .unwrap();
    assert_eq!(surviving_releases.len(), 1);
    assert_eq!(surviving_releases[0].id, release_beta.id);
}

#[tokio::test]
async fn set_identity_does_not_touch_metadata_columns() {
    let (manager, _temp_dir) = setup_test_manager().await;

    let mut album = create_test_album();
    album.title = "Initial Title".to_string();
    album.year = Some(1999);
    manager.database.insert_album(&album).await.unwrap();

    let mut release = create_test_release(&album.id);
    release.pressing.format = Some("Vinyl".to_string());
    release.pressing.label = Some("My Label".to_string());
    release.pressing.catalog_number = Some("CAT-123".to_string());
    release.pressing.country = Some("US".to_string());
    release.pressing.barcode = Some("1234567890".to_string());
    release.pressing.year = Some(1999);
    manager.database.insert_release(&release).await.unwrap();
    manager
        .database
        .insert_release_identities(&release.id, &[mb_identity("g1", Some("mb-rel-1"))])
        .await
        .unwrap();

    // Insert a track too — we want to verify it survives.
    let track = crate::db::DbTrack {
        id: Uuid::new_v4().to_string(),
        release_id: release.id.clone(),
        title: "My Track".to_string(),
        side: 1,
        track_number: Some(1),
        duration_ms: Some(180_000),
        discogs_position: None,
        created_at: Utc::now(),
    };
    manager.database.insert_track(&track).await.unwrap();

    manager
        .set_identity(
            &release.id,
            vec![discogs_identity("dg1", Some("dg-rel-1"))],
            crate::import::MetadataPointer::External {
                source: crate::import::MetadataSource::Discogs,
                release_id: "dg-rel-1".to_string(),
            },
        )
        .await
        .unwrap();

    // Pressing fields untouched.
    let after = manager
        .database
        .find_release_by_id(&release.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after.pressing.format.as_deref(), Some("Vinyl"));
    assert_eq!(after.pressing.label.as_deref(), Some("My Label"));
    assert_eq!(after.pressing.catalog_number.as_deref(), Some("CAT-123"));
    assert_eq!(after.pressing.country.as_deref(), Some("US"));
    assert_eq!(after.pressing.barcode.as_deref(), Some("1234567890"));
    assert_eq!(after.pressing.year, Some(1999));

    // Album-level fields untouched (still in the same album, since
    // both old and new identities are in the only release).
    let after_album = manager
        .database
        .find_album_by_id(&after.album_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after_album.title, "Initial Title");
    assert_eq!(after_album.year, Some(1999));

    // Track survived.
    let tracks = manager
        .database
        .get_tracks_for_release(&release.id)
        .await
        .unwrap();
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].title, "My Track");
}

#[tokio::test]
async fn set_identity_to_fresh_album_preserves_album_artists() {
    let (manager, _temp_dir) = setup_test_manager().await;

    // Two extra artists so the album carries multiple album_artists
    // rows beyond the primary (which lives on `albums.artist_id`).
    let primary = DbArtist {
        id: "755ab566-9e71-4a7f-88df-fc5f573f882f".to_string(),
        name: "Primary".to_string(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: None,
        created_at: Utc::now(),
    };
    let secondary = DbArtist {
        id: "1d4f0221-7e2b-4e87-8376-93eaf8998bd7".to_string(),
        name: "Secondary".to_string(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: None,
        created_at: Utc::now(),
    };
    manager.database.insert_artist(&primary).await.unwrap();
    manager.database.insert_artist(&secondary).await.unwrap();

    // album_a holds release_alpha and release_beta on g1, with both
    // primary + secondary as album artists. We're going to move
    // release_alpha out via a non-fitting identity, forcing the
    // creation of a fresh album.
    let mut album_a = create_test_album();
    album_a.artist_id = primary.id.clone();
    manager.database.insert_album(&album_a).await.unwrap();
    manager
        .database
        .insert_album_artist(&DbAlbumArtist::new(
            &album_a.id,
            &primary.id,
            0,
            Uuid::new_v4().to_string(),
            Utc::now(),
        ))
        .await
        .unwrap();
    manager
        .database
        .insert_album_artist(&DbAlbumArtist::new(
            &album_a.id,
            &secondary.id,
            1,
            Uuid::new_v4().to_string(),
            Utc::now(),
        ))
        .await
        .unwrap();

    let release_alpha = create_test_release(&album_a.id);
    let release_beta = create_test_release(&album_a.id);
    manager
        .database
        .insert_release(&release_alpha)
        .await
        .unwrap();
    manager
        .database
        .insert_release(&release_beta)
        .await
        .unwrap();
    manager
        .database
        .insert_release_identities(&release_alpha.id, &[mb_identity("g1", None)])
        .await
        .unwrap();
    manager
        .database
        .insert_release_identities(&release_beta.id, &[mb_identity("g1", None)])
        .await
        .unwrap();

    // release_alpha takes a different group → can't stay in album_a
    // (g1 disagrees with g2), no other album holds g2 → fresh album.
    manager
        .set_identity(
            &release_alpha.id,
            vec![mb_identity("g2", None)],
            crate::import::MetadataPointer::External {
                source: crate::import::MetadataSource::MusicBrainz,
                release_id: "mb-rel-g2".to_string(),
            },
        )
        .await
        .unwrap();

    let new_album_id = manager
        .database
        .find_album_id_for_release(&release_alpha.id)
        .await
        .unwrap()
        .unwrap();
    assert_ne!(new_album_id, album_a.id);

    // The fresh album carries the same album_artists as the source.
    // get_artists_for_album joins both the primary (via albums.artist_id)
    // and album_artists rows, ordered by position.
    let new_album_artists = manager
        .database
        .get_artists_for_album(&new_album_id)
        .await
        .unwrap();
    let names: Vec<&str> = new_album_artists.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, vec!["Primary", "Primary", "Secondary"]);
}

#[tokio::test]
async fn set_identity_clears_primary_when_it_pointed_at_moved_release() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let now = Utc::now();
    let beta_track_id = TRACK_BETA.to_string();

    // album_a carries two releases on g1 and points
    // primary_release_id at release_alpha. Move release_alpha out.
    // The chosen release is gone, so primary_release_id becomes NULL
    // and the read path falls back to the remaining release_beta.
    let album_a = create_test_album();
    manager.database.insert_album(&album_a).await.unwrap();
    let release_alpha = create_test_release(&album_a.id);
    let release_beta = create_test_release(&album_a.id);

    manager
        .database
        .insert_release(&release_alpha)
        .await
        .unwrap();
    manager
        .database
        .insert_release(&release_beta)
        .await
        .unwrap();

    // A track on release_beta so the read-path resolution below has an
    // identifiable target: the fallback should surface beta's tracks.
    let beta_track = crate::db::DbTrack {
        id: beta_track_id.clone(),
        release_id: release_beta.id.clone(),
        title: "Track Title".to_string(),
        side: 1,
        track_number: Some(1),
        duration_ms: Some(180_000),
        discogs_position: None,
        created_at: now,
    };
    manager.database.insert_track(&beta_track).await.unwrap();

    // Point album_a.primary_release_id at release_alpha — the
    // release we're about to move out.
    manager
        .database
        .set_album_primary_release(&album_a.id, &release_alpha.id)
        .await
        .unwrap();

    manager
        .database
        .insert_release_identities(&release_alpha.id, &[mb_identity("g1", None)])
        .await
        .unwrap();
    manager
        .database
        .insert_release_identities(&release_beta.id, &[mb_identity("g1", None)])
        .await
        .unwrap();

    manager
        .set_identity(
            &release_alpha.id,
            vec![mb_identity("g2", None)],
            crate::import::MetadataPointer::External {
                source: crate::import::MetadataSource::MusicBrainz,
                release_id: "mb-rel-g2".to_string(),
            },
        )
        .await
        .unwrap();

    // album_a survives; its primary_release_id is cleared to NULL now
    // that the release it pointed at has left.
    let surviving_album = manager
        .database
        .find_album_by_id(&album_a.id)
        .await
        .unwrap()
        .expect("album should still exist with release_beta");
    assert_eq!(
        surviving_album.primary_release_id, None,
        "primary_release_id should be cleared when its release moves out",
    );

    // Read path falls back to the first remaining release: album_a now
    // resolves its primary to release_beta.
    let resolved_track_ids = manager
        .database
        .get_primary_release_track_ids_for_album(&album_a.id)
        .await
        .unwrap();
    assert_eq!(
        resolved_track_ids,
        Some(vec![beta_track_id.clone()]),
        "read path should resolve the cleared primary to release_beta",
    );
}

#[tokio::test]
async fn set_identity_atomic_rechecks_source_count_inside_transaction() {
    // The TOCTOU window: a separate writer lands a release into the source album
    // between `set_identity`'s pre-flight read and its atomic call. Drive the atomic
    // API directly with `current_album_id` at the source album, after seeding an
    // extra release into it. The atomic call must NOT delete the source — its
    // in-transaction recheck sees the surviving release.
    let (manager, _temp_dir) = setup_test_manager().await;

    let album_a = create_test_album();
    manager.database.insert_album(&album_a).await.unwrap();

    let release_alpha = create_test_release(&album_a.id);
    manager
        .database
        .insert_release(&release_alpha)
        .await
        .unwrap();
    manager
        .database
        .insert_release_identities(&release_alpha.id, &[mb_identity("g1", None)])
        .await
        .unwrap();

    // Build the fresh-album row the manager would have produced —
    // we're driving the atomic API by hand.
    let now = chrono::Utc::now();
    let fresh_album = DbAlbum {
        id: Uuid::new_v4().to_string(),
        title: album_a.title.clone(),
        artist_id: album_a.artist_id.clone(),
        year: album_a.year,
        primary_release_id: None,
        is_compilation: album_a.is_compilation,
        created_at: now,
    };

    // Race window: another writer lands release_intruder into
    // album_a after the (hypothetical) pre-flight read but before
    // the atomic call.
    let release_intruder = create_test_release(&album_a.id);
    manager
        .database
        .insert_release(&release_intruder)
        .await
        .unwrap();

    let outcome = manager
        .database
        .set_identity_atomic(
            &release_alpha.id,
            &[mb_identity("g2", None)],
            crate::db::ReleaseMetadataSource::MusicBrainz,
            Some("mb-rel-g2"),
            &album_a.id,
            &fresh_album.id,
            Some(&fresh_album),
        )
        .await
        .unwrap();

    assert!(
        !outcome.source_album_deleted,
        "atomic recheck must protect the late-arriving release"
    );

    // Source album survives, holding only release_intruder.
    let survivors = manager
        .database
        .get_releases_for_album(&album_a.id)
        .await
        .unwrap();
    assert_eq!(survivors.len(), 1);
    assert_eq!(survivors[0].id, release_intruder.id);

    // release_alpha landed in the fresh album.
    let fresh_releases = manager
        .database
        .get_releases_for_album(&fresh_album.id)
        .await
        .unwrap();
    assert_eq!(fresh_releases.len(), 1);
    assert_eq!(fresh_releases[0].id, release_alpha.id);
}

// ── re_identify_release ────────────────────────────────────────────
//
// Exact / Approximate fetch through MB / Discogs, so these tests seed the release
// cache and the cover-art lookups first and `prepare_release` reads locally
// instead of hitting the network. The Unknown path makes no source claim, so it
// needs no seeding.

/// The archived documents under one source release's own key.
async fn archived_for(
    manager: &LibraryManager,
    source: crate::import::PayloadSource,
    source_release_id: &str,
) -> Option<String> {
    manager
        .database
        .load_source_release_payloads(&[(source, source_release_id.to_string())])
        .await
        .unwrap()
        .remove(&(source, source_release_id.to_string()))
}

#[tokio::test]
async fn re_identify_to_unknown_clears_identities_and_moves_album() {
    use lofty::config::WriteOptions;
    use lofty::prelude::*;
    use lofty::tag::{Tag, TagType};
    use std::fs;

    let (manager, _temp_dir) = setup_test_manager().await;

    // Local audio files so the post-`set_identity` reseed can read tags.
    let media = TempDir::new().unwrap();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("flac");
    let mut filenames = Vec::new();
    for (name, title) in [("01.flac", "Tag One"), ("02.flac", "Tag Two")] {
        let dest = media.path().join(name);
        fs::copy(fixtures.join("01 Test Track 1.flac"), &dest).unwrap();
        let mut tagged = lofty::read_from_path(&dest).unwrap();
        let mut tag = Tag::new(TagType::VorbisComments);
        tag.set_title(title.to_string());
        tag.set_artist("Tag Artist".to_string());
        tag.set_album("Tag Album".to_string());
        tagged.insert_tag(tag);
        tagged.save_to_path(&dest, WriteOptions::default()).unwrap();
        filenames.push(name.to_string());
    }

    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.metadata_source = crate::db::ReleaseMetadataSource::MusicBrainz;
    release.metadata_source_release_id = Some("mb-rel-1".to_string());
    release.remote = false;

    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    // Two existing track rows align positionally with the two files.
    insert_n_tracks(&manager.database, &release.id, 2).await;
    let now = Utc::now();
    for name in &filenames {
        let file = crate::db::DbFile::new(
            &release.id,
            name,
            0,
            crate::util::content_type::ContentType::Flac,
            Uuid::new_v4().to_string(),
            now,
            crate::util::fs::hash_bytes(b"fixture"),
        );
        manager.database.insert_file(&file).await.unwrap();
    }
    // Register the files as coven external refs (in-place files of a Local
    // release) AFTER inserting them, so the file-tag re-read resolves paths.
    manager
        .database
        .register_release_external_refs_for_test(&release.id, &media.path().to_string_lossy())
        .await
        .unwrap();
    manager
        .database
        .insert_release_identities(&release.id, &[mb_identity("g1", Some("mb-rel-1"))])
        .await
        .unwrap();

    // The source release this one was seeded from has an archived document.
    manager
        .database
        .save_source_release_payloads(&[crate::db::DbSourceReleasePayload {
            source: crate::import::PayloadSource::MusicBrainz,
            source_release_id: "mb-rel-1".to_string(),
            json: r#"{"id":"mb-rel-1"}"#.to_string(),
            fetched_at: Utc::now(),
        }])
        .await
        .unwrap();

    manager
        .re_identify_release(&release.id, crate::import::IdentityChoice::Unknown)
        .await
        .unwrap();

    // Original (single-release) album is gone; release sits on a
    // fresh one.
    assert!(manager
        .database
        .find_album_by_id(&album.id)
        .await
        .unwrap()
        .is_none());
    let new_album_id = manager
        .database
        .find_album_id_for_release(&release.id)
        .await
        .unwrap()
        .unwrap();
    assert_ne!(new_album_id, album.id);

    // Identity rows wiped, metadata pointer flipped to file_tags.
    let identities = manager
        .database
        .get_release_identities(&release.id)
        .await
        .unwrap();
    assert!(identities.is_empty());
    let updated = manager
        .database
        .find_release_by_id(&release.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        updated.metadata_source,
        crate::db::ReleaseMetadataSource::FileTags
    );
    assert_eq!(updated.metadata_source_release_id, None);
    // The archived document describes `mb-rel-1`, not this release, and is
    // shared with every candidate that matched it. Dropping the pointer is what
    // stops it being read here; nothing deletes it.
    assert!(
        archived_for(
            &manager,
            crate::import::PayloadSource::MusicBrainz,
            "mb-rel-1"
        )
        .await
        .is_some(),
        "documents are keyed by the source release, so re-pointing must not delete them"
    );
}

// ── re_identify_release Exact / Approximate (MB cache-seeded) ────
//
// Drive the network-side `prepare_release` through the MB LRU cache
// (`seed_release_cache` + `seed_release_group_json_cache`) and the cover-art
// client's own so these tests don't hit the network. The caches are
// process-global LRUs, so each test uses a unique MB release ID and no other
// test's seed bleeds in.

/// Build a synthetic MB release response with `n` track rows on a
/// single CD medium, plus a release group reference. Suitable for
/// driving `prepare_release` via cache seeding.
fn make_mb_release_for_re_identify(
    release_id: &str,
    release_group_id: &str,
    track_count: usize,
) -> crate::musicbrainz::MbReleaseResponse {
    use crate::musicbrainz::{
        MbArtistCredit, MbArtistRef, MbMedium, MbReleaseGroupRef, MbReleaseResponse, MbTrack,
    };
    MbReleaseResponse {
        id: release_id.to_string(),
        title: "Album Title".to_string(),
        date: Some("2024-01-01".to_string()),
        country: None,
        barcode: None,
        artist_credit: vec![MbArtistCredit {
            name: "Artist Name".to_string(),
            artist: Some(MbArtistRef {
                id: Some("mb-artist-1".to_string()),
                name: Some("Artist Name".to_string()),
                sort_name: Some("Artist Name".to_string()),
            }),
        }],
        release_group: Some(MbReleaseGroupRef {
            id: release_group_id.to_string(),
            first_release_date: Some("2024-01-01".to_string()),
            relations: Some(vec![]),
        }),
        label_info: vec![],
        media: vec![MbMedium {
            format: Some("CD".to_string()),
            tracks: (1..=track_count)
                .map(|n| MbTrack {
                    position: Some(n as i64),
                    number: Some(n.to_string()),
                    title: Some(format!("Track {n}")),
                    length: None,
                    recording: None,
                    artist_credit: vec![],
                })
                .collect(),
        }],
        relations: vec![],
        cover_art_archive: crate::musicbrainz::MbCoverArtArchive {
            front: false,
            darkened: false,
        },
    }
}

/// Insert `n` plain track rows for a release. Mirrors the row shape
/// `prepared.parsed.tracks` would produce so the track-count check
/// in `re_identify_release` accepts the picked release.
async fn insert_n_tracks(database: &Database, release_id: &str, n: usize) {
    for i in 1..=n {
        let track = crate::db::DbTrack {
            id: Uuid::new_v4().to_string(),
            release_id: release_id.to_string(),
            title: format!("Track {i}"),
            side: 1,
            track_number: Some(i as i32),
            duration_ms: None,
            discogs_position: None,
            created_at: Utc::now(),
        };
        database.insert_track(&track).await.unwrap();
    }
}

#[tokio::test]
async fn re_identify_release_exact_archives_the_picked_release() {
    use crate::import::{IdentityChoice, MetadataRef, MetadataSource};
    use crate::musicbrainz::{seed_release_cache, seed_release_group_json_cache};

    let (manager, _temp_dir) = setup_test_manager().await;

    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.metadata_source = crate::db::ReleaseMetadataSource::MusicBrainz;
    release.metadata_source_release_id = Some("mb-rel-old".to_string());

    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    manager
        .database
        .insert_release_identities(&release.id, &[mb_identity("g-old", Some("mb-rel-old"))])
        .await
        .unwrap();
    insert_n_tracks(&manager.database, &release.id, 3).await;

    // Cache the picked release so `prepare_release` skips the network. The raw
    // JSON is what gets archived under the picked release's own key.
    let new_release_id = "exact-re-identify-mb-rel-new";
    let new_group_id = "exact-re-identify-mb-group-new";
    let new_response = make_mb_release_for_re_identify(new_release_id, new_group_id, 3);
    // What the archive holds is what the client returned, so the projection that
    // replays it later reads the same release the cache handed over now.
    let new_raw_json = serde_json::to_string(&new_response).unwrap();
    seed_release_cache(new_release_id, (new_response, None, new_raw_json.clone()));
    seed_release_group_json_cache(
        new_group_id,
        r#"{"id":"exact-re-identify-mb-group-new"}"#.to_string(),
    );

    manager
        .re_identify_release(
            &release.id,
            IdentityChoice::Exact {
                release_ref: MetadataRef {
                    source: MetadataSource::MusicBrainz,
                    id: new_release_id.to_string(),
                },
            },
        )
        .await
        .unwrap();

    // Identity row updated to Exact at the new pressing.
    let identities = manager
        .database
        .get_release_identities(&release.id)
        .await
        .unwrap();
    assert_eq!(identities.len(), 1);
    assert_eq!(identities[0].source, MetadataSource::MusicBrainz);
    assert_eq!(identities[0].source_group_id, new_group_id);
    assert_eq!(
        identities[0].source_release_id.as_deref(),
        Some(new_release_id)
    );

    // Pointer columns flipped to the new source release.
    let updated = manager
        .database
        .find_release_by_id(&release.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        updated.metadata_source,
        crate::db::ReleaseMetadataSource::MusicBrainz
    );
    assert_eq!(
        updated.metadata_source_release_id.as_deref(),
        Some(new_release_id)
    );

    // The picked release's documents are archived under its own key, which is
    // what the new pointer names.
    assert_eq!(
        archived_for(
            &manager,
            crate::import::PayloadSource::MusicBrainz,
            new_release_id
        )
        .await
        .as_deref(),
        Some(new_raw_json.as_str())
    );
    assert!(
        archived_for(
            &manager,
            crate::import::PayloadSource::MusicBrainzReleaseGroup,
            new_group_id
        )
        .await
        .is_some(),
        "the release group is archived alongside the release"
    );
}

#[tokio::test]
async fn re_identify_release_approximate_archives_the_picked_release() {
    use crate::import::{IdentityChoice, MetadataRef, MetadataSource};
    use crate::musicbrainz::{seed_release_cache, seed_release_group_json_cache};

    let (manager, _temp_dir) = setup_test_manager().await;

    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.metadata_source = crate::db::ReleaseMetadataSource::FileTags;
    release.metadata_source_release_id = None;

    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    insert_n_tracks(&manager.database, &release.id, 4).await;

    let new_release_id = "approx-re-identify-mb-rel-new";
    let new_group_id = "approx-re-identify-mb-group-new";
    let new_response = make_mb_release_for_re_identify(new_release_id, new_group_id, 4);
    let new_raw_json = serde_json::to_string(&new_response).unwrap();
    seed_release_cache(new_release_id, (new_response, None, new_raw_json.clone()));
    seed_release_group_json_cache(
        new_group_id,
        r#"{"id":"approx-re-identify-mb-group-new"}"#.to_string(),
    );

    manager
        .re_identify_release(
            &release.id,
            IdentityChoice::Approximate {
                release_ref: MetadataRef {
                    source: MetadataSource::MusicBrainz,
                    id: new_release_id.to_string(),
                },
            },
        )
        .await
        .unwrap();

    // Approximate clears `source_release_id` on the identity row
    // (group-only claim) but the metadata pointer still names the
    // picked pressing — reset-to-source reads cached payload through it.
    let identities = manager
        .database
        .get_release_identities(&release.id)
        .await
        .unwrap();
    assert_eq!(identities.len(), 1);
    assert_eq!(identities[0].source, MetadataSource::MusicBrainz);
    assert_eq!(identities[0].source_group_id, new_group_id);
    assert_eq!(identities[0].source_release_id, None);

    let updated = manager
        .database
        .find_release_by_id(&release.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        updated.metadata_source,
        crate::db::ReleaseMetadataSource::MusicBrainz
    );
    assert_eq!(
        updated.metadata_source_release_id.as_deref(),
        Some(new_release_id)
    );

    // An Approximate claim leaves the pointer on the picked pressing, so the
    // documents archived under that pressing are what a reset reads back.
    assert_eq!(
        archived_for(
            &manager,
            crate::import::PayloadSource::MusicBrainz,
            new_release_id
        )
        .await
        .as_deref(),
        Some(new_raw_json.as_str())
    );
    assert!(archived_for(
        &manager,
        crate::import::PayloadSource::MusicBrainzReleaseGroup,
        new_group_id
    )
    .await
    .is_some());
}

#[tokio::test]
async fn re_identify_release_rejects_track_count_mismatch() {
    // Re-identify re-points the identity without re-binding any audio, so a
    // source naming a different number of tracks leaves rows with nothing to
    // point at: a 12-track release can't replace a 10-track rip. A folder
    // import maps its own audio into track slots instead, where a count
    // disagreement is a row to look at rather than a refusal.
    use crate::import::{IdentityChoice, MetadataRef, MetadataSource};
    use crate::musicbrainz::{seed_release_cache, seed_release_group_json_cache};

    let (manager, _temp_dir) = setup_test_manager().await;

    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    // Local release has 10 tracks; picked release has 12.
    insert_n_tracks(&manager.database, &release.id, 10).await;

    let new_release_id = "mismatch-re-identify-mb-rel-new";
    let new_group_id = "mismatch-re-identify-mb-group-new";
    let new_response = make_mb_release_for_re_identify(new_release_id, new_group_id, 12);
    let new_raw_json = serde_json::to_string(&new_response).unwrap();
    seed_release_cache(new_release_id, (new_response, None, new_raw_json));
    seed_release_group_json_cache(
        new_group_id,
        r#"{"id":"mismatch-re-identify-mb-group-new"}"#.to_string(),
    );

    let err = manager
        .re_identify_release(
            &release.id,
            IdentityChoice::Exact {
                release_ref: MetadataRef {
                    source: MetadataSource::MusicBrainz,
                    id: new_release_id.to_string(),
                },
            },
        )
        .await
        .expect_err("track-count mismatch must error before identity write");
    let msg = err.to_string();
    assert!(
        msg.contains("Track count mismatch") && msg.contains("10") && msg.contains("12"),
        "error must name both counts so the UI can render a useful banner: {msg}"
    );

    // No identity row written.
    let identities = manager
        .database
        .get_release_identities(&release.id)
        .await
        .unwrap();
    assert!(
        identities.is_empty(),
        "mismatched commit must not leave a partial identity row"
    );
}

#[tokio::test]
async fn re_identify_release_followed_by_reset_succeeds() {
    // End to end: after a re-identify commit, `reset_metadata_to_source`
    // projects through the new pointer and reaches the documents that commit
    // archived. A regression here means re-identify pointed the release at a
    // source release whose documents it never wrote.
    use crate::import::{IdentityChoice, MetadataRef, MetadataSource};
    use crate::musicbrainz::{seed_release_cache, seed_release_group_json_cache};

    let (manager, _temp_dir) = setup_test_manager().await;

    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.metadata_source = crate::db::ReleaseMetadataSource::FileTags;
    release.metadata_source_release_id = None;

    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    insert_n_tracks(&manager.database, &release.id, 2).await;

    let new_release_id = "reset-re-identify-mb-rel-new";
    let new_group_id = "reset-re-identify-mb-group-new";
    let new_response = make_mb_release_for_re_identify(new_release_id, new_group_id, 2);
    let new_raw_json = serde_json::to_string(&new_response).unwrap();
    seed_release_cache(new_release_id, (new_response, None, new_raw_json));
    seed_release_group_json_cache(
        new_group_id,
        r#"{"id":"reset-re-identify-mb-group-new"}"#.to_string(),
    );

    manager
        .re_identify_release(
            &release.id,
            IdentityChoice::Exact {
                release_ref: MetadataRef {
                    source: MetadataSource::MusicBrainz,
                    id: new_release_id.to_string(),
                },
            },
        )
        .await
        .unwrap();

    // Reset replays the seed through the new pointer. A stale
    // cache would surface here as a missing key, a
    // parse error, or a divergence-guard `Err`. Success means
    // re_identify_release left the cache aligned.
    let edit = manager
        .reset_metadata_to_source(&release.id)
        .await
        .expect("reset must replay through aligned cache after re-identify");
    assert_eq!(edit.album_title, "Album Title");
    assert_eq!(edit.tracks.len(), 2);
}

#[tokio::test]
async fn re_identify_to_unknown_reseeds_rows_from_file_tags() {
    // A release carrying MusicBrainz-shaped rows, with local audio
    // files whose embedded tags say something different. Re-identifying
    // as Unknown must reseed the album/track rows from those tags — not
    // leave the old MB metadata displayed under a "use my files" claim.
    use crate::import::IdentityChoice;
    use lofty::config::WriteOptions;
    use lofty::prelude::*;
    use lofty::tag::{Tag, TagType};
    use std::fs;

    let (manager, _temp_dir) = setup_test_manager().await;

    // Local files live in a local folder so `local_file_path`
    // resolves to disk where lofty can read the embedded tags.
    let media = TempDir::new().unwrap();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("flac");

    let tag_file = |name: &str, title: &str| -> String {
        let src = fixtures.join("01 Test Track 1.flac");
        let dest = media.path().join(name);
        fs::copy(&src, &dest).unwrap();
        let mut tagged = lofty::read_from_path(&dest).unwrap();
        let mut tag = Tag::new(TagType::VorbisComments);
        tag.set_title(title.to_string());
        tag.set_artist("Tagged Artist".to_string());
        tag.set_album("Tagged Album".to_string());
        tagged.insert_tag(tag);
        tagged.save_to_path(&dest, WriteOptions::default()).unwrap();
        name.to_string()
    };
    let f1 = tag_file("01.flac", "Tagged One");
    let f2 = tag_file("02.flac", "Tagged Two");

    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    // MusicBrainz-shaped pointer; the rows below carry MB metadata.
    release.metadata_source = crate::db::ReleaseMetadataSource::MusicBrainz;
    release.metadata_source_release_id = Some("mb-rel-1".to_string());
    release.remote = false;

    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    manager
        .database
        .insert_release_identities(&release.id, &[mb_identity("g1", Some("mb-rel-1"))])
        .await
        .unwrap();

    // MB-shaped track rows — distinct from the embedded tags.
    for (i, (id, title)) in [
        ("08c7ff07-b56a-4e16-8df6-ae2967fa0806", "MB Track One"),
        ("08c7fe07-b56a-4c63-8df6-ad2967fa0653", "MB Track Two"),
    ]
    .into_iter()
    .enumerate()
    {
        let track = crate::db::DbTrack {
            id: id.to_string(),
            release_id: release.id.clone(),
            title: title.to_string(),
            side: 1,
            track_number: Some(i as i32 + 1),
            duration_ms: None,
            discogs_position: None,
            created_at: Utc::now(),
        };
        manager.database.insert_track(&track).await.unwrap();
    }
    let now = Utc::now();
    for name in [&f1, &f2] {
        let file = crate::db::DbFile::new(
            &release.id,
            name,
            0,
            crate::util::content_type::ContentType::Flac,
            Uuid::new_v4().to_string(),
            now,
            crate::util::fs::hash_bytes(b"fixture"),
        );
        manager.database.insert_file(&file).await.unwrap();
    }
    // Register the in-place files as coven external refs after inserting them.
    manager
        .database
        .register_release_external_refs_for_test(&release.id, &media.path().to_string_lossy())
        .await
        .unwrap();

    manager
        .re_identify_release(&release.id, IdentityChoice::Unknown)
        .await
        .unwrap();

    // Pointer flipped to file_tags.
    let updated = manager
        .database
        .find_release_by_id(&release.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        updated.metadata_source,
        crate::db::ReleaseMetadataSource::FileTags
    );

    // Album + track rows now reflect the embedded tags, not the MB seed.
    let landing_album_id = manager
        .database
        .find_album_id_for_release(&release.id)
        .await
        .unwrap()
        .unwrap();
    let landing_album = manager
        .database
        .find_album_by_id(&landing_album_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(landing_album.title, "Tagged Album");

    let tracks = manager
        .database
        .get_tracks_for_release(&release.id)
        .await
        .unwrap();
    let titles: Vec<&str> = tracks.iter().map(|t| t.title.as_str()).collect();
    assert!(
        titles.contains(&"Tagged One") && titles.contains(&"Tagged Two"),
        "track rows must carry the embedded tag titles, got {titles:?}"
    );
    assert!(
        !titles.iter().any(|t| t.starts_with("MB ")),
        "old MusicBrainz track titles must be gone, got {titles:?}"
    );
}
