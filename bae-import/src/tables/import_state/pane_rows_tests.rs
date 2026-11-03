//! A draft's rows, the cover and the artist assignments as stored and read back.

use crate::tables::import_state::tests::*;
use crate::{ArtistAssignment, CoverSelection, RawTrackEdit, TrackArtistAssignments};
use bae_catalogs::catalog::Catalog;
use bae_catalogs::credits::ArtistCredit;
use bae_db::test_fixtures::empty_db;
use bae_db::watched_path::host_root;
use bae_db::ExistingArtist;

fn remote_with_copies(url: &str) -> CoverSelection {
    use bae_catalogs::remote_image::{DownscaledCopy, RemoteImageSet};
    CoverSelection::Remote(
        RemoteImageSet::with_copies(
            url.to_string(),
            [250, 500]
                .map(|max_edge| DownscaledCopy { url: format!("{url}-{max_edge}"), max_edge })
                .to_vec(),
        ),
        Catalog::MusicBrainz,
    )
}

/// The bytes a remote cover is stored with, fetched with it; none for a cover
/// the folder holds.
fn fetched_bytes(cover: &CoverSelection) -> Option<bae_catalogs::remote_image::RemoteImage> {
    matches!(cover, CoverSelection::Remote(_, _)).then(|| bae_catalogs::remote_image::RemoteImage {
        bytes: vec![1, 2, 3],
        content_type: bae_util::content_type::ContentType::Jpeg,
    })
}

/// Every kind of cover choice comes back as it went in, a remote one with its
/// downscaled copies.
#[tokio::test]
async fn a_cover_choice_round_trips_in_every_shape() {
    for cover in [
        CoverSelection::Local("cover.jpg".to_string()),
        CoverSelection::Remote(
            bae_catalogs::remote_image::RemoteImageSet::original(
                "https://example.invalid/front".to_string(),
            ),
            Catalog::Discogs,
        ),
        remote_with_copies("https://example.invalid/front"),
    ] {
        let (db, _tmp) = empty_db().await;
        let tables = crate::tables::ImportTables::new(db.clone());
        let (_, hash) = stored_pane_candidate(&db).await;

        crate::CandidatePreparations::new(db.clone())
            .set_cover(&hash, &cover, fetched_bytes(&cover).as_ref())
            .await
            .unwrap();

        assert_eq!(tables.load_import_candidate_pane_rows(&hash).await.unwrap().cover, Some(cover));
    }
}

/// Choosing again replaces the previous choice's copies rather than adding the
/// new ones beside them.
#[tokio::test]
async fn a_new_cover_choice_leaves_none_of_the_old_copies() {
    let (db, _tmp) = empty_db().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let (_, hash) = stored_pane_candidate(&db).await;
    let preparations = crate::CandidatePreparations::new(db.clone());

    for cover in [
        remote_with_copies("https://example.invalid/first"),
        CoverSelection::Local("cover.jpg".to_string()),
        remote_with_copies("https://example.invalid/second"),
    ] {
        preparations.set_cover(&hash, &cover, fetched_bytes(&cover).as_ref()).await.unwrap();
        assert_eq!(tables.load_import_candidate_pane_rows(&hash).await.unwrap().cover, Some(cover));
    }
}

#[tokio::test]
async fn picked_artist_assignments_resolve_the_canonical_artist_row() {
    let (db, _tmp) = empty_db().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let (_, hash) = stored_pane_candidate(&db).await;
    let existing = existing_artist();
    db.insert_artist(&existing).await.unwrap();

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
    crate::CandidatePreparations::new(db.clone())
        .set_album_artists(&hash, &assignments)
        .await
        .unwrap();

    let stored = tables.load_import_candidate_pane_rows(&hash).await.unwrap();
    assert_eq!(stored.draft.album_artist_assignments, assignments);

    let explicit_empty = RawTrackEdit {
        id: "candidate-track-0".to_string(),
        title: "Track Title".to_string(),
        artist_assignments: TrackArtistAssignments::Explicit(Vec::new()),
        file: Some(stored.draft.tracks[0].edit.file.clone()),
    };
    crate::CandidatePreparations::new(db.clone())
        .set_track_edit(&hash, &explicit_empty)
        .await
        .unwrap();
    assert_eq!(
        tables.load_import_candidate_pane_rows(&hash).await.unwrap().draft.tracks[0]
            .edit
            .artist_assignments,
        TrackArtistAssignments::Explicit(Vec::new())
    );
}

#[tokio::test]
async fn a_picked_artist_assignment_to_a_missing_row_is_rejected() {
    let (db, _tmp) = empty_db().await;
    let (_, hash) = stored_pane_candidate(&db).await;

    let error = crate::CandidatePreparations::new(db.clone())
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
    let (db, _tmp) = empty_db().await;
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
    crate::CandidatePreparations::new(db.clone()).set_track_edit(&hash, &edit).await.unwrap();

    let stored = tables.load_import_candidate_pane_rows(&hash).await.unwrap();
    assert_eq!(stored.draft.tracks.len(), 1);
    assert_eq!(stored.draft.tracks[0].edit.title, "Edited title");
    assert_eq!(Some(stored.draft.tracks[0].edit.file.clone()), edit.file);
}

/// A track length a catalog states past what SQLite stores fails the
/// preparation's save with an error, rather than stopping the import.
#[tokio::test]
async fn a_stated_length_past_sqlites_range_fails_the_save() {
    let (db, _tmp) = empty_db().await;
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
                assets: crate::CandidatePreparedAssets::default(),
            },
        )
        .await;

    assert!(saved.is_err(), "{saved:?}");
}
