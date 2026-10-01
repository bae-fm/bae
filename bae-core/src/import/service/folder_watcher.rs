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
use crate::import::ImportError;

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
/// library. A root whose own watch will not start fails the same way, alone.
pub(crate) struct FolderWatcher {
    state: Mutex<Result<Watches, String>>,
}

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
    pub(crate) fn new(fs_tx: mpsc::UnboundedSender<WatchReport>) -> Self {
        let (raw_tx, raw_rx) = std::sync::mpsc::channel();
        let state = std::thread::Builder::new()
            .name("folder-watch-batches".to_string())
            .spawn(move || super::watch_batches::run(raw_rx, fs_tx))
            .map(|_| Watches {
                open: platform_watch(raw_tx),
                roots: HashMap::new(),
            })
            .map_err(|error| {
                let detail = format!("failed to start gathering folder watch events: {error}");
                error!("{detail}");
                detail
            });
        Self {
            state: Mutex::new(state),
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
        let watches = state
            .as_mut()
            .map_err(|e| ImportError::Watch { detail: e.clone() })?;
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
        let mut state = self.state.lock().unwrap();
        let watches = state.as_mut().map_err(|error| ImportError::Watch {
            detail: error.clone(),
        })?;
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
        let removed = match self.state.lock().unwrap().as_mut() {
            Ok(watches) => watches.roots.remove(path),
            // The watches never started; nothing was ever installed.
            Err(_) => None,
        };
        let Some(watch) = removed else {
            return FolderWatchSnapshot::default();
        };
        let snapshot = FolderWatchSnapshot {
            directories: watch.installed.iter().cloned().collect(),
        };
        drop(watch);
        snapshot
    }

    pub(super) fn reinstall(
        &self,
        root: &Path,
        snapshot: &FolderWatchSnapshot,
    ) -> Result<(), ImportError> {
        let mut failures = Vec::new();
        let mut state = self.state.lock().unwrap();
        let watches = state.as_mut().map_err(|error| ImportError::Watch {
            detail: error.clone(),
        })?;
        for directory in &snapshot.directories {
            let Some(mode) = watch_mode(root, directory) else {
                continue;
            };
            if watches
                .roots
                .get(root)
                .is_some_and(|watch| watch.installed.contains(directory))
            {
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
            backend
                .watch(directory, mode)
                .map_err(|error| watch_failed(directory, error))?;
            let fresh = RootWatch {
                backend,
                installed: HashSet::from([directory.to_path_buf()]),
            };
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
    ImportError::Watch {
        detail: format!("failed to watch {}: {error}", path.display()),
    }
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
mod tests {
    use super::*;

    /// Which watch a call reached — the root it was started for, and the
    /// how-manyth watch it was — and the call.
    #[derive(Debug, Clone, PartialEq)]
    enum Call {
        Watch(PathBuf, usize, PathBuf),
        Unwatch(PathBuf, usize, PathBuf),
        Stop(PathBuf, usize),
    }

    #[derive(Clone, Default)]
    struct FakeWatches {
        calls: Arc<Mutex<Vec<Call>>>,
        opened: Arc<Mutex<usize>>,
        fail_watch: Arc<Mutex<bool>>,
        unwatch_not_found: Arc<Mutex<bool>>,
    }

    struct FakeBackend {
        root: PathBuf,
        serial: usize,
        watches: FakeWatches,
    }

    impl WatchBackend for FakeBackend {
        fn watch(&mut self, path: &Path, _mode: RecursiveMode) -> notify::Result<()> {
            self.watches.calls.lock().unwrap().push(Call::Watch(
                self.root.clone(),
                self.serial,
                path.to_path_buf(),
            ));
            if *self.watches.fail_watch.lock().unwrap() {
                Err(notify::Error::generic("watch failed"))
            } else {
                Ok(())
            }
        }

        fn unwatch(&mut self, path: &Path) -> notify::Result<()> {
            self.watches.calls.lock().unwrap().push(Call::Unwatch(
                self.root.clone(),
                self.serial,
                path.to_path_buf(),
            ));
            if *self.watches.unwatch_not_found.lock().unwrap() {
                Err(notify::Error::watch_not_found())
            } else {
                Ok(())
            }
        }
    }

    impl Drop for FakeBackend {
        fn drop(&mut self) {
            self.watches
                .calls
                .lock()
                .unwrap()
                .push(Call::Stop(self.root.clone(), self.serial));
        }
    }

    impl FakeWatches {
        fn calls(&self) -> Vec<Call> {
            self.calls.lock().unwrap().clone()
        }
    }

    fn watcher(fake: &FakeWatches) -> FolderWatcher {
        let fake = fake.clone();
        FolderWatcher {
            state: Mutex::new(Ok(Watches {
                open: Box::new(move |root: &Path| {
                    let mut opened = fake.opened.lock().unwrap();
                    *opened += 1;
                    Ok(Box::new(FakeBackend {
                        root: root.to_path_buf(),
                        serial: *opened,
                        watches: fake.clone(),
                    }) as Box<dyn WatchBackend>)
                }),
                roots: HashMap::new(),
            })),
        }
    }

    fn installed(watcher: &FolderWatcher, root: &Path) -> Option<HashSet<PathBuf>> {
        let state = watcher.state.lock().unwrap();
        state
            .as_ref()
            .unwrap()
            .roots
            .get(root)
            .map(|watch| watch.installed.clone())
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
    fn backend_calls_follow_platform_policy_and_removal_stops_the_watch() {
        let fake = FakeWatches::default();
        let watcher = watcher(&fake);
        let root = PathBuf::from("/music");
        let child = root.join("artist");

        watcher.install_directory(&root, &root).unwrap();
        watcher.install_directory(&root, &child).unwrap();
        let snapshot = watcher.uninstall(&root);

        let mut directories = snapshot.directories;
        directories.sort();
        if uses_recursive_root_watch() {
            assert_eq!(
                fake.calls(),
                [
                    Call::Watch(root.clone(), 1, root.clone()),
                    Call::Stop(root.clone(), 1),
                ]
            );
            assert_eq!(directories, vec![root.clone()]);
        } else {
            assert_eq!(
                fake.calls(),
                [
                    Call::Watch(root.clone(), 1, root.clone()),
                    Call::Watch(root.clone(), 1, child.clone()),
                    Call::Stop(root.clone(), 1),
                ]
            );
            assert_eq!(directories, [root.clone(), child]);
        }
        assert_eq!(installed(&watcher, &root), None);
    }

    /// Installing, reading again, and removing one root reaches only that
    /// root's watch: another root's watch is started once and never touched.
    #[test]
    fn each_root_has_a_watch_of_its_own() {
        let fake = FakeWatches::default();
        let watcher = watcher(&fake);
        let first = PathBuf::from("/music");
        let second = PathBuf::from("/more music");

        watcher.install_directory(&first, &first).unwrap();
        watcher.install_directory(&second, &second).unwrap();
        watcher.install_directory(&first, &first).unwrap();
        watcher.uninstall(&first);

        let second_calls: Vec<Call> = fake
            .calls()
            .into_iter()
            .filter(|call| {
                matches!(call, Call::Watch(root, ..) | Call::Unwatch(root, ..) | Call::Stop(root, ..) if *root == second)
            })
            .collect();
        assert_eq!(second_calls, [Call::Watch(second.clone(), 2, second.clone())]);
        assert_eq!(installed(&watcher, &second), Some(HashSet::from([second])));
    }

    #[test]
    fn nonrecursive_backend_reissues_a_recreated_directory_watch() {
        if uses_recursive_root_watch() {
            return;
        }
        let fake = FakeWatches::default();
        let watcher = watcher(&fake);
        let root = PathBuf::from("/music");
        let child = root.join("artist");

        watcher.install_directory(&root, &child).unwrap();
        watcher.install_directory(&root, &child).unwrap();

        assert_eq!(
            fake.calls(),
            [
                Call::Watch(root.clone(), 1, child.clone()),
                Call::Watch(root, 1, child),
            ]
        );
    }

    /// Each authoritative reading binds the root a fresh watch, started
    /// before the one it replaces stops.
    #[test]
    fn recursive_root_is_rebound_on_each_authoritative_scan() {
        if !uses_recursive_root_watch() {
            return;
        }
        let fake = FakeWatches::default();
        let watcher = watcher(&fake);
        let root = PathBuf::from("/music");

        watcher.install_directory(&root, &root).unwrap();
        watcher.install_directory(&root, &root).unwrap();

        assert_eq!(
            fake.calls(),
            [
                Call::Watch(root.clone(), 1, root.clone()),
                Call::Watch(root.clone(), 2, root.clone()),
                Call::Stop(root, 1),
            ]
        );
    }

    /// A fresh watch that will not start leaves the one it would have
    /// replaced in place, and the next reading binds again.
    #[test]
    fn failed_rebind_keeps_the_watch_there() {
        if !uses_recursive_root_watch() {
            return;
        }
        let fake = FakeWatches::default();
        let watcher = watcher(&fake);
        let root = PathBuf::from("/music");

        watcher.install_directory(&root, &root).unwrap();
        *fake.fail_watch.lock().unwrap() = true;
        assert!(watcher.install_directory(&root, &root).is_err());
        assert_eq!(installed(&watcher, &root), Some(HashSet::from([root.clone()])));
        *fake.fail_watch.lock().unwrap() = false;
        watcher.install_directory(&root, &root).unwrap();

        assert_eq!(
            fake.calls(),
            [
                Call::Watch(root.clone(), 1, root.clone()),
                Call::Watch(root.clone(), 2, root.clone()),
                Call::Stop(root.clone(), 2),
                Call::Watch(root.clone(), 3, root.clone()),
                Call::Stop(root, 1),
            ]
        );
    }

    #[test]
    fn failed_watch_is_retried_on_the_next_scan() {
        let fake = FakeWatches::default();
        let watcher = watcher(&fake);
        let root = PathBuf::from("/music");

        *fake.fail_watch.lock().unwrap() = true;
        assert!(watcher.install_directory(&root, &root).is_err());
        *fake.fail_watch.lock().unwrap() = false;
        watcher.install_directory(&root, &root).unwrap();

        assert_eq!(
            fake.calls()
                .iter()
                .filter(|call| matches!(call, Call::Watch(..)))
                .count(),
            2
        );
        assert_eq!(installed(&watcher, &root), Some(HashSet::from([root])));
    }

    #[test]
    fn missing_nonrecursive_watch_is_removed_during_reconciliation() {
        if uses_recursive_root_watch() {
            return;
        }
        let fake = FakeWatches::default();
        let watcher = watcher(&fake);
        let root = PathBuf::from("/music");
        let child = root.join("artist");
        watcher.install_directory(&root, &child).unwrap();

        *fake.unwatch_not_found.lock().unwrap() = true;
        watcher.retain_directories(&root, &HashSet::new()).unwrap();

        assert_eq!(installed(&watcher, &root), Some(HashSet::new()));
    }

    /// A removal that does not land puts back the watches it stopped.
    #[test]
    fn reinstall_puts_back_what_uninstall_stopped() {
        let fake = FakeWatches::default();
        let watcher = watcher(&fake);
        let root = PathBuf::from("/music");
        let child = root.join("artist");
        watcher.install_directory(&root, &root).unwrap();
        watcher.install_directory(&root, &child).unwrap();
        let before = installed(&watcher, &root);

        let snapshot = watcher.uninstall(&root);
        watcher.reinstall(&root, &snapshot).unwrap();

        assert_eq!(installed(&watcher, &root), before);
        assert!(fake
            .calls()
            .iter()
            .any(|call| matches!(call, Call::Watch(_, 2, path) if *path == root)));
    }

    /// The platform's own watch reaches the batches: a file written into a
    /// watched folder arrives once that folder has gone quiet, in a batch
    /// naming nothing outside it but the root, spelled as the root was given.
    /// The temporary directory is reached through a symlink on macOS
    /// (`/var` is `/private/var`), whose resolved spelling FSEvents reports.
    /// FSEvents can report the root's own creation, made just before the
    /// watch, after the watch starts; a report of the root reads it whole and
    /// takes the folder's events along.
    #[tokio::test]
    async fn a_written_file_arrives_as_its_folders_batch() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_path_buf();
        let album = root.join("Album");
        std::fs::create_dir(&album).unwrap();
        let (fs_tx, mut fs_rx) = mpsc::unbounded_channel();
        let watcher = FolderWatcher::new(fs_tx);
        watcher.install_directory(&root, &root).unwrap();
        watcher.install_directory(&root, &album).unwrap();

        std::fs::write(album.join("01.flac"), b"audio").unwrap();

        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            let report = tokio::time::timeout_at(deadline, fs_rx.recv())
                .await
                .expect("the written file was reported")
                .expect("the watch is still sending");
            let events = report.expect("the watch reports no error");
            if events
                .iter()
                .flat_map(|event| &event.paths)
                .any(|path| path.ends_with("01.flac"))
            {
                assert!(
                    events
                        .iter()
                        .flat_map(|event| &event.paths)
                        .all(|path| path.starts_with(&album) || *path == root),
                    "{events:?}"
                );
                break;
            }
        }
    }
}
