use super::*;
use crate::config::{Config, ConfigHandle};
use crate::db::Database;
use crate::import::folder_registry::host_root;
use crate::keys::StoreKeys;
use coven::FixedClock;
use coven::SequentialIdProvider;
use std::time::Duration;
use tempfile::TempDir;

/// A stand-in watched root, spelled for the running host. Every `/`-spelled
/// root literal below goes through this or [`host_root`] before it reaches the
/// registry, because a watched root has to be absolute by the OS's own rule.
/// Path literals that never become a watched root — the prefix arithmetic
/// `roots_for_watch_error` does, say — need no rewrite and get none.
fn root_path(posix: &str) -> PathBuf {
    PathBuf::from(host_root(posix))
}

async fn setup_import_service() -> (ImportService, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let database = Database::new_test(
        db_path.to_str().unwrap(),
        Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
    )
    .await
    .unwrap();
    let library_dir = coven::StoreDir::new(temp_dir.path());
    let library_id = format!("test-{}", temp_dir.path().display());
    let config = Config::with_defaults(
        library_id.clone(),
        "test-device".to_string(),
        library_dir.clone(),
        "Test Library".to_string(),
    );
    crate::config::install_test_keyring();
    let manager = LibraryManager::new(
        database,
        Arc::new(ConfigHandle::new(config)),
        StoreKeys::bind(library_id),
        Arc::new(coven::SystemClock),
        Arc::new(coven::UuidProvider),
        crate::diagnostics::Diagnostics::noop(),
        tokio::runtime::Handle::current(),
    );
    let (_commands_tx, commands_rx) = tokio::sync::mpsc::unbounded_channel();
    let (event_tx, _) = tokio::sync::broadcast::channel(16);
    (
        ImportService {
            commands_rx,
            event_tx,
            library_manager: manager,
            // Near-zero so a test that drives an unreachable cover URL through the
            // retry loop doesn't sleep the real 1s + 2s + 4s of backoff.
            cover_retry_base_delay: std::time::Duration::from_millis(0),
        },
        temp_dir,
    )
}

#[derive(Clone)]
struct FakeScanStarter {
    scans: Arc<Mutex<Vec<FakeStartedScan>>>,
    started: Arc<tokio::sync::Notify>,
}

struct FakeStartedScan {
    cancellation: crate::import::folder_scanner::ScanCancellation,
    completion: Option<tokio::sync::oneshot::Sender<Result<(), String>>>,
    abort: tokio::task::AbortHandle,
}

impl FakeScanStarter {
    fn new() -> (Self, RootScanStarter) {
        let fake = Self {
            scans: Arc::new(Mutex::new(Vec::new())),
            started: Arc::new(tokio::sync::Notify::new()),
        };
        let captured = fake.clone();
        let starter: RootScanStarter = Arc::new(move |id, path, completion| {
            let cancellation = crate::import::folder_scanner::ScanCancellation::new();
            let (finish, finished) = tokio::sync::oneshot::channel();
            let task = tokio::spawn(async move {
                let result = finished
                    .await
                    .expect("fake scan completion sender was retained");
                completion
                    .send(RootScanCompletion { id, path, result })
                    .expect("coordinator still receives completions");
            });
            captured.scans.lock().unwrap().push(FakeStartedScan {
                cancellation: cancellation.clone(),
                completion: Some(finish),
                abort: task.abort_handle(),
            });
            captured.started.notify_one();
            RootScanTask { cancellation, task }
        });
        (fake, starter)
    }

    async fn wait_for_count(&self, count: usize) {
        while self.scans.lock().unwrap().len() < count {
            tokio::time::timeout(Duration::from_secs(2), self.started.notified())
                .await
                .expect("scan coordinator did not start the expected scan");
        }
    }

    fn complete(&self, index: usize, result: Result<(), String>) {
        self.scans.lock().unwrap()[index]
            .completion
            .take()
            .expect("fake scan has not completed")
            .send(result)
            .expect("coordinator still waits for the fake scan");
    }

    fn cancellation(&self, index: usize) -> crate::import::folder_scanner::ScanCancellation {
        self.scans.lock().unwrap()[index].cancellation.clone()
    }

    fn abort(&self, index: usize) {
        self.scans.lock().unwrap()[index].abort.abort();
    }

    async fn wait_for_cancellation(&self, index: usize) {
        tokio::time::timeout(Duration::from_secs(2), async {
            while !self.cancellation(index).is_cancelled() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("scan coordinator did not cancel the active scan");
    }
}

#[derive(Default)]
struct FakeRemovalBackend {
    uninstall_error: Mutex<Option<String>>,
    remove_error: Mutex<Option<String>>,
    reinstall_error: Mutex<Option<String>>,
    block_reinstall: std::sync::atomic::AtomicBool,
    reinstall_started: tokio::sync::Notify,
    release_reinstall: tokio::sync::Notify,
    calls: Mutex<Vec<&'static str>>,
}

#[async_trait::async_trait]
impl RootRemovalBackend for FakeRemovalBackend {
    async fn uninstall(&self, _path: &Path) -> Result<FolderWatchSnapshot, String> {
        self.calls.lock().unwrap().push("uninstall");
        match self.uninstall_error.lock().unwrap().clone() {
            Some(error) => Err(error),
            None => Ok(FolderWatchSnapshot::default()),
        }
    }

    async fn reinstall(&self, _path: &Path, _snapshot: &FolderWatchSnapshot) -> Result<(), String> {
        self.calls.lock().unwrap().push("reinstall");
        if self
            .block_reinstall
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            self.reinstall_started.notify_one();
            self.release_reinstall.notified().await;
        }
        match self.reinstall_error.lock().unwrap().clone() {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    async fn remove_durable_root(&self, _path: &Path) -> Result<(), String> {
        self.calls.lock().unwrap().push("remove");
        match self.remove_error.lock().unwrap().clone() {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

struct CoordinatorHarness {
    commands: tokio::sync::mpsc::UnboundedSender<WatcherCommand>,
    fs_events: tokio::sync::mpsc::UnboundedSender<DebounceEventResult>,
    scans: FakeScanStarter,
    folder_registry: Arc<Mutex<ImportFolderRegistry>>,
    candidate_state: Arc<Mutex<ImportCandidateState>>,
    folder_state_commit: Arc<tokio::sync::Mutex<()>>,
    removal_backend: Arc<FakeRemovalBackend>,
    coordinator_thread: Mutex<Option<std::thread::JoinHandle<()>>>,
    _temp: TempDir,
}

impl CoordinatorHarness {
    async fn new() -> Self {
        Self::with_roots(&["/music"]).await
    }

    /// `roots` are `/`-spelled labels; each is registered in the host's own
    /// spelling, the same one [`root_path`] gives the tests that address them.
    async fn with_roots(roots: &[&str]) -> Self {
        let roots: Vec<String> = roots.iter().map(|root| host_root(root)).collect();
        let (service, temp) = setup_import_service().await;
        for root in &roots {
            service
                .library_manager
                .add_watched_import_folder(root)
                .await
                .unwrap();
        }
        let (commands, command_rx) = tokio::sync::mpsc::unbounded_channel();
        let (fs_events, fs_rx) = tokio::sync::mpsc::unbounded_channel();
        let registry = Arc::new(Mutex::new(
            ImportFolderRegistry::from_stored(roots.clone(), Vec::new()).unwrap(),
        ));
        let state = Arc::new(Mutex::new(ImportCandidateState::default()));
        if let Some(root) = roots.first() {
            state
                .lock()
                .unwrap()
                .upsert_invalid(crate::import::InvalidCandidate {
                    path: PathBuf::from(root).join("Group/Release"),
                    name: "Release".to_string(),
                    watched_folder_path: root.clone(),
                    display_path: "Group/Release".to_string(),
                    resolved_boundaries: Vec::new(),
                    reason: crate::import::InvalidReason::NoValidAudio,
                });
        }
        let folder_state_commit = Arc::new(tokio::sync::Mutex::new(()));
        let (scans, starter) = FakeScanStarter::new();
        let removal_backend = Arc::new(FakeRemovalBackend::default());
        let coordinator_thread = ImportService::start_watcher_with_starter(
            command_rx,
            fs_rx,
            service.event_tx,
            service.library_manager,
            registry.clone(),
            state.clone(),
            folder_state_commit.clone(),
            starter,
            removal_backend.clone(),
        );
        Self {
            commands,
            fs_events,
            scans,
            folder_registry: registry,
            candidate_state: state,
            folder_state_commit,
            removal_backend,
            coordinator_thread: Mutex::new(Some(coordinator_thread)),
            _temp: temp,
        }
    }

    async fn shutdown(&self) {
        let (completion, done) = std::sync::mpsc::channel();
        self.commands
            .send(WatcherCommand::Shutdown { completion })
            .unwrap();
        tokio::task::spawn_blocking(move || done.recv())
            .await
            .unwrap()
            .unwrap();
        let thread = self
            .coordinator_thread
            .lock()
            .unwrap()
            .take()
            .expect("coordinator has not already shut down");
        tokio::task::spawn_blocking(move || thread.join())
            .await
            .unwrap()
            .unwrap();
    }
}

#[tokio::test]
async fn coordinator_coalesces_same_root_to_one_followup_scan() {
    let harness = CoordinatorHarness::new().await;
    let root = root_path("/music");
    harness
        .commands
        .send(WatcherCommand::Rescan(root.clone()))
        .unwrap();
    harness.scans.wait_for_count(1).await;
    harness
        .commands
        .send(WatcherCommand::Rescan(root.clone()))
        .unwrap();
    harness.commands.send(WatcherCommand::Rescan(root)).unwrap();
    harness.scans.complete(0, Ok(()));
    harness.scans.wait_for_count(2).await;
    harness.scans.complete(1, Ok(()));
    tokio::task::yield_now().await;
    assert_eq!(harness.scans.scans.lock().unwrap().len(), 2);
    harness.shutdown().await;
}

#[tokio::test]
async fn coordinator_removal_waits_for_the_active_scan_to_finish() {
    let harness = CoordinatorHarness::new().await;
    let root = root_path("/music");
    harness
        .commands
        .send(WatcherCommand::Rescan(root.clone()))
        .unwrap();
    harness.scans.wait_for_count(1).await;

    let (completion, result) = tokio::sync::oneshot::channel();
    harness
        .commands
        .send(WatcherCommand::Remove {
            path: root,
            completion,
        })
        .unwrap();
    harness.scans.wait_for_cancellation(0).await;

    let mut result = Box::pin(result);
    assert!(
        tokio::time::timeout(Duration::from_millis(50), result.as_mut())
            .await
            .is_err(),
        "removal completed while the scan could still install a late watch"
    );

    harness.scans.complete(0, Ok(()));
    assert_eq!(result.await.unwrap(), Ok(()));
    harness.shutdown().await;
}

#[tokio::test]
async fn coordinator_coalesces_duplicate_removals_for_one_root() {
    let harness = CoordinatorHarness::new().await;
    let root = root_path("/music");
    harness
        .commands
        .send(WatcherCommand::Rescan(root.clone()))
        .unwrap();
    harness.scans.wait_for_count(1).await;

    let (first_completion, first_result) = tokio::sync::oneshot::channel();
    harness
        .commands
        .send(WatcherCommand::Remove {
            path: root.clone(),
            completion: first_completion,
        })
        .unwrap();
    harness.scans.wait_for_cancellation(0).await;
    let (second_completion, second_result) = tokio::sync::oneshot::channel();
    harness
        .commands
        .send(WatcherCommand::Remove {
            path: root,
            completion: second_completion,
        })
        .unwrap();

    harness.scans.complete(0, Ok(()));
    assert_eq!(first_result.await.unwrap(), Ok(()));
    assert_eq!(second_result.await.unwrap(), Ok(()));
    assert_eq!(
        harness.removal_backend.calls.lock().unwrap().as_slice(),
        ["uninstall", "remove"]
    );
    harness.shutdown().await;
}

#[tokio::test]
async fn coordinator_blocked_root_removal_does_not_block_another_roots_refresh() {
    let harness = CoordinatorHarness::with_roots(&["/music/one", "/music/two"]).await;
    harness
        .commands
        .send(WatcherCommand::Rescan(root_path("/music/one")))
        .unwrap();
    harness.scans.wait_for_count(1).await;

    let (remove_completion, remove_result) = tokio::sync::oneshot::channel();
    harness
        .commands
        .send(WatcherCommand::Remove {
            path: root_path("/music/one"),
            completion: remove_completion,
        })
        .unwrap();
    harness.scans.wait_for_cancellation(0).await;

    let (refresh_completion, refresh_result) = tokio::sync::oneshot::channel();
    harness
        .commands
        .send(WatcherCommand::Refresh {
            path: root_path("/music/two"),
            completion: refresh_completion,
        })
        .unwrap();
    harness.scans.wait_for_count(2).await;
    harness.scans.complete(1, Ok(()));
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), refresh_result)
            .await
            .expect("another root's refresh was blocked by removal")
            .unwrap(),
        Ok(())
    );

    assert!(
        tokio::time::timeout(Duration::from_millis(50), remove_result)
            .await
            .is_err(),
        "removal completed before its blocked scan"
    );
    harness.scans.complete(0, Ok(()));
    harness.shutdown().await;
}

#[tokio::test]
async fn coordinator_removal_join_failure_restores_a_runnable_root_schedule() {
    let harness = CoordinatorHarness::new().await;
    let root = root_path("/music");
    harness
        .commands
        .send(WatcherCommand::Rescan(root.clone()))
        .unwrap();
    harness.scans.wait_for_count(1).await;

    let (completion, result) = tokio::sync::oneshot::channel();
    harness
        .commands
        .send(WatcherCommand::Remove {
            path: root,
            completion,
        })
        .unwrap();
    harness.scans.wait_for_cancellation(0).await;
    harness.scans.abort(0);

    let error = result.await.unwrap().unwrap_err();
    assert!(error.contains("folder scan task failed while removing"));
    harness.scans.wait_for_count(2).await;
    harness.scans.complete(1, Ok(()));
    harness.shutdown().await;
}

#[tokio::test]
async fn coordinator_removal_uninstall_failure_restores_a_runnable_root_schedule() {
    let harness = CoordinatorHarness::new().await;
    *harness.removal_backend.uninstall_error.lock().unwrap() =
        Some("injected uninstall failure".to_string());
    let root = root_path("/music");
    harness
        .commands
        .send(WatcherCommand::Rescan(root.clone()))
        .unwrap();
    harness.scans.wait_for_count(1).await;

    let (completion, result) = tokio::sync::oneshot::channel();
    harness
        .commands
        .send(WatcherCommand::Remove {
            path: root,
            completion,
        })
        .unwrap();
    harness.scans.wait_for_cancellation(0).await;
    harness.scans.complete(0, Ok(()));

    let error = result.await.unwrap().unwrap_err();
    assert!(error.contains("injected uninstall failure"));
    assert_eq!(
        harness.removal_backend.calls.lock().unwrap().as_slice(),
        ["uninstall"]
    );
    harness.scans.wait_for_count(2).await;
    harness.scans.complete(1, Ok(()));
    harness.shutdown().await;
}

#[tokio::test]
async fn coordinator_removal_database_failure_reinstalls_and_rescans_before_returning() {
    let harness = CoordinatorHarness::new().await;
    *harness.removal_backend.remove_error.lock().unwrap() =
        Some("injected database failure".to_string());
    let root = root_path("/music");
    harness
        .commands
        .send(WatcherCommand::Rescan(root.clone()))
        .unwrap();
    harness.scans.wait_for_count(1).await;

    let (completion, result) = tokio::sync::oneshot::channel();
    harness
        .commands
        .send(WatcherCommand::Remove {
            path: root,
            completion,
        })
        .unwrap();
    harness.scans.wait_for_cancellation(0).await;
    harness.scans.complete(0, Ok(()));

    let error = result.await.unwrap().unwrap_err();
    assert!(error.contains("injected database failure"));
    assert_eq!(
        harness.removal_backend.calls.lock().unwrap().as_slice(),
        ["uninstall", "remove", "reinstall"]
    );
    assert_eq!(
        harness.folder_registry.lock().unwrap().watched_folders(),
        vec![crate::import::WatchedFolder::from_path(host_root("/music"))]
    );
    harness.scans.wait_for_count(2).await;
    harness.scans.complete(1, Ok(()));
    harness.shutdown().await;
}

#[tokio::test]
async fn coordinator_blocked_reinstall_does_not_block_another_roots_persistence() {
    let harness = CoordinatorHarness::with_roots(&["/music/one", "/music/two"]).await;
    *harness.removal_backend.remove_error.lock().unwrap() =
        Some("injected database failure".to_string());
    harness
        .removal_backend
        .block_reinstall
        .store(true, std::sync::atomic::Ordering::SeqCst);
    harness
        .commands
        .send(WatcherCommand::Rescan(root_path("/music/one")))
        .unwrap();
    harness.scans.wait_for_count(1).await;

    let (remove_completion, remove_result) = tokio::sync::oneshot::channel();
    harness
        .commands
        .send(WatcherCommand::Remove {
            path: root_path("/music/one"),
            completion: remove_completion,
        })
        .unwrap();
    harness.scans.wait_for_cancellation(0).await;
    harness.scans.complete(0, Ok(()));
    tokio::time::timeout(
        Duration::from_secs(2),
        harness.removal_backend.reinstall_started.notified(),
    )
    .await
    .expect("failed durable removal did not start watch restoration");

    let (refresh_completion, refresh_result) = tokio::sync::oneshot::channel();
    harness
        .commands
        .send(WatcherCommand::Refresh {
            path: root_path("/music/two"),
            completion: refresh_completion,
        })
        .unwrap();
    harness.scans.wait_for_count(2).await;
    let other_root_commit = tokio::time::timeout(
        Duration::from_millis(50),
        harness.folder_state_commit.lock(),
    )
    .await;
    let other_root_was_blocked = other_root_commit.is_err();
    drop(other_root_commit);

    harness.removal_backend.release_reinstall.notify_one();
    harness.scans.complete(1, Ok(()));
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), refresh_result)
            .await
            .expect("another root's refresh did not complete")
            .unwrap(),
        Ok(())
    );
    assert!(
        remove_result.await.unwrap().is_err(),
        "injected durable removal failure was not returned"
    );
    harness.scans.wait_for_count(3).await;
    harness.scans.complete(2, Ok(()));
    harness.shutdown().await;

    assert!(
        !other_root_was_blocked,
        "watch restoration held the persistence guard needed by another root"
    );
}

#[tokio::test]
async fn coordinator_removal_database_and_restore_failures_return_both_errors() {
    let harness = CoordinatorHarness::new().await;
    *harness.removal_backend.remove_error.lock().unwrap() =
        Some("injected database failure".to_string());
    *harness.removal_backend.reinstall_error.lock().unwrap() =
        Some("injected restore failure".to_string());
    let root = root_path("/music");
    harness
        .commands
        .send(WatcherCommand::Rescan(root.clone()))
        .unwrap();
    harness.scans.wait_for_count(1).await;

    let (completion, result) = tokio::sync::oneshot::channel();
    harness
        .commands
        .send(WatcherCommand::Remove {
            path: root,
            completion,
        })
        .unwrap();
    harness.scans.wait_for_cancellation(0).await;
    harness.scans.complete(0, Ok(()));

    let error = result.await.unwrap().unwrap_err();
    assert!(error.contains("injected database failure"));
    assert!(error.contains("injected restore failure"));
    harness.scans.wait_for_count(2).await;
    harness.scans.complete(1, Ok(()));
    harness.shutdown().await;
}

#[tokio::test]
async fn coordinator_runs_different_roots_concurrently() {
    let harness = CoordinatorHarness::with_roots(&["/music/one", "/music/two"]).await;
    for root in ["/music/one", "/music/two"] {
        harness
            .commands
            .send(WatcherCommand::Rescan(root_path(root)))
            .unwrap();
    }
    harness.scans.wait_for_count(2).await;
    assert!(!harness.scans.cancellation(0).is_cancelled());
    assert!(!harness.scans.cancellation(1).is_cancelled());
    harness.scans.complete(0, Ok(()));
    harness.scans.complete(1, Ok(()));
    harness.shutdown().await;
}

#[tokio::test]
async fn coordinator_completes_refresh_waiter_with_its_scan_result() {
    let harness = CoordinatorHarness::new().await;
    let (completion, result) = tokio::sync::oneshot::channel();
    harness
        .commands
        .send(WatcherCommand::Refresh {
            path: root_path("/music"),
            completion,
        })
        .unwrap();
    harness.scans.wait_for_count(1).await;
    harness.scans.complete(0, Err("offline".to_string()));
    assert_eq!(result.await.unwrap(), Err("offline".to_string()));
    harness.shutdown().await;
}

#[tokio::test]
async fn coordinator_completes_scan_while_filesystem_batches_remain_ready() {
    let harness = CoordinatorHarness::new().await;
    let (completion, result) = tokio::sync::oneshot::channel();
    harness
        .commands
        .send(WatcherCommand::Refresh {
            path: root_path("/music"),
            completion,
        })
        .unwrap();
    harness.scans.wait_for_count(1).await;
    for _ in 0..10_000 {
        harness.fs_events.send(Ok(Vec::new())).unwrap();
    }
    harness.scans.complete(0, Ok(()));
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), result)
            .await
            .expect("ready filesystem batches starved scan completion")
            .unwrap(),
        Ok(())
    );
    harness.shutdown().await;
}

#[tokio::test]
async fn cancelled_scan_task_does_not_begin_a_durable_generation() {
    let (service, tmp) = setup_import_service().await;
    let root = tmp.path().join("watched");
    std::fs::create_dir(&root).unwrap();
    service
        .library_manager
        .add_watched_import_folder(&root.to_string_lossy())
        .await
        .unwrap();
    let registry = Arc::new(Mutex::new(
        ImportFolderRegistry::from_stored(vec![root.to_string_lossy().into_owned()], Vec::new())
            .unwrap(),
    ));
    let state = Arc::new(Mutex::new(ImportCandidateState::default()));
    let (watch_tx, _watch_rx) = tokio::sync::mpsc::unbounded_channel();
    let watcher = Arc::new(FolderWatcher::new(watch_tx));
    let (completion_tx, mut completion_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut events = service.event_tx.subscribe();

    let scan = spawn_root_scan(
        1,
        root,
        service.event_tx.clone(),
        service.library_manager.clone(),
        registry,
        state.clone(),
        Arc::new(tokio::sync::Mutex::new(())),
        watcher,
        completion_tx,
    );
    scan.cancellation.cancel();
    tokio::time::timeout(Duration::from_secs(2), completion_rx.recv())
        .await
        .expect("cancelled scan did not report task completion")
        .expect("scan task completion channel closed");
    scan.task.await.unwrap();

    assert!(state
        .lock()
        .unwrap()
        .snapshot(Vec::new())
        .folder_scan_statuses
        .is_empty());
    assert!(service
        .library_manager
        .database_for_test()
        .load_folder_scan_snapshots()
        .await
        .unwrap()
        .is_empty());
    assert!(matches!(
        events.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Empty)
    ));
}

#[tokio::test]
async fn coordinator_decision_waits_for_cancelled_scan_before_starting_replacement() {
    let harness = CoordinatorHarness::new().await;
    let root = root_path("/music");
    harness
        .commands
        .send(WatcherCommand::Rescan(root.clone()))
        .unwrap();
    harness.scans.wait_for_count(1).await;
    let (decision_completion, decision_result) = tokio::sync::oneshot::channel();
    harness
        .commands
        .send(WatcherCommand::SetFolderReleaseDecision {
            target: (
                crate::import::FolderReleaseDecisionKey {
                    watched_folder_path: root.to_string_lossy().into_owned(),
                    relative_folder_path: "Group".to_string(),
                },
                crate::import::FolderReleaseDecision::CombineAsOneRelease,
            ),
            completion: decision_completion,
        })
        .unwrap();
    harness.scans.wait_for_cancellation(0).await;
    assert_eq!(harness.scans.scans.lock().unwrap().len(), 1);
    harness.scans.complete(0, Ok(()));
    harness.scans.wait_for_count(2).await;
    assert!(!harness.scans.cancellation(1).is_cancelled());
    harness.scans.complete(1, Ok(()));
    assert_eq!(decision_result.await.unwrap(), Ok(()));
    harness.shutdown().await;
}

#[tokio::test]
async fn coordinator_decision_validates_after_the_cancelled_scan_releases_its_commit() {
    let harness = CoordinatorHarness::new().await;
    let root = root_path("/music");
    harness
        .commands
        .send(WatcherCommand::Rescan(root.clone()))
        .unwrap();
    harness.scans.wait_for_count(1).await;

    let commit = harness.folder_state_commit.clone().lock_owned().await;
    let (decision_completion, decision_result) = tokio::sync::oneshot::channel();
    harness
        .commands
        .send(WatcherCommand::SetFolderReleaseDecision {
            target: (
                crate::import::FolderReleaseDecisionKey {
                    watched_folder_path: root.to_string_lossy().into_owned(),
                    relative_folder_path: "Group".to_string(),
                },
                crate::import::FolderReleaseDecision::CombineAsOneRelease,
            ),
            completion: decision_completion,
        })
        .unwrap();
    harness.scans.wait_for_cancellation(0).await;
    harness.candidate_state.lock().unwrap().remove_root(&root);
    drop(commit);

    assert_eq!(
        decision_result.await.unwrap(),
        Err("Group is not a current release boundary".to_string())
    );
    harness.scans.complete(0, Ok(()));
    harness.scans.wait_for_count(2).await;
    harness.scans.complete(1, Ok(()));
    harness.shutdown().await;
}

#[tokio::test]
async fn coordinator_shutdown_waits_for_active_scan() {
    let harness = CoordinatorHarness::new().await;
    harness
        .commands
        .send(WatcherCommand::Rescan(root_path("/music")))
        .unwrap();
    harness.scans.wait_for_count(1).await;
    let (shutdown_completion, shutdown_done) = std::sync::mpsc::channel();
    harness
        .commands
        .send(WatcherCommand::Shutdown {
            completion: shutdown_completion,
        })
        .unwrap();
    tokio::task::yield_now().await;
    assert!(matches!(
        shutdown_done.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Empty)
    ));
    harness.scans.complete(0, Ok(()));
    tokio::task::spawn_blocking(move || shutdown_done.recv())
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn cancelling_a_panicked_folder_walk_surfaces_the_join_failure() {
    async fn panic_during_walk() -> (
        Result<(), crate::import::folder_scanner::FolderScanError>,
        HashSet<PathBuf>,
    ) {
        panic!("folder walk panic");
    }

    let cancellation = crate::import::folder_scanner::ScanCancellation::new();
    let (item_tx, mut item_rx) = tokio::sync::mpsc::channel(1);
    drop(item_tx);
    let error = ImportService::cancel_and_join_folder_walk(
        Path::new("/music"),
        &cancellation,
        &mut item_rx,
        tokio::spawn(panic_during_walk()),
    )
    .await
    .expect_err("a panicked traversal task cannot report a successful cancellation");

    assert!(cancellation.is_cancelled());
    assert!(error.to_string().contains("folder scan task failed"));
}

#[tokio::test]
async fn a_db_accepted_failure_that_memory_rejects_is_an_internal_error() {
    let (service, _temp) = setup_import_service().await;
    let root = &root_path("/music");
    service
        .library_manager
        .add_watched_import_folder(root.to_str().unwrap())
        .await
        .unwrap();
    let generation = service
        .library_manager
        .begin_folder_scan(root.to_str().unwrap())
        .await
        .unwrap();
    let candidate_state = Arc::new(Mutex::new(ImportCandidateState::default()));
    let folder_state_commit = Arc::new(tokio::sync::Mutex::new(()));

    let error = ImportService::record_scan_failure(
        root,
        generation,
        "share unavailable".to_string(),
        &service.event_tx,
        &service.library_manager,
        &candidate_state,
        &folder_state_commit,
    )
    .await
    .expect_err("DB and memory generation disagreement must fail");

    assert!(error.to_string().contains("was not current in memory"));
}

fn write_test_jpeg(path: &Path) {
    let image = ::image::RgbImage::from_pixel(1, 1, ::image::Rgb([0, 0, 0]));
    image.save(path).unwrap();
}

#[test]
fn affected_roots_maps_changed_paths_to_their_watched_roots() {
    let root_a = PathBuf::from("/music/new rips");
    let root_b = PathBuf::from("/downloads/bandcamp");
    let roots = vec![root_a.clone(), root_b.clone()];

    // A change inside one root flags only that root.
    let changed = [Path::new("/music/new rips/Album/01.flac")];
    assert_eq!(affected_roots(&changed, &roots), vec![root_a.clone()]);

    // Changes under both roots flag both, in roots order, deduped.
    let changed = [
        Path::new("/downloads/bandcamp/X/cover.jpg"),
        Path::new("/music/new rips/Y"),
        Path::new("/music/new rips/Z"),
    ];
    assert_eq!(affected_roots(&changed, &roots), vec![root_a, root_b]);

    // A change outside every watched root flags nothing.
    let changed = [Path::new("/elsewhere/file")];
    assert!(affected_roots(&changed, &roots).is_empty());
}

#[test]
fn watcher_error_without_a_mapped_path_rescans_every_root() {
    let roots = vec![PathBuf::from("/music/a"), PathBuf::from("/music/b")];
    assert_eq!(roots_for_watch_error(&[], &roots), roots);
    assert_eq!(
        roots_for_watch_error(&[PathBuf::from("/outside")], &roots),
        roots
    );
    assert_eq!(
        roots_for_watch_error(&[PathBuf::from("/music/b/release")], &roots),
        vec![PathBuf::from("/music/b")]
    );
}

/// `common_ancestor` derives the local-path root by folding over the
/// files' parent dirs. It must compare path components, not string
/// prefixes, so `/m/Album` and `/m/Album2` collapse to `/m` (a string
/// prefix would wrongly keep `/m/Album`), and an ancestor argument returns
/// itself rather than descending.
#[test]
fn common_ancestor_cases() {
    use std::path::Path;
    // Sibling files share their parent.
    assert_eq!(
        common_ancestor(Path::new("/m/Album/01.flac"), Path::new("/m/Album/02.flac")),
        Path::new("/m/Album")
    );
    // `a` is already an ancestor of `b`: keep `a`.
    assert_eq!(
        common_ancestor(Path::new("/m/Album"), Path::new("/m/Album/Disc1/01.flac")),
        Path::new("/m/Album")
    );
    // Component-wise, not string-prefix: Album vs Album2 don't share /m/Album.
    assert_eq!(
        common_ancestor(Path::new("/m/Album/x"), Path::new("/m/Album2/y")),
        Path::new("/m")
    );
    // Disjoint trees collapse to the root.
    assert_eq!(
        common_ancestor(Path::new("/a/b"), Path::new("/c/d")),
        Path::new("/")
    );
}

/// image_cover_priority decides which folder image wins as the cover when
/// the user makes no explicit pick: a name containing "cover" or "front"
/// (case-insensitive, anywhere in the name) ranks first, everything else
/// second. The fallback sort relies on this ordering.
#[test]
fn image_cover_priority_ranks_front_and_cover_first() {
    assert_eq!(ImportService::image_cover_priority("Cover.jpg"), 0);
    assert_eq!(ImportService::image_cover_priority("front.png"), 0);
    assert_eq!(ImportService::image_cover_priority("FRONT.JPG"), 0);
    assert_eq!(
        ImportService::image_cover_priority("album-front-scan.jpg"),
        0
    );
    assert_eq!(ImportService::image_cover_priority("Back.jpg"), 1);
    assert_eq!(ImportService::image_cover_priority("inlay.png"), 1);
    assert_eq!(ImportService::image_cover_priority("disc1.jpg"), 1);
}

#[tokio::test]
async fn explicit_bmp_cover_is_selected() {
    let (service, tmp) = setup_import_service().await;
    let bmp = tmp.path().join("cover.bmp");
    let jpg = tmp.path().join("front.jpg");
    std::fs::write(&bmp, b"bmp bytes").unwrap();
    std::fs::write(&jpg, b"jpg bytes").unwrap();
    let discovered = vec![
        ScannedFile::new(bmp.clone(), "cover.bmp".to_string(), 9),
        ScannedFile::new(jpg, "front.jpg".to_string(), 9),
    ];

    let candidate = service
        .pick_folder_cover(&discovered, Some("cover.bmp"))
        .unwrap()
        .expect("selected cover should be picked");

    assert_eq!(candidate.source, "local");
    assert_eq!(candidate.source_url.as_deref(), Some("release://cover.bmp"));
    assert_eq!(candidate.bytes, b"bmp bytes");
}

#[tokio::test]
async fn explicit_local_cover_missing_from_discovered_images_is_an_error() {
    let (service, tmp) = setup_import_service().await;
    let fallback = tmp.path().join("front.jpg");
    std::fs::write(&fallback, b"jpg bytes").unwrap();
    let discovered = vec![ScannedFile::new(fallback, "front.jpg".to_string(), 9)];

    let err = service
        .pick_folder_cover(&discovered, Some("cover.bmp"))
        .unwrap_err();

    assert!(
        matches!(&err, crate::import::ImportError::CoverArt { detail } if detail.contains("Selected cover") && detail.contains("not found")),
        "got: {err}"
    );
}

#[tokio::test]
async fn explicit_local_cover_with_no_discovered_images_is_an_error() {
    let (service, _tmp) = setup_import_service().await;

    let err = service
        .pick_folder_cover(&[], Some("cover.bmp"))
        .unwrap_err();

    assert!(
        matches!(&err, crate::import::ImportError::CoverArt { detail } if detail.contains("Selected cover") && detail.contains("not found")),
        "got: {err}"
    );
}

#[tokio::test]
async fn selected_local_cover_path_must_match_discovered_file() {
    let (service, tmp) = setup_import_service().await;
    let folder = tmp.path().join("release");
    std::fs::create_dir(&folder).unwrap();
    write_test_jpeg(&folder.join("front.jpg"));
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/flac/01 Test Track 1.flac"),
        folder.join("01.flac"),
    )
    .unwrap();
    let expected_content_hash =
        crate::import::folder_scanner::collect_release_candidate_files_with_scope(
            &folder,
            crate::import::ReleaseFileScope::Recursive,
            &crate::import::folder_scanner::StoredCandidateEdits::none(),
        )
        .unwrap()
        .content_hash();

    let result = service
        .prepare_and_run_folder_import(
            "import-1".to_string(),
            folder.to_string_lossy().into_owned(),
            folder,
            crate::import::folder_scanner::ReleaseFileScope::Recursive,
            expected_content_hash,
            0,
            Some(CoverSelection::Local("cover.bmp".to_string())),
            StorageMode::Local,
            false,
            crate::import::IdentityChoice::Unknown,
            None,
        )
        .await;

    let err = result.unwrap_err();
    assert!(
        matches!(&err, crate::import::ImportError::CoverArt { detail } if detail.contains("Selected cover cover.bmp not found")),
        "got: {err}"
    );
}

#[tokio::test]
async fn failed_import_before_finalize_leaves_only_import_audit_row() {
    let (service, tmp) = setup_import_service().await;
    let folder = tmp.path().join("release");
    std::fs::create_dir(&folder).unwrap();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/flac/01 Test Track 1.flac"),
        folder.join("01.flac"),
    )
    .unwrap();

    let import_id = "import-1".to_string();
    let expectation = super::ImportExpectation {
        content_hash: crate::import::folder_scanner::collect_release_candidate_files_with_scope(
            &folder,
            crate::import::ReleaseFileScope::Recursive,
            &crate::import::folder_scanner::StoredCandidateEdits::none(),
        )
        .unwrap()
        .content_hash(),
        edit_revision: 0,
    };
    service
        .do_import(
            ImportCommand {
                import_id: import_id.clone(),
                candidate_key: folder.to_string_lossy().into_owned(),
                folder,
                scope: crate::import::folder_scanner::ReleaseFileScope::Recursive,
                selected_cover: Some(CoverSelection::Remote(
                    "http://127.0.0.1:9/missing.jpg".to_string(),
                    MetadataSource::MusicBrainz,
                )),
                storage_mode: StorageMode::Local,
                pin: false,
                identity_choice: crate::import::IdentityChoice::Unknown,
                user_edit: None,
            },
            expectation,
        )
        .await;

    let database = service.library_manager.database_for_test();
    let (artist_count, artist_image_count) = database
        .handle()
        .sql_read(move |conn| {
            let artist_count = conn.query_row("SELECT COUNT(*) FROM artists", [], |row| {
                row.get::<_, i64>(0)
            })?;
            let artist_image_count =
                conn.query_row("SELECT COUNT(*) FROM artist_images", [], |row| {
                    row.get::<_, i64>(0)
                })?;
            Ok::<_, coven::CovenError>((artist_count, artist_image_count))
        })
        .await
        .unwrap();

    assert_eq!(artist_count, 0);
    assert_eq!(artist_image_count, 0);
}

#[cfg(unix)]
#[tokio::test]
async fn unreadable_selected_cover_is_an_error() {
    use std::os::unix::fs::PermissionsExt;

    let (service, tmp) = setup_import_service().await;
    let cover = tmp.path().join("cover.jpg");
    std::fs::write(&cover, b"jpg bytes").unwrap();
    std::fs::set_permissions(&cover, std::fs::Permissions::from_mode(0o000)).unwrap();
    let discovered = vec![ScannedFile::new(cover.clone(), "cover.jpg".to_string(), 9)];

    let result = service.pick_folder_cover(&discovered, Some("cover.jpg"));

    std::fs::set_permissions(&cover, std::fs::Permissions::from_mode(0o600)).unwrap();
    let err = result.unwrap_err();
    assert!(
        matches!(&err, crate::import::ImportError::CoverArt { detail } if detail.contains("Failed to read cover art")),
        "got: {err}"
    );
}

async fn rescan_seeded_root(
    service: &ImportService,
    root: &Path,
) -> (
    tokio::sync::broadcast::Receiver<crate::import::handle::ImportEvent>,
    Arc<Mutex<crate::import::handle::ImportCandidateState>>,
    Result<(), crate::import::ImportError>,
) {
    let (event_tx, events) = tokio::sync::broadcast::channel(16);
    let folder_registry = Arc::new(Mutex::new(
        crate::import::folder_registry::ImportFolderRegistry::default(),
    ));
    let candidate_state = Arc::new(Mutex::new(
        crate::import::handle::ImportCandidateState::default(),
    ));
    let (fs_tx, _fs_rx) = tokio::sync::mpsc::unbounded_channel();
    let folder_watcher = Arc::new(super::FolderWatcher::new(fs_tx));
    let cancellation = crate::import::folder_scanner::ScanCancellation::new();
    service
        .library_manager
        .add_watched_import_folder(&root.to_string_lossy())
        .await
        .unwrap();
    folder_registry
        .lock()
        .unwrap()
        .apply_added(root.to_string_lossy().into_owned());
    candidate_state
        .lock()
        .unwrap()
        .upsert_invalid(crate::import::InvalidCandidate {
            path: root.join("old-key"),
            name: "Old Candidate".to_string(),
            watched_folder_path: root.to_string_lossy().into_owned(),
            display_path: "old-key".to_string(),
            resolved_boundaries: Vec::new(),
            reason: crate::import::InvalidReason::NoValidAudio,
        });

    let result = ImportService::rescan_and_reconcile(
        root,
        &event_tx,
        &service.library_manager,
        &folder_registry,
        &candidate_state,
        &Arc::new(tokio::sync::Mutex::new(())),
        &folder_watcher,
        &cancellation,
    )
    .await;

    (events, candidate_state, result)
}

#[tokio::test]
async fn rescan_missing_root_fails_and_preserves_previous_candidates() {
    let (service, tmp) = setup_import_service().await;
    let root = tmp.path().join("missing-root");
    let (mut events, candidate_state, result) = rescan_seeded_root(&service, &root).await;
    assert!(result.is_err());

    let failed = loop {
        match events.recv().await.unwrap() {
            crate::import::handle::ImportEvent::Scan(ScanEvent::FolderScanStatusChanged {
                status:
                    crate::import::handle::WatchedFolderScanStatus {
                        status: crate::import::handle::FolderScanStatus::Failed { error },
                        ..
                    },
            }) => break error,
            crate::import::handle::ImportEvent::Scan(ScanEvent::CandidateRemoved {
                candidate_key,
            }) => panic!("missing root removed {candidate_key}"),
            _ => {}
        }
    };
    // The reported failure names the root that could not be read. Its reason is
    // the OS's own wording for an absent path ("No such file or directory" on
    // Unix, "The system cannot find the path specified" on Windows), so the
    // root — the part core promises — is what this asserts on.
    assert!(
        failed.contains(&root.to_string_lossy().into_owned()),
        "{failed}"
    );
    assert_eq!(
        candidate_state
            .lock()
            .unwrap()
            .snapshot(vec![crate::import::WatchedFolder::from_path(
                root.to_string_lossy().into_owned(),
            )])
            .invalid_candidates
            .len(),
        1
    );
}

#[tokio::test]
async fn rescan_non_directory_root_keeps_previous_candidates() {
    let (service, tmp) = setup_import_service().await;
    let root = tmp.path().join("not-a-directory");
    std::fs::write(&root, b"not a directory").unwrap();
    let (mut events, candidate_state, result) = rescan_seeded_root(&service, &root).await;
    assert!(result.is_err(), "a non-directory root must fail its scan");

    loop {
        match events.recv().await.unwrap() {
            crate::import::handle::ImportEvent::Scan(ScanEvent::FolderScanStatusChanged {
                status:
                    crate::import::handle::WatchedFolderScanStatus {
                        status: crate::import::handle::FolderScanStatus::Failed { error },
                        ..
                    },
            }) => {
                assert!(
                    error.to_lowercase().contains("not a directory"),
                    "got: {error}"
                );
                break;
            }
            crate::import::handle::ImportEvent::Scan(ScanEvent::FolderScanStatusChanged {
                ..
            }) => {}
            event => panic!("expected scan status, got {event:?}"),
        }
    }
    assert_eq!(
        candidate_state
            .lock()
            .unwrap()
            .snapshot(vec![crate::import::WatchedFolder::from_path(
                root.to_string_lossy().into_owned(),
            )])
            .invalid_candidates
            .len(),
        1
    );
}

#[test]
fn resolve_file_content_type_uses_probe_for_new_audio_formats() {
    let fixture = |name: &str| {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("test-fixtures")
            .join("audio-format")
            .join(name)
    };
    for (name, expected) in [
        (
            "placeholder-pcm.wav",
            crate::util::content_type::ContentType::Pcm,
        ),
        (
            "placeholder-pcm.aiff",
            crate::util::content_type::ContentType::Pcm,
        ),
        (
            "placeholder-opus.opus",
            crate::util::content_type::ContentType::Opus,
        ),
        (
            "placeholder-vorbis.ogg",
            crate::util::content_type::ContentType::Vorbis,
        ),
        (
            "placeholder-wavpack.wv",
            crate::util::content_type::ContentType::WavPack,
        ),
        (
            "placeholder-dsd.dsf",
            crate::util::content_type::ContentType::Dsd,
        ),
        (
            "placeholder-dsd.dff",
            crate::util::content_type::ContentType::Dsd,
        ),
    ] {
        assert_eq!(
            resolve_file_content_type(&fixture(name)).unwrap(),
            expected,
            "{name}"
        );
    }
}

#[test]
fn import_trace_line_escapes_json_strings() {
    let line = import_trace_line(
        "2024-01-01T00:00:00+00:00".to_string(),
        "import-1",
        "Album \\ Title\nA",
        "Artist \"Name\"",
        Duration::from_millis(42),
        &[("resolve_metadata", Duration::from_millis(7))],
    );

    let parsed: serde_json::Value =
        serde_json::from_str(&line).expect("trace line must be valid JSON");
    assert_eq!(parsed["album"], "Album \\ Title\nA");
    assert_eq!(parsed["artist"], "Artist \"Name\"");
    assert_eq!(parsed["steps"]["resolve_metadata"], 7);
}

/// Deterministic clock for the `apply_user_edit_to_seed` tests — the
/// exact instant is immaterial to what they assert (artist-row
/// preservation / rebuild), only that the same one feeds every row.
fn test_clock() -> FixedClock {
    FixedClock(
        chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc),
    )
}

// ── apply_identity_choice ──────────────────────────────────────────

fn mb_id_exact(group: &str, release: &str) -> crate::import::ReleaseIdentity {
    crate::import::ReleaseIdentity {
        source: crate::import::MetadataSource::MusicBrainz,
        source_group_id: group.to_string(),
        source_release_id: Some(release.to_string()),
    }
}

fn discogs_id_exact(group: &str, release: &str) -> crate::import::ReleaseIdentity {
    crate::import::ReleaseIdentity {
        source: crate::import::MetadataSource::Discogs,
        source_group_id: group.to_string(),
        source_release_id: Some(release.to_string()),
    }
}

fn mb_release_ref() -> crate::import::MetadataRef {
    crate::import::MetadataRef::new("rel-mb", crate::import::MetadataSource::MusicBrainz)
}

/// What each of the three claims writes to `release_identities`, pinned in one
/// place. The header above this projection has been rewritten from a two-state
/// toggle into a claim line; what the three claims mean to the database has
/// not, and this is what stops a later interface change from silently moving it.
///
/// - **Exact** keeps the mapper's rows as they are: `source_release_id` stays
///   `Some`, on the primary row AND on any cross-source row MB↔Discogs url-rels
///   produced.
/// - **Approximate** NULLs `source_release_id` on every one of those rows while
///   keeping their group ids — the claim is at the group level across the board.
/// - **Unknown** passes its input through untouched, which is how it writes no
///   rows: the file-tag mapper it always pairs with emits an empty identity
///   vec. The emptiness lives in the mapper, not in this projection, and the
///   assertion below says so by handing Unknown a *non-empty* vec — asserting
///   only that `[]` stays `[]` would be true of any implementation. Pinning the
///   passthrough is what makes this the test that fires the day the file-tag
///   mapper starts producing identities: an Unknown import would then silently
///   write them, and that has to be a deliberate decision, not a surprise.
#[test]
fn the_three_claims_write_what_they_always_wrote() {
    let mapper_output = vec![
        mb_id_exact("rg-mb", "rel-mb"),
        discogs_id_exact("master-d", "rel-d"),
    ];

    let exact = apply_identity_choice(
        &mapper_output,
        &crate::import::IdentityChoice::Exact {
            release_ref: mb_release_ref(),
        },
    );
    assert_eq!(exact, mapper_output, "Exact keeps every mapper row as-is");

    let approximate = apply_identity_choice(
        &mapper_output,
        &crate::import::IdentityChoice::Approximate {
            release_ref: mb_release_ref(),
        },
    );
    assert_eq!(approximate.len(), 2);
    for id in &approximate {
        assert!(
            id.source_release_id.is_none(),
            "Approximate must NULL source_release_id, got {id:?}"
        );
    }
    assert_eq!(approximate[0].source_group_id, "rg-mb");
    assert_eq!(approximate[1].source_group_id, "master-d");

    assert!(
        apply_identity_choice(&[], &crate::import::IdentityChoice::Unknown).is_empty(),
        "Unknown paired with the file-tag mapper's empty vec writes no rows"
    );
    assert_eq!(
        apply_identity_choice(&mapper_output, &crate::import::IdentityChoice::Unknown),
        mapper_output,
        "Unknown passes its input through — it does not itself enforce emptiness, \
         so a mapper that starts emitting identities would start writing them"
    );
}

// ── apply_user_edit_to_seed ────────────────────────────────────────

fn make_seed_album_release_track() -> (
    crate::db::DbAlbum,
    crate::db::DbRelease,
    crate::db::DbTrack,
    crate::db::DbArtist,
) {
    let now = chrono::Utc::now();
    let artist = crate::db::DbArtist {
        id: "artist-orig".to_string(),
        name: "Artist Name".to_string(),
        sort_name: Some("Artist Name".to_string()),
        discogs_artist_id: None,
        musicbrainz_artist_id: None,
        created_at: now,
    };
    let album = crate::db::DbAlbum {
        id: "9fd7bfa8-3c7c-4026-8559-da66af02f636".to_string(),
        title: "Album Title".to_string(),
        artist_id: artist.id.clone(),
        year: Some(2020),
        primary_release_id: None,
        is_compilation: false,
        created_at: now,
    };
    let release = crate::db::DbRelease {
        id: "release-1".to_string(),
        album_id: album.id.clone(),
        release_name: None,
        pressing: crate::db::Pressing {
            year: Some(2020),
            format: Some("CD".to_string()),
            label: Some("Label Name".to_string()),
            catalog_number: Some("CAT-001".to_string()),
            country: None,
            barcode: None,
        },
        disc_id: None,
        metadata_source: crate::db::ReleaseMetadataSource::MusicBrainz,
        metadata_source_release_id: Some("rel-mb".to_string()),
        remote: true,
        source_folder_name: None,
        content_hash: None,
        album_loudness_lufs: None,
        album_peak_linear: None,
        created_at: now,
    };
    let track = crate::db::DbTrack {
        id: "track-1".to_string(),
        release_id: release.id.clone(),
        title: "Original Title".to_string(),
        side: 1,
        track_number: Some(1),
        duration_ms: Some(180000),
        discogs_position: None,
        created_at: now,
    };
    (album, release, track, artist)
}

#[test]
fn user_edit_overrides_seeded_pressing_fields() {
    let (mut album, mut release, track, seed_artist) = make_seed_album_release_track();
    let mut tracks = vec![track];
    let mut artists = vec![seed_artist];
    let mut album_artists = Vec::new();
    let mut track_artists = Vec::new();

    let edit = crate::import::ReleaseUserEdit {
        album_title: "Edited Title".to_string(),
        album_artist_names: vec!["Edited Artist".to_string()],
        pressing: crate::import::PressingEdit {
            year: Some(1995),
            format: Some("Vinyl".to_string()),
            label: Some("Edited Label".to_string()),
            catalog_number: Some("EDIT-1".to_string()),
            country: Some("JP".to_string()),
            barcode: Some("4943674000000".to_string()),
        },
        tracks: vec![crate::import::TrackUserEdit {
            title: "Edited Track".to_string(),
            side: 1,
            track_number: Some(1),
            artist_names: vec![],
            file: None,
        }],
    };

    apply_user_edit_to_seed(
        &edit,
        &mut album,
        &mut release,
        &mut tracks,
        &mut artists,
        &mut album_artists,
        &mut track_artists,
        &test_clock(),
        &SequentialIdProvider::new("seed"),
    )
    .unwrap();

    assert_eq!(album.title, "Edited Title");
    assert_eq!(release.pressing.year, Some(1995));
    assert_eq!(release.pressing.format.as_deref(), Some("Vinyl"));
    assert_eq!(release.pressing.label.as_deref(), Some("Edited Label"));
    assert_eq!(release.pressing.catalog_number.as_deref(), Some("EDIT-1"));
    assert_eq!(release.pressing.country.as_deref(), Some("JP"));
    assert_eq!(release.pressing.barcode.as_deref(), Some("4943674000000"));
    assert_eq!(tracks[0].title, "Edited Track");

    // The new album artist gets a placeholder DbArtist row so the
    // import pipeline can canonicalize it at DB-write time.
    assert!(artists.iter().any(|a| a.name == "Edited Artist"));
    assert_eq!(
        album.artist_id,
        artists
            .iter()
            .find(|a| a.name == "Edited Artist")
            .unwrap()
            .id
    );
}

#[test]
fn user_edit_can_fill_country_for_approximate_seed() {
    // Approximate seed clears pressing fields; the user can supply
    // them via the editor and the overlay applies the value.
    let (mut album, mut release, track, seed_artist) = make_seed_album_release_track();
    // Simulate the Approximate-cleared release row.
    release.pressing = crate::db::Pressing::blank();
    let mut tracks = vec![track];
    let mut artists = vec![seed_artist];
    let mut album_artists = Vec::new();
    let mut track_artists = Vec::new();

    let edit = crate::import::ReleaseUserEdit {
        album_title: album.title.clone(),
        album_artist_names: vec![artists[0].name.clone()],
        pressing: crate::import::PressingEdit {
            country: Some("JP".to_string()),
            ..crate::import::PressingEdit::blank()
        },
        tracks: vec![crate::import::TrackUserEdit {
            title: tracks[0].title.clone(),
            side: tracks[0].side,
            track_number: tracks[0].track_number,
            artist_names: vec![],
            file: None,
        }],
    };

    apply_user_edit_to_seed(
        &edit,
        &mut album,
        &mut release,
        &mut tracks,
        &mut artists,
        &mut album_artists,
        &mut track_artists,
        &test_clock(),
        &SequentialIdProvider::new("seed"),
    )
    .unwrap();

    assert_eq!(release.pressing.country.as_deref(), Some("JP"));
    assert!(release.pressing.year.is_none());
    assert!(release.pressing.format.is_none());
}

#[test]
fn user_edit_track_count_mismatch_is_an_error() {
    let (mut album, mut release, track, seed_artist) = make_seed_album_release_track();
    let mut tracks = vec![track];
    let mut artists = vec![seed_artist];
    let mut album_artists = Vec::new();
    let mut track_artists = Vec::new();

    let edit = crate::import::ReleaseUserEdit {
        album_title: "T".to_string(),
        album_artist_names: vec!["A".to_string()],
        pressing: crate::import::PressingEdit::blank(),
        // Two edits but seed has one track.
        tracks: vec![
            crate::import::TrackUserEdit {
                title: "X".to_string(),
                side: 1,
                track_number: Some(1),
                artist_names: vec![],
                file: None,
            },
            crate::import::TrackUserEdit {
                title: "Y".to_string(),
                side: 1,
                track_number: Some(2),
                artist_names: vec![],
                file: None,
            },
        ],
    };

    let err = apply_user_edit_to_seed(
        &edit,
        &mut album,
        &mut release,
        &mut tracks,
        &mut artists,
        &mut album_artists,
        &mut track_artists,
        &test_clock(),
        &SequentialIdProvider::new("seed"),
    )
    .unwrap_err();
    assert!(
        matches!(&err, crate::import::ImportError::Internal { detail } if detail.contains("Track count mismatch")),
        "got: {err}"
    );
}

/// Source-id linkage on artist rows (e.g. `musicbrainz_artist_id`)
/// must survive a user edit that doesn't touch artist names. The
/// editor round-trips an unchanged artist field as the same string
/// it was seeded with, so the apply step must compare and short-
/// circuit rather than rebuild rows from name-only placeholders.
#[test]
fn user_edit_preserves_source_id_artist_rows_when_names_unchanged() {
    let now = chrono::Utc::now();
    // Seeded artist row carrying the MB id the mapper attached.
    let seed_artist = crate::db::DbArtist {
        id: "artist-mb".to_string(),
        name: "Artist Name".to_string(),
        sort_name: Some("Artist Name".to_string()),
        discogs_artist_id: None,
        musicbrainz_artist_id: Some("mb-artist-1".to_string()),
        created_at: now,
    };
    let album = crate::db::DbAlbum {
        id: "9fd7bfa8-3c7c-4026-8559-da66af02f636".to_string(),
        title: "Album Title".to_string(),
        artist_id: seed_artist.id.clone(),
        year: Some(2020),
        primary_release_id: None,
        is_compilation: false,
        created_at: now,
    };
    let release = crate::db::DbRelease {
        id: "release-1".to_string(),
        album_id: album.id.clone(),
        release_name: None,
        pressing: crate::db::Pressing {
            year: Some(2020),
            format: None,
            label: None,
            catalog_number: None,
            country: None,
            barcode: None,
        },
        disc_id: None,
        metadata_source: crate::db::ReleaseMetadataSource::MusicBrainz,
        metadata_source_release_id: Some("rel-mb".to_string()),
        remote: true,
        source_folder_name: None,
        content_hash: None,
        album_loudness_lufs: None,
        album_peak_linear: None,
        created_at: now,
    };
    let track = crate::db::DbTrack {
        id: "track-1".to_string(),
        release_id: release.id.clone(),
        title: "Track Title".to_string(),
        side: 1,
        track_number: Some(1),
        duration_ms: None,
        discogs_position: None,
        created_at: now,
    };
    // Seeded track credit pointing at the MB-id-bearing artist.
    let seed_track_artist = crate::db::DbTrackArtist::new(
        &track.id,
        &seed_artist.id,
        0,
        "track-artist-1".to_string(),
        now,
    );

    let mut album = album;
    let mut release = release;
    let mut tracks = vec![track];
    let mut artists = vec![seed_artist.clone()];
    let mut album_artists = Vec::<crate::db::DbAlbumArtist>::new();
    let mut track_artists = vec![seed_track_artist.clone()];

    // The user changes pressing fields but leaves artist names
    // alone. The track's edit ships `artist_names = []` because
    // the editor's "no override" form maps to empty when the
    // track's credit equals the album's.
    let edit = crate::import::ReleaseUserEdit {
        album_title: album.title.clone(),
        album_artist_names: vec![seed_artist.name.clone()],
        pressing: crate::import::PressingEdit {
            year: Some(1995),
            ..crate::import::PressingEdit::blank()
        },
        tracks: vec![crate::import::TrackUserEdit {
            title: tracks[0].title.clone(),
            side: tracks[0].side,
            track_number: tracks[0].track_number,
            artist_names: vec![],
            file: None,
        }],
    };

    apply_user_edit_to_seed(
        &edit,
        &mut album,
        &mut release,
        &mut tracks,
        &mut artists,
        &mut album_artists,
        &mut track_artists,
        &test_clock(),
        &SequentialIdProvider::new("seed"),
    )
    .unwrap();

    // The MB-id-bearing artist row must still exist with its
    // source binding intact — no fresh placeholder created.
    assert_eq!(artists.len(), 1, "no extra placeholder rows expected");
    assert_eq!(
        artists[0].musicbrainz_artist_id.as_deref(),
        Some("mb-artist-1"),
        "MB artist id must survive the edit",
    );
    assert_eq!(
        album.artist_id, seed_artist.id,
        "album.artist_id should still reference the seeded row",
    );

    // Track credit must still reference the seeded artist row.
    assert_eq!(track_artists.len(), 1);
    assert_eq!(track_artists[0].artist_id, seed_artist.id);
}

/// User-renaming an artist must rebuild the credit rows. The new
/// name has no source binding, so the inserted `DbArtist` row
/// carries `None` for both source ids.
#[test]
fn user_edit_renaming_album_artist_rebuilds_credits() {
    let (mut album, mut release, track, seed_artist) = make_seed_album_release_track();
    let mut tracks = vec![track];
    let mut artists = vec![seed_artist.clone()];
    let mut album_artists = Vec::new();
    let mut track_artists = Vec::new();

    let edit = crate::import::ReleaseUserEdit {
        album_title: album.title.clone(),
        album_artist_names: vec!["Different Artist".to_string()],
        pressing: crate::import::PressingEdit::blank(),
        tracks: vec![crate::import::TrackUserEdit {
            title: tracks[0].title.clone(),
            side: tracks[0].side,
            track_number: tracks[0].track_number,
            artist_names: vec![],
            file: None,
        }],
    };

    apply_user_edit_to_seed(
        &edit,
        &mut album,
        &mut release,
        &mut tracks,
        &mut artists,
        &mut album_artists,
        &mut track_artists,
        &test_clock(),
        &SequentialIdProvider::new("seed"),
    )
    .unwrap();

    let new_artist = artists
        .iter()
        .find(|a| a.name == "Different Artist")
        .expect("new placeholder should be inserted");
    assert!(new_artist.musicbrainz_artist_id.is_none());
    assert!(new_artist.discogs_artist_id.is_none());
    assert_eq!(album.artist_id, new_artist.id);
}

// ── build_audio_formats: CUE track byte windows ────────────────────

/// Build the `TrackFile::CueBacked` list for a single-file CUE album, reusing
/// the import pipeline's own analysis (`analyze_cue_audio`) so the container is
/// probed exactly as a real import probes it.
fn cue_backed_tracks(dir: &str) -> Vec<TrackFile> {
    let audio_path = PathBuf::from(format!("{dir}/Test Album.ape"));
    let cue_path = PathBuf::from(format!("{dir}/Test Album.cue"));
    let cue_sheet = crate::cue_flac::parse_cue_sheet(&cue_path).expect("parse cue");
    let probe = crate::import::track_slots::analyze_cue_audio(&audio_path).expect("analyze ape");
    let cue_pair = Arc::new(crate::import::types::CueFlacAnalysis {
        cue_sheet,
        audio_files: vec![crate::import::types::CueAnalyzedAudioFile {
            file_reference: "Test Album.ape".to_string(),
            path: audio_path.clone(),
            probe,
        }],
    });
    (0..cue_pair.cue_sheet.tracks.len())
        .map(|index| TrackFile::CueBacked {
            db_track: DbTrack {
                id: format!("track-{index}"),
                release_id: "rel".to_string(),
                title: format!("Track {index}"),
                side: 1,
                track_number: Some(index as i32 + 1),
                duration_ms: None,
                discogs_position: None,
                created_at: test_clock().0,
            },
            file_path: audio_path.clone(),
            cue_pair: Arc::clone(&cue_pair),
            cue_index: index,
        })
        .collect()
}

/// A CUE track's read-ahead ceiling is its `end_byte`; playback fills up to it
/// and stops, so every non-last track must carry a real end byte or the fill
/// streams the whole rest of the shared file. Ends derive from the next track's
/// start byte (`start[N+1]`), computed via `seek_landing_bytes` -- the AVIO
/// landing, defined for every format, including APE whose packets carry no byte
/// position. This drives `build_audio_formats` over the APE CUE fixture and
/// asserts the user-visible outcome: the two non-last tracks get `Some`,
/// ascending, in-file end bytes and the last track runs to EOF (`None`).
#[test]
fn build_audio_formats_gives_ape_cue_tracks_real_end_bytes() {
    crate::audio_codec::init();
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/cue_ape");
    let tracks = cue_backed_tracks(dir);
    assert_eq!(
        tracks.len(),
        3,
        "fixture is a 3-track single-file CUE album"
    );

    let file_size = std::fs::metadata(format!("{dir}/Test Album.ape"))
        .unwrap()
        .len() as i64;
    let mut file_ids = HashMap::new();
    file_ids.insert(
        PathBuf::from(format!("{dir}/Test Album.ape")),
        "file-1".to_string(),
    );

    let built = ImportService::build_audio_formats(
        &tracks,
        &file_ids,
        &test_clock(),
        &SequentialIdProvider::new("seed"),
    )
    .expect("build_audio_formats");

    let main_segments: Vec<_> = built
        .audio_segments
        .iter()
        .filter(|segment| segment.role == crate::db::DbAudioSegmentRole::Main)
        .collect();
    let ends: Vec<Option<i64>> = main_segments
        .iter()
        .map(|segment| segment.end_byte)
        .collect();
    // Non-last tracks carry a real, ascending, in-file end byte.
    let e0 = ends[0].expect("track 1 (non-last) must have an end byte");
    let e1 = ends[1].expect("track 2 (non-last) must have an end byte");
    assert!(
        e0 > 0 && e0 < file_size,
        "track 1 end within file: {e0} of {file_size}"
    );
    assert!(
        e1 > 0 && e1 < file_size,
        "track 2 end within file: {e1} of {file_size}"
    );
    assert!(e1 > e0, "end bytes ascend track to track: {ends:?}");
    // The last track runs to EOF.
    assert_eq!(ends[2], None, "the last track runs to EOF");

    // Each track's end is the next track's start byte -- one boundary, not two.
    assert_eq!(
        main_segments[1].start_byte,
        Some(e0),
        "track 2 starts where track 1 ends"
    );
    assert_eq!(
        main_segments[2].start_byte,
        Some(e1),
        "track 3 starts where track 2 ends"
    );
}
