//! A pass over a watched root, as the coordinator (`crate::watching`)
//! starts it: the whole root read and reconciled against what is stored, the
//! folders under it whose contents changed read again, or one folder read as
//! a person decided; each stores what it read and announces what changed.

use crate::folder_scanner::{FolderEntry, ScanItem};
use crate::handle::ScanEvent;
use crate::volume::{directory_modified_at, volume_kind, VolumeKind};
use crate::watching::{FolderReadingRequest, FolderWatcher, RootPass};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

mod folder_reading;
mod scan_items;
mod scanning;

use folder_reading::{change_folder_reading, read_changed_folders};
pub(crate) use scan_items::scan_item_seed;
#[cfg(any(test, feature = "test-utils"))]
pub use scan_items::store_scan_item;
use scan_items::{announce_regrouped, announce_scan_write, store_scan_items};
use scanning::{announce_scan_failure, record_scan_failure, rescan_and_reconcile};

/// What the blocking folder walk returns: whether it read the tree, every
/// directory it visited, and their mtimes when it could read all of them.
type FolderWalkOutcome = (
    Result<(), crate::folder_scanner::FolderScanError>,
    HashSet<PathBuf>,
    Option<Vec<(PathBuf, i64)>>,
);

type FolderWalk = tokio::task::JoinHandle<FolderWalkOutcome>;

/// How many scan items the walk hands over before it waits on the store, and
/// so the most one write stores.
const SCAN_ITEMS_WAITING: usize = 8;

/// One pass running over a root: what stops it, and its task.
pub(crate) struct RootScanTask {
    cancellation: crate::folder_scanner::ScanCancellation,
    /// The pass, which says it ended on the completion channel even when it
    /// panics.
    task: bae_diagnostics::ReportedTask,
}

impl RootScanTask {
    pub(crate) fn new(
        cancellation: crate::folder_scanner::ScanCancellation,
        task: bae_diagnostics::ReportedTask,
    ) -> Self {
        Self { cancellation, task }
    }

    /// Ask the pass to stop; it stops at its next check.
    pub(crate) fn cancel(&self) {
        self.cancellation.cancel();
    }

    /// Wait for the pass's task to end, however it ended.
    pub(crate) async fn ended(self) {
        self.task.ended().await;
    }
}

/// One scan pass has ended. It carries no result because a scan records and
/// announces its own failure; this only frees the root and ends the wait.
pub(crate) struct RootScanCompletion {
    pub(crate) id: u64,
    pub(crate) path: PathBuf,
}

/// Run one pass over the root at `path` as `name`, sending its completion on
/// `completion_tx` when `pass` ends. A pass that panics is reported, and its
/// completion is sent all the same, so the root is freed and its waiters are
/// answered.
fn spawn_pass(
    name: &'static str,
    id: u64,
    path: PathBuf,
    diagnostics: &bae_diagnostics::Diagnostics,
    completion_tx: mpsc::UnboundedSender<RootScanCompletion>,
    pass: impl std::future::Future<Output = ()> + Send + 'static,
) -> bae_diagnostics::ReportedTask {
    let panicked_tx = completion_tx.clone();
    let panicked_path = path.clone();
    diagnostics.spawn_reported(
        &tokio::runtime::Handle::current(),
        name,
        async move {
            pass.await;
            if completion_tx.send(RootScanCompletion { id, path }).is_err() {
                debug!("folder scan coordinator ended before a pass completed");
            }
        },
        move |_| {
            if panicked_tx.send(RootScanCompletion { id, path: panicked_path }).is_err() {
                debug!("folder scan coordinator ended before a panicked pass completed");
            }
        },
    )
}

pub(crate) type RootScanStarter = Arc<
    dyn Fn(u64, PathBuf, RootPass, mpsc::UnboundedSender<RootScanCompletion>) -> RootScanTask
        + Send
        + Sync,
>;

/// What one folder scan runs on: the import service's dependencies and the
/// watcher the walk registers each directory with.
#[derive(Clone)]
pub(crate) struct ScanServices {
    services: crate::ImportServices,
    folder_watcher: Arc<FolderWatcher>,
}

impl ScanServices {
    pub(crate) fn new(services: crate::ImportServices, folder_watcher: Arc<FolderWatcher>) -> Self {
        Self { services, folder_watcher }
    }
}

/// Start one pass over the root at `path`: all of it, one folder decision, or
/// the folders whose contents changed.
pub(crate) fn spawn_root_pass(
    id: u64,
    path: PathBuf,
    pass: RootPass,
    scan: ScanServices,
    completion_tx: mpsc::UnboundedSender<RootScanCompletion>,
) -> RootScanTask {
    match pass {
        RootPass::WholeRoot => spawn_root_scan(id, path, scan, completion_tx),
        RootPass::Decision(request) => spawn_folder_reading(id, path, request, scan, completion_tx),
        RootPass::Folders(folders) => spawn_changed_folders(id, path, folders, scan, completion_tx),
    }
}

/// Store one folder decision under `path` and answer whoever asked.
fn spawn_folder_reading(
    id: u64,
    path: PathBuf,
    request: FolderReadingRequest,
    scan: ScanServices,
    completion_tx: mpsc::UnboundedSender<RootScanCompletion>,
) -> RootScanTask {
    let cancellation = crate::folder_scanner::ScanCancellation::new();
    let reading_cancellation = cancellation.clone();
    let diagnostics = scan.services.diagnostics.clone();
    let pass_path = path.clone();
    let task =
        spawn_pass("store a folder decision", id, path, &diagnostics, completion_tx, async move {
            let path = pass_path;
            let target = request.target().clone();
            // A reading that panics answers its caller as one that failed.
            let result = bae_util::task_failure::caught(
                "store a folder decision",
                change_folder_reading(&path, &target, &scan, &reading_cancellation),
            )
            .await
            .unwrap_or_else(|failed| Err(scan.services.diagnostics.bug(failed).into()))
            // The caller wraps the detail as a watch failure itself.
            .map_err(|error| match error {
                crate::ImportError::Watch { detail } => detail,
                other => other.to_string(),
            });
            if let Err(error) = &result {
                warn!(
                    "folder decision for {} under {} was not stored: {error}",
                    target.0.relative_folder.text(),
                    path.display()
                );
            }
            request.answer(result);
        });
    RootScanTask::new(cancellation, task)
}

/// Read again the folders under `path` whose contents changed.
fn spawn_changed_folders(
    id: u64,
    path: PathBuf,
    folders: std::collections::BTreeSet<std::ffi::OsString>,
    scan: ScanServices,
    completion_tx: mpsc::UnboundedSender<RootScanCompletion>,
) -> RootScanTask {
    let cancellation = crate::folder_scanner::ScanCancellation::new();
    let reading_cancellation = cancellation.clone();
    let diagnostics = scan.services.diagnostics.clone();
    let pass_path = path.clone();
    let task = spawn_pass(
        "read changed folders again",
        id,
        path,
        &diagnostics,
        completion_tx,
        async move {
            read_changed_folders(&pass_path, &folders, &scan, &reading_cancellation).await;
        },
    );
    RootScanTask::new(cancellation, task)
}

fn spawn_root_scan(
    id: u64,
    path: PathBuf,
    scan: ScanServices,
    completion_tx: mpsc::UnboundedSender<RootScanCompletion>,
) -> RootScanTask {
    let cancellation = crate::folder_scanner::ScanCancellation::new();
    let scan_cancellation = cancellation.clone();
    let diagnostics = scan.services.diagnostics.clone();
    let pass_path = path.clone();
    let task =
        spawn_pass("scan a watched folder", id, path, &diagnostics, completion_tx, async move {
            if !scan_cancellation.is_cancelled() {
                // `rescan_and_reconcile` already recorded and announced any
                // error; passing it on would show it twice.
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "the rescan records and announces its own error"
                )]
                let _ = rescan_and_reconcile(&pass_path, &scan, &scan_cancellation).await;
            }
        });
    RootScanTask::new(cancellation, task)
}

#[cfg(test)]
#[path = "scan_passes_tests.rs"]
pub(crate) mod tests;
