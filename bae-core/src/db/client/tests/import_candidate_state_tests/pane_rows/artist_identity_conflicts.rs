use super::*;

fn artist_identity_failure(discogs: &DbArtist, musicbrainz: &DbArtist) -> ImportFailure {
    ImportFailure {
        error: "the artist identities disagree".to_string(),
        failed_at: fixed_identified_at(),
        artist_identity_conflict: Some(crate::import::ArtistIdentityConflict {
            incoming_artist_name: "Artist One".to_string(),
            discogs_artist_id: "discogs-1".to_string(),
            musicbrainz_artist_id: "mb-1".to_string(),
            discogs_artist: discogs.clone().into(),
            musicbrainz_artist: musicbrainz.clone().into(),
        }),
    }
}

#[tokio::test]
async fn an_artist_identity_conflict_round_trips_with_both_library_artists() {
    let (db, _tmp) = empty_db().await;
    let (_, hash) = stored_pane_candidate(&db).await;
    let discogs = DbArtist {
        id: bae_test_support::test_uuid("conflict-round-trip-discogs-artist"),
        name: "Artist One".to_string(),
        sort_name: None,
        discogs_artist_id: Some("discogs-1".to_string()),
        musicbrainz_artist_id: None,
        created_at: fixed_identified_at(),
    };
    let musicbrainz = DbArtist {
        id: bae_test_support::test_uuid("conflict-round-trip-musicbrainz-artist"),
        name: "Artist One".to_string(),
        sort_name: Some("Artist One, The".to_string()),
        discogs_artist_id: None,
        musicbrainz_artist_id: Some("mb-1".to_string()),
        created_at: fixed_identified_at(),
    };
    db.insert_artist(&discogs).await.unwrap();
    db.insert_artist(&musicbrainz).await.unwrap();
    let expected = artist_identity_failure(&discogs, &musicbrainz);

    db.save_import_candidate_failure(&hash, 0, &expected)
        .await
        .unwrap();

    let stored = db
        .load_import_candidate_pane_rows(&hash)
        .await
        .unwrap()
        .failure
        .unwrap();
    assert_eq!(stored, expected);
}

#[tokio::test]
async fn resolving_an_artist_identity_conflict_merges_library_links_and_clears_the_failure() {
    let (db, _tmp) = empty_db().await;
    let (_, hash) = stored_pane_candidate(&db).await;
    let discogs = DbArtist {
        id: bae_test_support::test_uuid("conflict-merge-discogs-artist"),
        name: "Artist One".to_string(),
        sort_name: None,
        discogs_artist_id: Some("discogs-1".to_string()),
        musicbrainz_artist_id: None,
        created_at: fixed_identified_at(),
    };
    let musicbrainz = DbArtist {
        id: bae_test_support::test_uuid("conflict-merge-musicbrainz-artist"),
        name: "Artist One".to_string(),
        sort_name: Some("Artist One, The".to_string()),
        discogs_artist_id: None,
        musicbrainz_artist_id: Some("mb-1".to_string()),
        created_at: fixed_identified_at(),
    };
    db.insert_artist(&discogs).await.unwrap();
    db.insert_artist(&musicbrainz).await.unwrap();
    let album = DbAlbum {
        id: bae_test_support::test_uuid("conflict-merge-album"),
        title: "Album Title".to_string(),
        artist_id: musicbrainz.id.clone(),
        year: None,
        primary_release_id: None,
        is_compilation: false,
        created_at: fixed_identified_at(),
    };
    db.insert_album(&album).await.unwrap();
    let release_id = bae_test_support::test_uuid("conflict-merge-release");
    let track_id = bae_test_support::test_uuid("conflict-merge-track");
    let work_id = bae_test_support::test_uuid("conflict-merge-work");
    let pending_hash = "pending-artist-merge".to_string();
    let seed_discogs_id = discogs.id.clone();
    let seed_musicbrainz_id = musicbrainz.id.clone();
    let seed_album_id = album.id.clone();
    let seed_release_id = release_id.clone();
    let seed_track_id = track_id.clone();
    let seed_work_id = work_id.clone();
    let seed_pending_hash = pending_hash.clone();
    db.call(move |sql| {
        let reg = sql.stamp();
        sql.execute(
            "INSERT INTO releases (id, album_id, metadata_source, remote, _updated_at, created_at) \
             VALUES (?, ?, 'file_tags', 1, ?, '2026-01-02T03:04:05Z')",
            params![seed_release_id, seed_album_id, reg],
        )?;
        sql.execute(
            "INSERT INTO tracks (id, release_id, title, side, _updated_at, created_at) \
             VALUES (?, ?, 'Track Title', 1, ?, '2026-01-02T03:04:05Z')",
            params![seed_track_id, seed_release_id, reg],
        )?;
        sql.execute(
            "INSERT INTO works (id, title, musicbrainz_work_id, _updated_at, created_at) \
             VALUES (?, 'Work Title', 'work-1', ?, '2026-01-02T03:04:05Z')",
            params![seed_work_id, reg],
        )?;
        let reference_tables = [
            ("album_artists", "album_id", seed_album_id.as_str(), false),
            ("track_artists", "track_id", seed_track_id.as_str(), false),
            ("work_artists", "work_id", seed_work_id.as_str(), true),
            (
                "release_artist_roles",
                "release_id",
                seed_release_id.as_str(),
                true,
            ),
            (
                "track_artist_roles",
                "track_id",
                seed_track_id.as_str(),
                true,
            ),
        ];
        for (artist_index, artist_id) in [&seed_discogs_id, &seed_musicbrainz_id]
            .into_iter()
            .enumerate()
        {
            for (table, owner_column, owner_id, has_source) in reference_tables {
                let source_column = if has_source { ", source" } else { "" };
                let source_value = if has_source { ", 'musicbrainz'" } else { "" };
                sql.execute(
                    &format!(
                        "INSERT INTO {table} \
                             (id, {owner_column}, artist_id, position{source_column}, \
                              _updated_at, created_at) \
                         VALUES (?, ?, ?, 0{source_value}, ?, '2026-01-02T03:04:05Z')"
                    ),
                    params![
                        bae_test_support::test_uuid(&format!(
                            "conflict-merge-{table}-{artist_index}"
                        )),
                        owner_id,
                        artist_id,
                        reg
                    ],
                )?;
            }
        }
        sql.execute(
            "INSERT INTO import_candidate_state (content_hash, folder_path) VALUES (?, '/pending')",
            [&seed_pending_hash],
        )?;
        sql.execute(
            "INSERT INTO import_candidate_edit (content_hash, album_title, year, format, label, \
                 catalog_number, country, barcode) VALUES (?, 'Album Title', '', '', '', '', '', '')",
            [&seed_pending_hash],
        )?;
        sql.execute(
            "INSERT INTO import_candidate_track (content_hash, track_id, position, title, \
                 artist_assignment_kind, side, named_by_source, dropped, file_author) \
             VALUES (?, 'draft-track', 0, 'Track Title', 'explicit', 1, 1, 0, 'automatic')",
            [&seed_pending_hash],
        )?;
        for (position, artist_id) in [&seed_discogs_id, &seed_musicbrainz_id]
            .into_iter()
            .enumerate()
        {
            sql.execute(
                "INSERT INTO import_candidate_album_artist_assignment \
                     (content_hash, position, assignment_kind, artist_id) \
                 VALUES (?, ?, 'existing', ?)",
                params![seed_pending_hash, position as i64, artist_id],
            )?;
            sql.execute(
                "INSERT INTO import_candidate_track_artist_assignment \
                     (content_hash, track_id, position, assignment_kind, artist_id) \
                 VALUES (?, 'draft-track', ?, 'existing', ?)",
                params![seed_pending_hash, position as i64, artist_id],
            )?;
        }
        sql.execute(
            "INSERT INTO artist_images (id, blob_id, content_type, file_size, source, hash, \
                 _updated_at, created_at) \
             VALUES \
                 (?, '44444444-4444-4444-8444-444444444444', 'image/jpeg', 4, 'discogs', \
                  '1111111111111111111111111111111111111111111111111111111111111111', \
                  ?, '2026-01-02T03:04:05Z'), \
                 (?, '33333333-3333-4333-8333-333333333333', 'image/jpeg', 3, 'musicbrainz', \
                  '0000000000000000000000000000000000000000000000000000000000000000', \
                  ?, '2026-01-02T03:04:05Z')",
            params![seed_discogs_id, reg, seed_musicbrainz_id, reg],
        )?;
        Ok(())
    })
    .await
    .unwrap();
    let failure = artist_identity_failure(&discogs, &musicbrainz);
    db.save_import_candidate_failure(&hash, 0, &failure)
        .await
        .unwrap();
    db.save_import_candidate_failure(&pending_hash, 0, &failure)
        .await
        .unwrap();

    db.merge_import_artist_identity_conflict(&hash, &discogs.id)
        .await
        .unwrap();

    let survivor = db.find_artist_by_id(&discogs.id).await.unwrap().unwrap();
    assert_eq!(survivor.discogs_artist_id.as_deref(), Some("discogs-1"));
    assert_eq!(survivor.musicbrainz_artist_id.as_deref(), Some("mb-1"));
    assert!(db
        .find_artist_by_id(&musicbrainz.id)
        .await
        .unwrap()
        .is_none());
    assert_eq!(
        db.find_album_by_id(&album.id)
            .await
            .unwrap()
            .unwrap()
            .artist_id,
        discogs.id
    );
    let absorbed_id = musicbrainz.id.clone();
    let survivor_id = discogs.id.clone();
    let pending_hash_for_read = pending_hash.clone();
    let (absorbed_references, survivor_image_blob, pending_album_artists, pending_track_artists) = db
        .read(move |sql| {
            let mut absorbed_references = 0_i64;
            for table in [
                "albums",
                "album_artists",
                "track_artists",
                "work_artists",
                "release_artist_roles",
                "track_artist_roles",
                "import_candidate_album_artist_assignment",
                "import_candidate_track_artist_assignment",
            ] {
                absorbed_references += sql.query_row(
                    &format!("SELECT COUNT(*) FROM {table} WHERE artist_id = ?"),
                    [&absorbed_id],
                    |row| row.get::<_, i64>(0),
                )?;
            }
            let survivor_image_blob = sql.query_row(
                "SELECT blob_id FROM artist_images WHERE id = ?",
                [&survivor_id],
                |row| row.get::<_, String>(0),
            )?;
            let pending_album_artists = sql.query_row(
                "SELECT COUNT(*) FROM import_candidate_album_artist_assignment \
                 WHERE content_hash = ? AND artist_id = ?",
                params![pending_hash_for_read, survivor_id],
                |row| row.get::<_, i64>(0),
            )?;
            let pending_track_artists = sql.query_row(
                "SELECT COUNT(*) FROM import_candidate_track_artist_assignment \
                 WHERE content_hash = ? AND artist_id = ?",
                params![pending_hash_for_read, survivor_id],
                |row| row.get::<_, i64>(0),
            )?;
            Ok((
                absorbed_references,
                survivor_image_blob,
                pending_album_artists,
                pending_track_artists,
            ))
        })
        .await
        .unwrap();
    assert_eq!(absorbed_references, 0);
    assert_eq!(
        survivor_image_blob,
        "44444444-4444-4444-8444-444444444444"
    );
    assert_eq!(pending_album_artists, 1);
    assert_eq!(pending_track_artists, 1);
    assert!(db
        .load_import_candidate_pane_rows(&hash)
        .await
        .unwrap()
        .failure
        .is_none());
    assert!(db
        .load_import_candidate_pane_rows(&pending_hash)
        .await
        .unwrap()
        .failure
        .is_none());
}

#[tokio::test]
async fn resolving_a_conflict_refuses_a_third_provider_identity_without_changing_state() {
    let (db, _tmp) = empty_db().await;
    let (_, hash) = stored_pane_candidate(&db).await;
    let discogs = DbArtist {
        id: bae_test_support::test_uuid("three-identity-discogs-artist"),
        name: "Artist One".to_string(),
        sort_name: None,
        discogs_artist_id: Some("discogs-1".to_string()),
        musicbrainz_artist_id: Some("mb-other".to_string()),
        created_at: fixed_identified_at(),
    };
    let musicbrainz = DbArtist {
        id: bae_test_support::test_uuid("three-identity-musicbrainz-artist"),
        name: "Artist One".to_string(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: Some("mb-1".to_string()),
        created_at: fixed_identified_at(),
    };
    db.insert_artist(&discogs).await.unwrap();
    db.insert_artist(&musicbrainz).await.unwrap();
    db.save_import_candidate_failure(
        &hash,
        0,
        &artist_identity_failure(&discogs, &musicbrainz),
    )
    .await
    .unwrap();

    db.merge_import_artist_identity_conflict(&hash, &discogs.id)
        .await
        .expect_err("a two-artist merge must not absorb an unrelated provider identity");

    assert!(db.find_artist_by_id(&discogs.id).await.unwrap().is_some());
    assert!(db
        .find_artist_by_id(&musicbrainz.id)
        .await
        .unwrap()
        .is_some());
    assert!(db
        .load_import_candidate_pane_rows(&hash)
        .await
        .unwrap()
        .failure
        .is_some());
}
