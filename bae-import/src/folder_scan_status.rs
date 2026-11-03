//! Where each watched folder's scan stands: scanning (with how many releases
//! it has found), complete, or failed; whether the folder is on a network
//! volume; and the walks under way, totalled for the list's chrome.

#[derive(Debug, Clone, PartialEq)]
pub struct WatchedFolderScanStatus {
    pub watched_folder_path: std::path::PathBuf,
    pub watched_folder_name: String,
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
    Scanning { found_count: u64 },
    Complete,
    Failed { error: String },
}

/// Where each watched folder's scan stands, for the chrome around the list.
///
/// Read apart from the list, by its own live query, and sent on its own
/// subscription: a scan re-confirms every folder it walks by moving its row
/// to the scan's generation, which moves the found count and nothing the list
/// shows. Counted inside the list's read, each of those writes cost a
/// whole-queue read, and a rescan of a large folder kept the list reading back
/// to back until it finished; sent with the list, each resent its page.
#[derive(Debug, Clone, PartialEq)]
pub struct FolderScanProgress {
    /// Every scanned root, in watched-folder order.
    pub statuses: Vec<WatchedFolderScanStatus>,
    /// The current walks, already filtered and totalled for the filter-bar
    /// activity control. Absent as soon as no root is scanning.
    pub activity: Option<FolderScanActivity>,
}

impl FolderScanProgress {
    pub(crate) fn of(statuses: Vec<WatchedFolderScanStatus>) -> Self {
        let folders: Vec<ActiveFolderScan> = statuses
            .iter()
            .filter_map(|folder| match folder.status {
                FolderScanStatus::Scanning { found_count } => Some(ActiveFolderScan {
                    watched_folder_path: folder.watched_folder_path.clone(),
                    watched_folder_name: folder.watched_folder_name.clone(),
                    found_count,
                }),
                FolderScanStatus::Complete | FolderScanStatus::Failed { .. } => None,
            })
            .collect();
        let activity = (!folders.is_empty()).then(|| FolderScanActivity {
            found_count: folders.iter().map(|folder| folder.found_count).sum(),
            folders,
        });
        Self { statuses, activity }
    }
}

/// Live folder-scan activity for the list chrome. Counts come from each
/// root's current generation, never from the list windows a UI has loaded.
#[derive(Debug, Clone, PartialEq)]
pub struct FolderScanActivity {
    pub found_count: u64,
    pub folders: Vec<ActiveFolderScan>,
}

/// One root in [`FolderScanActivity`], in watched-folder order.
#[derive(Debug, Clone, PartialEq)]
pub struct ActiveFolderScan {
    pub watched_folder_path: std::path::PathBuf,
    pub watched_folder_name: String,
    pub found_count: u64,
}
