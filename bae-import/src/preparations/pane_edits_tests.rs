//! The pane's own writes, each against the candidate as stored.

use crate::cover_art::PreparedCover;
use crate::tables::import_state::tests::*;
use crate::tables::tests::*;
use crate::{AudioFile, CandidateEditField};
use bae_catalogs::catalog::Catalog;
use bae_db::test_fixtures::empty_db;
use bae_db::watched_path::host_root;

/// A pane edit with no candidate row under it is a defect, not a state to
/// absorb: the form is drawn only under a pick, and a pick writes that row.
#[tokio::test]
async fn a_pane_edit_without_a_candidate_row_is_refused() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let hash = pane_candidate().content_hash();

    for error in [
        crate::CandidatePreparations::new(db.clone())
            .set_prepared_cover(
                &host_root("/music"),
                &pane_candidate_key(),
                &as_read(&hash, 0),
                &crate::cover_art::PreparedCover::Local("cover.jpg".to_string()),
            )
            .await
            .expect_err("a cover with nothing picked"),
        crate::CandidatePreparations::new(db.clone())
            .set_field_prepared(
                &host_root("/music"),
                &pane_candidate_key(),
                &hash,
                0,
                crate::DraftFieldEdit::Text {
                    field: CandidateEditField::PressingYear,
                    value: "1991".to_string(),
                },
            )
            .await
            .expect_err("a field with nothing picked"),
        crate::CandidatePreparations::new(db.clone())
            .set_track_edit_prepared(
                &host_root("/music"),
                &pane_candidate_key(),
                &as_read(&hash, 0),
                &edited_row("import-track-0", "Track Title", None),
            )
            .await
            .expect_err("a row with nothing picked"),
    ] {
        assert!(
            error.to_string().contains("no candidate state row"),
            "{error} should say what is missing"
        );
    }
}

#[tokio::test]
async fn cover_write_refuses_a_candidate_key_that_now_names_other_files() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let (_, hash) = stored_pane_candidate(&db).await;
    let root = host_root("/music");
    let mut replacement = scanned_candidate(&root, "Album");
    let crate::folder_scanner::ScanItem::Valid(candidate) = &mut replacement else {
        unreachable!("the fixture creates a valid candidate");
    };
    candidate.files.files[0].file.version.size += 1;
    let generation = tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
    tables
        .save_folder_scan_item(&root, generation, &replacement)
        .await
        .unwrap()
        .expect("the current scan accepts the replacement");
    tables.finish_folder_scan(&root, generation, None).await.unwrap();

    let error = crate::CandidatePreparations::new(db.clone())
        .set_prepared_cover(
            &root,
            &pane_candidate_key(),
            &as_read(&hash, 0),
            &PreparedCover::Local("cover.jpg".to_string()),
        )
        .await
        .expect_err("the candidate key no longer names the prepared files");
    assert!(error.to_string().contains("candidate changed"), "{error}");
}

#[tokio::test]
async fn a_stale_remote_cover_write_leaves_the_current_selection_and_bytes() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let (_, hash) = stored_pane_candidate(&db).await;
    let current_cover = PreparedCover::Remote {
        image: bae_catalogs::remote_image::RemoteImageSet::original(
            "https://example.invalid/current".to_string(),
        ),
        source: Catalog::Discogs,
        fetched: bae_catalogs::remote_image::RemoteImage {
            bytes: vec![1, 2, 3],
            content_type: bae_util::content_type::ContentType::Jpeg,
        },
    };
    crate::CandidatePreparations::new(db.clone())
        .set_prepared_cover(
            &host_root("/music"),
            &pane_candidate_key(),
            &as_read(&hash, 0),
            &current_cover,
        )
        .await
        .unwrap();

    let stale_cover = PreparedCover::Remote {
        image: bae_catalogs::remote_image::RemoteImageSet::original(
            "https://example.invalid/stale".to_string(),
        ),
        source: Catalog::MusicBrainz,
        fetched: bae_catalogs::remote_image::RemoteImage {
            bytes: vec![4, 5, 6],
            content_type: bae_util::content_type::ContentType::Png,
        },
    };
    crate::CandidatePreparations::new(db.clone())
        .set_prepared_cover(
            &host_root("/music"),
            &pane_candidate_key(),
            &as_read(&hash, 0),
            &stale_cover,
        )
        .await
        .expect_err("revision zero is stale after the first selection");

    let stored = crate::preparations::stored_preparation(&tables, &hash)
        .await
        .unwrap()
        .expect("the candidate is prepared");
    assert_eq!(stored.metadata.cover, Some(current_cover));
}

/// Field writes update the one complete draft and leave its other values intact.
#[tokio::test]
async fn draft_field_writes_keep_album_and_pressing_years_distinct() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    fetched(&db, "rel-1").await;
    let (_, hash) = stored_pane_candidate(&db).await;
    let seed = metadata_draft("Seeded Title", "Artist Name");
    crate::CandidatePreparations::new(db.clone())
        .replace_metadata(&hash, &pane_candidate_key(), &seed, Some(&release_pick("rel-1")), &[])
        .await
        .unwrap();

    PaneControls::of_pane_candidate(&db)
        .type_field(&hash, CandidateEditField::AlbumYear, "1987")
        .await
        .unwrap();
    PaneControls::of_pane_candidate(&db)
        .type_field(&hash, CandidateEditField::PressingYear, "1991")
        .await
        .unwrap();
    PaneControls::of_pane_candidate(&db)
        .type_field(&hash, CandidateEditField::AlbumTitle, "Album Title")
        .await
        .unwrap();

    let stored = tables.load_import_candidate_pane_rows(&hash).await.unwrap().draft;
    assert_eq!(stored.album_title, "Album Title");
    assert_eq!(stored.album_year, "1987");
    assert_eq!(stored.pressing.year, "1991");
    assert_eq!(stored.album_artist_assignments, seed.album_artist_assignments);
    let mut expected_tracks = seed.tracks;
    expected_tracks[0].file = Some(AudioFile::Standalone { file_id: "01.flac".into() });
    assert_eq!(stored.release_edit().tracks, expected_tracks);
}

#[tokio::test]
async fn metadata_revision_advances_for_every_draft_and_cover_mutation() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    fetched(&db, "rel-1").await;
    let (_, hash) = stored_pane_candidate(&db).await;

    assert_eq!(
        crate::CandidatePreparations::new(db.clone())
            .replace_metadata(
                &hash,
                &pane_candidate_key(),
                &metadata_draft("Album", "Artist"),
                Some(&release_pick("rel-1")),
                &[],
            )
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        PaneControls::of_pane_candidate(&db)
            .type_field(&hash, CandidateEditField::PressingYear, "1991")
            .await
            .unwrap(),
        2
    );
    assert_eq!(
        PaneControls::of_pane_candidate(&db)
            .choose_cover(&hash, &crate::cover_art::PreparedCover::Local("cover.jpg".to_string()))
            .await
            .unwrap(),
        3
    );
    assert_eq!(
        PaneControls::of_pane_candidate(&db)
            .set_album_artists(&hash, &[crate::Credited::plain(credit_named("Different Artist"))],)
            .await
            .unwrap(),
        4
    );
    assert_eq!(
        PaneControls::of_pane_candidate(&db)
            .edit_track(
                &hash,
                &edited_row(
                    "candidate-track-0",
                    "Changed title",
                    Some(AudioFile::Standalone { file_id: "01.flac".into() })
                ),
            )
            .await
            .unwrap(),
        5
    );
}

/// A person editing the draft identification wrote makes it theirs: after the
/// edit it is their answer, not the run's pick waiting on the auto-import check.
#[tokio::test]
async fn an_edit_to_identification_s_draft_makes_the_person_its_author() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    fetched(&db, "rel-1").await;
    let candidate =
        track_files_candidate(&[("01 Track.flac", 123_456), ("02 Track.flac", 234_567)]);
    let hash = candidate.content_hash();
    let row = concluding(
        new_candidate_row(&hash, &host_root("/music/Some Album"), &sample_verdict()),
        "rel-1",
    );
    store_candidate_state(&db, &candidate, &row.candidate_key.test_folder()).await;
    crate::CandidatePreparations::new(db.clone()).store_verdict(&row).await.unwrap();

    PaneControls::new(&db, &host_root("/music"), &row.candidate_key)
        .type_field(&hash, crate::CandidateEditField::AlbumTitle, "Album")
        .await
        .unwrap();

    let loaded = tables.load_import_candidate_states().await.unwrap();
    let stored = loaded.get(&hash).expect("the candidate reads back");
    assert_eq!(stored.metadata_author, crate::MetadataAuthor::Person);
    assert!(
        matches!(
            stored.metadata_provenance,
            Some(crate::MetadataProvenance::ExternalRelease { .. })
        ),
        "the edit leaves where the draft was read from"
    );
}
