//! Each root's scan status, as the stored scan and the coordinator's memory
//! put it together.

use super::*;

fn stored(outcome: StoredScanOutcome) -> Vec<StoredRootScan> {
    vec![StoredRootScan {
        watched_folder_path: PathBuf::from("/music"),
        last_scan: Some(StoredScan { outcome, on_network_volume: false }),
    }]
}

fn unstored(error: &str, volume: crate::VolumeKind) -> RootScanInMemory {
    RootScanInMemory::UnstoredFailure(UnstoredScanFailure { error: error.to_string(), volume })
}

fn holding(in_memory: RootScanInMemory) -> FolderScansInMemory {
    FolderScansInMemory::from([(PathBuf::from("/music"), in_memory)])
}

fn status(progress: &FolderScanProgress) -> &FolderScanStatus {
    &progress.statuses[0].status
}

/// A generation with no outcome is scanning while the root is being read.
#[test]
fn an_unfinished_scan_being_read_is_scanning() {
    let progress = FolderScanProgress::of(
        stored(StoredScanOutcome::Unfinished),
        &holding(RootScanInMemory::Running),
    );
    assert_eq!(status(&progress), &FolderScanStatus::Scanning);
    assert_eq!(progress.activity.map(|activity| activity.folders.len()), Some(1));
}

/// A generation with no outcome that nothing reads ended without storing
/// one, and says so: it never reads as scanning.
#[test]
fn an_unfinished_scan_nothing_reads_failed() {
    let progress =
        FolderScanProgress::of(stored(StoredScanOutcome::Unfinished), &FolderScansInMemory::new());
    assert_eq!(
        status(&progress),
        &FolderScanStatus::Failed { error: SCAN_STOPPED_UNSTORED.to_string() }
    );
    assert_eq!(progress.activity, None);
}

/// A failure the store would not take is the root's status, over whatever
/// an earlier pass stored.
#[test]
fn a_failure_the_store_refused_is_the_status() {
    let progress = FolderScanProgress::of(
        stored(StoredScanOutcome::Complete),
        &holding(unstored("the disk is full", crate::VolumeKind::Local)),
    );
    assert_eq!(
        status(&progress),
        &FolderScanStatus::Failed { error: "the disk is full".to_string() }
    );
}

/// A whole-root pass that has not opened its generation yet leaves the
/// last outcome standing, and one that stored its outcome shows it before
/// the coordinator hears the pass ended.
#[test]
fn a_stored_outcome_stands_while_the_root_is_being_read() {
    let progress = FolderScanProgress::of(
        stored(StoredScanOutcome::Failed { error: "share disconnected".to_string() }),
        &holding(RootScanInMemory::Running),
    );
    assert_eq!(
        status(&progress),
        &FolderScanStatus::Failed { error: "share disconnected".to_string() }
    );
}

/// A folder no scan has opened a generation for is listed once a failure is
/// held for it, on the volume the failing pass found it on, and not before.
#[test]
fn a_folder_with_no_scan_is_listed_once_its_failure_is_held() {
    let never_scanned =
        || vec![StoredRootScan { watched_folder_path: PathBuf::from("/music"), last_scan: None }];

    let progress = FolderScanProgress::of(
        never_scanned(),
        &holding(unstored("the disk is full", crate::VolumeKind::Network)),
    );
    assert_eq!(
        progress.statuses,
        [WatchedFolderScanStatus {
            watched_folder_path: PathBuf::from("/music"),
            watched_folder_name: "music".to_string(),
            status: FolderScanStatus::Failed { error: "the disk is full".to_string() },
            on_network_volume: true,
        }]
    );

    for in_memory in [FolderScansInMemory::new(), holding(RootScanInMemory::Running)] {
        assert_eq!(FolderScanProgress::of(never_scanned(), &in_memory).statuses, []);
    }
}

/// A tree read in full whose directories could not all be watched is its own
/// status, with the reason: it is neither complete nor a failed scan, and it
/// is not scanning.
#[test]
fn a_tree_read_but_not_watched_is_its_own_status() {
    let progress = FolderScanProgress::of(
        stored(StoredScanOutcome::NotWatched { reason: "/music/a: too many open files".into() }),
        &FolderScansInMemory::new(),
    );
    assert_eq!(
        status(&progress),
        &FolderScanStatus::NotWatched { reason: "/music/a: too many open files".to_string() }
    );
    assert_eq!(progress.activity, None);
}
