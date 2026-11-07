//! Watched folders: the filesystem watches installed on them, and the
//! coordinator that decides when each is read (from commands, the watches'
//! reports, and the periodic check of folders on network volumes), starting
//! each pass over a root (`crate::scan_passes`) or its removal.

use crate::scan_passes::{
    spawn_root_pass, RootScanCompletion, RootScanStarter, RootScanTask, ScanServices,
};
use crate::ScanEvent;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

mod active_roots;
mod coordinator;
mod folder_watcher;
mod root_change;
mod root_removal;
mod root_scan_cause;
mod watch_batches;

use active_roots::{ActiveRoots, RemovalOutcome};
pub(crate) use active_roots::{FolderReadingRequest, RootPass};
pub(crate) use coordinator::{run_watcher, WatcherCommand};
use folder_watcher::FolderWatchSnapshot;
pub(crate) use folder_watcher::FolderWatcher;
use root_change::{
    affected_roots, changed_events_summary, changed_paths, network_changes, root_change_of,
    roots_for_watch_error, RootChange,
};
use root_removal::{
    run_root_removal, RootRemovalBackend, RootRemovalResult, ServiceRootRemovalBackend,
};
use root_scan_cause::RootScanCause;
pub(crate) use watch_batches::WatchReport;

#[cfg(test)]
#[path = "watching_tests.rs"]
pub(crate) mod tests;
