/// Lists folders as the OS does, and remembers every folder it was asked for.
#[derive(Default)]
struct RecordingListing {
    read: std::sync::Mutex<Vec<PathBuf>>,
}

impl RecordingListing {
    fn take(&self) -> Vec<PathBuf> {
        std::mem::take(&mut *self.read.lock().unwrap())
    }
}

impl crate::import::folder_scanner::DirectoryReader for RecordingListing {
    fn read(
        &self,
        root: &Path,
        directory: &Path,
        cancellation: &crate::import::folder_scanner::ScanCancellation,
    ) -> Result<crate::import::folder_scanner::DirectoryListing, crate::import::folder_scanner::FolderScanError>
    {
        self.read.lock().unwrap().push(root.join(directory));
        crate::import::folder_scanner::OsDirectoryReader.read(root, directory, cancellation)
    }
}

/// `count` playable FLAC files in `folder`, named `01.flac` onwards.
fn write_disc(folder: &Path, count: usize) {
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
struct DecisionFixture {
    test: TestService,
    scan: TestScan,
    listing: Arc<RecordingListing>,
    root: PathBuf,
    _events: tokio::sync::broadcast::Receiver<crate::import::handle::ImportEvent>,
}

impl DecisionFixture {
    async fn new(discs: [&str; 2], tracks: usize) -> Self {
        let test = setup_import_service().await;
        let root = test.temp.path().join("music");
        for disc in discs {
            write_disc(&root.join("Artist").join("Album").join(disc), tracks);
        }
        write_disc(&root.join("Other").join("Record"), 1);
        let root = PathBuf::from(
            crate::import::watched_folder::canonical_absolute_root(&root.to_string_lossy())
                .unwrap(),
        );
        test.service
            .library_manager
            .add_watched_import_folder(&root.to_string_lossy())
            .await
            .unwrap();
        let listing = Arc::new(RecordingListing::default());
        let (scan, events) = test.scan_with(
            Arc::new(crate::import::file_tag_snapshot::LoftyFileTagReader),
            listing.clone(),
        );
        scan.rescan(&root).await.unwrap();
        listing.take();
        Self {
            test,
            scan,
            listing,
            root,
            _events: events,
        }
    }

    fn manager(&self) -> &LibraryManager {
        &self.test.service.library_manager
    }

    fn album_key(&self) -> crate::import::FolderReleaseDecisionKey {
        crate::import::FolderReleaseDecisionKey {
            watched_folder_path: self.root.to_string_lossy().into_owned(),
            relative_folder_path: "Artist/Album".to_string(),
        }
    }

    async fn stored_paths(&self) -> Vec<String> {
        let mut paths: Vec<String> = self
            .manager()
            .load_folder_scan_items(&self.root.to_string_lossy())
            .await
            .unwrap()
            .iter()
            .filter_map(|item| item.display_path().map(str::to_string))
            .collect();
        paths.sort();
        paths
    }

    /// Read `Artist/Album` as `decision`, watching the import list the whole
    /// time: every list the live query delivered, up to the first one showing
    /// `settled`.
    async fn decide_watching_the_list(
        &self,
        decision: crate::import::FolderReleaseDecision,
        settled: &[&str],
    ) -> Vec<Vec<String>> {
        let mut live = self
            .manager()
            .subscribe_import_list(crate::import::ImportListRequest {
                view: crate::import::ImportListView {
                    order: crate::import::ImportListOrder::PathAscending,
                    ..crate::import::ImportListView::default()
                },
                windows: [crate::library::LibraryPageWindow {
                    offset: 0,
                    limit: 50,
                }]
                .into_iter()
                .collect(),
                upload_standing: Default::default(),
            });
        let (seen_tx, mut seen_rx) = tokio::sync::mpsc::unbounded_channel();
        let watcher = tokio::spawn(async move {
            loop {
                let projection = live.next().await.into_result().unwrap();
                let names: Vec<String> = projection
                    .windows
                    .iter()
                    .flat_map(|window| &window.items)
                    .filter_map(|item| match item {
                        crate::import::ImportListItem::Candidate { row, .. } => {
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
        while !settled
            .iter()
            .all(|name| seen.last().unwrap().contains(name))
        {
            seen.push(
                tokio::time::timeout(Duration::from_secs(5), seen_rx.recv())
                    .await
                    .expect("the list shows the new reading")
                    .expect("the list is still delivering"),
            );
        }
        watcher.abort();
        seen
    }
}

/// Every list the live query delivered shows the folder one way or the
/// other: the combined release, or both discs. None shows neither.
fn assert_never_neither(seen: &[Vec<String>], combined: &str, separate: [&str; 2]) {
    for names in seen {
        let shows_combined = names.iter().any(|name| name == combined);
        let shows_separate = separate
            .iter()
            .all(|disc| names.iter().any(|name| name == disc));
        assert!(
            shows_combined || shows_separate,
            "the list showed the folder as neither reading: {names:?} (all delivered: {seen:?})"
        );
    }
}

/// Keeping a folder's discs separate stores the two disc candidates and
/// drops the combined one in one write — the list never shows the folder
/// with neither — and reads nothing outside the folder's own branch.
#[tokio::test]
async fn keeping_discs_separate_trades_the_combined_row_for_both_discs_at_once() {
    let fixture = DecisionFixture::new(["Disc 1", "Disc 2"], 3).await;
    assert_eq!(
        fixture.stored_paths().await,
        vec!["Artist/Album".to_string(), "Other/Record".to_string()],
        "numbered discs are read as one release"
    );
    let other = fixture.root.join("Other").join("Record");
    let sibling_before = fixture
        .manager()
        .load_folder_scan_item(&other.to_string_lossy())
        .await
        .unwrap();

    let seen = fixture
        .decide_watching_the_list(
            crate::import::FolderReleaseDecision::KeepAsSeparateReleases,
            &["Disc 1", "Disc 2"],
        )
        .await;

    assert_never_neither(&seen, "Album", ["Disc 1", "Disc 2"]);
    assert_eq!(
        fixture.stored_paths().await,
        vec![
            "Artist/Album/Disc 1".to_string(),
            "Artist/Album/Disc 2".to_string(),
            "Other/Record".to_string(),
        ]
    );
    assert_eq!(
        fixture
            .manager()
            .load_folder_release_decisions(&fixture.root.to_string_lossy())
            .await
            .unwrap()
            .get("Artist/Album")
            .map(|reading| (reading.decision, reading.author)),
        Some((
            crate::import::FolderReleaseDecision::KeepAsSeparateReleases,
            crate::import::folder_scanner::FolderReleaseDecisionAuthor::User,
        ))
    );
    let read = fixture.listing.take();
    assert!(!read.is_empty());
    assert!(
        read.iter().all(|folder| folder.starts_with(fixture.root.join("Artist"))),
        "only the decided folder's branch is read again: {read:?}"
    );
    assert_eq!(
        fixture
            .manager()
            .load_folder_scan_item(&other.to_string_lossy())
            .await
            .unwrap(),
        sibling_before,
        "the sibling folder's entry is untouched"
    );
}

/// Combining a folder's parts trades both part rows for the combined one in
/// one write, the same way.
#[tokio::test]
async fn combining_parts_trades_both_rows_for_the_combined_one_at_once() {
    let fixture = DecisionFixture::new(["Live A", "Live B"], 3).await;
    assert_eq!(
        fixture.stored_paths().await,
        vec![
            "Artist/Album/Live A".to_string(),
            "Artist/Album/Live B".to_string(),
            "Other/Record".to_string(),
        ],
        "unnumbered parts are read as separate releases"
    );

    let seen = fixture
        .decide_watching_the_list(
            crate::import::FolderReleaseDecision::CombineAsOneRelease,
            &["Album"],
        )
        .await;

    assert_never_neither(&seen, "Album", ["Live A", "Live B"]);
    assert_eq!(
        fixture.stored_paths().await,
        vec!["Artist/Album".to_string(), "Other/Record".to_string()]
    );
    let read = fixture.listing.take();
    assert!(
        read.iter().all(|folder| folder.starts_with(fixture.root.join("Artist"))),
        "only the decided folder's branch is read again: {read:?}"
    );
}

/// A root whose entries were written after the reading began is not the root
/// the reading describes, so the decision fails and stores nothing.
#[tokio::test]
async fn a_folder_reading_refuses_a_root_that_moved_while_it_read() {
    let fixture = DecisionFixture::new(["Disc 1", "Disc 2"], 1).await;
    let before = fixture.stored_paths().await;
    let decisions_before = fixture
        .manager()
        .load_folder_release_decisions(&fixture.root.to_string_lossy())
        .await
        .unwrap()
        .get("Artist/Album")
        .cloned();

    // Hold the commit lock, so the reading waits at it with its walk done,
    // and move the root's generation under it the way a pass over the root
    // would.
    let commit = fixture.scan.services.services.folder_state_commit.clone();
    let held = commit.lock().await;
    let reading = {
        let root = fixture.root.clone();
        let key = fixture.album_key();
        let services = fixture.scan.services.clone();
        let cancellation = fixture.scan.cancellation.clone();
        tokio::spawn(async move {
            ImportService::change_folder_reading(
                &root,
                &(key, crate::import::FolderReleaseDecision::KeepAsSeparateReleases),
                &services,
                &cancellation,
            )
            .await
        })
    };
    tokio::time::timeout(Duration::from_secs(5), async {
        while fixture.listing.take().is_empty() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("the reading started walking");
    fixture
        .manager()
        .begin_folder_scan(&fixture.root.to_string_lossy())
        .await
        .unwrap();
    drop(held);

    let error = reading.await.unwrap().unwrap_err();
    assert!(
        error.to_string().contains("changed while it was being read again"),
        "{error}"
    );
    assert_eq!(fixture.stored_paths().await, before);
    assert_eq!(
        fixture
            .manager()
            .load_folder_release_decisions(&fixture.root.to_string_lossy())
            .await
            .unwrap()
            .get("Artist/Album")
            .cloned(),
        decisions_before
    );
}

/// A change inside a folder under the root is a reading of that folder; the
/// root is read whole only where the change reaches the root's own release.
#[test]
fn a_change_reads_the_folder_it_is_in_and_the_root_only_where_it_must() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_disc(&root.join("Artist").join("Album"), 1);
    std::fs::write(root.join("notes.txt"), b"notes").unwrap();
    let folders = |names: &[&str]| {
        RootChange::Folders(names.iter().map(|name| name.to_string()).collect())
    };
    let change = |paths: &[PathBuf], holds: bool| {
        let paths: Vec<&Path> = paths.iter().map(PathBuf::as_path).collect();
        root_change(root, &paths, holds)
    };

    assert_eq!(
        change(&[root.join("Artist/Album/02.flac")], false),
        folders(&["Artist"]),
        "a file inside a folder reads that folder"
    );
    assert_eq!(
        change(&[root.join("Gone/Album/01.flac"), root.join("Gone")], false),
        folders(&["Gone"]),
        "a folder that went away is read as empty"
    );
    assert_eq!(
        change(&[root.join("Artist/.DS_Store"), root.join(".hidden")], false),
        folders(&[]),
        "a hidden entry is never listed, so it changes nothing"
    );
    assert_eq!(
        change(&[root.join("notes.txt")], false),
        folders(&[]),
        "a file beside no tracks at the root belongs to no release"
    );
    assert_eq!(
        change(&[root.join("01.flac")], false),
        RootChange::WholeRoot,
        "audio directly in the root is the root's own release"
    );
    assert_eq!(
        change(&[root.join("Artist/Album/02.flac")], true),
        RootChange::WholeRoot,
        "a root with tracks of its own takes in every folder beside them"
    );
    assert_eq!(
        change(&[root.to_path_buf()], false),
        RootChange::WholeRoot,
        "the root itself changing is the whole root"
    );
}

/// The cheap check of a network folder names what moved: a nested folder
/// that was written to, and a folder that came directly under the root.
#[tokio::test]
async fn the_network_check_names_the_folders_that_moved() {
    let fixture = DecisionFixture::new(["Disc 1", "Disc 2"], 1).await;
    let root = &fixture.root;
    let recorded = fixture
        .manager()
        .load_folder_scan_directories(&root.to_string_lossy())
        .await
        .unwrap();
    assert_eq!(network_changes(root, &recorded), Some(Vec::new()));

    write_disc(&root.join("Artist/Album/Disc 1"), 2);
    write_disc(&root.join("New/Record"), 1);
    let mut changes = network_changes(root, &recorded).expect("the record answers");
    changes.sort();
    let changes: Vec<&Path> = changes.iter().map(PathBuf::as_path).collect();
    assert_eq!(
        root_change(root, &changes, false),
        RootChange::Folders(
            ["Artist".to_string(), "New".to_string()]
                .into_iter()
                .collect()
        )
    );
}

/// A folder whose contents changed is read again alone, in a write of its
/// own: a new track lands, a folder that went away takes its entries with
/// it, a new folder brings its own, and nothing else under the root is read.
#[tokio::test]
async fn changed_folders_are_read_again_and_nothing_beside_them() {
    let fixture = DecisionFixture::new(["Disc 1", "Disc 2"], 1).await;
    let root = &fixture.root;
    let album = root.join("Artist").join("Album");
    let album_before = fixture
        .manager()
        .load_folder_scan_item(&album.to_string_lossy())
        .await
        .unwrap();

    write_disc(&root.join("Other").join("Record"), 2);
    write_disc(&root.join("New").join("Record"), 1);
    let folders = ["New".to_string(), "Other".to_string()].into_iter().collect();
    ImportService::read_changed_folders(
        root,
        &folders,
        &fixture.scan.services,
        &fixture.scan.cancellation,
    )
    .await;

    assert_eq!(
        fixture.stored_paths().await,
        vec![
            "Artist/Album".to_string(),
            "New/Record".to_string(),
            "Other/Record".to_string(),
        ]
    );
    let other = fixture
        .manager()
        .load_folder_scan_item(&root.join("Other").join("Record").to_string_lossy())
        .await
        .unwrap();
    let Some(ScanItem::Valid(other)) = other else {
        panic!("the changed folder is a valid candidate: {other:?}");
    };
    assert_eq!(other.track_count(), 2, "the new track landed");
    let read = fixture.listing.take();
    assert!(
        read.iter()
            .all(|folder| folder.starts_with(root.join("New"))
                || folder.starts_with(root.join("Other"))),
        "only the changed folders are read: {read:?}"
    );
    assert_eq!(
        fixture
            .manager()
            .load_folder_scan_item(&album.to_string_lossy())
            .await
            .unwrap(),
        album_before,
        "the folder beside them is untouched"
    );

    std::fs::remove_dir_all(root.join("New")).unwrap();
    ImportService::read_changed_folders(
        root,
        &["New".to_string()].into_iter().collect(),
        &fixture.scan.services,
        &fixture.scan.cancellation,
    )
    .await;
    assert_eq!(
        fixture.stored_paths().await,
        vec!["Artist/Album".to_string(), "Other/Record".to_string()],
        "a folder that went away takes its entries with it"
    );
}
