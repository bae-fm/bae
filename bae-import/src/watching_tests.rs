//! The fixtures the watching tests share: a coordinator over a test library
//! whose passes and removals a test drives, and the commands and reports it
//! takes.

use super::*;
use crate::folder_scanner::ScanItem;
use crate::service::tests::{setup_import_service, TestService};
use crate::tables::watched_path::host_root;
use bae_store::Store;
use bae_util::test_channels::HANG_GUARD;
use std::sync::Mutex;
use tempfile::TempDir;

/// A `/`-spelled watched root in the host's spelling, since a watched root must
/// be absolute by the OS's own rule.
pub(crate) fn root_path(posix: &str) -> PathBuf {
    host_root(posix)
}

#[derive(Clone)]
pub(crate) struct FakeScanStarter {
    pub(crate) scans: Arc<Mutex<Vec<FakeStartedScan>>>,
    pub(crate) started: Arc<tokio::sync::Notify>,
    /// Taken by every start before anything else: while a test holds it, a
    /// coordinator starting a pass waits there, taking nothing else.
    pub(crate) start_gate: Arc<Mutex<()>>,
}

impl FakeScanStarter {
    pub(crate) fn new() -> (Self, RootScanStarter) {
        let fake = Self {
            scans: Arc::new(Mutex::new(Vec::new())),
            started: Arc::new(tokio::sync::Notify::new()),
            start_gate: Arc::new(Mutex::new(())),
        };
        let captured = fake.clone();
        let starter: RootScanStarter = Arc::new(move |id, path, pass, completion| {
            drop(captured.start_gate.lock().unwrap());
            let cancellation = crate::folder_scanner::ScanCancellation::new();
            let (finish, finished) = tokio::sync::oneshot::channel();
            let started_path = path.clone();
            // The pass ends when it is finished, or when its finish is
            // dropped unsent: it ended without telling the coordinator.
            let task = bae_diagnostics::Diagnostics::noop().spawn_reported(
                &tokio::runtime::Handle::current(),
                "a fake pass",
                async move {
                    #[expect(
                        clippy::let_underscore_must_use,
                        reason = "a finish dropped unsent ends the pass as one sent does"
                    )]
                    let _ = finished.await;
                },
                |_| {},
            );
            let (reading, folders) = match pass {
                RootPass::WholeRoot => (None, None),
                RootPass::Decision(request) => (Some(request), None),
                RootPass::Folders(folders) => (None, Some(folders)),
            };
            captured.scans.lock().unwrap().push(FakeStartedScan {
                path: started_path,
                reading,
                folders,
                cancellation: cancellation.clone(),
                completion: Some((finish, completion, RootScanCompletion { id, path })),
            });
            captured.started.notify_one();
            RootScanTask::new(cancellation, task)
        });
        (fake, starter)
    }

    /// Hold every pass start until the guard drops.
    pub(crate) fn hold_starts(&self) -> std::sync::MutexGuard<'_, ()> {
        self.start_gate.lock().unwrap()
    }

    pub(crate) async fn wait_for_count(&self, count: usize) {
        while self.scans.lock().unwrap().len() < count {
            tokio::time::timeout(HANG_GUARD, self.started.notified())
                .await
                .expect("scan coordinator did not start the expected scan");
        }
    }

    /// End the scan at `index`, telling a folder decision's caller it was
    /// stored.
    ///
    /// The pass's end is queued to the coordinator before this returns, but
    /// from this thread, so it is not ordered against commands or watch
    /// reports sent before or after it: the coordinator checks its channels
    /// one after another, and the end can arrive after it found the others
    /// empty. A test that needs its commands taken first waits for them with
    /// [`CoordinatorHarness::commands_handled`].
    pub(crate) fn complete(&self, index: usize) {
        if let Some(reading) = self.scans.lock().unwrap()[index].reading.take() {
            reading.answer(Ok(Vec::new()));
        }
        let (finish, completions, completion) = self.scans.lock().unwrap()[index]
            .completion
            .take()
            .expect("fake scan has not completed");
        finish.send(()).expect("the fake scan's task is still running");
        completions.send(completion).expect("coordinator still receives completions");
    }

    /// The folder decision the pass at `index` stores, if it is one.
    pub(crate) fn reading(
        &self,
        index: usize,
    ) -> Option<(crate::FolderReleaseDecisionKey, crate::FolderReleaseDecision)> {
        self.scans.lock().unwrap()[index].reading.as_ref().map(|reading| reading.target().clone())
    }

    /// The changed folders the pass at `index` reads again, if it is one.
    pub(crate) fn folders(&self, index: usize) -> Option<Vec<std::ffi::OsString>> {
        self.scans.lock().unwrap()[index]
            .folders
            .as_ref()
            .map(|folders| folders.iter().cloned().collect())
    }

    pub(crate) fn path(&self, index: usize) -> PathBuf {
        self.scans.lock().unwrap()[index].path.clone()
    }

    pub(crate) fn cancellation(&self, index: usize) -> crate::folder_scanner::ScanCancellation {
        self.scans.lock().unwrap()[index].cancellation.clone()
    }

    /// End the pass at `index` without telling the coordinator it ended.
    pub(crate) fn abort(&self, index: usize) {
        drop(
            self.scans.lock().unwrap()[index]
                .completion
                .take()
                .expect("fake scan has not completed"),
        );
    }

    pub(crate) async fn wait_for_cancellation(&self, index: usize) {
        tokio::time::timeout(HANG_GUARD, self.cancellation(index).cancelled())
            .await
            .expect("scan coordinator did not cancel the active scan");
    }
}

pub(crate) struct FakeStartedScan {
    pub(crate) path: PathBuf,
    /// The folder decision this pass stores, with the caller it answers.
    pub(crate) reading: Option<FolderReadingRequest>,
    /// The folders this pass reads again because they changed on disk.
    pub(crate) folders: Option<std::collections::BTreeSet<std::ffi::OsString>>,
    pub(crate) cancellation: crate::folder_scanner::ScanCancellation,
    /// Ends the pass's task, then tells the coordinator the pass is over.
    pub(crate) completion: Option<(
        tokio::sync::oneshot::Sender<()>,
        tokio::sync::mpsc::UnboundedSender<RootScanCompletion>,
        RootScanCompletion,
    )>,
}

#[derive(Default)]
pub(crate) struct FakeRemovalBackend {
    pub(crate) uninstall_error: Mutex<Option<String>>,
    pub(crate) remove_error: Mutex<Option<String>>,
    pub(crate) reinstall_error: Mutex<Option<String>>,
    pub(crate) block_reinstall: std::sync::atomic::AtomicBool,
    pub(crate) reinstall_started: tokio::sync::Notify,
    pub(crate) release_reinstall: tokio::sync::Notify,
    pub(crate) calls: Mutex<Vec<&'static str>>,
    /// A store the removal leaves unreadable as a list of watched folders,
    /// as a read that fails after the removal's write would find it.
    pub(crate) unreadable_after_remove: Mutex<Option<Store>>,
    /// The folders the removal's write leaves watched.
    pub(crate) leaves_watched: Mutex<Vec<PathBuf>>,
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
        if self.block_reinstall.load(std::sync::atomic::Ordering::SeqCst) {
            self.reinstall_started.notify_one();
            self.release_reinstall.notified().await;
        }
        match self.reinstall_error.lock().unwrap().clone() {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    async fn remove_durable_roots(
        &self,
        _roots: &[PathBuf],
        _parent: Option<&Path>,
    ) -> Result<crate::tables::WatchedFoldersRemoved, String> {
        self.calls.lock().unwrap().push("remove");
        let unreadable = self.unreadable_after_remove.lock().unwrap().take();
        if let Some(db) = unreadable {
            make_watched_folders_unreadable(&db).await;
        }
        match self.remove_error.lock().unwrap().clone() {
            Some(error) => Err(error),
            None => Ok(crate::tables::WatchedFoldersRemoved {
                removed_keys: Vec::new(),
                watched: self
                    .leaves_watched
                    .lock()
                    .unwrap()
                    .iter()
                    .cloned()
                    .map(crate::WatchedFolder::from_path)
                    .collect(),
            }),
        }
    }
}

/// Store a watched folder inside another, which every read of the watched
/// folders refuses.
async fn make_watched_folders_unreadable(db: &Store) {
    let inner = coven::path_bytes(&root_path("/music/inner"));
    db.call(move |sql| {
        sql.execute("INSERT INTO watched_import_folders (path, position) VALUES (?, 99)", [inner])?;
        Ok(())
    })
    .await
    .unwrap();
}

pub(crate) struct CoordinatorHarness {
    pub(crate) commands: tokio::sync::mpsc::UnboundedSender<WatcherCommand>,
    pub(crate) fs_events: tokio::sync::mpsc::UnboundedSender<WatchReport>,
    pub(crate) scans: FakeScanStarter,
    /// The store the coordinator reads which folders are watched from.
    pub(crate) db: Store,
    /// The bugs the coordinator reported.
    pub(crate) bugs: Arc<bae_diagnostics::RecordingBugReporter>,
    pub(crate) folder_state_commit: crate::FolderStateCommit,
    pub(crate) removal_backend: Arc<FakeRemovalBackend>,
    /// Every event the coordinator sent.
    pub(crate) events: tokio::sync::Mutex<crate::ImportEvents>,
    pub(crate) coordinator_thread: Mutex<Option<std::thread::JoinHandle<()>>>,
    pub(crate) _temp: TempDir,
}

impl CoordinatorHarness {
    pub(crate) async fn new() -> Self {
        Self::with_roots(&["/music"]).await
    }

    /// `roots` are `/`-spelled; each is registered in the host's spelling, as
    /// [`root_path`] gives it.
    pub(crate) async fn with_roots(roots: &[&str]) -> Self {
        let roots: Vec<PathBuf> = roots.iter().map(|root| host_root(root)).collect();
        let mut test = setup_import_service().await;
        let bugs = Arc::new(bae_diagnostics::RecordingBugReporter::default());
        test.report_bugs_to(bugs.clone());
        let tables = test.tables().clone();
        for root in &roots {
            tables.add_watched_import_folder(std::path::Path::new(root)).await.unwrap();
        }
        let (commands, command_rx) = tokio::sync::mpsc::unbounded_channel();
        let (fs_events, fs_rx) = tokio::sync::mpsc::unbounded_channel();
        if let Some(root) = roots.first() {
            let root_path = PathBuf::from(root);
            let generation = tables
                .begin_folder_scan(std::path::Path::new(root), crate::VolumeKind::Local)
                .await
                .unwrap();
            crate::store_scan_item(
                &tables,
                &bae_diagnostics::Diagnostics::noop(),
                std::path::Path::new(root),
                generation,
                &ScanItem::Invalid(crate::InvalidCandidate {
                    path: root_path.join("Group/Release"),
                    name: "Release".to_string(),
                    watched_folder_path: std::path::PathBuf::from(&root),
                    relative_folder: crate::RelativeFolder::test("Group/Release"),
                    grouping: None,
                    files: Vec::new(),
                    tags: None,
                }),
            )
            .await
            .unwrap()
            .expect("the seeded scan generation is current");
        }
        let folder_state_commit = crate::FolderStateCommit::default();
        let (scans, starter) = FakeScanStarter::new();
        let removal_backend = Arc::new(FakeRemovalBackend::default());
        let events = tokio::sync::Mutex::new(test.event_tx().every_event());
        let coordinator_thread = coordinator::start_watcher_with_starter(
            command_rx,
            fs_rx,
            test.import_services(
                test.event_tx().clone(),
                Arc::new(crate::file_tag_reader::LoftyFileTagReader),
                Arc::new(crate::folder_scanner::OsDirectoryReader),
                folder_state_commit.clone(),
            ),
            starter,
            removal_backend.clone(),
        );
        let TestService { db, temp, .. } = test;
        Self {
            commands,
            fs_events,
            scans,
            db,
            bugs,
            folder_state_commit,
            removal_backend,
            events,
            coordinator_thread: Mutex::new(Some(coordinator_thread)),
            _temp: temp,
        }
    }

    /// Store a watched folder inside `/music`, which the store's own add
    /// refuses, so that reading the watched folders fails from here on.
    pub(crate) async fn corrupt_watched_folders(&self) {
        let inner = root_path("/music").join("artist");
        self.db
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO watched_import_folders (path, position) VALUES (?, 99)",
                    rusqlite::params![coven::path_bytes(&inner)],
                )?;
                Ok(())
            })
            .await
            .unwrap();
    }

    /// Hand the coordinator `report` as its watch would, and wait until it
    /// has asked for the reads the report calls for.
    pub(crate) async fn watch_report(&self, report: WatchReport) {
        let (taken, wait) = tokio::sync::oneshot::channel();
        self.commands.send(WatcherCommand::WatchReport { report, taken }).unwrap();
        tokio::time::timeout(HANG_GUARD, wait)
            .await
            .expect("the coordinator takes the watch report")
            .expect("the coordinator is running");
    }

    /// Wait until the coordinator has handled every command sent before this
    /// one: commands are taken in order, and a report of no changes asks for
    /// nothing.
    pub(crate) async fn commands_handled(&self) {
        self.watch_report(Ok(Vec::new())).await;
    }

    pub(crate) async fn shutdown(&self) {
        let (completion, done) = std::sync::mpsc::channel();
        self.commands.send(WatcherCommand::Shutdown { completion }).unwrap();
        tokio::task::spawn_blocking(move || done.recv()).await.unwrap().unwrap();
        let thread = self
            .coordinator_thread
            .lock()
            .unwrap()
            .take()
            .expect("coordinator has not already shut down");
        tokio::task::spawn_blocking(move || thread.join()).await.unwrap().unwrap();
    }
}

/// Rescan `root`, wait for its scan to start, and return the root in the
/// host's spelling.
pub(crate) async fn rescan_and_wait(harness: &CoordinatorHarness, root: &str) -> PathBuf {
    let root = root_path(root);
    harness.commands.send(WatcherCommand::Rescan(root.clone())).unwrap();
    harness.scans.wait_for_count(1).await;
    root
}

/// Ask to remove `root` while it is being scanned, and wait for the coordinator
/// to cancel that scan. The caller ends the scan.
pub(crate) async fn removal_awaiting_its_cancelled_scan(
    harness: &CoordinatorHarness,
    root: &str,
) -> tokio::sync::oneshot::Receiver<Result<(), String>> {
    let root = rescan_and_wait(harness, root).await;
    let (completion, result) = tokio::sync::oneshot::channel();
    harness
        .commands
        .send(WatcherCommand::Remove { roots: vec![root], parent: None, completion })
        .unwrap();
    harness.scans.wait_for_cancellation(0).await;
    result
}

pub(crate) fn request_refresh(
    harness: &CoordinatorHarness,
    root: &str,
) -> tokio::sync::oneshot::Receiver<Result<(), String>> {
    let (completion, result) = tokio::sync::oneshot::channel();
    harness.commands.send(WatcherCommand::Refresh { path: root_path(root), completion }).unwrap();
    result
}

/// Ask for `Group` under `root` to become one release.
pub(crate) fn request_group_decision(
    harness: &CoordinatorHarness,
    root: &Path,
) -> tokio::sync::oneshot::Receiver<Result<Vec<crate::CandidateKey>, String>> {
    let (completion, result) = tokio::sync::oneshot::channel();
    harness
        .commands
        .send(WatcherCommand::SetFolderReleaseDecision {
            target: (
                crate::FolderReleaseDecisionKey {
                    watched_folder_path: root.to_path_buf(),
                    relative_folder: crate::RelativeFolder::test("Group"),
                },
                crate::FolderReleaseDecision::CombineAsOneRelease,
            ),
            completion,
        })
        .unwrap();
    result
}

/// Report a file created in a folder of `/music` nothing else touches, and
/// assert that the pass it starts, the `index`th, is the root's next one and
/// reads that folder alone. The coordinator may take this report before or
/// after the completed pass's end; either way, a pass that end still owed
/// starts as the `index`th, covering the change or leaving it queued behind.
pub(crate) async fn assert_next_pass_reads_only_a_later_change(
    harness: &CoordinatorHarness,
    index: usize,
) {
    harness
        .fs_events
        .send(Ok(vec![watch_event(
            notify::EventKind::Create(notify::event::CreateKind::File),
            root_path("/music").join("Later").join("01.flac"),
        )]))
        .unwrap();
    harness.scans.wait_for_count(index + 1).await;
    assert_eq!(
        harness.scans.folders(index),
        Some(vec!["Later".into()]),
        "a pass was owed before the later change"
    );
    harness.scans.complete(index);
}

/// Whether `result` has been answered yet.
pub(crate) fn is_answered<T>(
    result: &mut tokio::sync::oneshot::Receiver<Result<T, String>>,
) -> bool {
    !matches!(result.try_recv(), Err(tokio::sync::oneshot::error::TryRecvError::Empty))
}

pub(crate) fn watch_event(kind: notify::EventKind, path: PathBuf) -> notify::Event {
    notify::Event::new(kind).add_path(path)
}
