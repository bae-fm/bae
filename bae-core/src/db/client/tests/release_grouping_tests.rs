//! Releases picked together, as the store keeps them: built from the releases
//! they take in, in the same write as whatever changes one of them.

use super::{candidate, empty_db};
use crate::import::folder_scanner::{FolderCandidate, ScanItem};
use crate::import::watched_folder::host_root;

/// `names` under one watched root, scanned to completion.
async fn scanned(
    names: &[&str],
) -> (
    super::super::Database,
    tempfile::TempDir,
    Vec<FolderCandidate>,
) {
    let (db, temp) = empty_db().await;
    let root = host_root("/music");
    db.add_watched_import_folder(&root).await.unwrap();
    let generation = db
        .begin_folder_scan(&root, crate::import::VolumeKind::Local)
        .await
        .unwrap();
    let mut candidates = Vec::new();
    for name in names {
        let mut folder = candidate(&root, name);
        // A file of its own per folder, so no two share a path on disk.
        for entry in &mut folder.files.files {
            entry.file.path = folder.path.join(&entry.file.relative_path);
        }
        db.save_folder_scan_item(&root, generation, &ScanItem::Valid(folder.clone()))
            .await
            .unwrap()
            .expect("the scan generation is current");
        candidates.push(folder);
    }
    db.finish_folder_scan(&root, generation, None)
        .await
        .unwrap();
    (db, temp, candidates)
}

async fn stored(db: &super::super::Database, key: &str) -> Option<ScanItem> {
    db.load_folder_scan_item(key).await.unwrap()
}

/// A release that one of a grouping's releases changes under is rebuilt in
/// the write that changes it: the grouping never lists files its release no
/// longer holds.
#[tokio::test]
async fn a_changed_release_rebuilds_the_grouping_in_the_same_write() {
    let (db, _temp, members) = scanned(&["Volume A", "Volume B"]).await;
    let root = host_root("/music");
    db.combine_releases("grouping:test".into(), members.clone())
        .await
        .unwrap();
    let Some(ScanItem::Valid(before)) = stored(&db, "grouping:test").await else {
        panic!("the grouping built its release");
    };

    let mut changed = members[1].clone();
    changed.files.files[0].file.size += 1;
    let generation = db
        .begin_folder_scan(&root, crate::import::VolumeKind::Local)
        .await
        .unwrap();
    let write = db
        .save_folder_scan_item(&root, generation, &ScanItem::Valid(changed.clone()))
        .await
        .unwrap()
        .unwrap();
    let rebuilt = write.regrouped().expect("the write stored the change");
    assert_eq!(rebuilt.written.len(), 1);
    let Some(ScanItem::Valid(after)) = stored(&db, "grouping:test").await else {
        panic!("the grouping still has its release");
    };
    assert_ne!(after.files.content_hash(), before.files.content_hash());
    assert!(after
        .files
        .release_files()
        .any(|file| file.size == changed.files.files[0].file.size));
}

/// A release a grouping takes in that goes away leaves the grouping's release
/// as it was last built, saying why it cannot be imported, until the release
/// comes back or the grouping is undone.
#[tokio::test]
async fn a_grouping_missing_one_of_its_releases_says_so_and_can_be_undone() {
    let (db, _temp, members) = scanned(&["Volume A", "Volume B"]).await;
    let root = host_root("/music");
    db.combine_releases("grouping:test".into(), members.clone())
        .await
        .unwrap();

    let generation = db
        .begin_folder_scan(&root, crate::import::VolumeKind::Local)
        .await
        .unwrap();
    db.save_folder_scan_item(&root, generation, &ScanItem::Valid(members[0].clone()))
        .await
        .unwrap();
    db.finish_folder_scan(&root, generation, None)
        .await
        .unwrap();

    let error = db
        .load_release_candidate("grouping:test")
        .await
        .expect_err("a grouping missing a release cannot be worked on");
    assert!(error.to_string().contains("Volume B"), "{error}");
    let detail = db
        .load_import_candidate("grouping:test")
        .await
        .unwrap()
        .expect("the release is still listed");
    assert!(!detail.actionable);
    assert_eq!(
        detail.resolve(&Default::default()).grouping_action,
        Some(crate::import::grouping::GroupingAction::Separate)
    );

    let returned = db.separate_picked_grouping("grouping:test").await.unwrap();
    assert_eq!(returned, vec![ScanItem::Valid(members[0].clone())]);
    assert!(stored(&db, "grouping:test").await.is_none());
}

/// A folder already in one grouping cannot be taken into another.
#[tokio::test]
async fn a_release_is_taken_into_one_grouping_at_most() {
    let (db, _temp, members) = scanned(&["Volume A", "Volume B", "Volume C"]).await;
    db.combine_releases("grouping:first".into(), members[..2].to_vec())
        .await
        .unwrap();
    assert!(db
        .combine_releases("grouping:second".into(), members[1..].to_vec())
        .await
        .is_err());
}

/// One grouping table family holds every reading of folders as releases:
/// the two it replaced are gone from the schema.
#[tokio::test]
async fn the_schema_holds_one_grouping_model() {
    let (db, _temp) = empty_db().await;
    let tables: Vec<String> = db
        .read(|sql| {
            Ok(sql.query(
                "SELECT name FROM sqlite_master WHERE type IN ('table', 'trigger')",
                [],
                |row| row.get::<_, String>(0),
            )?)
        })
        .await
        .unwrap();
    for gone in [
        "folder_release_decisions",
        "candidate_combination",
        "candidate_combination_member",
        "scan_candidate_resolved_boundary",
    ] {
        assert!(
            !tables.iter().any(|table| table == gone),
            "{gone} is still in the schema"
        );
    }
    for kept in ["release_grouping", "release_grouping_member"] {
        assert!(
            tables.iter().any(|table| table == kept),
            "{kept} is missing"
        );
    }
}
