//! Each root's scan status, as the stored scan and the coordinator's memory
//! put it together.

use super::*;

fn stored(outcome: StoredScanOutcome) -> Vec<StoredRootScan> {
    vec![StoredRootScan {
        watched_folder_path: PathBuf::from("/music"),
        outcome,
        on_network_volume: false,
    }]
}

fn holding(in_memory: RootScanInMemory) -> FolderScansInMemory {
    FolderScansInMemory::from([(PathBuf::from("/music"), in_memory)])
}

fn status(progress: &FolderScanProgress) -> &FolderScanStatus {
    &progress.statuses[0].status
}

/// A generation with no outcome is scanning while the root is being read,
/// counting what it has found.
#[test]
fn an_unfinished_scan_being_read_is_scanning() {
    let progress = FolderScanProgress::of(
        stored(StoredScanOutcome::Unfinished { found_count: 3 }),
        &holding(RootScanInMemory::Running),
    );
    assert_eq!(status(&progress), &FolderScanStatus::Scanning { found_count: 3 });
    assert_eq!(progress.activity.map(|activity| activity.found_count), Some(3));
}

/// A generation with no outcome that nothing reads ended without storing
/// one, and says so: it never reads as scanning.
#[test]
fn an_unfinished_scan_nothing_reads_failed() {
    let progress = FolderScanProgress::of(
        stored(StoredScanOutcome::Unfinished { found_count: 3 }),
        &FolderScansInMemory::new(),
    );
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
        &holding(RootScanInMemory::UnstoredFailure { error: "the disk is full".to_string() }),
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
