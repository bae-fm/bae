//! What each watched folder's scan stored, as its live query reads it.

use crate::tables::tests::*;
use bae_records::test_fixtures::live_query::live_db;
use bae_util::test_channels::HANG_GUARD;

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

/// The found count of each folder that the scanning folders list.
fn found_counts(folders: &[crate::ScanningFolder]) -> Vec<u64> {
    folders.iter().map(|folder| folder.found).collect()
}

/// The only watched folder's status.
async fn only_status(tables: &crate::tables::ImportTables) -> crate::FolderScanStatus {
    let progress = tables.load_folder_scan_progress(Default::default()).await.unwrap();
    let [status] = <[_; 1]>::try_from(progress.statuses).expect("the library watches one root");
    status.status
}

/// What the stopped pass's folder shows when its generation stored no
/// outcome and nothing reads it.
const STOPPED: &str = "the scan stopped before bae stored its result";

/// An open generation is scanning while the coordinator holds its folder as
/// running, until the walk stores its outcome. The walk's stored folders do
/// not wake the progress, whose next value is the outcome: the scanning
/// folders' count has a read of its own.
#[tokio::test]
async fn progress_follows_the_walk_in_flight() {
    let (tables, root, _temp) = watching_music().await;
    let root = &root;
    tables.hold_root_scans(running(root)).await.unwrap();
    let generation = tables.begin_folder_scan(root, crate::VolumeKind::Local).await.unwrap();
    tables.save_folder_scan_item(root, generation, &scan_candidate(root, "first")).await.unwrap();

    let mut live = tables.subscribe_folder_scan_progress();
    let initial = live.next_inline().await.expect("the query stays open").result.unwrap();
    assert_eq!(
        initial.statuses[0].status,
        crate::FolderScanStatus::Scanning { on_network_volume: false }
    );
    assert!(!initial.has_problem);
    assert!(initial.scanning);

    tables.save_folder_scan_item(root, generation, &scan_candidate(root, "second")).await.unwrap();

    tables.finish_folder_scan(root, generation, None).await.unwrap();
    let complete = tokio::time::timeout(HANG_GUARD, live.next_inline())
        .await
        .expect("finishing the scan wakes the progress")
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(
        complete.statuses[0].status,
        crate::FolderScanStatus::Complete { on_network_volume: false }
    );
    assert!(!complete.scanning);
}

/// The scanning folders list each folder that the coordinator holds as
/// running and whose generation has stored no outcome, with the folders its
/// walk has stored, live. Each folder the walk stores adds one to the count,
/// and the walk's end takes the folder off the list.
#[tokio::test]
async fn scanning_folders_follow_the_walk_in_flight() {
    let (tables, root, _temp) = watching_music().await;
    let root = &root;
    tables.hold_root_scans(running(root)).await.unwrap();
    let generation = tables.begin_folder_scan(root, crate::VolumeKind::Local).await.unwrap();
    tables.save_folder_scan_item(root, generation, &scan_candidate(root, "first")).await.unwrap();

    let mut live = tables.subscribe_scanning_folders();
    let initial = live.next_inline().await.expect("the query stays open").unwrap();
    assert_eq!(
        initial,
        [crate::ScanningFolder {
            folder: crate::WatchedFolder { path: root.clone(), name: "music".to_string() },
            found: 1,
        }]
    );

    tables.save_folder_scan_item(root, generation, &scan_candidate(root, "second")).await.unwrap();
    let grown = tokio::time::timeout(HANG_GUARD, live.next_inline())
        .await
        .expect("a stored folder wakes the scanning folders")
        .expect("the query stays open")
        .unwrap();
    assert_eq!(found_counts(&grown), [2]);

    tables.finish_folder_scan(root, generation, None).await.unwrap();
    let done = tokio::time::timeout(HANG_GUARD, live.next_inline())
        .await
        .expect("finishing the scan wakes the scanning folders")
        .expect("the query stays open")
        .unwrap();
    assert!(done.is_empty(), "the scanning folders do not list a finished scan");
}

/// A generation with no outcome that the coordinator does not hold as
/// running ended without storing one: its folder's status is Failed with the
/// stopped error, never Scanning.
#[tokio::test]
async fn an_unfinished_scan_nothing_reads_failed() {
    let (tables, root, _temp) = watching_music().await;
    tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();

    let progress = tables.load_folder_scan_progress(Default::default()).await.unwrap();
    assert_eq!(
        progress.statuses[0].status,
        crate::FolderScanStatus::Failed { error: STOPPED.to_string(), on_network_volume: false }
    );
    assert!(progress.has_problem);
    assert!(!progress.scanning);
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
        crate::FolderScanStatus::Failed {
            error: "the disk is full".to_string(),
            on_network_volume: false
        }
    );

    tables.hold_root_scans(crate::folder_scan_status::HeldRootScans::new()).await.unwrap();
    assert_eq!(
        only_status(&tables).await,
        crate::FolderScanStatus::Complete { on_network_volume: false }
    );
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

    let progress = tables.load_folder_scan_progress(Default::default()).await.unwrap();
    assert_eq!(
        progress.statuses[0].status,
        crate::FolderScanStatus::Failed {
            error: "share disconnected".to_string(),
            on_network_volume: false
        }
    );
    assert!(!progress.scanning);
}

/// A folder that no scan has opened a generation for shows Not scanned
/// until the coordinator holds a failure for it, on the volume the failing
/// pass found it on.
#[tokio::test]
async fn a_folder_with_no_scan_is_listed_once_its_failure_is_held() {
    let (tables, root, _temp) = watching_music().await;
    for held in [crate::folder_scan_status::HeldRootScans::new(), running(&root)] {
        tables.hold_root_scans(held).await.unwrap();
        let progress = tables.load_folder_scan_progress(Default::default()).await.unwrap();
        assert_eq!(progress.statuses[0].status, crate::FolderScanStatus::NotScanned);
        assert!(!progress.has_problem);
    }

    tables
        .hold_root_scans(unstored(&root, "the disk is full", crate::VolumeKind::Network))
        .await
        .unwrap();
    let progress = tables.load_folder_scan_progress(Default::default()).await.unwrap();
    assert_eq!(
        progress.statuses,
        [crate::WatchedFolderScanStatus {
            folder: crate::WatchedFolder { path: root.clone(), name: "music".to_string() },
            status: crate::FolderScanStatus::Failed {
                error: "the disk is full".to_string(),
                on_network_volume: true
            },
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
        .finish_folder_scan_unwatched(&root, generation, &unwatched("too many open files"))
        .await
        .unwrap();

    let progress = tables.load_folder_scan_progress(Default::default()).await.unwrap();
    assert_eq!(
        progress.statuses[0].status,
        crate::FolderScanStatus::NotWatched {
            reason: unwatched("too many open files"),
            on_network_volume: false
        }
    );
    assert!(progress.has_problem);
    assert!(!progress.scanning);
}

/// Why `/music/a` could not be watched, for `message`.
fn unwatched(message: &str) -> crate::UnwatchedReason {
    crate::UnwatchedReason {
        below: std::path::PathBuf::from("a"),
        cause: crate::UnwatchedCause::LimitReached,
        message: message.to_string(),
    }
}

/// The problems the folder scans list alerts for with `raised` as the alerts
/// the app raised.
async fn listed_alerts(
    tables: &crate::tables::ImportTables,
    raised: &[crate::FolderScanAlert],
) -> Vec<crate::FolderScanProblem> {
    tables
        .load_folder_scan_progress(raised.iter().cloned().collect())
        .await
        .unwrap()
        .alerts
        .into_iter()
        .map(|alert| alert.problem)
        .collect()
}

/// The alert for `/music`'s `problem`.
fn music_alert(
    root: &std::path::Path,
    problem: crate::FolderScanProblem,
) -> crate::FolderScanAlert {
    crate::FolderScanAlert {
        folder: crate::WatchedFolder { path: root.to_path_buf(), name: "music".to_string() },
        problem,
    }
}

/// A failed scan's problem, for `error`.
fn failed(error: &str) -> crate::FolderScanProblem {
    crate::FolderScanProblem::Failed { error: error.to_string() }
}

/// The progress lists an alert for each folder's problem that is not among
/// the alerts the app raised: a raised problem stays unlisted whatever stores
/// it, and the progress lists a problem with another detail, or with the
/// same detail of another kind.
#[tokio::test]
async fn the_progress_lists_each_problem_the_app_raised_no_alert_for() {
    let (tables, root, _temp) = watching_music().await;
    let generation = tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
    tables.finish_folder_scan(&root, generation, Some("offline")).await.unwrap();
    let progress = tables.load_folder_scan_progress(Default::default()).await.unwrap();
    assert_eq!(progress.alerts, [music_alert(&root, failed("offline"))]);

    let raised = [music_alert(&root, failed("offline"))];
    assert_eq!(listed_alerts(&tables, &raised).await, []);

    tables
        .hold_root_scans(unstored(&root, "no such column", crate::VolumeKind::Local))
        .await
        .unwrap();
    assert_eq!(listed_alerts(&tables, &raised).await, [failed("no such column")]);
    tables.hold_root_scans(unstored(&root, "offline", crate::VolumeKind::Local)).await.unwrap();
    assert_eq!(listed_alerts(&tables, &raised).await, []);

    tables.hold_root_scans(crate::folder_scan_status::HeldRootScans::new()).await.unwrap();
    let generation = tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
    tables.finish_folder_scan_unwatched(&root, generation, &unwatched("offline")).await.unwrap();
    assert_eq!(
        listed_alerts(&tables, &raised).await,
        [crate::FolderScanProblem::NotWatched { reason: unwatched("offline") }]
    );
}

/// A new request on the same live query changes the alerts it lists: the
/// alerts the app raised drop out of the next value.
#[tokio::test]
async fn raising_an_alert_drops_it_from_the_live_progress() {
    let (tables, root, _temp) = watching_music().await;
    let generation = tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
    tables.finish_folder_scan(&root, generation, Some("offline")).await.unwrap();

    let mut live = tables.subscribe_folder_scan_progress();
    let first = live.next_inline().await.expect("the query stays open").result.unwrap();
    assert_eq!(first.alerts, [music_alert(&root, failed("offline"))]);

    let raised = crate::RaisedScanAlerts::from([music_alert(&root, failed("offline"))]);
    live.requests().set(raised.clone()).unwrap();
    let after = tokio::time::timeout(HANG_GUARD, async {
        loop {
            let event = live.next_inline().await.expect("the query stays open");
            if event.request == raised {
                break event.result.unwrap();
            }
        }
    })
    .await
    .expect("the new request delivers");
    assert_eq!(after.alerts, []);
    assert!(after.has_problem, "the folder's problem stands while its alert is raised");
}

/// The library opens with nothing the coordinator held.
#[tokio::test]
async fn opening_the_library_clears_what_the_coordinator_held() {
    let (tables, root, _temp) = watching_music().await;
    let other = crate::tables::watched_path::host_root("/other");
    tables.hold_root_scans(running(&root)).await.unwrap();
    tables.hold_root_scans(running(&other)).await.unwrap();

    tables.clear_held_folder_scans().await.unwrap();

    assert_eq!(tables.load_held_root_scans().await.unwrap(), Default::default());
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

    tables.hold_root_scans(running(root)).await.unwrap();

    assert_eq!(found_counts(&tables.load_scanning_folders().await.unwrap()), [1]);
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
    let progress = tables.load_folder_scan_progress(Default::default()).await.unwrap();
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

/// The scanning folders' read of a running scan with 1,000 candidates takes
/// at most four steps for each candidate.
#[tokio::test]
async fn reading_the_scanning_folders_counts_off_an_index() {
    let (db, _temp) = live_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = &crate::tables::watched_path::host_root("/music");
    tables.add_watched_import_folder(root).await.unwrap();
    let generation = tables.begin_folder_scan(root, crate::VolumeKind::Local).await.unwrap();
    let counter = db.track_work_for_test().await.unwrap();
    tables.hold_root_scans(running(root)).await.unwrap();
    let mut stored = 0;
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
        let folders = tables.load_scanning_folders().await.unwrap();
        assert_eq!(found_counts(&folders), [size]);
        found_steps.push(counter.take().app_read_steps);
    }
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
            sql.query(&format!("EXPLAIN QUERY PLAN {}", super::SCANNING_FOLDERS), [], |row| {
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
