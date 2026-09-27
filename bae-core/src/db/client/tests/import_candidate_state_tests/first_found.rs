// A release is found once: the first write that settles its folder.

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

/// The valid write after a tentative one finds the release; a rescan, even of
/// other files in the folder, does not.
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

/// A folder that failed validation is already found, so fixing it finds
/// nothing.
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

/// A takeover finds only folders new to the parent; removing the watched
/// folder forgets what it found, so adding it again finds them again.
#[tokio::test]
async fn a_takeover_finds_only_folders_new_to_the_parent() {
    let (db, _tmp) = empty_db().await;
    let music = host_root("/music");
    let inner = host_root("/music/inner");
    db.add_watched_import_folder(&inner).await.unwrap();
    let generation = db.begin_folder_scan(&inner, crate::import::VolumeKind::Local).await.unwrap();
    assert!(write(&db, &inner, generation, &scanned_candidate(&inner, "Album")).await.found());
    db.finish_folder_scan(&inner, generation, None).await.unwrap();

    db.remove_watched_import_folders(vec![inner.clone()], Some(music.clone()))
        .await
        .unwrap()
        .expect("the inner folder was watched");
    let generation = db.begin_folder_scan(&music, crate::import::VolumeKind::Local).await.unwrap();
    assert!(
        !write(&db, &music, generation, &scanned_candidate(&music, "inner/Album")).await.found(),
        "the parent reads a folder its inner watched folder had found"
    );
    assert!(
        write(&db, &music, generation, &scanned_candidate(&music, "New Album")).await.found(),
        "a folder new to the parent is found"
    );
    db.finish_folder_scan(&music, generation, None).await.unwrap();

    db.remove_watched_import_folders(vec![music.clone()], None)
        .await
        .unwrap()
        .expect("the parent was watched");
    db.add_watched_import_folder(&music).await.unwrap();
    let generation = db.begin_folder_scan(&music, crate::import::VolumeKind::Local).await.unwrap();
    assert!(
        write(&db, &music, generation, &scanned_candidate(&music, "inner/Album")).await.found(),
        "a folder watched again after its removal is found again"
    );
}

/// Releases read together are a new release when combined, and only then: a
/// rebuild of the grouping under the parent that took over its folder finds
/// nothing.
#[tokio::test]
async fn a_grouping_is_found_when_combined_only() {
    use crate::import::folder_scanner::ScanItem;

    let (db, _tmp) = empty_db().await;
    let music = host_root("/music");
    let inner = host_root("/music/inner");
    db.add_watched_import_folder(&inner).await.unwrap();
    let generation = db.begin_folder_scan(&inner, crate::import::VolumeKind::Local).await.unwrap();
    let members: Vec<_> = ["Volume A", "Volume B"]
        .into_iter()
        .map(|name| super::candidate(&inner, name))
        .collect();
    for member in &members {
        write(&db, &inner, generation, &ScanItem::Valid(member.clone())).await;
    }
    db.finish_folder_scan(&inner, generation, None).await.unwrap();
    let combined = db
        .combine_releases("grouping:picked".to_string(), members)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(combined.found, vec!["grouping:picked".to_string()]);

    db.remove_watched_import_folders(vec![inner.clone()], Some(music.clone()))
        .await
        .unwrap()
        .expect("the inner folder was watched");
    let generation = db.begin_folder_scan(&music, crate::import::VolumeKind::Local).await.unwrap();
    let mut rebuilt = Vec::new();
    for name in ["inner/Volume A", "inner/Volume B"] {
        let stored = write(
            &db,
            &music,
            generation,
            &ScanItem::Valid(super::candidate(&music, name)),
        )
        .await;
        assert!(!stored.found(), "{name} was found under the inner folder");
        rebuilt.extend(stored.regrouped().cloned());
    }
    assert!(
        rebuilt.iter().any(|changes| !changes.written.is_empty()),
        "the grouping is built again under the parent"
    );
    assert!(rebuilt.iter().all(|changes| changes.found.is_empty()));
}
