use super::*;
use crate::config::{Config, ConfigHandle};
use crate::db::Database;
use crate::import::folder_registry::host_root;
use crate::import::MetadataSource;
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
        Arc::new(coven::SystemClock),
        Arc::new(coven::UuidProvider),
        crate::diagnostics::Diagnostics::noop(),
        tokio::runtime::Handle::current(),
        crate::import::cover_art::RemoteImageCache::for_test(),
    );
    let (_commands_tx, commands_rx) = tokio::sync::mpsc::unbounded_channel();
    let (event_tx, _) = tokio::sync::broadcast::channel(16);
    (
        ImportService {
            commands_rx,
            event_tx,
            library_manager: manager,
            clock: Arc::new(coven::SystemClock),
            ids: Arc::new(coven::UuidProvider),
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

    async fn remove_durable_root(&self, _path: &Path) -> Result<Vec<String>, String> {
        self.calls.lock().unwrap().push("remove");
        match self.remove_error.lock().unwrap().clone() {
            Some(error) => Err(error),
            None => Ok(Vec::new()),
        }
    }
}

struct CoordinatorHarness {
    commands: tokio::sync::mpsc::UnboundedSender<WatcherCommand>,
    fs_events: tokio::sync::mpsc::UnboundedSender<DebounceEventResult>,
    scans: FakeScanStarter,
    folder_registry: Arc<Mutex<ImportFolderRegistry>>,
    library_manager: LibraryManager,
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
        if let Some(root) = roots.first() {
            let root_path = PathBuf::from(root);
            let generation = service
                .library_manager
                .begin_folder_scan(root)
                .await
                .unwrap();
            service
                .library_manager
                .save_folder_scan_item(
                    root,
                    generation,
                    &ScanItem::Invalid(crate::import::InvalidCandidate {
                        path: root_path.join("Group/Release"),
                        name: "Release".to_string(),
                        watched_folder_path: root.clone(),
                        display_path: "Group/Release".to_string(),
                        resolved_boundaries: Vec::new(),
                        reason: crate::import::InvalidReason::NoValidAudio,
                    }),
                )
                .await
                .unwrap()
                .expect("the seeded scan generation is current");
        }
        let folder_state_commit = Arc::new(tokio::sync::Mutex::new(()));
        let (scans, starter) = FakeScanStarter::new();
        let removal_backend = Arc::new(FakeRemovalBackend::default());
        let coordinator_thread = ImportService::start_watcher_with_starter(
            command_rx,
            fs_rx,
            service.event_tx,
            service.library_manager.clone(),
            registry.clone(),
            folder_state_commit.clone(),
            starter,
            removal_backend.clone(),
        );
        Self {
            commands,
            fs_events,
            scans,
            folder_registry: registry,
            library_manager: service.library_manager,
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

include!("tests/coordinator.rs");
include!("tests/cover_and_rescan.rs");
include!("tests/edits_and_formats.rs");
