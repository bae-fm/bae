//! Each watched folder's scan status: scanning, complete, failed, or
//! read but not watched; whether the folder is on a network volume; the
//! folders being scanned, for the list's chrome; and the problems whose alert
//! the app has not raised yet.
//!
//! One live query reads it all (`folder_scans/progress.sql`). The query puts
//! each folder's stored scan beside the state the folder scan coordinator
//! keeps for the folder that no scan stores: whether a pass is reading it,
//! and a failure the scan could not store. The coordinator writes those rows
//! in the step that changes them, and the import service empties them as the
//! library opens, so a folder never shows as scanning with nothing scanning
//! it. The count of folders a scan has found changes each time the scan
//! stores a folder, and a live query of its own reads it.

use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub struct WatchedFolderScanStatus {
    pub watched_folder_path: std::path::PathBuf,
    pub watched_folder_name: String,
    /// The whole path, shown as text.
    pub watched_folder_path_text: String,
    pub status: FolderScanStatus,
    /// Whether this folder lives on a volume served over the network, which
    /// changes how it is watched: a filesystem watch on such a volume reports
    /// only what this machine does to it, so the folder is checked on a
    /// schedule as well. The list says so, because "I added an album on the
    /// server and bae has not noticed" is otherwise a mystery.
    pub on_network_volume: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FolderScanStatus {
    /// No scan has opened a generation for the folder yet, and the folder
    /// scan coordinator holds no failure for it.
    NotScanned,
    Scanning,
    Complete,
    Failed {
        error: String,
    },
    /// The folder was read in full, and a directory in it could not be
    /// watched, for this reason: a change there reaches bae only when the
    /// folder is read again.
    NotWatched {
        reason: String,
    },
}

/// How many folders each scan under way has stored, by its root.
pub type ScanFoundCounts = BTreeMap<PathBuf, u64>;

/// Each watched folder's scan status, for the chrome around the list.
///
/// Read apart from the list, by its own live query, and sent on its own
/// subscription: a scan re-confirms every folder it walks by moving its row
/// to the scan's generation, which moves the found count and nothing the list
/// shows. This query does not read the found count: the count has its own
/// query and stream, so the writes that change it neither rerun this query
/// nor reach the screen with it.
#[derive(Debug, Clone, PartialEq)]
pub struct FolderScanProgress {
    /// Every watched root, in watched-folder order.
    pub statuses: Vec<WatchedFolderScanStatus>,
    /// Whether any root's last scan failed or left it not watched.
    pub has_problem: bool,
    /// The roots being scanned, for the filter-bar activity control. Absent
    /// while no root is scanning.
    pub activity: Option<FolderScanActivity>,
    /// Each root's problem that the app has raised no alert for, in
    /// watched-folder order.
    pub alerts: Vec<FolderScanAlert>,
}

/// A watched folder's problem that the app has raised no alert for. The app
/// raises one and marks it shown
/// ([`crate::ImportServiceHandle::mark_folder_scan_alert_shown`]), and the
/// app raises an alert again for the folder's next problem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderScanAlert {
    pub watched_folder_path: PathBuf,
    pub watched_folder_name: String,
    pub kind: FolderScanAlertKind,
    /// The failure, or why the folder is not watched, untranslated.
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderScanAlertKind {
    /// The folder's scan failed.
    Failed,
    /// The scan read the whole folder, and bae could not watch a directory
    /// in it.
    NotWatched,
}

/// Live folder-scan activity for the list chrome: the roots being scanned.
#[derive(Debug, Clone, PartialEq)]
pub struct FolderScanActivity {
    pub folders: Vec<ActiveFolderScan>,
}

/// One root in [`FolderScanActivity`], in watched-folder order.
#[derive(Debug, Clone, PartialEq)]
pub struct ActiveFolderScan {
    pub watched_folder_path: std::path::PathBuf,
    pub watched_folder_name: String,
}

/// One watched root's scan as its table row stores it.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredRootScan {
    pub watched_folder_path: PathBuf,
    /// The folder's current scan generation, `None` while no scan has opened
    /// one.
    pub last_scan: Option<StoredScan>,
}

/// A watched folder's current scan generation as its row stores it.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredScan {
    pub outcome: StoredScanOutcome,
    /// Whether the volume the scan began on is served over the network.
    pub on_network_volume: bool,
}

/// What the root's current scan generation stored of how it ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoredScanOutcome {
    /// Nothing: the scan is under way, or it ended without storing what
    /// became of it.
    Unfinished,
    Complete,
    Failed {
        error: String,
    },
    /// The scan read the whole tree, and bae could not watch a directory in
    /// it.
    NotWatched {
        reason: String,
    },
}

/// The state the folder scan coordinator keeps for one root's scan that no
/// scan stores. The coordinator writes each root's held state to
/// `folder_scan_held` in the step that changes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HeldRootScan {
    /// A whole-root pass is reading the root, the coordinator will start one
    /// once the running pass ends, or a removal holds the root: its scan is
    /// not over.
    Running,
    /// The root's last pass failed, and its scan could not store the
    /// failure. Cleared when the root's next pass starts.
    UnstoredFailure(UnstoredScanFailure),
}

/// A pass's failure its scan could not store, with the volume the pass found
/// the folder on: a folder whose first scan could not open its generation
/// has no stored volume to show beside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UnstoredScanFailure {
    pub(crate) error: String,
    pub(crate) volume: crate::VolumeKind,
}

/// Every root the coordinator holds something of, by path.
pub(crate) type HeldRootScans = BTreeMap<PathBuf, HeldRootScan>;
