//! What each watched folder's scan stored, as its live query reads it.

use crate::tables::tests::*;
use bae_records::test_fixtures::live_query::live_db;
use bae_util::test_channels::HANG_GUARD;

/// The only root's stored outcome.
fn only_outcome(scans: Vec<crate::StoredRootScan>) -> crate::StoredScanOutcome {
    let [scan] = <[_; 1]>::try_from(scans).expect("one root is watched");
    scan.last_scan.expect("a scan opened the root's generation").outcome
}

/// An open generation counts the folders its walk has stored, and stores
/// its outcome once the walk finishes.
#[tokio::test]
async fn stored_scans_follow_the_walk_in_flight() {
    let (db, _temp) = live_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = &crate::tables::watched_path::host_root("/music");
    tables.add_watched_import_folder(root).await.unwrap();
    let generation = tables.begin_folder_scan(root, crate::VolumeKind::Local).await.unwrap();
    tables.save_folder_scan_item(root, generation, &scan_candidate(root, "first")).await.unwrap();

    let mut live = tables.subscribe_stored_folder_scans();
    let initial = live.next().await.expect("the query stays open").unwrap();
    assert_eq!(only_outcome(initial), crate::StoredScanOutcome::Unfinished { found_count: 1 });

    tables.save_folder_scan_item(root, generation, &scan_candidate(root, "second")).await.unwrap();
    let grown = tokio::time::timeout(HANG_GUARD, live.next())
        .await
        .expect("a scan item wakes the stored scans")
        .expect("the query stays open")
        .unwrap();
    assert_eq!(only_outcome(grown), crate::StoredScanOutcome::Unfinished { found_count: 2 });

    tables.finish_folder_scan(root, generation, None).await.unwrap();
    let complete = tokio::time::timeout(HANG_GUARD, live.next())
        .await
        .expect("finishing the scan wakes the stored scans")
        .expect("the query stays open")
        .unwrap();
    assert_eq!(only_outcome(complete), crate::StoredScanOutcome::Complete);
}

/// Whether a watched folder is on a network volume is what its scan recorded
/// when it began — not something reading the progress asks the filesystem,
/// which for a share that stopped answering would wait as long as the mount.
/// A folder on this machine's disk recorded as network reads as network.
#[tokio::test]
async fn stored_scans_read_the_volume_their_scan_recorded() {
    let (db, _temp) = live_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = &crate::tables::watched_path::host_root("/music");
    tables.add_watched_import_folder(root).await.unwrap();
    let generation = tables.begin_folder_scan(root, crate::VolumeKind::Network).await.unwrap();
    tables.finish_folder_scan(root, generation, None).await.unwrap();

    let scans = tables.load_stored_folder_scans().await.unwrap();

    assert!(scans[0].last_scan.as_ref().expect("the scan began").on_network_volume);
}

/// A rescan retains the previous generation until the new walk succeeds, but
/// the found count is only the entries encountered by the generation in
/// flight.
#[tokio::test]
async fn stored_scans_count_only_the_generation_in_flight() {
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

    assert_eq!(
        only_outcome(tables.load_stored_folder_scans().await.unwrap()),
        crate::StoredScanOutcome::Unfinished { found_count: 1 }
    );
}

/// The stored scans list the watched folders in the order they are listed,
/// read in that order from their positions' index: the folder added first
/// comes first, whatever its path.
#[tokio::test]
async fn stored_scans_list_the_folders_in_their_order() {
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

    let scans = tables.load_stored_folder_scans().await.unwrap();
    assert_eq!(
        scans.iter().map(|scan| scan.watched_folder_path.clone()).collect::<Vec<_>>(),
        roots
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

/// A watched folder no scan has opened a generation for is listed with no
/// scan, in its place among the others.
#[tokio::test]
async fn stored_scans_list_a_folder_no_scan_has_begun() {
    let (db, _temp) = live_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let scanned = crate::tables::watched_path::host_root("/scanned");
    let never = crate::tables::watched_path::host_root("/never");
    tables.add_watched_import_folder(&never).await.unwrap();
    tables.add_watched_import_folder(&scanned).await.unwrap();
    let generation = tables.begin_folder_scan(&scanned, crate::VolumeKind::Local).await.unwrap();
    tables.finish_folder_scan(&scanned, generation, None).await.unwrap();

    assert_eq!(
        tables.load_stored_folder_scans().await.unwrap(),
        [
            crate::StoredRootScan { watched_folder_path: never, last_scan: None },
            crate::StoredRootScan {
                watched_folder_path: scanned,
                last_scan: Some(crate::StoredScan {
                    outcome: crate::StoredScanOutcome::Complete,
                    on_network_volume: false,
                }),
            },
        ]
    );
}
