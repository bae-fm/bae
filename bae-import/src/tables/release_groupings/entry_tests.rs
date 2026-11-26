//! A candidate's entry and where it stands, read as one statement, and the
//! writes that refuse by it.

use crate::file_tag_snapshot::FileTagSnapshot;
use crate::folder_scanner::FolderCandidate;
use crate::tables::folder_scans::tag_snapshot::tests::{scanned_tagged_candidate, tag_snapshot};
use crate::tables::tests::watched_root;
use crate::tables::ImportTables;
use crate::{CandidateFiles, CandidateStanding, ImportError};
use bae_store::Store;

/// A scanned release with a stored tag reading.
async fn scanned(
) -> (Store, bae_util::test_files::TestFolder, ImportTables, FolderCandidate, FileTagSnapshot) {
    let (db, temp, root) = watched_root().await;
    let (candidate, generation) = scanned_tagged_candidate(&db, &root).await;
    let tables = ImportTables::new(db.clone());
    let snapshot = tag_snapshot(generation, 0);
    assert!(tables
        .replace_candidate_file_tag_snapshot(&root, &candidate.key(), &snapshot)
        .await
        .unwrap());
    (db, temp, tables, candidate, snapshot)
}

#[tokio::test]
async fn one_statement_reads_the_entry_and_where_it_stands() {
    let (db, _temp, tables, candidate, snapshot) = scanned().await;
    let key = candidate.key();
    tables.set_import_candidate_skipped(&candidate.path, true).await.unwrap();
    tables.claim_candidate_import(&key, "import-1", None).await.unwrap();

    let counter = db.track_work_for_test().await.unwrap();
    let entry = tables.load_candidate_entry(&key).await.unwrap().expect("the scan stores it");
    assert_eq!(counter.take().app_read_statements, 1);
    assert_eq!(entry.candidate, candidate);
    assert_eq!(entry.generation, snapshot.scan_generation);
    assert!(entry.actionable());
    assert_eq!(
        entry.standing(),
        CandidateStanding { skipped: true, imported: None, claimed: true }
    );
    assert!(entry.runtime.importing());
}

#[tokio::test]
async fn one_statement_reads_the_entry_with_its_tag_snapshot() {
    let (db, _temp, tables, candidate, snapshot) = scanned().await;

    let counter = db.track_work_for_test().await.unwrap();
    let read = tables
        .load_candidate_file_tag_snapshot(&candidate.key())
        .await
        .unwrap()
        .expect("the scan stores it");
    assert_eq!(counter.take().app_read_statements, 1);
    assert_eq!(read.entry.candidate, candidate);
    assert_eq!(read.snapshot, Some(snapshot));
}

#[tokio::test]
async fn a_key_the_scan_stores_nothing_at_has_no_entry() {
    let (_db, _temp, tables, candidate, _) = scanned().await;
    let missing = crate::CandidateKey::folder(&candidate.path.join("missing"));
    assert!(tables.load_candidate_entry(&missing).await.unwrap().is_none());
}

/// Asked to set aside a release already set aside, the write changes nothing
/// and says so.
#[tokio::test]
async fn setting_aside_twice_changes_nothing_the_second_time() {
    let (_db, _temp, tables, candidate, _) = scanned().await;
    let key = candidate.key();
    assert!(!tables.set_candidate_skipped(&key, false).await.unwrap());
    assert!(tables.set_candidate_skipped(&key, true).await.unwrap());
    assert!(!tables.set_candidate_skipped(&key, true).await.unwrap());
    assert!(tables.set_candidate_skipped(&key, false).await.unwrap());
}

/// The claim reads the standing in its own write: a second import is refused,
/// and so is a claim prepared against other files.
#[tokio::test]
async fn a_claim_is_refused_by_what_its_own_write_reads() {
    let (_db, _temp, tables, candidate, _) = scanned().await;
    let key = candidate.key();
    let files = CandidateFiles::of(&candidate);
    let moved = CandidateFiles { file_edit_revision: 1, ..files.clone() };

    let refused = tables.claim_scanned_candidate_import(&key, &moved, "import-1", false).await;
    assert!(matches!(refused, Err(ImportError::Preparation(_))), "{refused:?}");
    tables.claim_scanned_candidate_import(&key, &files, "import-1", false).await.unwrap();
    let second = tables.claim_scanned_candidate_import(&key, &files, "import-2", false).await;
    assert!(matches!(second, Err(ImportError::CandidateImportInProgress)), "{second:?}");
}

/// A pane write on a claimed candidate is refused by the standing its own
/// transaction reads.
#[tokio::test]
async fn a_pane_write_is_refused_while_an_import_holds_the_candidate() {
    let (_db, _temp, tables, candidate, _) = scanned().await;
    let key = candidate.key();
    tables.claim_candidate_import(&key, "import-1", None).await.unwrap();

    let pick = tables.set_candidate_album_pick(&key, crate::AlbumPick::New).await;
    assert!(matches!(pick, Err(ImportError::CandidateImportInProgress)), "{pick:?}");
}
