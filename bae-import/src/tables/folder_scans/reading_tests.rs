//! A folder reading's commit: what it stores and what it prunes.

use crate::tables::folder_scans::tests::*;
use crate::tables::release_groupings::tests::{scanned, sidecar_in, sidecar_of};
use crate::tables::tests::*;
use crate::tables::watched_path::host_root;
use bae_store::test_fixtures::empty_db;
use bae_store::Store;

/// Reading one folder again prunes the sidecars it no longer yields at and
/// below that folder, and leaves those of the folders beside it, whatever
/// their names sort next to.
#[tokio::test]
async fn reading_a_folder_again_prunes_only_its_own_sidecars() {
    let (db, _temp, _) = scanned(&[]).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = host_root("/music");
    for folder in ["Album", "Album/Disc 1", "Album 2", "Album0"] {
        sidecar_in(&db, sidecar_of(folder, &["cover.jpg"])).await;
    }

    let stamp = tables.begin_folder_reading(&root).await.unwrap();
    tables
        .commit_folder_reading(crate::tables::FolderReadingCommit {
            watched_folder_path: root.clone(),
            folder: "Album".into(),
            stamp,
            decision: None,
            scanned_decisions: Vec::new(),
            items: Vec::new(),
            directories: Some(Vec::new()),
        })
        .await
        .unwrap();

    let folder = |name: &str| coven::path_text(&root.join(name));
    assert_eq!(
        db.query_texts_for_test("SELECT coven_path_text(folder) FROM scan_sidecar ORDER BY folder")
            .await
            .unwrap(),
        [folder("Album 2"), folder("Album0")]
    );
}

async fn row_generation(db: &Store, root: &std::path::Path, name: &str) -> i64 {
    let path = key_of(root, name);
    db.read(move |sql| {
        Ok(sql.query_row(
            "SELECT generation FROM scan_candidate WHERE path = ?",
            [path],
            |row| row.get::<_, i64>(0),
        )?)
    })
    .await
    .unwrap()
}

/// Every entry a folder reading took out, replaced or pruned.
fn removed_by(write: &crate::tables::FolderReadingWrite) -> Vec<crate::CandidateKey> {
    let mut removed: Vec<crate::CandidateKey> = write
        .writes
        .iter()
        .flat_map(|(_, write)| write.superseded_keys().to_vec())
        .chain(write.pruned.iter().cloned())
        .collect();
    removed.sort();
    removed.dedup();
    removed
}

/// Each decision replaces the folder's entries in the same write that stores
/// it, and leaves the sibling folder's row untouched.
#[tokio::test]
async fn a_folder_reading_trades_its_entries_in_the_write_that_stores_the_decision() {
    use crate::folder_scanner::{FolderReleaseDecision, FolderReleaseDecisionAuthor};

    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = &host_root("/mounted/library");
    let scanned = scanned_box_and_sibling(&db, root).await;
    let sibling_generation = row_generation(&db, root, "Other").await;

    let combined = commit_reading(
        &db,
        root,
        "Box",
        FolderReleaseDecision::CombineAsOneRelease,
        vec![combined_candidate(root, "Box")],
    )
    .await
    .unwrap();
    assert_eq!(removed_by(&combined), vec![key_of(root, "Box/CD1"), key_of(root, "Box/CD2")]);
    assert_eq!(stored_keys(&db, root).await, vec![key_of(root, "Box"), key_of(root, "Other")]);
    assert_eq!(
        tables
            .load_folder_release_decisions(root)
            .await
            .unwrap()
            .get(&crate::RelativeFolder::test("Box"))
            .map(|reading| (reading.decision, reading.author)),
        Some((FolderReleaseDecision::CombineAsOneRelease, FolderReleaseDecisionAuthor::User))
    );
    assert_eq!(row_generation(&db, root, "Other").await, sibling_generation);
    let snapshot = &tables.load_folder_scan_snapshots().await.unwrap()[0];
    assert!(snapshot.generation > scanned);
    assert_eq!(snapshot.status, crate::StoredScanOutcome::Complete);

    let separated = commit_reading(
        &db,
        root,
        "Box",
        FolderReleaseDecision::KeepAsSeparateReleases,
        vec![scanned_candidate(root, "Box/CD1"), scanned_candidate(root, "Box/CD2")],
    )
    .await
    .unwrap();
    assert_eq!(removed_by(&separated), vec![key_of(root, "Box")]);
    assert_eq!(
        stored_keys(&db, root).await,
        vec![key_of(root, "Box/CD1"), key_of(root, "Box/CD2"), key_of(root, "Other")]
    );
    assert_eq!(
        tables
            .load_folder_release_decisions(root)
            .await
            .unwrap()
            .get(&crate::RelativeFolder::test("Box"))
            .map(|reading| (reading.decision, reading.author)),
        Some((FolderReleaseDecision::KeepAsSeparateReleases, FolderReleaseDecisionAuthor::User))
    );
    assert_eq!(row_generation(&db, root, "Other").await, sibling_generation);
}

/// A folder reading takes out the folder's own entries and those below it,
/// and leaves a sibling whose name only begins with the folder's.
#[tokio::test]
async fn a_folder_reading_prunes_only_the_folder_and_below() {
    use crate::folder_scanner::FolderReleaseDecision;

    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = &host_root("/mounted/library");
    tables.add_watched_import_folder(root).await.unwrap();
    let generation = tables.begin_folder_scan(root, crate::VolumeKind::Local).await.unwrap();
    for name in ["Box/CD2", "Box 2", "Box/CD1"] {
        tables
            .save_folder_scan_item(root, generation, &scanned_candidate(root, name))
            .await
            .unwrap()
            .expect("the scan generation is current");
    }
    tables
        .finish_folder_scan(root, generation, None)
        .await
        .unwrap()
        .expect("the scan generation is current");

    let combined = commit_reading(
        &db,
        root,
        "Box",
        FolderReleaseDecision::CombineAsOneRelease,
        vec![combined_candidate(root, "Box")],
    )
    .await
    .unwrap();
    assert_eq!(removed_by(&combined), vec![key_of(root, "Box/CD1"), key_of(root, "Box/CD2")]);
    assert_eq!(stored_keys(&db, root).await, vec![key_of(root, "Box"), key_of(root, "Box 2")]);

    // Read with nothing found, the folder's own entry is pruned and the
    // sibling stays.
    let emptied =
        commit_reading(&db, root, "Box", FolderReleaseDecision::KeepAsSeparateReleases, Vec::new())
            .await
            .unwrap();
    assert_eq!(emptied.pruned, vec![key_of(root, "Box")]);
    assert_eq!(stored_keys(&db, root).await, vec![key_of(root, "Box 2")]);
}

/// A reading begun before another write to the root fails and writes nothing.
#[tokio::test]
async fn a_folder_reading_taken_before_the_root_moved_stores_nothing() {
    use crate::folder_scanner::FolderReleaseDecision;

    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = &host_root("/mounted/library");
    scanned_box_and_sibling(&db, root).await;
    let stamp = tables.begin_folder_reading(root).await.unwrap();
    let moved = tables.begin_folder_scan(root, crate::VolumeKind::Local).await.unwrap();

    let error = commit_reading_under(
        &db,
        root,
        "Box",
        FolderReleaseDecision::CombineAsOneRelease,
        vec![combined_candidate(root, "Box")],
        stamp,
    )
    .await
    .err()
    .expect("a reading of a root that moved is refused");
    assert!(
        matches!(
            &error,
            crate::ImportError::FolderReread(crate::tables::FolderRereadRefusal::RootMoved(_))
        ),
        "{error:?}"
    );
    assert!(tables
        .load_folder_release_decisions(root)
        .await
        .unwrap()
        .get(&crate::RelativeFolder::test("Box"))
        .is_none());
    assert_eq!(tables.load_folder_scan_snapshots().await.unwrap()[0].generation, moved);
    assert_eq!(
        stored_keys(&db, root).await,
        vec![key_of(root, "Box/CD1"), key_of(root, "Box/CD2"), key_of(root, "Other")]
    );
}

/// A reading that fails partway through its write leaves the decision, the
/// entries and the root's generation as they were.
#[tokio::test]
async fn a_folder_reading_that_fails_partway_rolls_back_whole() {
    use crate::folder_scanner::{FolderReleaseDecision, ScanItem};

    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = &host_root("/mounted/library");
    let generation = scanned_box_and_sibling(&db, root).await;
    let ScanItem::Valid(mut unstorable) = combined_candidate(root, "Box") else {
        panic!("the fixture is a valid candidate");
    };
    unstorable.files.files[0].file.version.size = u64::MAX;

    let failed = commit_reading(
        &db,
        root,
        "Box",
        FolderReleaseDecision::CombineAsOneRelease,
        vec![ScanItem::Valid(unstorable)],
    )
    .await
    .err()
    .expect("a file size SQLite cannot hold fails the write partway");
    assert!(
        matches!(failed, crate::ImportError::Database(bae_store::DbError::OutOfRange { .. })),
        "{failed:?}"
    );
    assert!(tables
        .load_folder_release_decisions(root)
        .await
        .unwrap()
        .get(&crate::RelativeFolder::test("Box"))
        .is_none());
    assert_eq!(tables.load_folder_scan_snapshots().await.unwrap()[0].generation, generation);
    assert_eq!(
        stored_keys(&db, root).await,
        vec![key_of(root, "Box/CD1"), key_of(root, "Box/CD2"), key_of(root, "Other")]
    );
}
