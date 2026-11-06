//! A draft's rows and the artist assignments as stored and read back.

use crate::tables::import_state::tests::*;
use crate::tables::watched_path::host_root;
use crate::{ArtistAssignment, RawTrackEdit, TrackArtistAssignments};
use bae_catalogs::credits::ArtistCredit;
use bae_records::ExistingArtist;
use bae_store::test_fixtures::empty_db;

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

/// A picked assignment's credit key and name come from one joined row, so a
/// stored value holding one without the other is damage, not "no credit name".
#[test]
fn a_picked_assignment_with_half_a_credit_name_is_unreadable() {
    for (key, name) in [(Some("alias"), None), (None, Some("Alias"))] {
        let stored: super::StoredAssignment = serde_json::from_value(serde_json::json!({
            "kind": "picked",
            "artist_id": "artist-1",
            "name": "Artist",
            "sort_name": null,
            "musicbrainz_artist_id": null,
            "discogs_artist_id": null,
            "credit_key": key,
            "credit_name": name,
            "credited_as": null,
            "join_phrase": null,
        }))
        .unwrap();

        let error = stored.credited().expect_err("half a credit name is damage");

        assert!(
            matches!(error, bae_store::DbError::Unreadable { .. }),
            "read as {error:?}, not refused as unreadable"
        );
    }
}
