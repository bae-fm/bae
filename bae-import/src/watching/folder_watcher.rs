//! Installs and tracks OS-level filesystem watches for the import service's
//! watched folders.
//!
//! Every watched root has a platform watch of its own. FSEvents restarts a
//! watch's stream whenever a path is added to it or taken out of it, and
//! misses whatever happens while it restarts; one watch shared by every root
//! would have reading or removing one root blind the others for that moment.
//! A watch of its own also says which root each event it reports belongs to,
//! whatever spelling of the path it reports it under.
//!
//! The scan coordinator owns one `FolderWatcher` and invokes it only from
//! blocking work. UI-facing watched-folder calls never enter notify/FSEvents.

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tracing::{debug, error};

use super::watch_batches::{RootEvent, WatchReport, WatchedRoot};
use crate::ImportError;
use bae_diagnostics::lock_or_crash;

trait WatchBackend: Send {
    fn watch(&mut self, path: &Path, mode: RecursiveMode) -> notify::Result<()>;
    fn unwatch(&mut self, path: &Path) -> notify::Result<()>;
}

impl WatchBackend for RecommendedWatcher {
    fn watch(&mut self, path: &Path, mode: RecursiveMode) -> notify::Result<()> {
        Watcher::watch(self, path, mode)
    }

    fn unwatch(&mut self, path: &Path) -> notify::Result<()> {
        Watcher::unwatch(self, path)
    }
}

fn watch_not_found(error: &notify::Error) -> bool {
    matches!(error.kind, notify::ErrorKind::WatchNotFound)
}

/// Starts a platform watch for one root, with nothing installed on it yet.
type OpenWatch = Box<dyn Fn(&Path) -> notify::Result<Box<dyn WatchBackend>> + Send>;

/// One root's platform watch, and the directories it has a watch installed
/// on. Dropping it stops the watch.
struct RootWatch {
    backend: Box<dyn WatchBackend>,
    installed: HashSet<PathBuf>,
}

/// Every root's watch, and how to start another.
struct Watches {
    open: OpenWatch,
    roots: HashMap<PathBuf, RootWatch>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct FolderWatchSnapshot {
    directories: Vec<PathBuf>,
}

/// Installs and tracks OS-level folder watches. Constructed once in
/// `ImportService::start`, before the coordinator spawns.
///
/// Failing to start the thread that gathers what the watches report is
/// stored rather than propagated: app start stays infallible, and every later
/// `install_directory`/`reinstall` returns the stored failure instead of
/// silently doing nothing — a broken watch breaks folder watching, not the
/// library. That thread panicking is stored the same way, reported, with
/// every watch stopped. A root whose own watch will not start fails the same
/// way, alone.
///
/// The watch table is changed in several steps under its lock, so a panic
/// while it is held leaves it partway through a change: the next caller to
/// take the lock reports that and ends the process.
pub(crate) struct FolderWatcher {
    state: Arc<Mutex<Result<Watches, String>>>,
    diagnostics: bae_diagnostics::Diagnostics,
}

/// The watch table, as a poisoned lock on it is reported.
const WATCH_TABLE: &str = "the folder watch table";

impl FolderWatcher {
    /// Start gathering what the roots' watches report into one batch per
    /// folder that went quiet (see [`super::watch_batches`]), sent on
    /// `fs_tx`. Never fails outwardly — see the type doc.
    ///
    /// The platform watcher runs without the file-id cache a rename-pairing
    /// debouncer builds: building it walks the watched tree and `stat`s every
    /// file and directory in it, which on a network share is tens of seconds
    /// per folder, and nothing here reads a rename as anything but the two
    /// paths it touched.
    pub(crate) fn new(
        fs_tx: mpsc::UnboundedSender<WatchReport>,
        diagnostics: bae_diagnostics::Diagnostics,
    ) -> Self {
        let (raw_tx, raw_rx) = std::sync::mpsc::channel();
        let state = Arc::new(Mutex::new(Ok(Watches {
            open: platform_watch(raw_tx),
            roots: HashMap::new(),
        })));
        let failed_state = Arc::downgrade(&state);
        let failed_diagnostics = diagnostics.clone();
        let gathering = diagnostics.spawn_thread_reported(
            "folder-watch-batches",
            move || super::watch_batches::run(raw_rx, fs_tx),
            move |failed| {
                let Some(state) = failed_state.upgrade() else {
                    debug!("folder watch events stopped being gathered after the watcher closed");
                    return;
                };
                // Every watch stops with the table; what they would report
                // has nowhere to go.
                *lock_or_crash(&failed_diagnostics, WATCH_TABLE, &state) =
                    Err(format!("gathering folder watch events stopped: {failed}"));
            },
        );
        if let Err(error) = gathering {
            let detail = format!("failed to start gathering folder watch events: {error}");
            error!("{detail}");
            *lock_or_crash(&diagnostics, WATCH_TABLE, &state) = Err(detail);
        }
        Self { state, diagnostics }
    }

    /// A watcher whose watches never started, for `detail`: every install
    /// and retain fails with it, as the stored failure of a real one does.
    #[cfg(test)]
    pub(crate) fn unavailable_for_test(detail: &str) -> Self {
        Self {
            state: Arc::new(Mutex::new(Err(detail.to_string()))),
            diagnostics: bae_diagnostics::Diagnostics::noop(),
        }
    }

    pub(crate) fn install_directory(
        &self,
        root: &Path,
        directory: &Path,
    ) -> Result<(), ImportError> {
        let Some(mode) = watch_mode(root, directory) else {
            return Ok(());
        };
        let mut state = lock_or_crash(&self.diagnostics, WATCH_TABLE, &self.state);
        let watches = state.as_mut().map_err(|e| ImportError::Watch { detail: e.clone() })?;
        watches.install(root, directory, mode)
    }

    pub(crate) fn retain_directories(
        &self,
        root: &Path,
        seen: &HashSet<PathBuf>,
    ) -> Result<(), ImportError> {
        self.retain_directories_under(root, root, seen)
    }

    /// Drop the watch on every directory under `scope` — inside `root` — that
    /// a completed reading of `scope` did not reach. Directories outside it
    /// were not looked at, so their watches stand.
    pub(crate) fn retain_directories_under(
        &self,
        root: &Path,
        scope: &Path,
        seen: &HashSet<PathBuf>,
    ) -> Result<(), ImportError> {
        if uses_recursive_root_watch() {
            return Ok(());
        }
        let mut state = lock_or_crash(&self.diagnostics, WATCH_TABLE, &self.state);
        let watches =
            state.as_mut().map_err(|error| ImportError::Watch { detail: error.clone() })?;
        let Some(watch) = watches.roots.get_mut(root) else {
            return Ok(());
        };
        let stale: Vec<_> = watch
            .installed
            .iter()
            .filter(|directory| directory.starts_with(scope) && !seen.contains(*directory))
            .cloned()
            .collect();
        for directory in &stale {
            match watch.backend.unwatch(directory) {
                Ok(()) => {}
                Err(error) if watch_not_found(&error) => {}
                Err(error) => {
                    return Err(ImportError::Watch {
                        detail: format!("failed to unwatch {}: {error}", directory.display()),
                    });
                }
            }
            watch.installed.remove(directory);
        }
        Ok(())
    }

    /// Stop `path`'s watch, returning what it had installed so the same
    /// watches can be put back; nothing when it has none. Stopping a watch
    /// cannot fail, and touches no other root's.
    pub(super) fn uninstall(&self, path: &Path) -> FolderWatchSnapshot {
        let removed = match lock_or_crash(&self.diagnostics, WATCH_TABLE, &self.state).as_mut() {
            Ok(watches) => watches.roots.remove(path),
            // The watches never started; nothing was ever installed.
            Err(_) => None,
        };
        let Some(watch) = removed else {
            return FolderWatchSnapshot::default();
        };
        let snapshot =
            FolderWatchSnapshot { directories: watch.installed.iter().cloned().collect() };
        drop(watch);
        snapshot
    }

    pub(super) fn reinstall(
        &self,
        root: &Path,
        snapshot: &FolderWatchSnapshot,
    ) -> Result<(), ImportError> {
        let mut failures = Vec::new();
        let mut state = lock_or_crash(&self.diagnostics, WATCH_TABLE, &self.state);
        let watches =
            state.as_mut().map_err(|error| ImportError::Watch { detail: error.clone() })?;
        for directory in &snapshot.directories {
            let Some(mode) = watch_mode(root, directory) else {
                continue;
            };
            if watches.roots.get(root).is_some_and(|watch| watch.installed.contains(directory)) {
                continue;
            }
            if let Err(error) = watches.install(root, directory, mode) {
                failures.push(error.to_string());
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(ImportError::Watch {
                detail: format!("failed to restore watches: {}", failures.join(", ")),
            })
        }
    }
}

impl Watches {
    /// Watch `directory` under `root` in `mode`.
    ///
    /// Where one watch on the root takes in everything under it, each
    /// authoritative reading of the root binds it a fresh watch, started
    /// before the one it replaces stops, so the root is watched throughout.
    /// When the fresh one will not start, the one there stays.
    fn install(
        &mut self,
        root: &Path,
        directory: &Path,
        mode: RecursiveMode,
    ) -> Result<(), ImportError> {
        if uses_recursive_root_watch() {
            let mut backend = (self.open)(root).map_err(|error| watch_failed(root, error))?;
            backend.watch(directory, mode).map_err(|error| watch_failed(directory, error))?;
            let fresh = RootWatch { backend, installed: HashSet::from([directory.to_path_buf()]) };
            // The replaced watch stops here, after the fresh one started.
            drop(self.roots.insert(root.to_path_buf(), fresh));
            return Ok(());
        }
        let watch = match self.roots.entry(root.to_path_buf()) {
            Entry::Occupied(watch) => watch.into_mut(),
            Entry::Vacant(vacant) => vacant.insert(RootWatch {
                backend: (self.open)(root).map_err(|error| watch_failed(root, error))?,
                installed: HashSet::new(),
            }),
        };
        if let Err(error) = watch.backend.watch(directory, mode) {
            watch.installed.remove(directory);
            return Err(watch_failed(directory, error));
        }
        watch.installed.insert(directory.to_path_buf());
        Ok(())
    }
}

fn watch_failed(path: &Path, error: notify::Error) -> ImportError {
    ImportError::Watch { detail: format!("failed to watch {}: {error}", path.display()) }
}

/// Start each root's watch as the platform's own, reporting on `events`
/// with the root it watches.
fn platform_watch(events: std::sync::mpsc::Sender<RootEvent>) -> OpenWatch {
    Box::new(move |root: &Path| {
        let root = Arc::new(
            WatchedRoot::resolve(root)
                .map_err(|error| notify::Error::io(error).add_path(root.to_path_buf()))?,
        );
        let events = events.clone();
        let watcher = RecommendedWatcher::new(
            move |result| {
                if events.send((root.clone(), result)).is_err() {
                    debug!("folder watch event dropped: nothing gathers them any more");
                }
            },
            notify::Config::default(),
        )?;
        Ok(Box::new(watcher) as Box<dyn WatchBackend>)
    })
}

const fn uses_recursive_root_watch() -> bool {
    cfg!(any(target_os = "macos", target_os = "windows"))
}

fn watch_mode(root: &Path, directory: &Path) -> Option<RecursiveMode> {
    if uses_recursive_root_watch() {
        (root == directory).then_some(RecursiveMode::Recursive)
    } else {
        Some(RecursiveMode::NonRecursive)
    }
}

#[cfg(test)]
#[path = "folder_watcher_tests.rs"]
mod tests;
