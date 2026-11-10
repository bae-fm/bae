//! The fixtures the scan passes' tests share: scan dependencies over a test
//! service's library, a folder read through them, a listing that records what
//! it read, and the events a pass announces; and a pass's start.

#![expect(
    clippy::disallowed_methods,
    reason = "test code: what it spawns ends with the test, which has no crash reporter to report to"
)]

use super::*;
use crate::service::tests::{setup_import_service, TestService};
use crate::watching::WatchReport;
use bae_util::test_channels::HANG_GUARD;

/// Scan dependencies over the test service's library, with a folder-state
/// commit lock of their own.
pub(crate) fn test_scan_services(
    test: &TestService,
    event_tx: crate::ImportEventBus,
    folder_watcher: Arc<FolderWatcher>,
    file_tags: Arc<dyn crate::file_tag_reader::FileTagReader>,
    directories: Arc<dyn crate::folder_scanner::DirectoryReader>,
) -> ScanServices {
    ScanServices::new(
        test.import_services(event_tx, file_tags, directories, crate::FolderStateCommit::new()),
        folder_watcher,
    )
}

/// What a test reads a folder with. `_fs_rx` keeps the folder watcher's sender
/// connected while the scan runs.
pub(crate) struct TestScan {
    pub(crate) services: ScanServices,
    pub(crate) cancellation: crate::folder_scanner::ScanCancellation,
    pub(crate) _fs_rx: tokio::sync::mpsc::UnboundedReceiver<WatchReport>,
}

impl TestScan {
    /// Read `root` and reconcile what it holds against what is stored; a
    /// scan that did not finish is the error, as it ended.
    pub(crate) async fn rescan(&self, root: &Path) -> Result<(), ScanEnded> {
        match rescan_and_reconcile(root, &self.services, &self.cancellation).await {
            ScanEnded::Finished => Ok(()),
            failed => Err(failed),
        }
    }
}

/// Lists folders as the OS does, and remembers every folder it was asked for.
#[derive(Default)]
pub(crate) struct RecordingListing {
    pub(crate) read: std::sync::Mutex<Vec<PathBuf>>,
}

impl RecordingListing {
    pub(crate) fn take(&self) -> Vec<PathBuf> {
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
pub(crate) fn write_disc(folder: &Path, count: usize) {
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
pub(crate) struct DecisionFixture {
    pub(crate) test: TestService,
    pub(crate) scan: TestScan,
    pub(crate) listing: Arc<RecordingListing>,
    pub(crate) root: PathBuf,
}

impl DecisionFixture {
    pub(crate) async fn new(discs: [&str; 2], tracks: usize) -> Self {
        let test = setup_import_service().await;
        let root = test.temp.path().join("music");
        for disc in discs {
            write_disc(&root.join("Artist").join("Album").join(disc), tracks);
        }
        write_disc(&root.join("Other").join("Record"), 1);
        let root = crate::tables::watched_path::canonical_absolute_root(&root).unwrap();
        test.tables().add_watched_import_folder(&root).await.unwrap();
        let listing = Arc::new(RecordingListing::default());
        let (scan, _) =
            test.scan_with(Arc::new(crate::file_tag_reader::LoftyFileTagReader), listing.clone());
        scan.rescan(&root).await.unwrap();
        listing.take();
        Self { test, scan, listing, root }
    }

    pub(crate) fn album_key(&self) -> crate::FolderReleaseDecisionKey {
        crate::FolderReleaseDecisionKey {
            watched_folder_path: self.root.clone(),
            relative_folder: crate::RelativeFolder::test("Artist/Album"),
        }
    }

    pub(crate) async fn stored_paths(&self) -> Vec<String> {
        let mut paths: Vec<String> = self
            .test
            .tables()
            .load_folder_scan_items(&self.root)
            .await
            .unwrap()
            .iter()
            .map(|item| item.relative_folder().text())
            .collect();
        paths.sort();
        paths
    }

    /// Read `Artist/Album` as `decision`, watching the import list the whole
    /// time: every list the live query delivered, up to the first one showing
    /// `settled`.
    pub(crate) async fn decide_watching_the_list(
        &self,
        decision: crate::FolderReleaseDecision,
        settled: &[&str],
    ) -> Vec<Vec<String>> {
        let tables = self.test.tables().clone();
        let mut live = tables.subscribe_import_list(crate::ImportListRequest {
            list: self
                .test
                .tables()
                .open_import_list(crate::ImportListView {
                    order: crate::ImportListOrder::PathAscending,
                    ..crate::ImportListView::default()
                })
                .await
                .unwrap(),
            windows: [bae_store::LibraryPageWindow { offset: 0, limit: 50 }].into_iter().collect(),
        });
        let (seen_tx, mut seen_rx) = tokio::sync::mpsc::unbounded_channel();
        let watcher = tokio::spawn(async move {
            loop {
                let projection = live.next().await.expect("the query stays open").result.unwrap();
                let names: Vec<String> = projection
                    .windows
                    .iter()
                    .flat_map(|window| &window.rows)
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

        change_folder_reading(
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

/// The two fixture tracks that carry real Vorbis comments — an album, its
/// artist, and a title per track.
pub(crate) const TAGGED_FLAC_FIXTURES: [&str; 2] = [
    "02 Test Artist - Track Two (White Noise).flac",
    "03 Test Artist - Track Three (Brown Noise).flac",
];

#[tokio::test]
async fn cancelled_scan_task_does_not_begin_a_durable_generation() {
    let test = setup_import_service().await;
    let tables = test.tables().clone();
    let root = test.temp.path().join("watched");
    std::fs::create_dir(&root).unwrap();
    tables.add_watched_import_folder(&root).await.unwrap();
    let (watch_tx, _watch_rx) = tokio::sync::mpsc::unbounded_channel();
    let watcher = Arc::new(FolderWatcher::new(watch_tx, bae_diagnostics::Diagnostics::noop()));
    let (completion_tx, mut completion_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut events = test.event_tx().every_event();

    let scan = spawn_root_scan(
        &tokio::runtime::Handle::current(),
        1,
        root,
        test_scan_services(
            &test,
            test.event_tx().clone(),
            watcher,
            std::sync::Arc::new(crate::file_tag_reader::LoftyFileTagReader),
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

/// The candidates a pass told anyone about, in the order it did.
pub(crate) fn announced_candidates(events: &mut crate::ImportEvents) -> Vec<String> {
    let mut announced = Vec::new();
    while let Ok(event) = events.try_recv() {
        match event {
            crate::ImportEvent::Scan(ScanEvent::FolderCandidate { candidate, .. })
            | crate::ImportEvent::Scan(ScanEvent::CandidateDiscovered { candidate, .. }) => {
                announced.push(candidate.relative_folder.text())
            }
            crate::ImportEvent::Scan(ScanEvent::CandidateRemoved { candidate_key }) => {
                announced.push(format!("removed {candidate_key}"))
            }
            _ => {}
        }
    }
    announced
}
