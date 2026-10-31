/// A tentative candidate at `name` below `root`.
fn tentative_candidate(root: &std::path::Path, name: &str) -> crate::import::folder_scanner::ScanItem {
    match scanned_candidate(root, name) {
        crate::import::folder_scanner::ScanItem::Valid(candidate) => {
            crate::import::folder_scanner::ScanItem::Discovered(candidate)
        }
        other => unreachable!("scanned_candidate is valid: {other:?}"),
    }
}

/// Save `items` in turn under one scan generation of `root`, returning each
/// write.
async fn save_in_turn(
    db: &Database,
    root: &std::path::Path,
    items: &[crate::import::folder_scanner::ScanItem],
) -> Vec<crate::db::ScanItemWrite> {
    let generation = db.begin_folder_scan(std::path::Path::new(root), crate::import::VolumeKind::Local).await.unwrap();
    let mut writes = Vec::new();
    for item in items {
        writes.push(
            db.save_folder_scan_item(std::path::Path::new(root), generation, item)
                .await
                .unwrap()
                .expect("the scan generation is current"),
        );
    }
    writes
}

/// A release read over a whole folder replaces every entry reading a folder
/// below it, tentative or settled, and none whose folder only starts with the
/// same letters.
#[tokio::test]
async fn a_release_read_over_a_folder_replaces_the_entries_below_it_and_no_sibling() {
    let (db, _tmp) = empty_db().await;
    let root = &host_root("/mounted/library");
    db.add_watched_import_folder(root).await.unwrap();
    let writes = save_in_turn(
        &db,
        root,
        &[
            scanned_candidate(root, "Box/CD1"),
            tentative_candidate(root, "Box/CD2"),
            scanned_candidate(root, "Box 2"),
            scanned_candidate(root, "Boxes/CD1"),
            combined_candidate(root, "Box"),
        ],
    )
    .await;

    let mut replaced = writes[4].superseded_keys().to_vec();
    replaced.sort();
    assert_eq!(replaced, vec![key_of(root, "Box/CD1"), key_of(root, "Box/CD2")]);
    assert_eq!(
        stored_keys(&db, root).await,
        vec![
            key_of(root, "Box"),
            key_of(root, "Box 2"),
            key_of(root, "Boxes/CD1")
        ]
    );
}

/// A release read from one folder replaces a release read over a folder
/// above it, but not one read from that folder's own files.
#[tokio::test]
async fn a_release_read_from_one_folder_replaces_only_a_whole_folder_above_it() {
    let (db, _tmp) = empty_db().await;
    let root = &host_root("/mounted/library");
    db.add_watched_import_folder(root).await.unwrap();
    let writes = save_in_turn(
        &db,
        root,
        &[
            combined_candidate(root, "Set"),
            scanned_candidate(root, "Single"),
            combined_candidate(root, "Set 2"),
            scanned_candidate(root, "Set/CD1"),
            scanned_candidate(root, "Single/Bonus"),
            scanned_candidate(root, "Set 2/CD1/Extra"),
        ],
    )
    .await;

    assert_eq!(writes[3].superseded_keys(), [key_of(root, "Set")]);
    assert!(writes[4].superseded_keys().is_empty(), "{:?}", writes[4]);
    assert_eq!(writes[5].superseded_keys(), [key_of(root, "Set 2")]);
    assert_eq!(
        stored_keys(&db, root).await,
        vec![
            key_of(root, "Set 2/CD1/Extra"),
            key_of(root, "Set/CD1"),
            key_of(root, "Single"),
            key_of(root, "Single/Bonus"),
        ]
    );
}

/// A release written again over its own row replaces only the other entries
/// reading its files.
#[tokio::test]
async fn a_release_written_again_does_not_replace_itself() {
    let (db, _tmp) = empty_db().await;
    let root = &host_root("/mounted/library");
    db.add_watched_import_folder(root).await.unwrap();
    let again = match combined_candidate(root, "Box") {
        crate::import::folder_scanner::ScanItem::Valid(mut candidate) => {
            candidate.name = "Box, read again".to_string();
            crate::import::folder_scanner::ScanItem::Valid(candidate)
        }
        other => unreachable!("combined_candidate is valid: {other:?}"),
    };
    let writes = save_in_turn(
        &db,
        root,
        &[
            combined_candidate(root, "Box"),
            scanned_candidate(root, "Box/CD1"),
            combined_candidate(root, "Box"),
            again,
        ],
    )
    .await;

    assert_eq!(writes[2].superseded_keys(), [key_of(root, "Box/CD1")]);
    assert!(writes[3].changed());
    assert!(writes[3].superseded_keys().is_empty(), "{:?}", writes[3]);
    assert_eq!(stored_keys(&db, root).await, vec![key_of(root, "Box")]);
}

/// A tentative candidate whose files a settled release already reads stands
/// down, keeping that release; one in a folder whose name only starts with
/// the release's is stored.
#[tokio::test]
async fn a_tentative_candidate_stands_down_only_for_a_release_reading_its_files() {
    let (db, _tmp) = empty_db().await;
    let root = &host_root("/mounted/library");
    db.add_watched_import_folder(root).await.unwrap();
    let writes = save_in_turn(
        &db,
        root,
        &[
            combined_candidate(root, "Box"),
            tentative_candidate(root, "Box/CD1"),
            tentative_candidate(root, "Box 2"),
            scanned_candidate(root, "Single"),
            tentative_candidate(root, "Single/Bonus"),
        ],
    )
    .await;

    assert!(!writes[1].changed(), "{:?}", writes[1]);
    assert!(writes[2].changed(), "{:?}", writes[2]);
    assert!(writes[4].changed(), "{:?}", writes[4]);
    assert_eq!(
        stored_keys(&db, root).await,
        vec![
            key_of(root, "Box"),
            key_of(root, "Box 2"),
            key_of(root, "Single"),
            key_of(root, "Single/Bonus"),
        ]
    );
}
