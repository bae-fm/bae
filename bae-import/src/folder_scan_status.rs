//! Where each watched folder's scan stands: scanning (with how many releases
//! it has found), complete, or failed; whether the folder is on a network
//! volume; and the walks under way, totalled for the list's chrome.
//!
//! Two sources make it. The table holds what a scan stored: the outcome of
//! the root's current generation, or none while that scan has stored none,
//! and how many folders it has found. The folder scan coordinator holds, in
//! memory, which roots are being read and the failure of a root's last pass
//! that the store would not take. Neither of those can outlive the process,
//! so a root never reads as scanning with nothing scanning it.

use std::collections::BTreeMap;
use std::path::PathBuf;

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

/// What a root shows when its current generation stored no outcome, nothing
/// is reading it, and no failure is held for it: the pass that read it ended
/// without storing what became of it, as one that panicked does.
pub(crate) const SCAN_STOPPED_UNSTORED: &str = "the scan stopped before its result could be stored";

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
    /// Each watched folder as the coordinator's memory says it stands. A
    /// failure the store would not take is the folder's status, on the
    /// volume the failing pass found it on; then the outcome its generation
    /// stored; a generation with none is scanning while the folder is being
    /// read, and otherwise ended without storing it. A folder with no scan
    /// stored and no failure held has no status yet, and is not listed: a
    /// pass reading it stores its generation, with its volume, as it starts.
    pub(crate) fn of(stored: Vec<StoredRootScan>, in_memory: &FolderScansInMemory) -> Self {
        let statuses: Vec<WatchedFolderScanStatus> = stored
            .into_iter()
            .filter_map(|root| {
                let held = in_memory.get(&root.watched_folder_path);
                let (status, on_network_volume) = match (held, root.last_scan) {
                    (Some(RootScanInMemory::UnstoredFailure(failure)), _) => (
                        FolderScanStatus::Failed { error: failure.error.clone() },
                        failure.volume == crate::VolumeKind::Network,
                    ),
                    (_, None) => return None,
                    (held, Some(StoredScan { outcome, on_network_volume })) => {
                        let status = match (held, outcome) {
                            (_, StoredScanOutcome::Complete) => FolderScanStatus::Complete,
                            (_, StoredScanOutcome::Failed { error }) => {
                                FolderScanStatus::Failed { error }
                            }
                            (
                                Some(RootScanInMemory::Running),
                                StoredScanOutcome::Unfinished { found_count },
                            ) => FolderScanStatus::Scanning { found_count },
                            (
                                None | Some(RootScanInMemory::UnstoredFailure(_)),
                                StoredScanOutcome::Unfinished { .. },
                            ) => FolderScanStatus::Failed {
                                error: SCAN_STOPPED_UNSTORED.to_string(),
                            },
                        };
                        (status, on_network_volume)
                    }
                };
                Some(WatchedFolderScanStatus {
                    watched_folder_name: crate::WatchedFolder::from_path(
                        root.watched_folder_path.clone(),
                    )
                    .name,
                    watched_folder_path: root.watched_folder_path,
                    status,
                    on_network_volume,
                })
            })
            .collect();
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
    /// became of it. `found_count` is how many folders it has stored.
    Unfinished {
        found_count: u64,
    },
    Complete,
    Failed {
        error: String,
    },
}

/// What the folder scan coordinator knows of one root's scan that no table
/// holds. It lives in the coordinator's memory, so it ends with the process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RootScanInMemory {
    /// A whole-root pass is reading the root or is owed once the pass
    /// running ends, or a removal holds the root: its scan is not over.
    Running,
    /// The root's last pass failed, and the store would not take the
    /// failure. Cleared when the root's next pass starts.
    UnstoredFailure(UnstoredScanFailure),
}

/// A pass's failure the store would not take, with the volume the pass found
/// the folder on: a folder whose first scan could not open its generation
/// has no stored volume to show beside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UnstoredScanFailure {
    pub(crate) error: String,
    pub(crate) volume: crate::VolumeKind,
}

/// Every root the coordinator knows something of, by path.
pub(crate) type FolderScansInMemory = BTreeMap<PathBuf, RootScanInMemory>;

/// The stored scans and the coordinator's memory, put into one progress each
/// time either changes it.
///
/// The stored scans are never older than the memory they are put with. A
/// pass stores its outcome before the coordinator hears it ended and stops
/// holding the root as running, so stored scans read before that memory
/// would show a root neither running nor finished. Each progress therefore
/// reads the stored scans afresh, after taking the memory; the stored scans'
/// own live query only says when they change.
pub(crate) struct FolderScansLive {
    tables: crate::tables::ImportTables,
    changes: coven::LiveQuery<Vec<StoredRootScan>, coven::CovenError>,
    in_memory: tokio::sync::watch::Receiver<FolderScansInMemory>,
    /// Whether the coordinator still holds the memory's sender. Once it has
    /// stopped, the memory says what it last said and only the table moves.
    coordinator_running: bool,
    /// Whether the stored scans' live query has delivered its first value.
    stored_read: bool,
    /// The progress last handed out, so one equal to it isn't handed out
    /// again.
    told: Option<FolderScanProgress>,
}

impl FolderScansLive {
    pub(crate) fn new(
        tables: crate::tables::ImportTables,
        in_memory: tokio::sync::watch::Receiver<FolderScansInMemory>,
    ) -> Self {
        Self {
            changes: tables.subscribe_stored_folder_scans(),
            tables,
            in_memory,
            coordinator_running: true,
            stored_read: false,
            told: None,
        }
    }

    /// The first progress once the table has been read, then one each time
    /// the table or the memory changes it. A failed read is handed on.
    /// `None` once the table's query has ended with its store.
    pub(crate) async fn next(&mut self) -> Option<coven::CovenResult<FolderScanProgress>> {
        loop {
            tokio::select! {
                changed = self.changes.next() => match changed? {
                    Ok(_) => self.stored_read = true,
                    Err(error) => return Some(Err(error)),
                },
                changed = self.in_memory.changed(), if self.coordinator_running => {
                    if changed.is_err() {
                        tracing::debug!(
                            "the folder scan coordinator stopped; scan progress follows the table alone"
                        );
                        self.coordinator_running = false;
                    }
                }
            }
            if !self.stored_read {
                continue;
            }
            let in_memory = self.in_memory.borrow_and_update().clone();
            let stored = match self.tables.load_stored_folder_scans().await {
                Ok(stored) => stored,
                Err(error) => return Some(Err(error.into())),
            };
            let progress = FolderScanProgress::of(stored, &in_memory);
            if self.told.as_ref() == Some(&progress) {
                continue;
            }
            self.told = Some(progress.clone());
            return Some(Ok(progress));
        }
    }
}

impl bae_store::live_query::LiveSource<FolderScanProgress> for FolderScansLive {
    fn next(
        &mut self,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Option<coven::CovenResult<FolderScanProgress>>>
                + Send
                + '_,
        >,
    > {
        Box::pin(FolderScansLive::next(self))
    }

    #[cfg(any(test, feature = "test-utils"))]
    fn runs(&self) -> coven::LiveQueryRuns {
        self.changes.runs()
    }
}

#[cfg(test)]
#[path = "folder_scan_status_tests.rs"]
mod tests;
