//! One candidate's whole state saved as a unit.

use crate::folder_scanner::FolderCandidate;
use crate::tables::folder_scans::tag_snapshot::tests::*;
use crate::tables::import_list::tests::*;
use crate::tables::tests::*;
use crate::MetadataProvenance;
use crate::TriageTab;
use bae_catalogs::catalog::LookupCatalog;

fn save_expectation(
    candidate: &FolderCandidate,
    preparation: &crate::preparations::CandidatePreparation,
) -> crate::tables::CandidateSaveExpectation {
    crate::tables::CandidateSaveExpectation {
        edit_revision: preparation.file_edits.revision,
        metadata_revision: preparation.metadata_revision,
        scanned: Some(crate::tables::ScannedCandidateKey {
            watched_folder_path: candidate.watched_folder_path.clone(),
            candidate_path: crate::CandidateKey::folder(&candidate.path),
        }),
    }
}

#[tokio::test]
async fn superseded_preparation_leaves_snapshot_facts_and_cover_unchanged() {
    let (db, _tmp, root) = watched_root().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let (candidate, generation) = scanned_tagged_candidate(&db, &root).await;
    let key = crate::CandidateKey::folder(&candidate.path);
    let hash = candidate.files.content_hash();
    let reading = tag_snapshot(generation, 0);
    tables.replace_candidate_file_tag_snapshot(&root, &key, &reading).await.unwrap();
    let mut stale =
        tables.load_candidate_preparation(&hash).await.unwrap().unwrap().stored().unwrap();
    let expected = save_expectation(&candidate, &stale);
    crate::tables::import_state::tests::PaneControls::new(&db, &root, &key)
        .type_field(&hash, crate::CandidateEditField::AlbumTitle, "Newer album")
        .await
        .unwrap();
    let before = tables.load_candidate_preparation(&hash).await.unwrap().unwrap().stored().unwrap();
    stale.metadata_revision += 1;
    let mut replacement = reading.clone();
    let crate::file_tag_snapshot::TagReading::Stated(tags) = &mut replacement.files[0].tags else {
        panic!("the reading's first file states its tags");
    };
    tags.title = Some("Stale title".into());
    replacement.embedded_cover.as_mut().unwrap().data = vec![9, 8, 7];
    let result = tables
        .save_candidate_preparation(
            stale,
            expected,
            crate::tables::CandidateSaveExtras {
                file_tag_snapshot: Some(replacement),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert!(matches!(result, crate::tables::CandidateSaved::Superseded));
    assert_eq!(
        tables.load_candidate_preparation(&hash).await.unwrap().unwrap().stored().unwrap(),
        before
    );
    assert_eq!(
        tables.load_candidate_file_tag_snapshot(&key).await.unwrap().unwrap().snapshot,
        Some(reading)
    );
}

#[tokio::test]
async fn preparation_reshape_stores_complete_tags_after_their_file_rows() {
    let (db, _tmp, root) = watched_root().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let (candidate, generation) = scanned_tagged_candidate(&db, &root).await;
    let key = crate::CandidateKey::folder(&candidate.path);
    let hash = candidate.files.content_hash();
    let mut prep =
        tables.load_candidate_preparation(&hash).await.unwrap().unwrap().stored().unwrap();
    let expected = save_expectation(&candidate, &prep);
    prep.file_edits.revision += 1;
    prep.metadata_revision += 1;
    let reading = tag_snapshot(generation, prep.file_edits.revision);
    let result = tables
        .save_candidate_preparation(
            prep,
            expected,
            crate::tables::CandidateSaveExtras {
                file_tag_snapshot: Some(reading.clone()),
                reshaped_files: Some(vec![(key.clone(), candidate.files)]),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert!(matches!(result, crate::tables::CandidateSaved::Landed(_)));
    let stored = tables.load_candidate_file_tag_snapshot(&key).await.unwrap().unwrap();
    assert_eq!(stored.candidate.file_edit_revision, 1);
    assert_eq!(stored.snapshot, Some(reading));
    assert_eq!(stored_reading_rows(&db, &root, &key).await, (1, 2));
}

#[tokio::test]
async fn preparation_save_rejects_incomplete_or_unowned_tag_readings_atomically() {
    for invalid_cover in [false, true] {
        let (db, _tmp, root) = watched_root().await;
        let tables = crate::tables::ImportTables::new(db.clone());
        let (candidate, generation) = scanned_tagged_candidate(&db, &root).await;
        let key = crate::CandidateKey::folder(&candidate.path);
        let hash = candidate.files.content_hash();
        let original = tag_snapshot(generation, 0);
        tables.replace_candidate_file_tag_snapshot(&root, &key, &original).await.unwrap();
        let before =
            tables.load_candidate_preparation(&hash).await.unwrap().unwrap().stored().unwrap();
        let expected = save_expectation(&candidate, &before);
        let mut prep = before.clone();
        prep.metadata_revision += 1;
        let mut invalid = original.clone();
        if invalid_cover {
            invalid.embedded_cover.as_mut().unwrap().source_relative_path = "unread.flac".into();
        } else {
            invalid.files.pop();
        }
        assert!(tables
            .save_candidate_preparation(
                prep,
                expected,
                crate::tables::CandidateSaveExtras {
                    file_tag_snapshot: Some(invalid),
                    ..Default::default()
                }
            )
            .await
            .is_err());
        assert_eq!(
            tables.load_candidate_preparation(&hash).await.unwrap().unwrap().stored().unwrap(),
            before
        );
        assert_eq!(
            tables.load_candidate_file_tag_snapshot(&key).await.unwrap().unwrap().snapshot,
            Some(original)
        );
    }
}

/// A pick of a release nothing stored is refused and leaves nothing behind.
#[tokio::test]
async fn a_pick_of_a_release_nothing_stored_is_refused() {
    let (db, _tmp, root) = watched_root().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let candidate = scanned(&db, &root, "Album").await;
    save_verdict(&db, &candidate, "mb-verdict").await;
    let draft = tables
        .load_import_candidate_pane_rows(&candidate.files.content_hash())
        .await
        .unwrap()
        .draft
        .release_edit();
    crate::tables::ImportTables::new(db.clone())
        .replace_metadata(
            &candidate.files.content_hash(),
            &crate::CandidateKey::folder(&candidate.path),
            &draft,
            Some(&MetadataProvenance::ExternalRelease {
                record: bae_catalogs::records::LookupRef::new(
                    LookupCatalog::MusicBrainz,
                    "mb-never-fetched".to_string(),
                ),
            }),
            &[],
        )
        .await
        .expect_err("a pick of a release nothing stored is refused");

    let projection = tables.load_import_list_read(request(TriageTab::Pending).await).await.unwrap();
    assert_eq!(
        rows(&projection)[0].metadata_provenance,
        None,
        "the refused pick left nothing behind"
    );
}

/// A preparation whose author could not have read its draft from where it
/// says is refused as that contradiction, before any row moves.
#[tokio::test]
async fn a_preparation_whose_author_contradicts_its_provenance_is_refused() {
    let (db, _tmp, root) = watched_root().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let (candidate, _generation) = scanned_tagged_candidate(&db, &root).await;
    let hash = candidate.files.content_hash();
    let before = tables.load_candidate_preparation(&hash).await.unwrap().unwrap().stored().unwrap();
    let expected = save_expectation(&candidate, &before);
    let mut prep = before.clone();
    prep.metadata_revision += 1;
    prep.author = crate::MetadataAuthor::Nobody;
    prep.metadata.provenance = Some(MetadataProvenance::FileMetadata);

    let error = tables
        .save_candidate_preparation(prep, expected, crate::tables::CandidateSaveExtras::default())
        .await
        .expect_err("nobody reads a draft from the files' tags");

    assert!(
        matches!(
            &error,
            crate::ImportError::Provenance {
                contradiction: crate::ProvenanceContradiction {
                    author: crate::MetadataAuthor::Nobody,
                    provenance: Some(MetadataProvenance::FileMetadata),
                },
                ..
            }
        ),
        "{error:?}"
    );
    assert_eq!(
        tables.load_candidate_preparation(&hash).await.unwrap().unwrap().stored().unwrap(),
        before
    );
}

/// A run's verdict that keeps a cover its scan does not hold is refused as
/// that cover's refusal, and nothing of it lands.
#[tokio::test]
async fn a_run_verdict_keeping_a_cover_the_scan_does_not_hold_is_refused_as_the_cover() {
    let (db, _tmp, root) = watched_root().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let (candidate, _generation) = scanned_tagged_candidate(&db, &root).await;
    let hash = candidate.files.content_hash();
    let before = tables.load_candidate_preparation(&hash).await.unwrap().unwrap().stored().unwrap();
    let expected = save_expectation(&candidate, &before);
    let mut prep = before.clone();
    prep.metadata_revision += 1;
    prep.metadata.cover = Some(crate::cover_art::PreparedCover::Local("gone.jpg".to_string()));

    let error = tables
        .save_run_verdict(
            prep,
            expected,
            crate::tables::CandidateSaveExtras::default(),
            crate::CandidateKey::folder(&candidate.path),
            crate::IdentifyRunId::for_test(1),
        )
        .await
        .expect_err("the scan stored no gone.jpg");

    assert!(
        matches!(&error, crate::ImportError::Cover(crate::CoverRefusal::NotInFolder { file_id }) if file_id == "gone.jpg"),
        "{error:?}"
    );
    assert_eq!(
        tables.load_candidate_preparation(&hash).await.unwrap().unwrap().stored().unwrap(),
        before
    );
}

/// A file-tag reading taken at another revision of the candidate's files
/// than the save writes is refused as the candidate having changed, and
/// nothing of the save lands.
#[tokio::test]
async fn a_tag_reading_of_another_file_revision_refuses_the_save_as_changed() {
    let (db, _tmp, root) = watched_root().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let (candidate, generation) = scanned_tagged_candidate(&db, &root).await;
    let hash = candidate.files.content_hash();
    let before = tables.load_candidate_preparation(&hash).await.unwrap().unwrap().stored().unwrap();
    let expected = save_expectation(&candidate, &before);
    let mut prep = before.clone();
    prep.metadata_revision += 1;
    let reading = tag_snapshot(generation, prep.file_edits.revision + 1);

    let error = tables
        .save_candidate_preparation(
            prep,
            expected,
            crate::tables::CandidateSaveExtras {
                file_tag_snapshot: Some(reading),
                ..Default::default()
            },
        )
        .await
        .expect_err("the reading is of other files than the save writes");

    assert!(
        matches!(&error, crate::ImportError::Preparation(crate::tables::PreparationRefusal::TagsChanged(key)) if *key == crate::CandidateKey::folder(&candidate.path)),
        "{error:?}"
    );
    assert_eq!(
        tables.load_candidate_preparation(&hash).await.unwrap().unwrap().stored().unwrap(),
        before
    );
}
