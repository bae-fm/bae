//! A candidate's sheet decisions as stored and read back by the next scan.

use crate::tables::import_state::tests::*;
use bae_store::test_fixtures::empty_db;

/// A binding the user cleared survives a relaunch: it is stored under the
/// candidate's content hash, read back from a cold database, and the scan
/// that follows reports the folder as they settled it rather than as its
/// filenames read.
///
/// The scan is the point — a binding that round-tripped through SQLite but
/// never reached a folder's roles would be a stored value nothing consumes.
#[tokio::test]
async fn a_cleared_binding_survives_a_relaunch() {
    use crate::folder_scanner::{
        collect_release_candidate_files_with_scope, CandidateFileEdits, SheetBindingEdits,
        StoredCandidateEdits, UserSheetBinding,
    };

    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let folder = walkthrough_folder();
    let scanned = collect_release_candidate_files_with_scope(
        folder.path(),
        crate::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .unwrap();
    assert_eq!(scanned.track_count(), 12, "the unique same-stem audio is bound automatically");
    let root = folder.path().to_path_buf();
    tables.add_watched_import_folder(std::path::Path::new(&root)).await.unwrap();
    let generation = tables
        .begin_folder_scan(std::path::Path::new(&root), crate::VolumeKind::Local)
        .await
        .unwrap();
    let candidate = crate::folder_scanner::FolderCandidate {
        path: folder.path().to_path_buf(),
        file_root: folder.path().to_path_buf(),
        name: "Release".to_string(),
        files: scanned.clone(),
        watched_folder_path: std::path::PathBuf::from(&root),
        scope: crate::ReleaseFileScope::Recursive,
        file_edit_revision: 0,
        relative_folder: crate::RelativeFolder::root(),
        grouping: None,
    };
    tables
        .save_folder_scan_item(
            std::path::Path::new(&root),
            generation,
            &crate::folder_scanner::ScanItem::Valid(candidate),
        )
        .await
        .unwrap();
    tables.finish_folder_scan(std::path::Path::new(&root), generation, None).await.unwrap();

    let mut edits = SheetBindingEdits::default();
    edits.set_reference("cd.cue".to_string(), "cd.wav".to_string(), UserSheetBinding::Cleared);
    let candidate_edits = CandidateFileEdits { sheet_bindings: edits, ..Default::default() };
    let mut settled = scanned.clone();
    settled.apply_candidate_file_edits(&candidate_edits).unwrap();
    let hash = scanned.content_hash();
    let (metadata_revision, mut draft) = current_draft(&db, &hash).await;
    draft.tracks = crate::pane::blank_candidate_draft(&settled).tracks;
    crate::CandidatePreparations::new(db.clone())
        .store_file_decisions(
            &as_read(&hash, metadata_revision),
            &crate::CandidateKey::folder(folder.path()),
            &candidate_edits,
            &[(crate::CandidateKey::folder(folder.path()), settled)],
            draft,
        )
        .await
        .unwrap();

    let current = tables.load_candidate_file_edits(&scanned.content_hash()).await.unwrap();
    assert_eq!(current.revision, 1);
    assert_eq!(
        current.sheet_bindings.get("cd.cue").and_then(|references| references.get("cd.wav")),
        Some(&UserSheetBinding::Cleared)
    );
    assert_eq!(
        tables.load_candidate_file_edits("missing").await.unwrap(),
        CandidateFileEdits::default()
    );

    let restored = tables.load_folder_scan_snapshots().await.unwrap();
    let crate::folder_scanner::ScanItem::Valid(restored_candidate) = &restored[0].items[0] else {
        panic!("the persisted candidate keeps its valid variant");
    };
    assert_eq!(restored_candidate.file_edit_revision, 1);
    assert_eq!(restored_candidate.track_count(), 1);
    assert!(restored_candidate.files.bound_sheets().is_empty());

    // A subsequent scan reads the same decisions, at the revision they stand
    // at, and derives the same shape as the candidate restored before that
    // scan.
    let stored = tables.load_stored_candidate_edits().await.unwrap();
    assert_eq!(stored.revision_for_hash(&scanned.content_hash()), 1);
    assert_eq!(stored.revision_for_hash("missing"), 0);
    let reopened = collect_release_candidate_files_with_scope(
        folder.path(),
        crate::ReleaseFileScope::Recursive,
        &stored,
    )
    .unwrap();

    assert_eq!(
        reopened.track_count(),
        1,
        "the cleared binding read back from disk is the one the scan applies"
    );
    assert!(reopened.bound_sheets().is_empty());
}

/// A disc assignment is stored under the candidate's content hash, and the
/// next scan orders the discs by it rather than by cue filename.
#[tokio::test]
async fn a_disc_assignment_survives_a_relaunch() {
    use crate::folder_scanner::{
        collect_release_candidate_files_with_scope, CandidateFileEdits, SheetDisc, SheetDiscEdits,
        StoredCandidateEdits,
    };

    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let folder = two_sheet_folder();
    let scanned = collect_release_candidate_files_with_scope(
        folder.path(),
        crate::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .unwrap();
    let root = folder.path().to_path_buf();
    tables.add_watched_import_folder(std::path::Path::new(&root)).await.unwrap();
    let generation = tables
        .begin_folder_scan(std::path::Path::new(&root), crate::VolumeKind::Local)
        .await
        .unwrap();
    let candidate = crate::folder_scanner::FolderCandidate {
        path: folder.path().to_path_buf(),
        file_root: folder.path().to_path_buf(),
        name: "Release".to_string(),
        files: scanned.clone(),
        watched_folder_path: std::path::PathBuf::from(&root),
        scope: crate::ReleaseFileScope::Recursive,
        file_edit_revision: 0,
        relative_folder: crate::RelativeFolder::root(),
        grouping: None,
    };
    tables
        .save_folder_scan_item(
            std::path::Path::new(&root),
            generation,
            &crate::folder_scanner::ScanItem::Valid(candidate),
        )
        .await
        .unwrap();
    tables.finish_folder_scan(std::path::Path::new(&root), generation, None).await.unwrap();

    // The rip named its sheets the other way round: `alpha.cue` is disc two.
    let mut sheet_discs = SheetDiscEdits::default();
    sheet_discs.set("alpha.cue".to_string(), SheetDisc::Disc { number: 2 });
    sheet_discs.set("beta.cue".to_string(), SheetDisc::Disc { number: 1 });
    let candidate_edits = CandidateFileEdits { sheet_discs, ..Default::default() };
    let mut settled = scanned.clone();
    settled.apply_candidate_file_edits(&candidate_edits).unwrap();
    let hash = scanned.content_hash();
    let (metadata_revision, mut draft) = current_draft(&db, &hash).await;
    draft.tracks = crate::pane::blank_candidate_draft(&settled).tracks;
    crate::CandidatePreparations::new(db.clone())
        .store_file_decisions(
            &as_read(&hash, metadata_revision),
            &crate::CandidateKey::folder(folder.path()),
            &candidate_edits,
            &[(crate::CandidateKey::folder(folder.path()), settled)],
            draft,
        )
        .await
        .unwrap();

    let current = tables.load_candidate_file_edits(&scanned.content_hash()).await.unwrap();
    assert_eq!(current.revision, 1);
    assert_eq!(current.sheet_discs.get("alpha.cue"), Some(&SheetDisc::Disc { number: 2 }));

    // The next scan reads the stored decisions.
    let stored = tables.load_stored_candidate_edits().await.unwrap();
    let reopened = collect_release_candidate_files_with_scope(
        folder.path(),
        crate::ReleaseFileScope::Recursive,
        &stored,
    )
    .unwrap();
    assert_eq!(
        reopened
            .carving_sheets()
            .iter()
            .map(|sheet| (sheet.file.relative_path.as_str(), sheet.disc))
            .collect::<Vec<_>>(),
        vec![
            ("alpha.cue", SheetDisc::Disc { number: 2 }),
            ("beta.cue", SheetDisc::Disc { number: 1 }),
        ],
    );
}

/// Two bound single-track sheets, each naming the audio beside it.
fn two_sheet_folder() -> tempfile::TempDir {
    let tmp = tempfile::TempDir::new().unwrap();
    for stem in ["alpha", "beta"] {
        std::fs::copy(
            bae_audio::test_fixtures::cue_flac_dir().join("Test Album.flac"),
            tmp.path().join(format!("{stem}.flac")),
        )
        .unwrap();
        std::fs::write(
            tmp.path().join(format!("{stem}.cue")),
            format!(
                "PERFORMER \"Artist Name\"\nTITLE \"Album Title\"\n\
                 FILE \"{stem}.flac\" WAVE\n  TRACK 01 AUDIO\n    \
                 TITLE \"Track Title\"\n    INDEX 01 00:00:00\n",
            ),
        )
        .unwrap();
    }
    tmp
}
