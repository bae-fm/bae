#[tokio::test]
async fn folder_release_decision_is_idempotent_and_root_scoped() {
    use crate::folder_scanner::{FolderReleaseDecision, FolderReleaseDecisionKey};

    let (db, _tmp) = empty_db().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let other = host_root("/other/library");
    let key = FolderReleaseDecisionKey {
        watched_folder_path: host_root("/mounted/library"),
        relative_folder: bae_db::RelativeFolder::test("Collection/Release Wrapper"),
    };
    tables.add_watched_import_folder(&key.watched_folder_path).await.unwrap();
    tables.add_watched_import_folder(&other).await.unwrap();

    store_user_folder_decision(&db, &key, FolderReleaseDecision::CombineAsOneRelease)
        .await
        .unwrap();
    store_user_folder_decision(&db, &key, FolderReleaseDecision::CombineAsOneRelease)
        .await
        .unwrap();
    store_user_folder_decision(
        &db,
        &FolderReleaseDecisionKey {
            watched_folder_path: other,
            relative_folder: key.relative_folder.clone(),
        },
        FolderReleaseDecision::KeepAsSeparateReleases,
    )
    .await
    .unwrap();

    let decisions = tables.load_folder_release_decisions(&key.watched_folder_path).await.unwrap();
    assert_eq!(
        decisions.get(&key.relative_folder).map(|reading| (reading.decision, reading.author)),
        Some((
            FolderReleaseDecision::CombineAsOneRelease,
            crate::folder_scanner::FolderReleaseDecisionAuthor::User
        ))
    );
}

/// The CD ripper a sheet names is kept with the scan, so a folder read back
/// from a cold database still proves the CD rip its sheet does.
#[tokio::test]
async fn a_sheet_s_ripper_survives_a_relaunch() {
    use crate::folder_scanner::{
        collect_release_candidate_files_with_scope, FolderCandidate, ScanItem, StoredCandidateEdits,
    };

    let (db, _tmp) = empty_db().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let folder = tempfile::TempDir::new().unwrap();
    std::fs::copy(
        bae_audio::test_fixtures::cue_flac_dir().join("Test Album.flac"),
        folder.path().join("cd.flac"),
    )
    .unwrap();
    std::fs::write(
        folder.path().join("cd.cue"),
        "REM COMMENT \"ExactAudioCopy v1.1\"\nPERFORMER \"Artist One\"\nTITLE \"Album One\"\n\
         FILE \"cd.wav\" WAVE\n  TRACK 01 AUDIO\n    INDEX 01 00:00:00\n",
    )
    .unwrap();
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
    tables.save_folder_scan_item(
        std::path::Path::new(&root),
        generation,
        &ScanItem::Valid(FolderCandidate {
            path: folder.path().to_path_buf(),
            file_root: folder.path().to_path_buf(),
            name: "Release".to_string(),
            files: scanned,
            watched_folder_path: std::path::PathBuf::from(&root),
            scope: crate::ReleaseFileScope::Recursive,
            file_edit_revision: 0,
            relative_folder: bae_db::RelativeFolder::root(),
            grouping: None,
        }),
    )
    .await
    .unwrap();
    tables.finish_folder_scan(std::path::Path::new(&root), generation, None).await.unwrap();

    let restored = tables.load_folder_scan_snapshots().await.unwrap();
    let ScanItem::Valid(restored) = &restored[0].items[0] else {
        panic!("the persisted candidate keeps its valid variant");
    };
    let sheets: Vec<_> = restored.files.track_sheets().map(|sheet| sheet.sheet.ripper).collect();
    assert_eq!(sheets, vec![Some(crate::cue_flac::CdRipper::ExactAudioCopy)]);
}

/// A sheet whose boundaries run past its audio reads back refused for its
/// timing, with the audio it named, from a cold database — the pane says why
/// the sheet carves nothing without working it out again.
#[tokio::test]
async fn a_timing_refusal_survives_a_relaunch() {
    use crate::folder_scanner::{
        collect_release_candidate_files_with_scope, FolderCandidate, ScanItem, SheetAudioFile,
        SheetBinding, StoredCandidateEdits,
    };

    let (db, _tmp) = empty_db().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let folder = tempfile::TempDir::new().unwrap();
    std::fs::copy(
        bae_audio::test_fixtures::cue_flac_dir().join("Test Album.flac"),
        folder.path().join("cd.flac"),
    )
    .unwrap();
    std::fs::write(
        folder.path().join("cd.cue"),
        "FILE \"cd.flac\" WAVE\n  TRACK 01 AUDIO\n    INDEX 01 99:00:00\n",
    )
    .unwrap();
    let scanned = collect_release_candidate_files_with_scope(
        folder.path(),
        crate::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .unwrap();
    let refused = SheetBinding::RefusedTiming {
        files: vec![SheetAudioFile {
            file_reference: "cd.flac".to_string(),
            file_id: "cd.flac".to_string(),
        }],
    };
    assert_eq!(scanned.track_sheets().next().unwrap().binding, &refused);
    let root = folder.path().to_path_buf();
    tables.add_watched_import_folder(std::path::Path::new(&root)).await.unwrap();
    let generation = tables
        .begin_folder_scan(std::path::Path::new(&root), crate::VolumeKind::Local)
        .await
        .unwrap();
    tables.save_folder_scan_item(
        std::path::Path::new(&root),
        generation,
        &ScanItem::Valid(FolderCandidate {
            path: folder.path().to_path_buf(),
            file_root: folder.path().to_path_buf(),
            name: "Release".to_string(),
            files: scanned,
            watched_folder_path: std::path::PathBuf::from(&root),
            scope: crate::ReleaseFileScope::Recursive,
            file_edit_revision: 0,
            relative_folder: bae_db::RelativeFolder::root(),
            grouping: None,
        }),
    )
    .await
    .unwrap();
    tables.finish_folder_scan(std::path::Path::new(&root), generation, None).await.unwrap();

    let restored = tables.load_folder_scan_snapshots().await.unwrap();
    let ScanItem::Valid(restored) = &restored[0].items[0] else {
        panic!("the persisted candidate keeps its valid variant");
    };
    assert_eq!(restored.files.track_sheets().next().unwrap().binding, &refused);
}
