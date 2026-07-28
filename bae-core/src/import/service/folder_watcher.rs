//! Installs and tracks OS-level filesystem watches for the import service's
//! watched folders.
//!
//! The scan coordinator owns one `FolderWatcher` and invokes it only from
//! blocking work. UI-facing registry calls never enter notify/FSEvents.

use notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{new_debouncer_opt, DebounceEventResult, Debouncer, NoCache};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{error, warn};

use crate::import::ImportError;

/// The platform's recommended watcher, with file-id tracking switched off.
///
/// The debouncer's file-id cache exists for exactly one thing: correlating a
/// rename-from with its rename-to so the pair coalesces into a single `Rename`
/// event. Building it walks the watched tree and `stat`s every file and
/// directory in it, which on a network share is tens of seconds per folder and
/// grows with the tree.
///
/// We never read an event's kind — [`ImportService::start_watcher`] takes the
/// paths out of each batch and re-scans whichever watched roots they fall under.
/// A rename arriving as a remove plus a create re-scans the same root as a
/// coalesced rename would, so the cache buys nothing and costs the entire
/// install. Linux and Android already run this way: `RecommendedCache` is
/// `NoCache` there.
///
/// [`ImportService::start_watcher`]: crate::import::service::ImportService
type FsDebouncer = Debouncer<RecommendedWatcher, NoCache>;

/// A debouncer that started successfully, plus the set of paths it currently
/// has an OS watch installed for.
struct ReadyWatcher {
    backend: Box<dyn WatchBackend>,
    installed: HashMap<PathBuf, HashSet<PathBuf>>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct FolderWatchSnapshot {
    directories: Vec<PathBuf>,
}

trait WatchBackend: Send {
    fn watch(&mut self, path: &Path, mode: RecursiveMode) -> notify::Result<()>;
    fn unwatch(&mut self, path: &Path) -> notify::Result<()>;
}

impl WatchBackend for FsDebouncer {
    fn watch(&mut self, path: &Path, mode: RecursiveMode) -> notify::Result<()> {
        self.watch(path, mode)
    }

    fn unwatch(&mut self, path: &Path) -> notify::Result<()> {
        self.unwatch(path)
    }
}

fn watch_not_found(error: &notify::Error) -> bool {
    matches!(error.kind, notify::ErrorKind::WatchNotFound)
}

/// Installs and tracks OS-level folder watches. Constructed once in
/// `ImportService::start`, before the coordinator spawns.
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
        let result = new_debouncer_opt::<_, RecommendedWatcher, NoCache>(
            Duration::from_secs(1),
            None,
            move |result| {
                // Runs on the debouncer's own thread. A send error means the
                // watcher task's receiver is gone (the service is shutting
                // down) — benign, but worth a line.
                if fs_tx.send(result).is_err() {
                    warn!("folder watcher event dropped: task receiver gone");
                }
            },
            NoCache::new(),
            notify::Config::default(),
        )
        .map(|debouncer| ReadyWatcher {
            backend: Box::new(debouncer) as Box<dyn WatchBackend>,
            installed: HashMap::new(),
        })
        .map_err(|e| e.to_string());

        if let Err(e) = &result {
            error!("failed to start folder watcher: {e}");
        }

        Self {
            state: Mutex::new(result),
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
        let mut state = self.state.lock().unwrap();
        let ready = state
            .as_mut()
            .map_err(|e| ImportError::Watch { detail: e.clone() })?;
        let installed = ready.installed.entry(root.to_path_buf()).or_default();
        if uses_recursive_root_watch() && installed.contains(directory) {
            match ready.backend.unwatch(directory) {
                Ok(()) => {}
                Err(error) if watch_not_found(&error) => {}
                Err(error) => {
                    return Err(ImportError::Watch {
                        detail: format!("failed to rebind {}: {error}", directory.display()),
                    });
                }
            }
            installed.remove(directory);
        }
        if let Err(e) = ready.backend.watch(directory, mode) {
            installed.remove(directory);
            return Err(ImportError::Watch {
                detail: format!("failed to watch {}: {e}", directory.display()),
            });
        }
        installed.insert(directory.to_path_buf());
        Ok(())
    }

    pub(crate) fn retain_directories(
        &self,
        root: &Path,
        seen: &HashSet<PathBuf>,
    ) -> Result<(), ImportError> {
        if uses_recursive_root_watch() {
            return Ok(());
        }
        let mut state = self.state.lock().unwrap();
        let ready = state.as_mut().map_err(|error| ImportError::Watch {
            detail: error.clone(),
        })?;
        let Some(installed) = ready.installed.get_mut(root) else {
            return Ok(());
        };
        let stale: Vec<_> = installed.difference(seen).cloned().collect();
        for directory in &stale {
            match ready.backend.unwatch(directory) {
                Ok(()) => {}
                Err(error) if watch_not_found(&error) => {}
                Err(error) => {
                    return Err(ImportError::Watch {
                        detail: format!("failed to unwatch {}: {error}", directory.display()),
                    });
                }
            }
            installed.remove(directory);
        }
        Ok(())
    }

    /// Uninstall the OS watch on `path` if one is installed; a no-op otherwise
    /// (dispatch on known state — never blind-call `unwatch`).
    ///
    /// Paths the OS refuses to unwatch remain registered so the same removal can
    /// be retried. Paths already absent from the OS are removed from the set.
    pub(super) fn uninstall(&self, path: &Path) -> Result<FolderWatchSnapshot, ImportError> {
        let mut state = self.state.lock().unwrap();
        let Ok(ready) = state.as_mut() else {
            // The debouncer never started; nothing was ever installed.
            return Ok(FolderWatchSnapshot::default());
        };
        let Some(installed) = ready.installed.get(path) else {
            return Ok(FolderWatchSnapshot::default());
        };
        let snapshot = FolderWatchSnapshot {
            directories: installed.iter().cloned().collect(),
        };
        let mut removed = Vec::new();
        let mut failures = Vec::new();
        for directory in &snapshot.directories {
            match ready.backend.unwatch(directory) {
                Ok(()) => removed.push(directory.clone()),
                Err(error) if watch_not_found(&error) => removed.push(directory.clone()),
                Err(error) => {
                    failures.push(format!("{}: {error}", directory.display()));
                }
            }
        }
        let installed = ready
            .installed
            .get_mut(path)
            .expect("the watched root remained installed during removal");
        for directory in &removed {
            installed.remove(directory);
        }
        if failures.is_empty() {
            ready.installed.remove(path);
            return Ok(snapshot);
        }

        let rollback = reinstall_locked(
            ready,
            path,
            &FolderWatchSnapshot {
                directories: removed,
            },
        );
        let mut detail = format!("failed to unwatch: {}", failures.join(", "));
        if let Err(error) = rollback {
            detail.push_str(&format!("; restoring removed watches also failed: {error}"));
        }
        Err(ImportError::Watch { detail })
    }

    pub(super) fn reinstall(
        &self,
        path: &Path,
        snapshot: &FolderWatchSnapshot,
    ) -> Result<(), ImportError> {
        let mut state = self.state.lock().unwrap();
        let ready = state.as_mut().map_err(|error| ImportError::Watch {
            detail: error.clone(),
        })?;
        reinstall_locked(ready, path, snapshot)
    }
}

fn reinstall_locked(
    ready: &mut ReadyWatcher,
    root: &Path,
    snapshot: &FolderWatchSnapshot,
) -> Result<(), ImportError> {
    let mut failures = Vec::new();
    for directory in &snapshot.directories {
        let Some(mode) = watch_mode(root, directory) else {
            continue;
        };
        if ready
            .installed
            .get(root)
            .is_some_and(|installed| installed.contains(directory))
        {
            continue;
        }
        match ready.backend.watch(directory, mode) {
            Ok(()) => {
                ready
                    .installed
                    .entry(root.to_path_buf())
                    .or_default()
                    .insert(directory.clone());
            }
            Err(error) => failures.push(format!("{}: {error}", directory.display())),
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
mod tests {
    use super::*;
    use std::sync::Arc;

    type WatchCall = (String, PathBuf, Option<RecursiveMode>);

    #[derive(Clone, Default)]
    struct FakeBackend {
        calls: Arc<Mutex<Vec<WatchCall>>>,
        fail_watch: Arc<Mutex<bool>>,
        fail_unwatch: Arc<Mutex<bool>>,
        fail_unwatch_path: Arc<Mutex<Option<PathBuf>>>,
        unwatch_not_found: Arc<Mutex<bool>>,
    }

    impl WatchBackend for FakeBackend {
        fn watch(&mut self, path: &Path, mode: RecursiveMode) -> notify::Result<()> {
            self.calls
                .lock()
                .unwrap()
                .push(("watch".to_string(), path.to_path_buf(), Some(mode)));
            if *self.fail_watch.lock().unwrap() {
                Err(notify::Error::generic("watch failed"))
            } else {
                Ok(())
            }
        }

        fn unwatch(&mut self, path: &Path) -> notify::Result<()> {
            self.calls
                .lock()
                .unwrap()
                .push(("unwatch".to_string(), path.to_path_buf(), None));
            if *self.unwatch_not_found.lock().unwrap() {
                Err(notify::Error::watch_not_found())
            } else if *self.fail_unwatch.lock().unwrap()
                || self.fail_unwatch_path.lock().unwrap().as_deref() == Some(path)
            {
                Err(notify::Error::generic("unwatch failed"))
            } else {
                Ok(())
            }
        }
    }

    fn watcher(backend: FakeBackend) -> FolderWatcher {
        FolderWatcher {
            state: Mutex::new(Ok(ReadyWatcher {
                backend: Box::new(backend),
                installed: HashMap::new(),
            })),
        }
    }

    #[test]
    fn platform_watch_policy_avoids_userspace_prewalk() {
        let root = Path::new("/music");
        let child = root.join("artist");
        if uses_recursive_root_watch() {
            assert_eq!(watch_mode(root, root), Some(RecursiveMode::Recursive));
            assert_eq!(watch_mode(root, &child), None);
        } else {
            assert_eq!(watch_mode(root, root), Some(RecursiveMode::NonRecursive));
            assert_eq!(watch_mode(root, &child), Some(RecursiveMode::NonRecursive));
        }
    }

    #[test]
    fn backend_calls_follow_platform_policy_and_remove_every_watch() {
        let backend = FakeBackend::default();
        let calls = backend.calls.clone();
        let watcher = watcher(backend);
        let root = PathBuf::from("/music");
        let child = root.join("artist");

        watcher.install_directory(&root, &root).unwrap();
        watcher.install_directory(&root, &child).unwrap();
        watcher.uninstall(&root).unwrap();

        let calls = calls.lock().unwrap();
        if uses_recursive_root_watch() {
            assert_eq!(
                calls.as_slice(),
                [
                    (
                        "watch".to_string(),
                        root.clone(),
                        Some(RecursiveMode::Recursive)
                    ),
                    ("unwatch".to_string(), root, None),
                ]
            );
        } else {
            assert_eq!(calls.iter().filter(|call| call.0 == "watch").count(), 2);
            assert_eq!(calls.iter().filter(|call| call.0 == "unwatch").count(), 2);
        }
    }

    #[test]
    fn nonrecursive_backend_reissues_a_recreated_directory_watch() {
        if uses_recursive_root_watch() {
            return;
        }
        let backend = FakeBackend::default();
        let calls = backend.calls.clone();
        let watcher = watcher(backend);
        let root = PathBuf::from("/music");
        let child = root.join("artist");

        watcher.install_directory(&root, &child).unwrap();
        watcher.install_directory(&root, &child).unwrap();

        assert_eq!(
            calls
                .lock()
                .unwrap()
                .iter()
                .filter(|call| call.0 == "watch" && call.1 == child)
                .count(),
            2
        );
    }

    #[test]
    fn recursive_root_is_rebound_on_each_authoritative_scan() {
        if !uses_recursive_root_watch() {
            return;
        }
        let backend = FakeBackend::default();
        let calls = backend.calls.clone();
        let watcher = watcher(backend);
        let root = PathBuf::from("/music");

        watcher.install_directory(&root, &root).unwrap();
        watcher.install_directory(&root, &root).unwrap();

        assert_eq!(
            calls.lock().unwrap().as_slice(),
            [
                (
                    "watch".to_string(),
                    root.clone(),
                    Some(RecursiveMode::Recursive)
                ),
                ("unwatch".to_string(), root.clone(), None),
                ("watch".to_string(), root, Some(RecursiveMode::Recursive)),
            ]
        );
    }

    #[test]
    fn failed_recursive_unwatch_is_retried_before_rebinding() {
        if !uses_recursive_root_watch() {
            return;
        }
        let backend = FakeBackend::default();
        let fail_unwatch = backend.fail_unwatch.clone();
        let calls = backend.calls.clone();
        let watcher = watcher(backend);
        let root = PathBuf::from("/music");

        watcher.install_directory(&root, &root).unwrap();
        *fail_unwatch.lock().unwrap() = true;
        assert!(watcher.install_directory(&root, &root).is_err());
        *fail_unwatch.lock().unwrap() = false;
        watcher.install_directory(&root, &root).unwrap();

        assert_eq!(
            calls
                .lock()
                .unwrap()
                .iter()
                .filter(|call| call.0 == "unwatch")
                .count(),
            2
        );
    }

    #[test]
    fn missing_recursive_watch_is_reinstalled_without_a_duplicate() {
        if !uses_recursive_root_watch() {
            return;
        }
        let backend = FakeBackend::default();
        let unwatch_not_found = backend.unwatch_not_found.clone();
        let calls = backend.calls.clone();
        let watcher = watcher(backend);
        let root = PathBuf::from("/music");

        watcher.install_directory(&root, &root).unwrap();
        *unwatch_not_found.lock().unwrap() = true;
        watcher.install_directory(&root, &root).unwrap();

        let calls = calls.lock().unwrap();
        assert_eq!(calls.iter().filter(|call| call.0 == "unwatch").count(), 1);
        assert_eq!(calls.iter().filter(|call| call.0 == "watch").count(), 2);
    }

    #[test]
    fn failed_watch_is_retried_on_the_next_scan() {
        let backend = FakeBackend::default();
        let fail_watch = backend.fail_watch.clone();
        let calls = backend.calls.clone();
        let watcher = watcher(backend);
        let root = PathBuf::from("/music");

        *fail_watch.lock().unwrap() = true;
        assert!(watcher.install_directory(&root, &root).is_err());
        *fail_watch.lock().unwrap() = false;
        watcher.install_directory(&root, &root).unwrap();

        assert_eq!(
            calls
                .lock()
                .unwrap()
                .iter()
                .filter(|call| call.0 == "watch")
                .count(),
            2
        );
    }

    #[test]
    fn missing_nonrecursive_watch_is_removed_during_reconciliation() {
        if uses_recursive_root_watch() {
            return;
        }
        let backend = FakeBackend::default();
        let unwatch_not_found = backend.unwatch_not_found.clone();
        let watcher = watcher(backend);
        let root = PathBuf::from("/music");
        let child = root.join("artist");
        watcher.install_directory(&root, &child).unwrap();

        *unwatch_not_found.lock().unwrap() = true;
        watcher.retain_directories(&root, &HashSet::new()).unwrap();

        let state = watcher.state.lock().unwrap();
        let installed = &state.as_ref().unwrap().installed;
        assert!(installed.get(&root).is_some_and(HashSet::is_empty));
    }

    #[test]
    fn uninstall_accepts_a_watch_the_os_already_removed() {
        let backend = FakeBackend::default();
        let unwatch_not_found = backend.unwatch_not_found.clone();
        let watcher = watcher(backend);
        let root = PathBuf::from("/music");
        watcher.install_directory(&root, &root).unwrap();

        *unwatch_not_found.lock().unwrap() = true;
        watcher.uninstall(&root).unwrap();

        let state = watcher.state.lock().unwrap();
        assert!(!state.as_ref().unwrap().installed.contains_key(&root));
    }

    #[test]
    fn failed_uninstall_keeps_the_watch_registered_for_retry() {
        let backend = FakeBackend::default();
        let fail_unwatch = backend.fail_unwatch.clone();
        let watcher = watcher(backend);
        let root = PathBuf::from("/music");
        watcher.install_directory(&root, &root).unwrap();

        *fail_unwatch.lock().unwrap() = true;
        assert!(watcher.uninstall(&root).is_err());

        let state = watcher.state.lock().unwrap();
        assert!(state
            .as_ref()
            .unwrap()
            .installed
            .get(&root)
            .is_some_and(|paths| paths.contains(&root)));
    }

    #[test]
    fn partial_uninstall_failure_restores_removed_directory_watches() {
        if uses_recursive_root_watch() {
            return;
        }
        let backend = FakeBackend::default();
        let fail_unwatch_path = backend.fail_unwatch_path.clone();
        let watcher = watcher(backend);
        let root = PathBuf::from("/music");
        let child = root.join("artist");
        watcher.install_directory(&root, &root).unwrap();
        watcher.install_directory(&root, &child).unwrap();

        *fail_unwatch_path.lock().unwrap() = Some(root.clone());
        assert!(watcher.uninstall(&root).is_err());

        let state = watcher.state.lock().unwrap();
        let installed = state
            .as_ref()
            .unwrap()
            .installed
            .get(&root)
            .expect("failed uninstall retains the root");
        assert_eq!(installed, &HashSet::from([root, child]));
    }
}
