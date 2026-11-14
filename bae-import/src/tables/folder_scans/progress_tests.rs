//! What each watched folder's scan stored, as its live query reads it.

use crate::tables::tests::*;
use bae_records::test_fixtures::live_query::live_db;
use bae_util::test_channels::HANG_GUARD;

/// The only root's stored outcome.
fn only_outcome(scans: Vec<crate::StoredRootScan>) -> crate::StoredScanOutcome {
    let [scan] = <[_; 1]>::try_from(scans).expect("one root is watched");
    scan.last_scan.expect("a scan opened the root's generation").outcome
}

/// An open generation has no outcome until its walk finishes, and the
/// walk's stored folders do not wake the stored scans, whose next value is
/// the outcome: the count has a read of its own.
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
    assert_eq!(only_outcome(initial), crate::StoredScanOutcome::Unfinished);

    tables.save_folder_scan_item(root, generation, &scan_candidate(root, "second")).await.unwrap();

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

/// An open generation counts the folders its walk has stored, live, and
/// holds no count once the walk finishes.
#[tokio::test]
async fn found_counts_follow_the_walk_in_flight() {
    let (db, _temp) = live_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = &crate::tables::watched_path::host_root("/music");
    tables.add_watched_import_folder(root).await.unwrap();
    let generation = tables.begin_folder_scan(root, crate::VolumeKind::Local).await.unwrap();
    tables.save_folder_scan_item(root, generation, &scan_candidate(root, "first")).await.unwrap();

    let mut live = tables.subscribe_scan_found_counts();
    let initial = live.next().await.expect("the query stays open").unwrap();
    assert_eq!(initial, crate::ScanFoundCounts::from([(root.clone(), 1)]));

    tables.save_folder_scan_item(root, generation, &scan_candidate(root, "second")).await.unwrap();
    let grown = tokio::time::timeout(HANG_GUARD, live.next())
        .await
        .expect("a scan item wakes the found counts")
        .expect("the query stays open")
        .unwrap();
    assert_eq!(grown, crate::ScanFoundCounts::from([(root.clone(), 2)]));

    tables.finish_folder_scan(root, generation, None).await.unwrap();
    let done = tokio::time::timeout(HANG_GUARD, live.next())
        .await
        .expect("finishing the scan wakes the found counts")
        .expect("the query stays open")
        .unwrap();
    assert!(done.is_empty(), "a finished scan holds no count");
}

/// A rescan retains the previous generation until the new walk succeeds, but
/// the found count is only the entries encountered by the generation in
/// flight.
#[tokio::test]
async fn found_counts_count_only_the_generation_in_flight() {
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
        crate::StoredScanOutcome::Unfinished
    );
    assert_eq!(
        tables.load_scan_found_counts().await.unwrap(),
        crate::ScanFoundCounts::from([(root.clone(), 1)])
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

/// Reading the stored scans runs the same SQLite steps however many
/// candidates the generation in flight holds, since every scan write wakes
/// this read. Reading the found counts walks one index entry for each
/// candidate, and no more than a few steps for each: counting them off the
/// tables cost nine steps a candidate.
#[tokio::test]
async fn reading_the_stored_scans_does_not_grow_and_counting_walks_an_index() {
    let (db, _temp) = live_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = &crate::tables::watched_path::host_root("/music");
    tables.add_watched_import_folder(root).await.unwrap();
    let generation = tables.begin_folder_scan(root, crate::VolumeKind::Local).await.unwrap();
    let counter = db.track_work_for_test().await.unwrap();
    let mut stored = 0;
    let mut stored_steps = Vec::new();
    let mut found_steps = Vec::new();
    for size in [10, 1_000] {
        while stored < size {
            tables
                .save_folder_scan_item(
                    root,
                    generation,
                    &scan_candidate(root, &format!("a{stored}")),
                )
                .await
                .unwrap();
            stored += 1;
        }
        counter.take();
        let scans = tables.load_stored_folder_scans().await.unwrap();
        assert_eq!(only_outcome(scans), crate::StoredScanOutcome::Unfinished);
        stored_steps.push(counter.take().app_read_steps);
        let found = tables.load_scan_found_counts().await.unwrap();
        assert_eq!(found, crate::ScanFoundCounts::from([(root.clone(), size)]));
        found_steps.push(counter.take().app_read_steps);
    }
    assert_eq!(stored_steps[0], stored_steps[1], "steps per read at 10 and at 1,000 candidates");
    assert!(
        found_steps[1] <= 4 * 1_000,
        "counting 1,000 candidates took {} steps, over four each",
        found_steps[1]
    );
}

/// A scan's folder count is the candidates of its root's current generation
/// counted off the index of the folders' candidates, with nothing kept
/// beside them.
#[tokio::test]
async fn the_found_count_is_counted_off_the_found_folders_index() {
    let (db, _temp) = live_db(crate::TEST_SCHEMA).await;
    let plan = db
        .read(|sql| {
            sql.query(&format!("EXPLAIN QUERY PLAN {}", super::SCAN_FOUND_COUNTS), [], |row| {
                row.get::<_, String>(3)
            })
            .map_err(bae_store::DbError::from)
        })
        .await
        .unwrap();
    assert!(
        plan.iter().any(|step| step.contains("USING COVERING INDEX idx_scan_candidate_found")),
        "the count walks the index of a root's folders: {plan:?}"
    );
}
