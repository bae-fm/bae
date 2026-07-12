//! Installs and tracks OS-level filesystem watches for the import service's
//! watched folders.
//!
//! `ImportServiceHandle` owns one `FolderWatcher` (behind `Arc`, shared with
//! every clone) and installs/uninstalls watches on it atomically with the
//! folder-registry mutation — see `ImportServiceHandle::add_watched_folder`. The
//! watcher task (`ImportService::start_watcher`) never touches the debouncer,
//! only the `DebounceEventResult` batches its callback forwards.

use notify::RecursiveMode;
use notify_debouncer_full::{new_debouncer, DebounceEventResult, Debouncer, RecommendedCache};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{error, warn};

use crate::import::ImportError;

/// The concrete type `new_debouncer` returns: the platform's recommended
/// watcher with its built-in file-id cache.
type FsDebouncer = Debouncer<notify::RecommendedWatcher, RecommendedCache>;

/// A debouncer that started successfully, plus the set of paths it currently
/// has an OS watch installed for.
struct ReadyWatcher {
    debouncer: FsDebouncer,
    installed: HashSet<PathBuf>,
}

/// Installs and tracks OS-level folder watches, atomic with the caller's
/// registry mutation. Constructed once in `ImportService::start`, before the
/// watcher task spawns.
///
/// A construction failure (FSEvents/inotify init failing) is stored rather than
/// propagated: app start stays infallible, and every later `install`/`uninstall`
/// returns the stored failure instead of silently doing nothing — a broken watch
/// backend breaks folder watching, not the library.
pub(crate) struct FolderWatcher {
    state: Mutex<Result<ReadyWatcher, String>>,
}

impl FolderWatcher {
    /// Start the debouncer, forwarding every debounced batch (or error) to
    /// `fs_tx`. Never fails outwardly — see the type doc.
    pub(crate) fn new(fs_tx: mpsc::UnboundedSender<DebounceEventResult>) -> Self {
        let result = new_debouncer(Duration::from_secs(1), None, move |result| {
            // Runs on the debouncer's own thread. A send error means the watcher
            // task's receiver is gone (the service is shutting down) — benign,
            // but worth a line.
            if fs_tx.send(result).is_err() {
                warn!("folder watcher event dropped: task receiver gone");
            }
        })
        .map(|debouncer| ReadyWatcher {
            debouncer,
            installed: HashSet::new(),
        })
        .map_err(|e| e.to_string());

        if let Err(e) = &result {
            error!("failed to start folder watcher: {e}");
        }

        Self {
            state: Mutex::new(result),
        }
    }

    /// Install an OS watch on `path` if one isn't already installed.
    /// Idempotent — safe to call again after a previous success.
    pub(crate) fn install(&self, path: &Path) -> Result<(), ImportError> {
        let mut state = self.state.lock().unwrap();
        let ready = state
            .as_mut()
            .map_err(|e| ImportError::Watch { detail: e.clone() })?;
        if ready.installed.contains(path) {
            return Ok(());
        }
        ready
            .debouncer
            .watch(path, RecursiveMode::Recursive)
            .map_err(|e| ImportError::Watch {
                detail: format!("failed to watch {}: {e}", path.display()),
            })?;
        ready.installed.insert(path.to_path_buf());
        Ok(())
    }

    /// Uninstall the OS watch on `path` if one is installed; a no-op otherwise
    /// (dispatch on known state — never blind-call `unwatch`).
    ///
    /// On an uninstall failure, `path` still drops from the installed set before
    /// the error returns: the registry (the durable authority) is removing the
    /// folder regardless, so a leaked OS watch is only process-transient — the
    /// watcher task resolves events against the registry, so events from a leaked
    /// watch on a removed folder match nothing and are ignored.
    pub(crate) fn uninstall(&self, path: &Path) -> Result<(), ImportError> {
        let mut state = self.state.lock().unwrap();
        let Ok(ready) = state.as_mut() else {
            // The debouncer never started; nothing was ever installed.
            return Ok(());
        };
        if !ready.installed.remove(path) {
            return Ok(());
        }
        ready
            .debouncer
            .unwatch(path)
            .map_err(|e| ImportError::Watch {
                detail: format!("failed to unwatch {}: {e}", path.display()),
            })
    }
}
