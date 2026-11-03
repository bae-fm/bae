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
        self.watches.calls.lock().unwrap().push(Call::Stop(self.root.clone(), self.serial));
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
        diagnostics: bae_diagnostics::Diagnostics::noop(),
        state: Arc::new(Mutex::new(Ok(Watches {
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
        }))),
    }
}

fn installed(watcher: &FolderWatcher, root: &Path) -> Option<HashSet<PathBuf>> {
    let state = watcher.state.lock().unwrap();
    state.as_ref().unwrap().roots.get(root).map(|watch| watch.installed.clone())
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
            [Call::Watch(root.clone(), 1, root.clone()), Call::Stop(root.clone(), 1),]
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
        [Call::Watch(root.clone(), 1, child.clone()), Call::Watch(root, 1, child),]
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

    assert_eq!(fake.calls().iter().filter(|call| matches!(call, Call::Watch(..))).count(), 2);
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
    let watcher = FolderWatcher::new(fs_tx, bae_diagnostics::Diagnostics::noop());
    watcher.install_directory(&root, &root).unwrap();
    watcher.install_directory(&root, &album).unwrap();

    std::fs::write(album.join("01.flac"), b"audio").unwrap();

    let deadline = tokio::time::Instant::now() + bae_util::test_channels::HANG_GUARD;
    loop {
        let report = tokio::time::timeout_at(deadline, fs_rx.recv())
            .await
            .expect("the written file was reported")
            .expect("the watch is still sending");
        let events = report.expect("the watch reports no error");
        if events.iter().flat_map(|event| &event.paths).any(|path| path.ends_with("01.flac")) {
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
