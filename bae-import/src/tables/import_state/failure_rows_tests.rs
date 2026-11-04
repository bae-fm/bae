//! An import's failure as stored on its candidate and read back.

use crate::tables::import_state::tests::*;
use crate::tables::tests::*;
use crate::ImportFailure;
use bae_db::test_fixtures::empty_db;
use bae_db::test_fixtures::fixed_now;
use bae_db::watched_path::host_root;

/// A failed import is recorded on the discovered candidate, and a later one
/// replaces it.
#[tokio::test]
async fn a_failure_on_a_discovered_candidate_is_replaced_then_cleared() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let (_, hash) = stored_pane_candidate(&db).await;

    tables
        .save_import_candidate_failure(
            &hash,
            0,
            &ImportFailure::error_only("the folder vanished", fixed_now()),
        )
        .await
        .unwrap();

    let failure = tables
        .load_import_candidate_pane_rows(&hash)
        .await
        .unwrap()
        .failure
        .expect("the failure is stored");
    assert_eq!(failure.reason, crate::ImportFailureReason::error("the folder vanished"));
    assert_eq!(failure.failed_at, fixed_now());
    assert!(tables
        .load_import_candidate_pane_rows(&hash)
        .await
        .unwrap()
        .draft
        .release_edit()
        .is_blank());

    tables
        .save_import_candidate_failure(
            &hash,
            0,
            &ImportFailure::error_only("the disc would not read", fixed_now()),
        )
        .await
        .unwrap();
    assert_eq!(
        tables.load_import_candidate_pane_rows(&hash).await.unwrap().failure.unwrap().reason,
        crate::ImportFailureReason::error("the disc would not read"),
        "the second failure replaces the first"
    );
}

#[tokio::test]
async fn a_late_import_failure_cannot_recreate_state_after_root_removal() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = host_root("/mounted/library");
    let candidate = scanned_candidate(&root, "Release");
    let crate::folder_scanner::ScanItem::Valid(folder) = &candidate else {
        panic!("the fixture must produce a valid candidate");
    };
    let content_hash = folder.files.content_hash();
    tables.add_watched_import_folder(&root).await.unwrap();
    let generation = tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
    tables.save_folder_scan_item(&root, generation, &candidate).await.unwrap();
    tables.remove_watched_import_folders(vec![root.clone()], None).await.unwrap();

    tables
        .save_import_candidate_failure(
            &content_hash,
            0,
            &crate::ImportFailure::error_only("the source disappeared", fixed_now()),
        )
        .await
        .expect_err("a removed candidate cannot receive an import failure");
    assert!(tables.load_import_candidate_state(&content_hash).await.unwrap().is_none());
}
