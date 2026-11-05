//! A draft's rows and the artist assignments as stored and read back.

use crate::tables::import_state::tests::*;
use crate::tables::watched_path::host_root;
use crate::{ArtistAssignment, RawTrackEdit, TrackArtistAssignments};
use bae_catalogs::credits::ArtistCredit;
use bae_records::test_fixtures::{ARTIST_A, ARTIST_B};
use bae_records::ExistingArtist;
use bae_store::test_fixtures::{empty_db, exec, exec_batch, temp_db};

#[tokio::test]
async fn picked_artist_assignments_resolve_the_canonical_artist_row() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let records = bae_records::test_fixtures::test_records(&db);
    let tables = crate::tables::ImportTables::new(db.clone());
    let (_, hash) = stored_pane_candidate(&db).await;
    let existing = existing_artist();
    records.insert_artist(&existing).await.unwrap();

    let assignments = crate::Credited::plain_list(vec![
        ArtistAssignment::picked(existing.into()),
        ArtistAssignment::Credit {
            credit: ArtistCredit {
                credited_as: None,
                name: "New Artist".to_string(),
                sort_name: Some("Artist, New".to_string()),
                musicbrainz_artist_id: Some("mb-new".to_string()),
                discogs_artist_id: Some("discogs-new".to_string()),
            },
        },
    ]);
    PaneControls::of_pane_candidate(&db).set_album_artists(&hash, &assignments).await.unwrap();

    let stored = tables.load_import_candidate_pane_rows(&hash).await.unwrap();
    assert_eq!(stored.draft.album_artist_assignments, assignments);

    let explicit_empty = RawTrackEdit {
        id: "candidate-track-0".to_string(),
        title: "Track Title".to_string(),
        artist_assignments: TrackArtistAssignments::Explicit(Vec::new()),
        file: Some(stored.draft.tracks[0].edit.file.clone()),
    };
    PaneControls::of_pane_candidate(&db).edit_track(&hash, &explicit_empty).await.unwrap();
    assert_eq!(
        tables.load_import_candidate_pane_rows(&hash).await.unwrap().draft.tracks[0]
            .edit
            .artist_assignments,
        TrackArtistAssignments::Explicit(Vec::new())
    );
}

#[tokio::test]
async fn a_picked_artist_assignment_to_a_missing_row_is_rejected() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let (_, hash) = stored_pane_candidate(&db).await;

    let error = PaneControls::of_pane_candidate(&db)
        .set_album_artists(
            &hash,
            &[crate::Credited::plain(ArtistAssignment::picked(ExistingArtist {
                artist_id: bae_util::test_ids::test_uuid("missing-artist"),
                name: "Missing Artist".to_string(),
                sort_name: None,
                musicbrainz_artist_id: None,
                discogs_artist_id: None,
            }))],
        )
        .await
        .expect_err("a missing referenced artist cannot be stored");
    assert!(
        error.to_string().contains("FOREIGN KEY constraint failed"),
        "the database rejects the broken reference: {error}"
    );
}

/// A track metadata edit is stored over the row's audio and rejoins it in the
/// candidate pane.
#[tokio::test]
async fn a_track_row_round_trips_metadata_and_mapping() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    fetched(&db, "rel-1").await;
    let (_, hash) = stored_pane_candidate(&db).await;
    crate::CandidatePreparations::new(db.clone())
        .replace_metadata(
            &hash,
            &pane_candidate_key(),
            &metadata_draft("Album", "Artist"),
            Some(&release_pick("rel-1")),
            &[],
        )
        .await
        .unwrap();
    let before = tables.load_import_candidate_pane_rows(&hash).await.unwrap();
    let edit = edited_row(
        "candidate-track-0",
        "Edited title",
        Some(before.draft.tracks[0].edit.file.clone()),
    );
    PaneControls::of_pane_candidate(&db).edit_track(&hash, &edit).await.unwrap();

    let stored = tables.load_import_candidate_pane_rows(&hash).await.unwrap();
    assert_eq!(stored.draft.tracks.len(), 1);
    assert_eq!(stored.draft.tracks[0].edit.title, "Edited title");
    assert_eq!(Some(stored.draft.tracks[0].edit.file.clone()), edit.file);
}

/// A track length a catalog states past what SQLite stores fails the
/// preparation's save with an error, rather than stopping the import.
#[tokio::test]
async fn a_stated_length_past_sqlites_range_fails_the_save() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    fetched(&db, "release-1").await;
    let (_, hash) = stored_pane_candidate(&db).await;
    let mut draft = candidate_draft("Release Title", "Artist Name");
    draft.tracks[0].stated_length_ms = Some(u64::MAX);

    let saved = crate::CandidatePreparations::new(db.clone())
        .apply_source(
            &host_root("/music"),
            &as_read(&hash, 0),
            &pane_candidate_key(),
            &crate::CandidateMetadataDraft {
                draft,
                provenance: Some(release_pick("release-1")),
                cover: None,
            },
        )
        .await;

    assert!(saved.is_err(), "{saved:?}");
}

/// Deleting a library artist deletes the draft artist assignments that picked
/// it, and leaves the draft.
#[tokio::test]
async fn deleting_an_artist_removes_local_draft_references() {
    for removed in [ARTIST_A, ARTIST_B] {
        let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
        for artist in [ARTIST_A, ARTIST_B] {
            exec(
                &db,
                "INSERT INTO artists (id, name, name_key, created_at) VALUES (?, 'Artist Name', 'artist name', '2026-01-01T00:00:00Z')",
                &[artist],
            )
            .await;
        }
        exec_batch(
            &db,
            "INSERT INTO import_candidate_state (content_hash, candidate_key)
             VALUES ('candidate', CAST('/music/candidate' AS BLOB));
             INSERT INTO import_candidate_edit
                 (content_hash, album_title, album_year, year, author, draft_blank, draft_valid)
             VALUES ('candidate', 'Album Title', '', '', 'person', 0, 1);
             INSERT INTO import_candidate_medium (content_hash, position, side_count)
             VALUES ('candidate', 1, 1);
             INSERT INTO import_candidate_track
                 (content_hash, track_id, position, title, artist_assignment_kind,
                  medium, side, track_number, file_kind, file_id)
             VALUES ('candidate', 'track', 0, 'Track Title', 'explicit', 1, 1, 1, 'standalone', 'file');",
        )
        .await;
        for (position, artist) in [("0", ARTIST_A), ("1", ARTIST_B)] {
            exec(
                &db,
                "INSERT INTO import_candidate_album_artist_assignment
                     (content_hash, position, assignment_kind, artist_id)
                 VALUES ('candidate', ?, 'picked', ?)",
                &[position, artist],
            )
            .await;
            exec(
                &db,
                "INSERT INTO import_candidate_track_artist_assignment
                     (content_hash, track_id, position, assignment_kind, artist_id)
                 VALUES ('candidate', 'track', ?, 'picked', ?)",
                &[position, artist],
            )
            .await;
        }

        exec(&db, "DELETE FROM artists WHERE id = ?", &[removed]).await;

        let remaining = db
            .read(|sql| {
                Ok((
                    sql.query_row(
                        "SELECT count(*) FROM import_candidate_album_artist_assignment",
                        [],
                        |row| row.get::<_, i64>(0),
                    )?,
                    sql.query_row(
                        "SELECT count(*) FROM import_candidate_track_artist_assignment",
                        [],
                        |row| row.get::<_, i64>(0),
                    )?,
                    sql.query_row("SELECT count(*) FROM import_candidate_edit", [], |row| {
                        row.get::<_, i64>(0)
                    })?,
                ))
            })
            .await
            .unwrap();
        assert_eq!(remaining, (1, 1, 1));
    }
}
