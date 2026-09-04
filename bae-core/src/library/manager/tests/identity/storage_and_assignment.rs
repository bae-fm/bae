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

fn mb_identity(group: &str, release: &str) -> crate::import::ReleaseIdentity {
    crate::import::ReleaseIdentity {
        source: crate::import::MetadataSource::MusicBrainz,
        source_group_id: group.to_string(),
        source_release_id: release.to_string(),
    }
}

fn discogs_identity(group: &str, release: &str) -> crate::import::ReleaseIdentity {
    crate::import::ReleaseIdentity {
        source: crate::import::MetadataSource::Discogs,
        source_group_id: group.to_string(),
        source_release_id: release.to_string(),
    }
}

#[tokio::test]
async fn set_identity_to_file_tags_moves_release_to_fresh_album() {
    let (manager, _temp_dir) = setup_test_manager().await;

    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.metadata_provenance = Some(crate::import::MetadataProvenance::ExternalRelease {
        source: crate::import::MetadataSource::MusicBrainz,
        release_id: "mb-rel-1".to_string(),
        partners: vec![],
    });

    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    manager
        .database
        .insert_release_identities(&release.id, &[mb_identity("g1", "mb-rel-1")])
        .await
        .unwrap();

    manager
        .set_identity(
            &release.id,
            vec![],
            crate::import::MetadataProvenance::FileTags,
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
        updated.metadata_provenance,
        Some(crate::import::MetadataProvenance::FileTags)
    );
}

#[tokio::test]
async fn set_identity_replaces_rows_when_new_identity_fits_current_album() {
    let (manager, _temp_dir) = setup_test_manager().await;

    // Album has two releases, both MB identities on group g1.
    let album = create_test_album();
    let release1 = create_test_release(&album.id);
    let release2 = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release1).await.unwrap();
    manager.database.insert_release(&release2).await.unwrap();
    manager
        .database
        .insert_release_identities(&release1.id, &[mb_identity("g1", "g1-rel")])
        .await
        .unwrap();
    manager
        .database
        .insert_release_identities(&release2.id, &[mb_identity("g1", "g1-rel")])
        .await
        .unwrap();

    // Re-point release1 at another pressing within g1. The new row still
    // agrees with release2's group, so release1 stays put.
    manager
        .set_identity(
            &release1.id,
            vec![mb_identity("g1", "mb-rel-99")],
            crate::import::MetadataProvenance::ExternalRelease {
                source: crate::import::MetadataSource::MusicBrainz,
                release_id: "mb-rel-99".to_string(),
                partners: vec![],
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
    assert_eq!(identities[0].source_release_id, "mb-rel-99");

    let updated = manager
        .database
        .find_release_by_id(&release1.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        updated.metadata_provenance,
        Some(crate::import::MetadataProvenance::ExternalRelease {
            source: crate::import::MetadataSource::MusicBrainz,
            release_id: "mb-rel-99".to_string(),
            partners: vec![],
        }),
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
        .insert_release_identities(&release_alpha.id, &[mb_identity("g1", "g1-rel")])
        .await
        .unwrap();
    manager
        .database
        .insert_release_identities(&release_beta.id, &[mb_identity("g1", "g1-rel")])
        .await
        .unwrap();
    manager
        .database
        .insert_release_identities(&release_other.id, &[mb_identity("g3", "g3-rel")])
        .await
        .unwrap();

    // release_alpha takes on a brand-new MB group (g2). Its current
    // album (album_a) holds release_beta on g1, so it can't stay.
    // No other album holds g2 either → fresh album.
    manager
        .set_identity(
            &release_alpha.id,
            vec![mb_identity("g2", "g2-rel")],
            crate::import::MetadataProvenance::ExternalRelease {
                source: crate::import::MetadataSource::MusicBrainz,
                release_id: "mb-rel-g2".to_string(),
                partners: vec![],
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
        .insert_release_identities(&release_alpha.id, &[mb_identity("g1", "g1-rel")])
        .await
        .unwrap();
    manager
        .database
        .insert_release_identities(&release_other.id, &[mb_identity("g2", "g2-rel")])
        .await
        .unwrap();

    manager
        .set_identity(
            &release_alpha.id,
            vec![mb_identity("g2", "mb-rel-pressing")],
            crate::import::MetadataProvenance::ExternalRelease {
                source: crate::import::MetadataSource::MusicBrainz,
                release_id: "mb-rel-pressing".to_string(),
                partners: vec![],
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
        .insert_release_identities(&release_alpha.id, &[mb_identity("g1", "g1-rel")])
        .await
        .unwrap();
    manager
        .database
        .insert_release_identities(&release_beta.id, &[mb_identity("g1", "g1-rel")])
        .await
        .unwrap();

    manager
        .set_identity(
            &release_alpha.id,
            vec![mb_identity("g2", "g2-rel")],
            crate::import::MetadataProvenance::ExternalRelease {
                source: crate::import::MetadataSource::MusicBrainz,
                release_id: "mb-rel-g2".to_string(),
                partners: vec![],
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
        .insert_release_identities(&release.id, &[mb_identity("g1", "mb-rel-1")])
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
            vec![discogs_identity("dg1", "dg-rel-1")],
            crate::import::MetadataProvenance::ExternalRelease {
                source: crate::import::MetadataSource::Discogs,
                release_id: "dg-rel-1".to_string(),
                partners: vec![],
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
        .insert_release_identities(&release_alpha.id, &[mb_identity("g1", "g1-rel")])
        .await
        .unwrap();
    manager
        .database
        .insert_release_identities(&release_beta.id, &[mb_identity("g1", "g1-rel")])
        .await
        .unwrap();

    // release_alpha takes a different group → can't stay in album_a
    // (g1 disagrees with g2), no other album holds g2 → fresh album.
    manager
        .set_identity(
            &release_alpha.id,
            vec![mb_identity("g2", "g2-rel")],
            crate::import::MetadataProvenance::ExternalRelease {
                source: crate::import::MetadataSource::MusicBrainz,
                release_id: "mb-rel-g2".to_string(),
                partners: vec![],
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
        .insert_release_identities(&release_alpha.id, &[mb_identity("g1", "g1-rel")])
        .await
        .unwrap();
    manager
        .database
        .insert_release_identities(&release_beta.id, &[mb_identity("g1", "g1-rel")])
        .await
        .unwrap();

    manager
        .set_identity(
            &release_alpha.id,
            vec![mb_identity("g2", "g2-rel")],
            crate::import::MetadataProvenance::ExternalRelease {
                source: crate::import::MetadataSource::MusicBrainz,
                release_id: "mb-rel-g2".to_string(),
                partners: vec![],
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
        .insert_release_identities(&release_alpha.id, &[mb_identity("g1", "g1-rel")])
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

    manager
        .database
        .set_identity_atomic(
            &release_alpha.id,
            &[mb_identity("g2", "g2-rel")],
            Some(crate::import::MetadataProvenance::ExternalRelease {
                source: crate::import::MetadataSource::MusicBrainz,
                release_id: "mb-rel-g2".to_string(),
                partners: vec![],
            }),
            &album_a.id,
            &fresh_album.id,
            Some(&fresh_album),
        )
        .await
        .unwrap();

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
