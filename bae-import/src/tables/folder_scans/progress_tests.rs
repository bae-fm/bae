//! Where each watched folder's scan stands, as its live query reads it.

use crate::tables::tests::*;
use bae_db::test_fixtures::live_query::live_db;
use bae_util::test_channels::HANG_GUARD;

/// The scan progress counts the folders the walk in flight has reached, and
/// is gone once the walk finishes.
#[tokio::test]
async fn folder_scan_progress_follows_the_walk_in_flight() {
    let (db, _temp) = live_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = &crate::tables::watched_path::host_root("/music");
    tables.add_watched_import_folder(root).await.unwrap();
    let generation = tables.begin_folder_scan(root, crate::VolumeKind::Local).await.unwrap();
    tables.save_folder_scan_item(root, generation, &scan_candidate(root, "first")).await.unwrap();

    let mut live = tables.subscribe_folder_scan_progress();
    let initial = live
        .next()
        .await
        .expect("the query stays open")
        .unwrap()
        .activity
        .expect("the open generation projects scan activity");
    assert_eq!(initial.found_count, 1);
    assert_eq!(initial.folders[0].found_count, 1);

    tables.save_folder_scan_item(root, generation, &scan_candidate(root, "second")).await.unwrap();
    let grown = tokio::time::timeout(HANG_GUARD, live.next())
        .await
        .expect("a scan item wakes the scan progress")
        .expect("the query stays open")
        .unwrap()
        .activity
        .expect("the scan activity updates with the current generation");
    assert_eq!(grown.found_count, 2);
    assert_eq!(grown.folders[0].found_count, 2);

    tables.finish_folder_scan(root, generation, None).await.unwrap();
    let complete = tokio::time::timeout(HANG_GUARD, live.next())
        .await
        .expect("finishing the scan wakes the scan progress")
        .expect("the query stays open")
        .unwrap();
    assert_eq!(complete.activity, None);
    assert!(matches!(complete.statuses[0].status, crate::FolderScanStatus::Complete));
}

/// Whether a watched folder is on a network volume is what its scan recorded
/// when it began — not something reading the progress asks the filesystem,
/// which for a share that stopped answering would wait as long as the mount.
/// A folder on this machine's disk recorded as network reads as network.
#[tokio::test]
async fn folder_scan_progress_reads_the_volume_its_scan_recorded() {
    let (db, _temp) = live_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = &crate::tables::watched_path::host_root("/music");
    tables.add_watched_import_folder(root).await.unwrap();
    let generation = tables.begin_folder_scan(root, crate::VolumeKind::Network).await.unwrap();
    tables.finish_folder_scan(root, generation, None).await.unwrap();

    let progress = tables
        .subscribe_folder_scan_progress()
        .next()
        .await
        .expect("the query stays open")
        .unwrap();

    assert!(progress.statuses[0].on_network_volume);
}

/// A rescan retains the previous generation until the new walk succeeds, but
/// progress counts only the entries encountered by the generation in flight.
#[tokio::test]
async fn folder_scan_progress_excludes_retained_previous_generation_rows() {
    let (db, _temp) = live_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = &crate::tables::watched_path::host_root("/music");
    tables.add_watched_import_folder(root).await.unwrap();
    let old_generation = tables.begin_folder_scan(root, crate::VolumeKind::Local).await.unwrap();
    for name in ["retained", "encountered"] {
        tables
            .save_folder_scan_item(root, old_generation, &scan_candidate(root, name))
            .await
            .unwrap();
    }
    tables.finish_folder_scan(root, old_generation, None).await.unwrap();

    let current_generation =
        tables.begin_folder_scan(root, crate::VolumeKind::Local).await.unwrap();
    tables
        .save_folder_scan_item(root, current_generation, &scan_candidate(root, "encountered"))
        .await
        .unwrap();

    let activity = tables
        .subscribe_folder_scan_progress()
        .next()
        .await
        .expect("the query stays open")
        .unwrap()
        .activity
        .expect("the rescan projects activity");
    assert_eq!(activity.found_count, 1);
    assert_eq!(activity.folders[0].found_count, 1);
}

/// The scan progress lists the watched folders in the order they are
/// listed, read in that order from their positions' index: the folder added
/// first comes first, whatever its path.
#[tokio::test]
async fn folder_scan_progress_lists_the_folders_in_their_order() {
    let (db, _temp) = live_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let roots = [
        crate::tables::watched_path::host_root("/zeta"),
        crate::tables::watched_path::host_root("/alpha"),
    ];
    for root in &roots {
        tables.add_watched_import_folder(root).await.unwrap();
    }
    for root in roots.iter().rev() {
        tables.begin_folder_scan(root, crate::VolumeKind::Local).await.unwrap();
    }

    let progress = tables
        .subscribe_folder_scan_progress()
        .next()
        .await
        .expect("the query stays open")
        .unwrap();
    assert_eq!(
        progress
            .statuses
            .iter()
            .map(|status| status.watched_folder_name.as_str())
            .collect::<Vec<_>>(),
        ["zeta", "alpha"]
    );

    let plan = db
        .read(|sql| {
            sql.query(&format!("EXPLAIN QUERY PLAN {}", super::FOLDER_SCAN_PROGRESS), [], |row| {
                row.get::<_, String>(3)
            })
            .map_err(bae_store::DbError::from)
        })
        .await
        .unwrap();
    assert!(
        !plan.iter().any(|step| step.contains("TEMP B-TREE")),
        "the read sorts after reading: {plan:?}"
    );
}
