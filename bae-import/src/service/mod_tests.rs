//! The fixtures the service's tests share: a service over a test library, a
//! scan over it, and a coordinator whose passes and removals a test drives;
//! and a pass's start.

use super::*;
use bae_db::watched_path::host_root;
use bae_db::Database;
use bae_util::test_channels::HANG_GUARD;
use std::sync::Mutex;
use tempfile::TempDir;

/// A `/`-spelled watched root in the host's spelling, since a watched root must
/// be absolute by the OS's own rule.
pub(super) fn root_path(posix: &str) -> PathBuf {
    host_root(posix)
}

/// A worker for a test, with the candidate-preparations writer over the same
/// database.
pub(super) struct TestService {
    pub(super) service: ImportService,
    pub(super) preparations: crate::CandidatePreparations,
    /// The library the service imports into, for a test that reads or seeds
    /// rows no call reads or writes on its own.
    pub(super) database: Database,
    pub(super) temp: TempDir,
}

impl TestService {
    /// A scan over this service's library, and the stream it announces on.
    pub(super) fn scan(
        &self,
    ) -> (TestScan, tokio::sync::mpsc::UnboundedReceiver<crate::handle::ImportEvent>) {
        self.scan_with(
            Arc::new(crate::file_tag_snapshot::LoftyFileTagReader),
            Arc::new(crate::folder_scanner::OsDirectoryReader),
        )
    }

    /// The same scan, reading tags through `file_tags` and listings through
    /// `directories`.
    pub(super) fn scan_with(
        &self,
        file_tags: Arc<dyn crate::file_tag_snapshot::FileTagReader>,
        directories: Arc<dyn crate::folder_scanner::DirectoryReader>,
    ) -> (TestScan, tokio::sync::mpsc::UnboundedReceiver<crate::handle::ImportEvent>) {
        let event_tx = crate::ImportEventBus::new(self.service.runtime.clone());
        let events = event_tx.every_event();
        let (fs_tx, fs_rx) = tokio::sync::mpsc::unbounded_channel();
        let scan = TestScan {
            services: test_scan_services(
                &self.service,
                &self.database,
                &self.preparations,
                event_tx,
                Arc::new(FolderWatcher::new(fs_tx, bae_diagnostics::Diagnostics::noop())),
                file_tags,
                directories,
            ),
            cancellation: crate::folder_scanner::ScanCancellation::new(),
            _fs_rx: fs_rx,
        };
        (scan, events)
    }
}

pub(super) async fn setup_import_service() -> TestService {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let database =
        Database::new_test(db_path.to_str().unwrap(), Arc::new(coven::SystemClock)).await.unwrap();

    let preparations = crate::CandidatePreparations::new(database.clone());
    // The runtime writes its standings into the library the import commits
    // into.
    let tables = crate::tables::ImportTables::new(database.clone());
    let runtime =
        crate::CandidateRuntime::new(tables.clone(), bae_diagnostics::Diagnostics::noop());
    let catalogs = crate::CatalogSessions::new(
        bae_catalogs::providers::Providers::offline(),
        database.clone(),
        bae_diagnostics::Diagnostics::noop(),
        tokio::runtime::Handle::current(),
    );
    let (_commands_tx, commands_rx) = tokio::sync::mpsc::unbounded_channel();
    let event_tx = crate::ImportEventBus::new(runtime.clone());
    TestService {
        service: ImportService {
            commands_rx,
            event_tx,
            runtime,
            database: database.clone(),
            tables,
            catalogs,
            remote_images: crate::RemoteImages::new(
                bae_catalogs::remote_image::RemoteImageCache::for_test(
                    bae_util::http::Http::for_test(),
                ),
                bae_diagnostics::Diagnostics::noop(),
            ),
            clock: Arc::new(coven::SystemClock),
            ids: Arc::new(coven::UuidIds),
            import_cancels: Default::default(),
            diagnostics: bae_diagnostics::Diagnostics::noop(),
        },
        preparations,
        database,
        temp: temp_dir,
    }
}

impl ImportService {
    /// Queue `import_id` of the candidate at `key` as asking for an import
    /// does — its claim on the candidate and its cancel — for a test that runs
    /// the worker's steps itself.
    pub(super) async fn queue_import_for_test(&self, key: &crate::CandidateKey, import_id: &str) {
        self.runtime
            .claim_for_import(key, import_id, false)
            .await
            .expect("a test queues an import of a candidate no import owns");
        self.import_cancels.register(key, import_id);
    }
}

pub(super) async fn prepare_named_candidate(
    service: &ImportService,
    preparations: &crate::CandidatePreparations,
    content_hash: &str,
    watched_folder_path: &std::path::Path,
    candidate_path: &crate::CandidateKey,
    album_title: &str,
) -> u64 {
    let preparation = crate::preparation::import_preparation(&service.tables, content_hash)
        .await
        .unwrap()
        .expect("the scanned candidate has its initial preparation");
    let mut draft = preparation.draft;
    draft.album_title = album_title.to_string();
    draft.album_artist_assignments =
        vec![crate::Credited::plain(crate::ArtistAssignment::named("Artist Name"))];
    preparations
        .apply_source(
            watched_folder_path,
            &crate::CandidateAsRead {
                content_hash: content_hash.to_string(),
                file_edit_revision: preparation.file_edit_revision,
                metadata_revision: preparation.metadata_revision,
            },
            candidate_path,
            &crate::CandidateMetadataDraft {
                draft,
                provenance: preparation.metadata_provenance,
                cover: preparation.cover,
                assets: preparation.assets,
            },
        )
        .await
        .unwrap()
}

/// Store `folder` as a scanned candidate of its own watched root with a named
/// draft, returning its candidate key and what an import of it is queued
/// against: its revisions and the tag reading the scan stored.
pub(super) async fn store_scanned_candidate(
    test: &TestService,
    folder: &Path,
    name: &str,
) -> (crate::CandidateKey, super::ImportExpectation) {
    let service = &test.service;
    let tables = &test.service.tables;
    let files = crate::folder_scanner::collect_release_candidate_files_with_scope(
        folder,
        crate::ReleaseFileScope::Recursive,
        &crate::folder_scanner::StoredCandidateEdits::none(),
    )
    .unwrap();
    let content_hash = files.content_hash();
    let candidate_key = crate::CandidateKey::folder(folder);
    tables.add_watched_import_folder(folder).await.unwrap();
    let generation = tables.begin_folder_scan(folder, crate::VolumeKind::Local).await.unwrap();
    crate::store_scan_item(
        tables,
        &bae_diagnostics::Diagnostics::noop(),
        folder,
        generation,
        &ScanItem::Valid(crate::folder_scanner::FolderCandidate {
            path: folder.to_path_buf(),
            file_root: folder.to_path_buf(),
            name: name.to_string(),
            files,
            watched_folder_path: folder.to_path_buf(),
            scope: crate::ReleaseFileScope::Recursive,
            file_edit_revision: 0,
            relative_folder: bae_db::RelativeFolder::test(name),
            grouping: None,
        }),
    )
    .await
    .unwrap()
    .expect("the stored scan generation is current");
    tables.finish_folder_scan(folder, generation, None).await.unwrap();
    let metadata_revision = prepare_named_candidate(
        service,
        &test.preparations,
        &content_hash,
        folder,
        &candidate_key,
        name,
    )
    .await;
    let file_tag_snapshot = tables
        .load_candidate_file_tag_snapshot(folder, &candidate_key)
        .await
        .unwrap()
        .expect("the candidate is stored")
        .snapshot;
    (
        candidate_key,
        super::ImportExpectation {
            candidate: crate::CandidateAsRead {
                content_hash,
                file_edit_revision: 0,
                metadata_revision,
            },
            file_tag_snapshot,
        },
    )
}

#[derive(Clone)]
pub(super) struct FakeScanStarter {
    pub(super) scans: Arc<Mutex<Vec<FakeStartedScan>>>,
    pub(super) started: Arc<tokio::sync::Notify>,
    /// Taken by every start before anything else: while a test holds it, a
    /// coordinator starting a pass waits there, taking nothing else.
    pub(super) start_gate: Arc<Mutex<()>>,
}

impl FakeScanStarter {
    pub(super) fn new() -> (Self, RootScanStarter) {
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
            RootScanTask { cancellation, task }
        });
        (fake, starter)
    }

    /// Hold every pass start until the guard drops.
    pub(super) fn hold_starts(&self) -> std::sync::MutexGuard<'_, ()> {
        self.start_gate.lock().unwrap()
    }

    pub(super) async fn wait_for_count(&self, count: usize) {
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
    pub(super) fn complete(&self, index: usize) {
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
    pub(super) fn reading(
        &self,
        index: usize,
    ) -> Option<(crate::FolderReleaseDecisionKey, crate::FolderReleaseDecision)> {
        self.scans.lock().unwrap()[index].reading.as_ref().map(|reading| reading.target().clone())
    }

    /// The changed folders the pass at `index` reads again, if it is one.
    pub(super) fn folders(&self, index: usize) -> Option<Vec<std::ffi::OsString>> {
        self.scans.lock().unwrap()[index]
            .folders
            .as_ref()
            .map(|folders| folders.iter().cloned().collect())
    }

    pub(super) fn path(&self, index: usize) -> PathBuf {
        self.scans.lock().unwrap()[index].path.clone()
    }

    pub(super) fn cancellation(&self, index: usize) -> crate::folder_scanner::ScanCancellation {
        self.scans.lock().unwrap()[index].cancellation.clone()
    }

    /// End the pass at `index` without telling the coordinator it ended.
    pub(super) fn abort(&self, index: usize) {
        drop(
            self.scans.lock().unwrap()[index]
                .completion
                .take()
                .expect("fake scan has not completed"),
        );
    }

    pub(super) async fn wait_for_cancellation(&self, index: usize) {
        tokio::time::timeout(HANG_GUARD, async {
            while !self.cancellation(index).is_cancelled() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("scan coordinator did not cancel the active scan");
    }
}

pub(super) struct FakeStartedScan {
    pub(super) path: PathBuf,
    /// The folder decision this pass stores, with the caller it answers.
    pub(super) reading: Option<FolderReadingRequest>,
    /// The folders this pass reads again because they changed on disk.
    pub(super) folders: Option<std::collections::BTreeSet<std::ffi::OsString>>,
    pub(super) cancellation: crate::folder_scanner::ScanCancellation,
    /// Ends the pass's task, then tells the coordinator the pass is over.
    pub(super) completion: Option<(
        tokio::sync::oneshot::Sender<()>,
        tokio::sync::mpsc::UnboundedSender<RootScanCompletion>,
        RootScanCompletion,
    )>,
}

#[derive(Default)]
pub(super) struct FakeRemovalBackend {
    pub(super) uninstall_error: Mutex<Option<String>>,
    pub(super) remove_error: Mutex<Option<String>>,
    pub(super) reinstall_error: Mutex<Option<String>>,
    pub(super) block_reinstall: std::sync::atomic::AtomicBool,
    pub(super) reinstall_started: tokio::sync::Notify,
    pub(super) release_reinstall: tokio::sync::Notify,
    pub(super) calls: Mutex<Vec<&'static str>>,
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
    ) -> Result<Vec<crate::CandidateKey>, String> {
        self.calls.lock().unwrap().push("remove");
        match self.remove_error.lock().unwrap().clone() {
            Some(error) => Err(error),
            None => Ok(Vec::new()),
        }
    }
}

pub(super) struct CoordinatorHarness {
    pub(super) commands: tokio::sync::mpsc::UnboundedSender<WatcherCommand>,
    pub(super) fs_events: tokio::sync::mpsc::UnboundedSender<WatchReport>,
    pub(super) scans: FakeScanStarter,
    /// The store the coordinator reads which folders are watched from.
    pub(super) database: Database,
    pub(super) folder_state_commit: crate::FolderStateCommit,
    pub(super) removal_backend: Arc<FakeRemovalBackend>,
    pub(super) coordinator_thread: Mutex<Option<std::thread::JoinHandle<()>>>,
    pub(super) _temp: TempDir,
}

impl CoordinatorHarness {
    pub(super) async fn new() -> Self {
        Self::with_roots(&["/music"]).await
    }

    /// `roots` are `/`-spelled; each is registered in the host's spelling, as
    /// [`root_path`] gives it.
    pub(super) async fn with_roots(roots: &[&str]) -> Self {
        let roots: Vec<PathBuf> = roots.iter().map(|root| host_root(root)).collect();
        let TestService { service, preparations, database, temp } = setup_import_service().await;
        let tables = service.tables.clone();
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
                    relative_folder: bae_db::RelativeFolder::test("Group/Release"),
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
        let coordinator_thread = ImportService::start_watcher_with_starter(
            command_rx,
            fs_rx,
            crate::ImportServices {
                event_tx: service.event_tx,
                catalogs: service.catalogs.clone(),
                remote_images: service.remote_images.clone(),
                database: database.clone(),
                tables: service.tables.clone(),
                preparations: preparations.clone(),
                clock: service.clock.clone(),
                ids: service.ids.clone(),
                file_tags: Arc::new(crate::file_tag_snapshot::LoftyFileTagReader),
                directories: Arc::new(crate::folder_scanner::OsDirectoryReader),
                folder_state_commit: folder_state_commit.clone(),
                import_cancels: Default::default(),
                diagnostics: service.diagnostics.clone(),
                closing: tokio_util::sync::CancellationToken::new(),
            },
            starter,
            removal_backend.clone(),
        );
        Self {
            commands,
            fs_events,
            scans,
            database,
            folder_state_commit,
            removal_backend,
            coordinator_thread: Mutex::new(Some(coordinator_thread)),
            _temp: temp,
        }
    }

    /// Hand the coordinator `report` as its watch would, and wait until it
    /// has asked for the reads the report calls for.
    pub(super) async fn watch_report(&self, report: WatchReport) {
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
    pub(super) async fn commands_handled(&self) {
        self.watch_report(Ok(Vec::new())).await;
    }

    pub(super) async fn shutdown(&self) {
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

/// Scan dependencies over the test service's library, with a folder-state
/// commit lock of their own.
pub(super) fn test_scan_services(
    service: &ImportService,
    database: &bae_db::Database,
    preparations: &crate::CandidatePreparations,
    event_tx: crate::handle::ImportEventBus,
    folder_watcher: Arc<FolderWatcher>,
    file_tags: Arc<dyn crate::file_tag_snapshot::FileTagReader>,
    directories: Arc<dyn crate::folder_scanner::DirectoryReader>,
) -> ScanServices {
    ScanServices::new(
        crate::ImportServices {
            event_tx,
            database: database.clone(),
            tables: service.tables.clone(),
            catalogs: service.catalogs.clone(),
            remote_images: service.remote_images.clone(),
            preparations: preparations.clone(),
            clock: service.clock.clone(),
            ids: service.ids.clone(),
            file_tags,
            directories,
            folder_state_commit: crate::FolderStateCommit::default(),
            import_cancels: Default::default(),
            diagnostics: service.diagnostics.clone(),
            closing: tokio_util::sync::CancellationToken::new(),
        },
        folder_watcher,
    )
}

/// What a test reads a folder with. `_fs_rx` keeps the folder watcher's sender
/// connected while the scan runs.
pub(super) struct TestScan {
    pub(super) services: ScanServices,
    pub(super) cancellation: crate::folder_scanner::ScanCancellation,
    pub(super) _fs_rx: tokio::sync::mpsc::UnboundedReceiver<WatchReport>,
}

impl TestScan {
    /// Read `root` and reconcile what it holds against what is stored.
    pub(super) async fn rescan(&self, root: &Path) -> Result<(), crate::ImportError> {
        ImportService::rescan_and_reconcile(root, &self.services, &self.cancellation).await
    }
}

/// Rescan `root`, wait for its scan to start, and return the root in the
/// host's spelling.
pub(super) async fn rescan_and_wait(harness: &CoordinatorHarness, root: &str) -> PathBuf {
    let root = root_path(root);
    harness.commands.send(WatcherCommand::Rescan(root.clone())).unwrap();
    harness.scans.wait_for_count(1).await;
    root
}

/// Ask to remove `root` while it is being scanned, and wait for the coordinator
/// to cancel that scan. The caller ends the scan.
pub(super) async fn removal_awaiting_its_cancelled_scan(
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

pub(super) fn request_refresh(
    harness: &CoordinatorHarness,
    root: &str,
) -> tokio::sync::oneshot::Receiver<Result<(), String>> {
    let (completion, result) = tokio::sync::oneshot::channel();
    harness.commands.send(WatcherCommand::Refresh { path: root_path(root), completion }).unwrap();
    result
}

/// Ask for `Group` under `root` to become one release.
pub(super) fn request_group_decision(
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
                    relative_folder: bae_db::RelativeFolder::test("Group"),
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
pub(super) async fn assert_next_pass_reads_only_a_later_change(
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
pub(super) fn is_answered<T>(
    result: &mut tokio::sync::oneshot::Receiver<Result<T, String>>,
) -> bool {
    !matches!(result.try_recv(), Err(tokio::sync::oneshot::error::TryRecvError::Empty))
}

pub(super) fn watch_event(kind: notify::EventKind, path: PathBuf) -> notify::Event {
    notify::Event::new(kind).add_path(path)
}

/// Lists folders as the OS does, and remembers every folder it was asked for.
#[derive(Default)]
pub(super) struct RecordingListing {
    pub(super) read: std::sync::Mutex<Vec<PathBuf>>,
}

impl RecordingListing {
    pub(super) fn take(&self) -> Vec<PathBuf> {
        std::mem::take(&mut *self.read.lock().unwrap())
    }
}

impl crate::folder_scanner::DirectoryReader for RecordingListing {
    fn read(
        &self,
        root: &Path,
        directory: &Path,
        cancellation: &crate::folder_scanner::ScanCancellation,
    ) -> Result<crate::folder_scanner::DirectoryListing, crate::folder_scanner::FolderScanError>
    {
        self.read.lock().unwrap().push(root.join(directory));
        crate::folder_scanner::OsDirectoryReader.read(root, directory, cancellation)
    }

    fn folder_times(
        &self,
        root: &Path,
        folders: &[PathBuf],
        cancellation: &crate::folder_scanner::ScanCancellation,
    ) -> Result<Vec<crate::folder_scanner::FolderTimes>, crate::folder_scanner::FolderScanError>
    {
        crate::folder_scanner::OsDirectoryReader.folder_times(root, folders, cancellation)
    }
}

/// `count` playable FLAC files in `folder`, named `01.flac` onwards.
pub(super) fn write_disc(folder: &Path, count: usize) {
    std::fs::create_dir_all(folder).unwrap();
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/flac/01 Test Track 1.flac");
    for track in 1..=count {
        std::fs::copy(&fixture, folder.join(format!("{track:02}.flac"))).unwrap();
    }
}

/// A watched root holding `Artist/Album` with two discs of `tracks` tracks
/// named `discs`, and a sibling `Other/Record` with one track — read whole
/// once, the way the folder was first added.
pub(super) struct DecisionFixture {
    pub(super) test: TestService,
    pub(super) scan: TestScan,
    pub(super) listing: Arc<RecordingListing>,
    pub(super) root: PathBuf,
}

impl DecisionFixture {
    pub(super) async fn new(discs: [&str; 2], tracks: usize) -> Self {
        let test = setup_import_service().await;
        let root = test.temp.path().join("music");
        for disc in discs {
            write_disc(&root.join("Artist").join("Album").join(disc), tracks);
        }
        write_disc(&root.join("Other").join("Record"), 1);
        let root = bae_db::watched_path::canonical_absolute_root(&root).unwrap();
        test.service.tables.add_watched_import_folder(&root).await.unwrap();
        let listing = Arc::new(RecordingListing::default());
        let (scan, _) =
            test.scan_with(Arc::new(crate::file_tag_snapshot::LoftyFileTagReader), listing.clone());
        scan.rescan(&root).await.unwrap();
        listing.take();
        Self { test, scan, listing, root }
    }

    pub(super) fn album_key(&self) -> crate::FolderReleaseDecisionKey {
        crate::FolderReleaseDecisionKey {
            watched_folder_path: self.root.clone(),
            relative_folder: bae_db::RelativeFolder::test("Artist/Album"),
        }
    }

    pub(super) async fn stored_paths(&self) -> Vec<String> {
        let mut paths: Vec<String> = self
            .test
            .service
            .tables
            .load_folder_scan_items(&self.root)
            .await
            .unwrap()
            .iter()
            .filter_map(|item| item.relative_folder().map(bae_db::RelativeFolder::text))
            .collect();
        paths.sort();
        paths
    }

    /// Read `Artist/Album` as `decision`, watching the import list the whole
    /// time: every list the live query delivered, up to the first one showing
    /// `settled`.
    pub(super) async fn decide_watching_the_list(
        &self,
        decision: crate::FolderReleaseDecision,
        settled: &[&str],
    ) -> Vec<Vec<String>> {
        let tables = self.test.service.tables.clone();
        let mut live = tables.subscribe_import_list(crate::ImportListRequest {
            list: self
                .test
                .service
                .tables
                .open_import_list(crate::ImportListView {
                    order: crate::ImportListOrder::PathAscending,
                    ..crate::ImportListView::default()
                })
                .await
                .unwrap(),
            windows: [bae_db::LibraryPageWindow { offset: 0, limit: 50 }].into_iter().collect(),
        });
        let (seen_tx, mut seen_rx) = tokio::sync::mpsc::unbounded_channel();
        let watcher = tokio::spawn(async move {
            loop {
                let projection = live.next().await.expect("the query stays open").result.unwrap();
                let names: Vec<String> = projection
                    .windows
                    .iter()
                    .flat_map(|window| &window.items)
                    .filter_map(|item| match item {
                        crate::ImportListItem::Candidate { row, .. } => {
                            Some(row.folder_name.clone())
                        }
                        _ => None,
                    })
                    .collect();
                if seen_tx.send(names).is_err() {
                    return;
                }
            }
        });
        let first = seen_rx.recv().await.expect("the list delivers its first value");

        ImportService::change_folder_reading(
            &self.root,
            &(self.album_key(), decision),
            &self.scan.services,
            &self.scan.cancellation,
        )
        .await
        .unwrap();

        let mut seen = vec![first];
        let settled: Vec<String> = settled.iter().map(|name| name.to_string()).collect();
        while !settled.iter().all(|name| seen.last().unwrap().contains(name)) {
            seen.push(
                tokio::time::timeout(HANG_GUARD, seen_rx.recv())
                    .await
                    .expect("the list shows the new reading")
                    .expect("the list is still delivering"),
            );
        }
        watcher.abort();
        seen
    }
}

pub(super) fn flac() -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/flac/01 Test Track 1.flac"),
    )
    .unwrap()
}

/// The two fixture tracks that carry real Vorbis comments — an album, its
/// artist, and a title per track.
pub(super) const TAGGED_FLAC_FIXTURES: [&str; 2] = [
    "02 Test Artist - Track Two (White Noise).flac",
    "03 Test Artist - Track Three (Brown Noise).flac",
];

#[tokio::test]
async fn cancelled_scan_task_does_not_begin_a_durable_generation() {
    let TestService { service, preparations, database, temp: tmp } = setup_import_service().await;
    let tables = service.tables.clone();
    let root = tmp.path().join("watched");
    std::fs::create_dir(&root).unwrap();
    tables.add_watched_import_folder(&root).await.unwrap();
    let (watch_tx, _watch_rx) = tokio::sync::mpsc::unbounded_channel();
    let watcher = Arc::new(FolderWatcher::new(watch_tx, bae_diagnostics::Diagnostics::noop()));
    let (completion_tx, mut completion_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut events = service.event_tx.every_event();

    let scan = spawn_root_scan(
        1,
        root,
        test_scan_services(
            &service,
            &database,
            &preparations,
            service.event_tx.clone(),
            watcher,
            std::sync::Arc::new(crate::file_tag_snapshot::LoftyFileTagReader),
            std::sync::Arc::new(crate::folder_scanner::OsDirectoryReader),
        ),
        completion_tx,
    );
    scan.cancellation.cancel();
    tokio::time::timeout(HANG_GUARD, completion_rx.recv())
        .await
        .expect("cancelled scan did not report task completion")
        .expect("scan task completion channel closed");
    scan.task.ended().await;

    assert!(tables.load_folder_scan_snapshots().await.unwrap().is_empty());
    assert!(matches!(events.try_recv(), Err(tokio::sync::mpsc::error::TryRecvError::Empty)));
}

pub(super) fn write_test_jpeg(path: &Path) {
    let image = ::image::RgbImage::from_pixel(1, 1, ::image::Rgb([0, 0, 0]));
    image.save(path).unwrap();
}

/// The candidates a pass told anyone about, in the order it did.
pub(super) fn announced_candidates(
    events: &mut tokio::sync::mpsc::UnboundedReceiver<crate::handle::ImportEvent>,
) -> Vec<String> {
    let mut announced = Vec::new();
    while let Ok(event) = events.try_recv() {
        match event {
            crate::handle::ImportEvent::Scan(ScanEvent::FolderCandidate { candidate, .. })
            | crate::handle::ImportEvent::Scan(ScanEvent::CandidateDiscovered {
                candidate, ..
            }) => announced.push(candidate.relative_folder.text()),
            crate::handle::ImportEvent::Scan(ScanEvent::CandidateRemoved { candidate_key }) => {
                announced.push(format!("removed {candidate_key}"))
            }
            _ => {}
        }
    }
    announced
}
