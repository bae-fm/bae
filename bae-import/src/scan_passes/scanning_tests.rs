//! A pass over a watched root: its candidates announced as the walk finds
//! them, the folders it read recorded, and a walk that panics or a root that
//! cannot be read failing the pass and keeping what was stored.

use super::*;
use crate::scan_passes::tests::*;
use crate::service::tests::*;
use bae_db::Database;
use bae_util::test_channels::HANG_GUARD;

#[tokio::test]
async fn cancelling_a_panicked_folder_walk_fails_and_reports_it() {
    async fn panic_during_walk() -> crate::scan_passes::FolderWalkOutcome {
        panic!("folder walk panic");
    }

    let bugs = Arc::new(bae_diagnostics::RecordingBugReporter::default());
    let diagnostics = bae_diagnostics::Diagnostics::noop().reporting_bugs_to(bugs.clone());
    let cancellation = crate::folder_scanner::ScanCancellation::new();
    let (item_tx, mut item_rx) = tokio::sync::mpsc::channel(1);
    drop(item_tx);
    let error = cancel_and_join_folder_walk(
        &cancellation,
        &mut item_rx,
        tokio::spawn(panic_during_walk()),
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

/// Where the stored scan of the only watched root stands.
async fn root_scan_status(database: &Database) -> crate::FolderScanStatus {
    let tables = crate::tables::ImportTables::new(database.clone());
    let mut statuses = tables.load_folder_scan_progress().await.unwrap().statuses;
    assert_eq!(statuses.len(), 1, "one root is watched");
    statuses.remove(0).status
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
        Arc::new(crate::file_tag_snapshot::LoftyFileTagReader),
        Arc::new(ListingThatPanicsOnce { panicked: std::sync::atomic::AtomicBool::new(false) }),
    );
    let (commands, command_rx) = tokio::sync::mpsc::unbounded_channel();
    let (_fs_tx, fs_rx) = tokio::sync::mpsc::unbounded_channel();
    let coordinator = crate::watching::start_watcher(
        command_rx,
        fs_rx,
        scan.services.services.clone(),
        scan.services.folder_watcher.clone(),
    );
    let refresh = || {
        let (completion, answered) = tokio::sync::oneshot::channel();
        commands
            .send(crate::watching::WatcherCommand::Refresh { path: root.clone(), completion })
            .unwrap();
        answered
    };

    assert_eq!(
        tokio::time::timeout(HANG_GUARD, refresh())
            .await
            .expect("the refresh whose pass panicked is answered")
            .unwrap(),
        Ok(())
    );
    let crate::FolderScanStatus::Failed { error } = root_scan_status(&test.database).await else {
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

    assert_eq!(
        tokio::time::timeout(HANG_GUARD, refresh())
            .await
            .expect("the next refresh is answered")
            .unwrap(),
        Ok(())
    );
    assert!(matches!(root_scan_status(&test.database).await, crate::FolderScanStatus::Complete));

    let (completion, stopped) = std::sync::mpsc::channel();
    commands.send(crate::watching::WatcherCommand::Shutdown { completion }).unwrap();
    tokio::task::spawn_blocking(move || {
        stopped.recv().unwrap();
        coordinator.join().unwrap();
    })
    .await
    .unwrap();
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
        test.scan_with(Arc::new(crate::file_tag_snapshot::LoftyFileTagReader), reader.clone());

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
) -> (crate::ImportEvents, Result<(), crate::ImportError>) {
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
            relative_folder: bae_db::RelativeFolder::test("old-key"),
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

#[tokio::test]
async fn rescan_missing_root_fails_and_preserves_previous_candidates() {
    let test = setup_import_service().await;
    let root = test.temp.path().join("missing-root");
    let (mut events, result) = rescan_seeded_root(&test, &root).await;
    assert!(result.is_err());

    let failed = loop {
        match events.try_recv().expect("the failed pass announced its failure") {
            crate::ImportEvent::Scan(ScanEvent::FolderScanStatusChanged {
                status:
                    crate::WatchedFolderScanStatus {
                        status: crate::FolderScanStatus::Failed { error },
                        ..
                    },
            }) => break error,
            crate::ImportEvent::Scan(ScanEvent::CandidateRemoved { candidate_key }) => {
                panic!("missing root removed {candidate_key}")
            }
            _ => {}
        }
    };
    // The reported failure names the root that could not be read. Its reason is
    // the OS's own wording for an absent path ("No such file or directory" on
    // Unix, "The system cannot find the path specified" on Windows), so the
    // root — the part core promises — is what this asserts on.
    assert!(failed.contains(&root.to_string_lossy().into_owned()), "{failed}");
    assert_eq!(stored_invalid_candidates(&test, &root).await, 1);
}

#[tokio::test]
async fn rescan_non_directory_root_keeps_previous_candidates() {
    let test = setup_import_service().await;
    let root = test.temp.path().join("not-a-directory");
    std::fs::write(&root, b"not a directory").unwrap();
    let (mut events, result) = rescan_seeded_root(&test, &root).await;
    assert!(result.is_err(), "a non-directory root must fail its scan");

    loop {
        match events.try_recv().expect("the failed pass announced its failure") {
            crate::ImportEvent::Scan(ScanEvent::FolderScanStatusChanged {
                status:
                    crate::WatchedFolderScanStatus {
                        status: crate::FolderScanStatus::Failed { error },
                        ..
                    },
            }) => {
                assert!(error.to_lowercase().contains("not a directory"), "got: {error}");
                break;
            }
            crate::ImportEvent::Scan(ScanEvent::FolderScanStatusChanged { .. }) => {}
            event => panic!("expected scan status, got {event:?}"),
        }
    }
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
