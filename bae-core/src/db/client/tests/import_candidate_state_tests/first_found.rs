// A release is found once: the write that first stores it settled under its
// key.

async fn write(
    db: &Database,
    root: &str,
    generation: u64,
    item: &crate::import::folder_scanner::ScanItem,
) -> crate::db::ScanItemWrite {
    db.save_folder_scan_item(root, generation, item)
        .await
        .unwrap()
        .expect("the scan's generation is current")
}

/// The valid write that settles a tentative row finds the release; a rescan,
/// even one that stores other files under the key, does not.
#[tokio::test]
async fn a_release_is_found_by_its_first_settled_write_only() {
    use crate::import::folder_scanner::ScanItem;

    let (db, _tmp) = empty_db().await;
    let root = &host_root("/mounted/library");
    let settled = scanned_candidate(root, "Album");
    let ScanItem::Valid(candidate) = settled.clone() else {
        panic!("the fixture is a valid candidate")
    };
    let tentative = ScanItem::Discovered(candidate.clone());
    db.add_watched_import_folder(root).await.unwrap();

    let generation = db.begin_folder_scan(root, crate::import::VolumeKind::Local).await.unwrap();
    assert!(!write(&db, root, generation, &tentative).await.found());
    assert!(write(&db, root, generation, &settled).await.found());
    db.finish_folder_scan(root, generation, None).await.unwrap();

    let generation = db.begin_folder_scan(root, crate::import::VolumeKind::Local).await.unwrap();
    assert!(!write(&db, root, generation, &tentative).await.found());
    assert!(!write(&db, root, generation, &settled).await.found());
    let mut changed = candidate;
    changed.files = track_files_candidate(&[("01.flac", 456)]);
    let changed = write(&db, root, generation, &ScanItem::Valid(changed)).await;
    assert!(changed.changed(), "the new files are stored");
    assert!(!changed.found(), "under a key that already held the release");
}

/// A folder that failed validation is already found: fixed, it is the same
/// candidate, and the rescan's tentative write leaves its row standing.
#[tokio::test]
async fn a_fixed_folder_is_not_found_again() {
    use crate::import::folder_scanner::{InvalidCandidate, InvalidReason, ScanItem};

    let (db, _tmp) = empty_db().await;
    let root = &host_root("/mounted/library");
    let settled = scanned_candidate(root, "Album");
    let ScanItem::Valid(candidate) = settled.clone() else {
        panic!("the fixture is a valid candidate")
    };
    let invalid = ScanItem::Invalid(InvalidCandidate {
        path: candidate.path.clone(),
        name: candidate.name.clone(),
        watched_folder_path: root.clone(),
        display_path: candidate.display_path.clone(),
        grouping: None,
        reason: InvalidReason::CorruptImage {
            path: "front.jpg".to_string(),
        },
    });
    db.add_watched_import_folder(root).await.unwrap();
    let generation = db.begin_folder_scan(root, crate::import::VolumeKind::Local).await.unwrap();
    write(&db, root, generation, &invalid).await;
    db.finish_folder_scan(root, generation, None).await.unwrap();

    let generation = db.begin_folder_scan(root, crate::import::VolumeKind::Local).await.unwrap();
    assert!(!write(&db, root, generation, &ScanItem::Discovered(candidate))
        .await
        .changed());
    let fixed = write(&db, root, generation, &settled).await;
    assert!(fixed.changed());
    assert!(!fixed.found());
}
