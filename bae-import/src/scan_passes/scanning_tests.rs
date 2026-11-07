//! A pass over a watched root: its candidates announced as the walk finds
//! them, the folders it read recorded, and a walk that panics or a root that
//! cannot be read failing the pass and keeping what was stored.

use super::*;
use crate::scan_passes::tests::*;
use crate::service::tests::*;
use bae_store::Store;
use bae_util::test_channels::HANG_GUARD;

#[tokio::test]
async fn cancelling_a_panicked_folder_walk_fails_and_reports_it() {
    let bugs = Arc::new(bae_diagnostics::RecordingBugReporter::default());
    let diagnostics = bae_diagnostics::Diagnostics::noop().reporting_bugs_to(bugs.clone());
    let cancellation = crate::folder_scanner::ScanCancellation::new();
    let (item_tx, mut item_rx) = tokio::sync::mpsc::channel(1);
    drop(item_tx);
    let error = cancel_and_join_folder_walk(
        &cancellation,
        &mut item_rx,
        start_folder_walk(&diagnostics, || panic!("folder walk panic")),
        &diagnostics,
    )
    .await
    .expect_err("a panicked traversal task cannot report a successful cancellation");

    assert!(cancellation.is_cancelled());
    assert_eq!(
        error.to_string(),
        "internal import error: walk a watched folder panicked: folder walk panic"
    );
    assert_eq!(
        bugs.reports(),
        vec![(
            "walk a watched folder".to_string(),
            "walk a watched folder panicked: folder walk panic".to_string()
        )]
    );
}

/// A walk whose pass stopped waiting for it runs to its end, and the panic
/// of one of its threads still reaches the crash reporter. Sabotage: start
/// the walk with `tokio::task::spawn_blocking`, or have `FolderScanError`
/// carry no bug, and the panic goes nowhere.
#[tokio::test]
async fn a_walk_its_pass_stopped_waiting_for_reports_its_threads_panic() {
    let reports = Arc::new(bae_diagnostics::HeardBugReports::default());
    let diagnostics = bae_diagnostics::Diagnostics::noop().reporting_bugs_to(reports.clone());
    let (go, wait) = std::sync::mpsc::channel::<()>();
    let walk = start_folder_walk(&diagnostics, move || {
        wait.recv().expect("the test lets the walk end");
        Err(crate::folder_scanner::FolderScanError::Panicked(bae_util::task_failure::Bug {
            operation: "read folders' dates",
            message: "a folder vanished mid-read".to_string(),
        }))
    });
    // The pass stops waiting; the walk runs on, and reports.
    drop(walk);
    go.send(()).expect("the walk is waiting");
    let report =
        tokio::time::timeout(HANG_GUARD, reports.next()).await.expect("the panic is reported");
    assert_eq!(
        report,
        (
            "read folders' dates".to_string(),
            "read folders' dates panicked: a folder vanished mid-read".to_string()
        )
    );
}

/// Lists folders as the OS does, except that the first folder under the
/// root a walk asks for panics the first time.
struct ListingThatPanicsOnce {
    panicked: std::sync::atomic::AtomicBool,
}

impl crate::folder_scanner::DirectoryReader for ListingThatPanicsOnce {
    fn read(
        &self,
        root: &Path,
        directory: &Path,
        cancellation: &crate::folder_scanner::ScanCancellation,
    ) -> Result<crate::folder_scanner::DirectoryListing, crate::folder_scanner::FolderScanError>
    {
        if directory.components().count() == 1
            && !self.panicked.swap(true, std::sync::atomic::Ordering::SeqCst)
        {
            panic!("the listing broke");
        }
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

/// What the scan of the only watched root stored of how it ended.
async fn stored_outcome(db: &Store) -> crate::StoredScanOutcome {
    let tables = crate::tables::ImportTables::new(db.clone());
    let mut scans = tables.load_stored_folder_scans().await.unwrap();
    assert_eq!(scans.len(), 1, "one root is watched");
    scans.remove(0).last_scan.expect("a scan opened the root's generation").outcome
}

/// The folder scan coordinator over `scan`'s services, taking `command_rx`,
/// and what it holds of each root's scan.
fn start_coordinator(
    scan: &TestScan,
    mut command_rx: tokio::sync::mpsc::UnboundedReceiver<crate::watching::WatcherCommand>,
) -> (
    std::thread::JoinHandle<()>,
    tokio::sync::watch::Receiver<crate::folder_scan_status::FolderScansInMemory>,
) {
    let (_fs_tx, fs_rx) = tokio::sync::mpsc::unbounded_channel();
    let (in_memory_tx, in_memory) =
        tokio::sync::watch::channel(crate::folder_scan_status::FolderScansInMemory::new());
    let services = scan.services.services.clone();
    let folder_watcher = scan.services.folder_watcher.clone();
    let coordinator = std::thread::spawn(move || {
        crate::watching::run_watcher(&mut command_rx, fs_rx, services, folder_watcher, in_memory_tx)
    });
    (coordinator, in_memory)
}

/// Refresh `root` through `commands` and wait for the answer.
async fn refresh(
    commands: &tokio::sync::mpsc::UnboundedSender<crate::watching::WatcherCommand>,
    root: &Path,
) -> Result<(), String> {
    let (completion, answered) = tokio::sync::oneshot::channel();
    commands
        .send(crate::watching::WatcherCommand::Refresh { path: root.to_path_buf(), completion })
        .unwrap();
    tokio::time::timeout(HANG_GUARD, answered).await.expect("the refresh is answered").unwrap()
}

/// Stop the coordinator and wait for its thread.
async fn stop_coordinator(
    commands: &tokio::sync::mpsc::UnboundedSender<crate::watching::WatcherCommand>,
    coordinator: std::thread::JoinHandle<()>,
) {
    let (completion, stopped) = std::sync::mpsc::channel();
    commands.send(crate::watching::WatcherCommand::Shutdown { completion }).unwrap();
    bae_util::test_blocking::on_blocking_pool(move || {
        stopped.recv().unwrap();
        coordinator.join().unwrap();
    })
    .await;
}

/// A refresh whose pass panics partway through the walk is answered, the
/// root's scan is stored as failed with the panic, the crash reporter hears
/// of it once, and the root is free for the next refresh, which reads it.
#[tokio::test]
async fn a_pass_that_panics_fails_the_scan_and_frees_the_root() {
    let mut test = setup_import_service().await;
    let tables = test.tables().clone();
    let bugs = Arc::new(bae_diagnostics::RecordingBugReporter::default());
    test.report_bugs_to(bugs.clone());
    let root = test.temp.path().join("watched");
    for name in ["Release A", "Release B"] {
        let folder = root.join(name);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("track.flac"), flac()).unwrap();
    }
    tables.add_watched_import_folder(&root).await.unwrap();
    let (scan, _events) = test.scan_with(
        Arc::new(crate::file_tag_reader::LoftyFileTagReader),
        Arc::new(ListingThatPanicsOnce { panicked: std::sync::atomic::AtomicBool::new(false) }),
    );
    let (commands, command_rx) = tokio::sync::mpsc::unbounded_channel();
    let (coordinator, _in_memory) = start_coordinator(&scan, command_rx);

    assert_eq!(refresh(&commands, &root).await, Ok(()));
    let crate::StoredScanOutcome::Failed { error } = stored_outcome(&test.db).await else {
        panic!("the panicked pass is stored as failed");
    };
    assert!(error.contains("the listing broke"), "{error}");
    assert_eq!(
        bugs.reports(),
        vec![(
            "walk a folder under a watched root".to_string(),
            "walk a folder under a watched root panicked: the listing broke".to_string()
        )]
    );

    assert_eq!(refresh(&commands, &root).await, Ok(()));
    assert_eq!(stored_outcome(&test.db).await, crate::StoredScanOutcome::Complete);

    stop_coordinator(&commands, coordinator).await;
}

/// Lists folders as the OS does, except that the second release folder the
/// walk asks for stays closed until the test opens it. Which folder that is
/// depends on the walk's own order, which this test does not assume: it
/// records the first one and holds whichever comes next.
struct HeldListing {
    first: std::sync::Mutex<Option<PathBuf>>,
    entered: tokio::sync::mpsc::UnboundedSender<PathBuf>,
    gate: std::sync::Arc<(std::sync::Mutex<bool>, std::sync::Condvar)>,
}

impl HeldListing {
    fn new() -> (Self, tokio::sync::mpsc::UnboundedReceiver<PathBuf>) {
        let (entered, entered_rx) = tokio::sync::mpsc::unbounded_channel();
        (
            Self {
                first: std::sync::Mutex::new(None),
                entered,
                gate: std::sync::Arc::new((
                    std::sync::Mutex::new(false),
                    std::sync::Condvar::new(),
                )),
            },
            entered_rx,
        )
    }

    fn open(&self) {
        let (lock, condition) = &*self.gate;
        *lock.lock().unwrap() = true;
        condition.notify_all();
    }
}

impl crate::folder_scanner::DirectoryReader for HeldListing {
    fn read(
        &self,
        root: &Path,
        directory: &Path,
        cancellation: &crate::folder_scanner::ScanCancellation,
    ) -> Result<crate::folder_scanner::DirectoryListing, crate::folder_scanner::FolderScanError>
    {
        if directory.components().count() == 1 {
            // Decided under the lock and waited on outside it: the test reads
            // `first` while this holds the gate.
            let hold = {
                let mut first = self.first.lock().unwrap();
                match &*first {
                    None => {
                        *first = Some(directory.to_path_buf());
                        false
                    }
                    Some(first) => first != directory,
                }
            };
            if hold {
                self.entered
                    .send(directory.to_path_buf())
                    .expect("the test waits on the held folder");
                let (lock, condition) = &*self.gate;
                let mut open = lock.lock().unwrap();
                while !*open {
                    open = condition.wait(open).unwrap();
                }
            }
        }
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

/// Opens the gate when dropped, so a failed assertion releases the parked
/// walk and the test reports the failure instead of waiting on the walk at
/// runtime shutdown.
struct OpensOnDrop<'a>(&'a HeldListing);

impl Drop for OpensOnDrop<'_> {
    fn drop(&mut self) {
        self.0.open();
    }
}

/// A scan announces each candidate as the walk finds it, not as one batch
/// once the walk ends. Walk duration scales with the tree and with how fast
/// the volume answers — a network share is orders of magnitude slower than a
/// local disk — and a list that stays empty for that whole span is
/// indistinguishable from a scan that found nothing. This holds one folder's
/// listing closed and requires the folder read before it to be announced
/// meanwhile, whatever the walk's order and however it is built.
#[tokio::test]
async fn a_candidate_is_announced_while_the_walk_still_reads_the_next_folder() {
    let test = setup_import_service().await;
    let tables = test.tables().clone();
    let root = test.temp.path().join("watched");
    for name in ["Release A", "Release B"] {
        let folder = root.join(name);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("track.flac"), flac()).unwrap();
    }
    tables.add_watched_import_folder(&root).await.unwrap();
    let (reader, mut entered) = HeldListing::new();
    let reader = Arc::new(reader);
    let (scan, mut events) =
        test.scan_with(Arc::new(crate::file_tag_reader::LoftyFileTagReader), reader.clone());

    let observe = async {
        let _opens_on_failure = OpensOnDrop(&reader);
        let held = tokio::time::timeout(HANG_GUARD, entered.recv())
            .await
            .expect("the walk reaches a second folder")
            .expect("the reader is still held");
        let first =
            reader.first.lock().unwrap().clone().expect("a folder was read before the held one");
        let announced = tokio::time::timeout(HANG_GUARD, async {
            loop {
                if let crate::ImportEvent::Scan(
                    ScanEvent::FolderCandidate { candidate, .. }
                    | ScanEvent::CandidateDiscovered { candidate, .. },
                ) = events.recv().await.expect("the scan is still announcing")
                {
                    break candidate.path;
                }
            }
        })
        .await
        .expect("the folder read before the held one is announced while it is held");
        reader.open();
        (root.join(first), root.join(held), announced)
    };
    let (result, (first, held, announced)) = tokio::join!(scan.rescan(&root), observe);
    result.expect("the scan completes once the held folder opens");

    assert_ne!(first, held);
    assert_eq!(announced, first);
    assert!(
        announced_candidates(&mut events).iter().any(|path| root.join(path) == held),
        "the held folder is announced once its listing opens"
    );
}

async fn rescan_seeded_root(
    test: &TestService,
    root: &Path,
) -> (crate::ImportEvents, Result<(), ScanEnded>) {
    let tables = test.tables();
    tables.add_watched_import_folder(root).await.unwrap();
    let generation = tables.begin_folder_scan(root, crate::VolumeKind::Local).await.unwrap();
    crate::store_scan_item(
        tables,
        &bae_diagnostics::Diagnostics::noop(),
        root,
        generation,
        &ScanItem::Invalid(crate::InvalidCandidate {
            path: root.join("old-key"),
            name: "Old Candidate".to_string(),
            watched_folder_path: root.to_path_buf(),
            relative_folder: crate::RelativeFolder::test("old-key"),
            grouping: None,
            files: Vec::new(),
            tags: None,
        }),
    )
    .await
    .unwrap()
    .expect("the seeded scan generation is current");

    let (scan, events) = test.scan();
    let result = scan.rescan(root).await;

    (events, result)
}

/// The invalid candidates the stored scan of `root` still holds.
async fn stored_invalid_candidates(test: &TestService, root: &Path) -> usize {
    let tables = test.tables().clone();
    tables
        .load_folder_scan_items(root)
        .await
        .unwrap()
        .into_iter()
        .filter(|item| matches!(item, ScanItem::Invalid(_)))
        .count()
}

/// Fail if the pass removed any candidate.
fn assert_nothing_removed(events: &mut crate::ImportEvents) {
    while let Ok(event) = events.try_recv() {
        if let crate::ImportEvent::Scan(ScanEvent::CandidateRemoved { candidate_key }) = event {
            panic!("the failed pass removed {candidate_key}");
        }
    }
}

#[tokio::test]
async fn rescan_missing_root_fails_and_preserves_previous_candidates() {
    let test = setup_import_service().await;
    let root = test.temp.path().join("missing-root");
    let (mut events, result) = rescan_seeded_root(&test, &root).await;
    assert_eq!(result, Err(ScanEnded::Failed));

    let crate::StoredScanOutcome::Failed { error } = stored_outcome(&test.db).await else {
        panic!("the failed pass stored its failure");
    };
    // The stored failure names the root that could not be read. Its reason is
    // the OS's own wording for an absent path ("No such file or directory" on
    // Unix, "The system cannot find the path specified" on Windows), so the
    // root — the part core promises — is what this asserts on.
    assert!(error.contains(&root.to_string_lossy().into_owned()), "{error}");
    assert_nothing_removed(&mut events);
    assert_eq!(stored_invalid_candidates(&test, &root).await, 1);
}

/// A failed scan the store refuses to take is a write nothing should refuse,
/// so it is reported as the bug it is, and the failure is handed back for
/// the coordinator to hold.
#[tokio::test]
async fn a_failure_the_store_refuses_is_reported_as_a_bug() {
    let mut test = setup_import_service().await;
    let bugs = Arc::new(bae_diagnostics::RecordingBugReporter::default());
    test.report_bugs_to(bugs.clone());
    let root = test.temp.path().join("missing-root");
    test.tables().add_watched_import_folder(&root).await.unwrap();
    crate::tables::ImportTables::fail_failed_scan_writes_for_test(test.temp.path()).await;

    let (scan, _events) = test.scan();
    let Err(ScanEnded::FailedUnstored(crate::folder_scan_status::UnstoredScanFailure {
        error,
        ..
    })) = scan.rescan(&root).await
    else {
        panic!("a missing root fails its scan, and the failure is not stored");
    };
    assert!(error.contains(&root.to_string_lossy().into_owned()), "{error}");

    let reports = bugs.reports();
    assert_eq!(reports.len(), 1, "{reports:?}");
    let (operation, report) = &reports[0];
    assert_eq!(operation, "store a watched folder's failed scan");
    assert!(report.contains("the failed scan was refused"), "{report}");
}

/// The failure of a scan the store would not take is the root's status, from
/// the coordinator's memory, until the root's next pass starts; the stored
/// scan holds no outcome meanwhile.
#[tokio::test]
async fn a_failure_the_store_refuses_is_the_roots_status_until_its_next_pass() {
    let test = setup_import_service().await;
    let root = test.temp.path().join("missing-root");
    test.tables().add_watched_import_folder(&root).await.unwrap();
    crate::tables::ImportTables::fail_failed_scan_writes_for_test(test.temp.path()).await;
    let (scan, _events) = test.scan();
    let (commands, command_rx) = tokio::sync::mpsc::unbounded_channel();
    let (coordinator, in_memory) = start_coordinator(&scan, command_rx);
    let progress = || async {
        let held = in_memory.borrow().clone();
        let stored = test.tables().load_stored_folder_scans().await.unwrap();
        let progress = crate::FolderScanProgress::of(stored, &held);
        let [status] = <[_; 1]>::try_from(progress.statuses).expect("one root is watched");
        status.status
    };

    assert_eq!(refresh(&commands, &root).await, Ok(()));
    assert_eq!(
        stored_outcome(&test.db).await,
        crate::StoredScanOutcome::Unfinished { found_count: 0 }
    );
    let crate::FolderScanStatus::Failed { error } = progress().await else {
        panic!("the unstored failure is the root's status");
    };
    assert!(error.contains(&root.to_string_lossy().into_owned()), "{error}");

    std::fs::create_dir(&root).unwrap();
    assert_eq!(refresh(&commands, &root).await, Ok(()));
    assert_eq!(progress().await, crate::FolderScanStatus::Complete);

    stop_coordinator(&commands, coordinator).await;
}

/// A folder whose first scan cannot even open its generation has no stored
/// scan, and still shows as failed, with the error and the volume the pass
/// found it on.
#[tokio::test]
async fn a_first_scan_that_cannot_begin_shows_as_failed() {
    let mut test = setup_import_service().await;
    let bugs = Arc::new(bae_diagnostics::RecordingBugReporter::default());
    test.report_bugs_to(bugs.clone());
    let root = test.temp.path().join("watched");
    std::fs::create_dir(&root).unwrap();
    test.tables().add_watched_import_folder(&root).await.unwrap();
    crate::tables::ImportTables::fail_first_scan_begins_for_test(test.temp.path()).await;
    let (scan, _events) = test.scan();
    let (commands, command_rx) = tokio::sync::mpsc::unbounded_channel();
    let (coordinator, in_memory) = start_coordinator(&scan, command_rx);

    assert_eq!(refresh(&commands, &root).await, Ok(()));
    let held = in_memory.borrow().clone();
    let stored = test.tables().load_stored_folder_scans().await.unwrap();
    let progress = crate::FolderScanProgress::of(stored, &held);
    let [status] = <[_; 1]>::try_from(progress.statuses).expect("the watched folder is listed");
    assert_eq!(status.watched_folder_path, root);
    let crate::FolderScanStatus::Failed { error } = status.status else {
        panic!("the folder's first scan failed");
    };
    assert!(error.contains("the scan could not begin"), "{error}");
    assert_eq!(
        status.on_network_volume,
        crate::volume::volume_kind(&root, &bae_diagnostics::Diagnostics::noop()).await
            == crate::VolumeKind::Network
    );
    assert_eq!(bugs.reports().len(), 1, "{:?}", bugs.reports());

    stop_coordinator(&commands, coordinator).await;
}

#[tokio::test]
async fn rescan_non_directory_root_keeps_previous_candidates() {
    let test = setup_import_service().await;
    let root = test.temp.path().join("not-a-directory");
    std::fs::write(&root, b"not a directory").unwrap();
    let (mut events, result) = rescan_seeded_root(&test, &root).await;
    assert_eq!(result, Err(ScanEnded::Failed), "a non-directory root must fail its scan");

    let crate::StoredScanOutcome::Failed { error } = stored_outcome(&test.db).await else {
        panic!("the failed pass stored its failure");
    };
    assert!(error.to_lowercase().contains("not a directory"), "got: {error}");
    assert_nothing_removed(&mut events);
    assert_eq!(stored_invalid_candidates(&test, &root).await, 1);
}

/// A completed pass records every directory it read and when it was last
/// touched, and asked straight afterwards the recorded set says nothing moved.
///
/// That is what a folder on a network volume is re-read on instead of a walk:
/// if the record were missing, incomplete, or read at a precision the
/// filesystem does not keep, every check would claim a change and the walk
/// would happen anyway.
#[tokio::test]
async fn a_pass_records_the_directories_it_read() {
    let test = setup_import_service().await;
    let tables = test.tables().clone();
    let root = test.temp.path().join("watched");
    let album = root.join("Artist - Album");
    std::fs::create_dir_all(album.join("Artwork")).unwrap();
    std::fs::write(album.join("01.flac"), flac()).unwrap();
    tables.add_watched_import_folder(&root).await.unwrap();

    let (scan, _events) = test.scan();
    scan.rescan(&root).await.expect("the pass reads the folder");

    let recorded = tables.load_folder_scan_directories(&root).await.unwrap();
    let mut paths: Vec<&Path> = recorded.iter().map(|(path, _)| path.as_path()).collect();
    paths.sort_unstable();
    assert_eq!(
        paths,
        vec![root.as_path(), album.as_path(), album.join("Artwork").as_path()]
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
    );
    assert_eq!(crate::volume::changed_directories(&recorded), Some(Vec::new()));

    // And a file written into one of them is a change the check reports, in
    // that directory and nowhere else.
    std::fs::write(album.join("02.flac"), flac()).unwrap();
    assert_eq!(crate::volume::changed_directories(&recorded), Some(vec![album.clone()]));
}

/// A store file coven could not read or keep is the disk's failure, so the
/// refused write that carried it is not a bug.
#[test]
fn a_refused_scan_write_with_a_store_file_the_disk_failed_on_is_not_a_bug() {
    let disk = coven::FileError::Io {
        operation: "write",
        path: std::path::PathBuf::from("library.db"),
        source: Box::new(std::io::Error::from(std::io::ErrorKind::StorageFull)),
    };
    let refused = bae_store::DbError::Coven(Box::new(coven::CovenError::Database(
        coven::DbError::Disk(disk),
    )));
    assert!(!refusal_is_a_bug(&refused));
}

/// A refused write of a scan's state is a bug unless the disk under the
/// store failed, the store failed as a whole, or the library is closing.
#[test]
fn a_refused_scan_write_is_a_bug_unless_the_disk_or_the_store_failed() {
    let sqlite = |code| {
        bae_store::DbError::Sqlite(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(code),
            None,
        ))
    };
    let coven = |error| bae_store::DbError::Coven(Box::new(coven::CovenError::Database(error)));
    assert!(!refusal_is_a_bug(&sqlite(rusqlite::ffi::SQLITE_FULL)));
    assert!(!refusal_is_a_bug(&sqlite(rusqlite::ffi::SQLITE_IOERR)));
    assert!(!refusal_is_a_bug(&coven(coven::DbError::StoreClosed)));
    assert!(!refusal_is_a_bug(&coven(coven::DbError::RollbackFailed)));
    let damaged = std::sync::Arc::new(coven::DbError::DamagedDatabase);
    assert!(!refusal_is_a_bug(&coven(coven::DbError::StatisticsFailed(damaged))));
    // An earlier bug that failed the store's handle, reported as it happened.
    let bug = coven::task::Bug { operation: "authorize a statement", message: "boom".into() };
    assert!(!refusal_is_a_bug(&coven(coven::DbError::StoreFailed(bug))));
    assert!(!refusal_is_a_bug(&coven(coven::DbError::DamagedDatabase)));
    assert!(refusal_is_a_bug(&sqlite(rusqlite::ffi::SQLITE_CONSTRAINT_TRIGGER)));
    assert!(refusal_is_a_bug(&coven(coven::DbError::Invariant { what: "a rule" })));
    assert!(refusal_is_a_bug(&coven(coven::DbError::StatementFailed)));
    assert!(refusal_is_a_bug(&bae_store::DbError::unreadable("a stored column", "a value")));
}
