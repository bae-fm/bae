//! Consolidating two library artists after an import's identity conflict.

use super::*;
use crate::tables::import_state::tests::*;
use bae_db::test_fixtures::{empty_db, fixed_now};

/// "Same artist" merges the two library artists (bae-db's merge, tested
/// beside it), moves this device's drafts naming the absorbed artist to the
/// survivor, and dismisses every candidate's failure on the pair.
#[tokio::test]
async fn resolving_an_artist_identity_conflict_merges_the_artists_and_moves_the_drafts() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let (_, hash) = stored_pane_candidate(&db).await;
    let discogs = DbArtist {
        id: bae_util::test_ids::test_uuid("conflict-merge-discogs-artist"),
        name: "Artist One".to_string(),
        sort_name: None,
        discogs_artist_id: Some("discogs-1".to_string()),
        musicbrainz_artist_id: None,
        created_at: fixed_now(),
    };
    let musicbrainz = DbArtist {
        id: bae_util::test_ids::test_uuid("conflict-merge-musicbrainz-artist"),
        name: "Artist One".to_string(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: Some("mb-1".to_string()),
        created_at: fixed_now(),
    };
    db.insert_artist(&discogs).await.unwrap();
    db.insert_artist(&musicbrainz).await.unwrap();
    let pending_hash = "pending-artist-merge".to_string();
    let seed_discogs_id = discogs.id.clone();
    let seed_musicbrainz_id = musicbrainz.id.clone();
    let seed_pending_hash = pending_hash.clone();
    db.call(move |sql| {
        sql.execute(
            "INSERT INTO import_candidate_state (content_hash, candidate_key) VALUES (?, CAST('/pending' AS BLOB))",
            [&seed_pending_hash],
        )?;
        sql.execute(
            "INSERT INTO import_candidate_edit (content_hash, album_title, album_year, year, \
                 author, draft_blank, draft_valid) \
             VALUES (?, 'Album Title', '', '', 'person', 0, 1)",
            [&seed_pending_hash],
        )?;
        sql.execute(
            "INSERT INTO import_candidate_medium (content_hash, position, side_count) VALUES (?, 1, 1)",
            [&seed_pending_hash],
        )?;
        sql.execute(
            "INSERT INTO import_candidate_track (content_hash, track_id, position, title, \
                 artist_assignment_kind, medium, side, track_number, file_kind, file_id) \
             VALUES (?, 'draft-track', 0, 'Track Title', 'explicit', 1, 1, 1, 'standalone', '01.flac')",
            [&seed_pending_hash],
        )?;
        for (position, artist_id) in [&seed_discogs_id, &seed_musicbrainz_id]
            .into_iter()
            .enumerate()
        {
            sql.execute(
                "INSERT INTO import_candidate_album_artist_assignment \
                     (content_hash, position, assignment_kind, artist_id) \
                 VALUES (?, ?, 'picked', ?)",
                params![seed_pending_hash, position as i64, artist_id],
            )?;
            sql.execute(
                "INSERT INTO import_candidate_track_artist_assignment \
                     (content_hash, track_id, position, assignment_kind, artist_id) \
                 VALUES (?, 'draft-track', ?, 'picked', ?)",
                params![seed_pending_hash, position as i64, artist_id],
            )?;
        }
        Ok(())
    })
    .await
    .unwrap();
    let failure = artist_identity_failure(&discogs, &musicbrainz);
    tables.save_import_candidate_failure(&hash, 0, &failure).await.unwrap();
    tables.save_import_candidate_failure(&pending_hash, 0, &failure).await.unwrap();

    tables
        .choose_import_artist_identity(
            &hash,
            0,
            crate::ArtistIdentityConflictChoice::SameArtist,
            &coven::UuidIds,
        )
        .await
        .unwrap();

    let survivor = db.find_artist_by_id(&discogs.id).await.unwrap().unwrap();
    assert_eq!(survivor.musicbrainz_artist_id.as_deref(), Some("mb-1"), "the two were merged");
    let absorbed_id = musicbrainz.id.clone();
    let survivor_id = discogs.id.clone();
    let pending_hash_for_read = pending_hash.clone();
    let (absorbed_references, pending_album_artists, pending_track_artists) = db
        .read(move |sql| {
            let mut absorbed_references = 0_i64;
            for table in [
                "import_candidate_album_artist_assignment",
                "import_candidate_track_artist_assignment",
            ] {
                absorbed_references += sql.query_row(
                    &format!("SELECT COUNT(*) FROM {table} WHERE artist_id = ?"),
                    [&absorbed_id],
                    |row| row.get::<_, i64>(0),
                )?;
            }
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
            Ok((absorbed_references, pending_album_artists, pending_track_artists))
        })
        .await
        .unwrap();
    assert_eq!(absorbed_references, 0);
    assert_eq!(pending_album_artists, 1);
    assert_eq!(pending_track_artists, 1);
    assert!(tables.load_import_candidate_pane_rows(&hash).await.unwrap().failure.is_none());
    assert!(tables.load_import_candidate_pane_rows(&pending_hash).await.unwrap().failure.is_none());
}

#[tokio::test]
async fn resolving_a_conflict_refuses_a_third_provider_identity_without_changing_state() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let (_, hash) = stored_pane_candidate(&db).await;
    let discogs = DbArtist {
        id: bae_util::test_ids::test_uuid("three-identity-discogs-artist"),
        name: "Artist One".to_string(),
        sort_name: None,
        discogs_artist_id: Some("discogs-1".to_string()),
        musicbrainz_artist_id: Some("mb-other".to_string()),
        created_at: fixed_now(),
    };
    let musicbrainz = DbArtist {
        id: bae_util::test_ids::test_uuid("three-identity-musicbrainz-artist"),
        name: "Artist One".to_string(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: Some("mb-1".to_string()),
        created_at: fixed_now(),
    };
    db.insert_artist(&discogs).await.unwrap();
    db.insert_artist(&musicbrainz).await.unwrap();
    tables
        .save_import_candidate_failure(&hash, 0, &artist_identity_failure(&discogs, &musicbrainz))
        .await
        .unwrap();

    tables
        .choose_import_artist_identity(
            &hash,
            0,
            crate::ArtistIdentityConflictChoice::SameArtist,
            &coven::UuidIds,
        )
        .await
        .expect_err("a two-artist merge must not absorb an unrelated provider identity");

    assert!(db.find_artist_by_id(&discogs.id).await.unwrap().is_some());
    assert!(db.find_artist_by_id(&musicbrainz.id).await.unwrap().is_some());
    assert!(tables.load_import_candidate_pane_rows(&hash).await.unwrap().failure.is_some());
}
