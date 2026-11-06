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
        tables.load_candidate_file_tag_snapshot(&root, &key).await.unwrap().unwrap().snapshot,
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
    let stored = tables.load_candidate_file_tag_snapshot(&root, &key).await.unwrap().unwrap();
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
            tables.load_candidate_file_tag_snapshot(&root, &key).await.unwrap().unwrap().snapshot,
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
    crate::CandidatePreparations::new(db.clone())
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
