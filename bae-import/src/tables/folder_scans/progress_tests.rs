//! What each watched folder's scan stored, as its live query reads it.

use crate::tables::tests::*;
use bae_records::test_fixtures::live_query::live_db;
use bae_util::test_channels::HANG_GUARD;

/// The only root's stored outcome.
fn only_outcome(scans: Vec<crate::StoredRootScan>) -> crate::StoredScanOutcome {
    let [scan] = <[_; 1]>::try_from(scans).expect("one root is watched");
    scan.last_scan.expect("a scan opened the root's generation").outcome
}

/// The tables of a library watching `/music`, with its root.
async fn watching_music() -> (crate::tables::ImportTables, std::path::PathBuf, tempfile::TempDir) {
    let (db, temp) = live_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db);
    let root = crate::tables::watched_path::host_root("/music");
    tables.add_watched_import_folder(&root).await.unwrap();
    (tables, root, temp)
}

/// `/music`'s scan held as running.
fn running(root: &std::path::Path) -> crate::folder_scan_status::HeldRootScans {
    crate::folder_scan_status::HeldRootScans::from([(
        root.to_path_buf(),
        crate::folder_scan_status::HeldRootScan::Running,
    )])
}

/// `/music`'s scan held as failed with `error`, its scan unable to store it,
/// on `volume`.
fn unstored(
    root: &std::path::Path,
    error: &str,
    volume: crate::VolumeKind,
) -> crate::folder_scan_status::HeldRootScans {
    crate::folder_scan_status::HeldRootScans::from([(
        root.to_path_buf(),
        crate::folder_scan_status::HeldRootScan::UnstoredFailure(
            crate::folder_scan_status::UnstoredScanFailure { error: error.to_string(), volume },
        ),
    )])
}

/// The only watched folder's status.
async fn only_status(tables: &crate::tables::ImportTables) -> crate::FolderScanStatus {
    let progress = tables.load_folder_scan_progress().await.unwrap();
    let [status] = <[_; 1]>::try_from(progress.statuses).expect("one root is watched");
    status.status
}

/// What the stopped pass's folder shows when its generation stored no
/// outcome and nothing reads it.
const STOPPED: &str = "the scan stopped before bae stored its result";

/// An open generation is scanning while the coordinator holds its folder as
/// running, until the walk stores its outcome. The walk's stored folders do
/// not wake the progress, whose next value is the outcome: the count has a
/// read of its own.
#[tokio::test]
async fn progress_follows_the_walk_in_flight() {
    let (tables, root, _temp) = watching_music().await;
    let root = &root;
    tables.hold_root_scans(running(root)).await.unwrap();
    let generation = tables.begin_folder_scan(root, crate::VolumeKind::Local).await.unwrap();
    tables.save_folder_scan_item(root, generation, &scan_candidate(root, "first")).await.unwrap();

    let mut live = tables.subscribe_folder_scan_progress();
    let initial = live.next_inline().await.expect("the query stays open").unwrap();
    assert_eq!(initial.statuses[0].status, crate::FolderScanStatus::Scanning);
    assert!(!initial.has_problem);
    assert_eq!(
        initial.activity,
        Some(crate::FolderScanActivity {
            folders: vec![crate::WatchedFolder { path: root.clone(), name: "music".to_string() }],
        })
    );

    tables.save_folder_scan_item(root, generation, &scan_candidate(root, "second")).await.unwrap();

    tables.finish_folder_scan(root, generation, None).await.unwrap();
    let complete = tokio::time::timeout(HANG_GUARD, live.next_inline())
        .await
        .expect("finishing the scan wakes the progress")
        .expect("the query stays open")
        .unwrap();
    assert_eq!(complete.statuses[0].status, crate::FolderScanStatus::Complete);
    assert_eq!(complete.activity, None);
}

/// A generation with no outcome that the coordinator does not hold as
/// running ended without storing one: its folder's status is Failed with the
/// stopped error, never Scanning.
#[tokio::test]
async fn an_unfinished_scan_nothing_reads_failed() {
    let (tables, root, _temp) = watching_music().await;
    tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();

    let progress = tables.load_folder_scan_progress().await.unwrap();
    assert_eq!(
        progress.statuses[0].status,
        crate::FolderScanStatus::Failed { error: STOPPED.to_string() }
    );
    assert!(progress.has_problem);
    assert_eq!(progress.activity, None);
}

/// A failure the scan could not store is the folder's status, over whatever
/// an earlier pass stored, and the folder's next pass clears it.
#[tokio::test]
async fn a_failure_the_store_refused_is_the_status() {
    let (tables, root, _temp) = watching_music().await;
    let generation = tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
    tables.finish_folder_scan(&root, generation, None).await.unwrap();

    tables
        .hold_root_scans(unstored(&root, "the disk is full", crate::VolumeKind::Local))
        .await
        .unwrap();
    assert_eq!(
        only_status(&tables).await,
        crate::FolderScanStatus::Failed { error: "the disk is full".to_string() }
    );

    tables.hold_root_scans(crate::folder_scan_status::HeldRootScans::new()).await.unwrap();
    assert_eq!(only_status(&tables).await, crate::FolderScanStatus::Complete);
}

/// A whole-root pass that has not opened its generation yet leaves the last
/// outcome standing, and one that stored its outcome shows it before the
/// coordinator stops holding the folder as running.
#[tokio::test]
async fn a_stored_outcome_stands_while_the_root_is_being_read() {
    let (tables, root, _temp) = watching_music().await;
    let generation = tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
    tables.finish_folder_scan(&root, generation, Some("share disconnected")).await.unwrap();
    tables.hold_root_scans(running(&root)).await.unwrap();

    let progress = tables.load_folder_scan_progress().await.unwrap();
    assert_eq!(
        progress.statuses[0].status,
        crate::FolderScanStatus::Failed { error: "share disconnected".to_string() }
    );
    assert_eq!(progress.activity, None);
}

/// A folder that no scan has opened a generation for shows Not scanned
/// until the coordinator holds a failure for it, on the volume the failing
/// pass found it on.
#[tokio::test]
async fn a_folder_with_no_scan_is_listed_once_its_failure_is_held() {
    let (tables, root, _temp) = watching_music().await;
    for held in [crate::folder_scan_status::HeldRootScans::new(), running(&root)] {
        tables.hold_root_scans(held).await.unwrap();
        let progress = tables.load_folder_scan_progress().await.unwrap();
        assert_eq!(progress.statuses[0].status, crate::FolderScanStatus::NotScanned);
        assert!(!progress.has_problem);
    }

    tables
        .hold_root_scans(unstored(&root, "the disk is full", crate::VolumeKind::Network))
        .await
        .unwrap();
    let progress = tables.load_folder_scan_progress().await.unwrap();
    assert_eq!(
        progress.statuses,
        [crate::WatchedFolderScanStatus {
            folder: crate::WatchedFolder { path: root.clone(), name: "music".to_string() },
            status: crate::FolderScanStatus::Failed { error: "the disk is full".to_string() },
            on_network_volume: true,
        }]
    );
    assert!(progress.has_problem);
}

/// A tree that the scan read in full but bae could not watch every directory
/// of has its own status, with the reason: it is neither complete nor a
/// failed scan, and it is not scanning.
#[tokio::test]
async fn a_tree_read_but_not_watched_is_its_own_status() {
    let (tables, root, _temp) = watching_music().await;
    let generation = tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
    tables
        .finish_folder_scan_unwatched(&root, generation, "/music/a: too many open files")
        .await
        .unwrap();

    let progress = tables.load_folder_scan_progress().await.unwrap();
    assert_eq!(
        progress.statuses[0].status,
        crate::FolderScanStatus::NotWatched { reason: "/music/a: too many open files".to_string() }
    );
    assert!(progress.has_problem);
    assert_eq!(progress.activity, None);
}

/// The alerts the folder scans list, as kind and detail.
async fn listed_alerts(
    tables: &crate::tables::ImportTables,
) -> Vec<(crate::FolderScanAlertKind, String)> {
    tables
        .load_folder_scan_progress()
        .await
        .unwrap()
        .alerts
        .into_iter()
        .map(|alert| (alert.kind, alert.detail))
        .collect()
}

/// The progress lists an alert for a folder's problem until the app marks it
/// shown. The same problem stays shown; the progress lists a different one
/// again, and also a problem with the same detail of another kind.
#[tokio::test]
async fn the_progress_lists_a_problem_s_alert_until_the_app_marks_it_shown() {
    use crate::FolderScanAlertKind::{Failed, NotWatched};
    let (tables, root, _temp) = watching_music().await;
    let generation = tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
    tables.finish_folder_scan(&root, generation, Some("offline")).await.unwrap();
    let progress = tables.load_folder_scan_progress().await.unwrap();
    assert_eq!(
        progress.alerts,
        [crate::FolderScanAlert {
            folder: crate::WatchedFolder { path: root.clone(), name: "music".to_string() },
            kind: Failed,
            detail: "offline".to_string(),
        }]
    );

    tables.mark_folder_scan_alert_shown(&root, Failed, "offline".to_string()).await.unwrap();
    assert_eq!(listed_alerts(&tables).await, []);

    tables
        .hold_root_scans(unstored(&root, "no such column", crate::VolumeKind::Local))
        .await
        .unwrap();
    assert_eq!(listed_alerts(&tables).await, [(Failed, "no such column".to_string())]);
    tables.mark_folder_scan_alert_shown(&root, Failed, "no such column".to_string()).await.unwrap();
    assert_eq!(listed_alerts(&tables).await, []);

    tables.hold_root_scans(unstored(&root, "offline", crate::VolumeKind::Local)).await.unwrap();
    tables.mark_folder_scan_alert_shown(&root, Failed, "offline".to_string()).await.unwrap();
    tables.hold_root_scans(crate::folder_scan_status::HeldRootScans::new()).await.unwrap();
    let generation = tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
    tables.finish_folder_scan_unwatched(&root, generation, "offline").await.unwrap();
    assert_eq!(listed_alerts(&tables).await, [(NotWatched, "offline".to_string())]);
}

/// The progress lists an alert again for the next problem of a folder whose
/// problem ended, even one with the same detail: a new scan of the folder
/// ends the stored problem, and its next pass ends a failure the coordinator
/// held.
#[tokio::test]
async fn a_folder_that_recovers_gets_an_alert_for_its_next_problem() {
    use crate::FolderScanAlertKind::Failed;
    let (tables, root, _temp) = watching_music().await;
    let generation = tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
    tables.finish_folder_scan(&root, generation, Some("offline")).await.unwrap();
    tables.mark_folder_scan_alert_shown(&root, Failed, "offline".to_string()).await.unwrap();

    let generation = tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
    tables.finish_folder_scan(&root, generation, Some("offline")).await.unwrap();
    assert_eq!(listed_alerts(&tables).await, [(Failed, "offline".to_string())]);

    tables
        .hold_root_scans(unstored(&root, "the disk is full", crate::VolumeKind::Local))
        .await
        .unwrap();
    tables
        .mark_folder_scan_alert_shown(&root, Failed, "the disk is full".to_string())
        .await
        .unwrap();
    tables.hold_root_scans(running(&root)).await.unwrap();
    tables
        .hold_root_scans(unstored(&root, "the disk is full", crate::VolumeKind::Local))
        .await
        .unwrap();
    assert_eq!(listed_alerts(&tables).await, [(Failed, "the disk is full".to_string())]);
}

/// A completed scan ends the folder's problem, so the next failed or
/// unwatched scan lists its alert again, even with the detail the app
/// already showed.
#[tokio::test]
async fn a_completed_scan_ends_the_problem_so_the_next_one_alerts_again() {
    use crate::FolderScanAlertKind::{Failed, NotWatched};
    let (tables, root, _temp) = watching_music().await;
    let generation = tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
    tables.finish_folder_scan(&root, generation, Some("offline")).await.unwrap();
    tables.mark_folder_scan_alert_shown(&root, Failed, "offline".to_string()).await.unwrap();

    let generation = tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
    tables.finish_folder_scan(&root, generation, None).await.unwrap();
    assert_eq!(listed_alerts(&tables).await, [], "a completed scan has no problem");
    let generation = tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
    tables.finish_folder_scan(&root, generation, Some("offline")).await.unwrap();
    assert_eq!(listed_alerts(&tables).await, [(Failed, "offline".to_string())]);

    tables.mark_folder_scan_alert_shown(&root, Failed, "offline".to_string()).await.unwrap();
    let generation = tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
    tables.finish_folder_scan_unwatched(&root, generation, "limit reached").await.unwrap();
    tables
        .mark_folder_scan_alert_shown(&root, NotWatched, "limit reached".to_string())
        .await
        .unwrap();
    let generation = tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
    tables.finish_folder_scan(&root, generation, None).await.unwrap();
    let generation = tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
    tables.finish_folder_scan_unwatched(&root, generation, "limit reached").await.unwrap();
    assert_eq!(listed_alerts(&tables).await, [(NotWatched, "limit reached".to_string())]);
}

/// A folder no longer watched records no alert, and the library opens with
/// nothing held and an alert listed again for every problem.
#[tokio::test]
async fn opening_the_library_clears_what_the_coordinator_held_and_the_alerts_shown() {
    use crate::FolderScanAlertKind::Failed;
    let (tables, root, _temp) = watching_music().await;
    let generation = tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
    tables.finish_folder_scan(&root, generation, Some("offline")).await.unwrap();
    tables.mark_folder_scan_alert_shown(&root, Failed, "offline".to_string()).await.unwrap();
    let other = crate::tables::watched_path::host_root("/other");
    tables.mark_folder_scan_alert_shown(&other, Failed, "offline".to_string()).await.unwrap();
    tables.hold_root_scans(running(&other)).await.unwrap();

    tables.clear_held_folder_scans().await.unwrap();

    assert_eq!(tables.load_held_root_scans().await.unwrap(), Default::default());
    assert_eq!(listed_alerts(&tables).await, [(Failed, "offline".to_string())]);
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
    let initial = live.next_inline().await.expect("the query stays open").unwrap();
    assert_eq!(initial, crate::ScanFoundCounts::from([(root.clone(), 1)]));

    tables.save_folder_scan_item(root, generation, &scan_candidate(root, "second")).await.unwrap();
    let grown = tokio::time::timeout(HANG_GUARD, live.next_inline())
        .await
        .expect("a scan item wakes the found counts")
        .expect("the query stays open")
        .unwrap();
    assert_eq!(grown, crate::ScanFoundCounts::from([(root.clone(), 2)]));

    tables.finish_folder_scan(root, generation, None).await.unwrap();
    let done = tokio::time::timeout(HANG_GUARD, live.next_inline())
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

/// The progress lists the folders in the order the person added them, which
/// the query reads from their positions' index.
#[tokio::test]
async fn progress_lists_the_folders_in_their_order_from_the_index() {
    let (db, _temp) = live_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let roots = [
        crate::tables::watched_path::host_root("/zeta"),
        crate::tables::watched_path::host_root("/alpha"),
    ];
    for root in &roots {
        tables.add_watched_import_folder(root).await.unwrap();
    }
    let progress = tables.load_folder_scan_progress().await.unwrap();
    assert_eq!(
        progress.statuses.iter().map(|status| status.folder.path.clone()).collect::<Vec<_>>(),
        roots
    );

    for statement in [super::FOLDER_SCAN_STATUSES, super::SCANNING_FOLDERS] {
        let plan = db
            .read(move |sql| {
                sql.query(&format!("EXPLAIN QUERY PLAN {statement}"), [], |row| {
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

/// The progress read counts the folders of a scan's root and current
/// generation from the index of folder candidates, and bae stores no separate
/// count.
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
