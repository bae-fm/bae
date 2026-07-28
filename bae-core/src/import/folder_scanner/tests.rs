use super::*;
use crate::cue_flac::CueTrackMode;

/// Valid FLAC fixture bytes.
fn fake_flac() -> Vec<u8> {
    std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/flac/01 Test Track 1.flac"
    ))
    .expect("read FLAC fixture")
}

/// One audio-role entry, for the hand-built `CategorizedFiles` in the
/// content-hash tests.
fn audio_entry(path: &str, relative_path: &str, size: u64) -> CandidateFile {
    CandidateFile {
        file: ScannedFile::new(PathBuf::from(path), relative_path.to_string(), size),
        role: FileRole::Audio,
        proposed_audio: true,
    }
}

/// The final projected scan items for `root`. The callback is an update stream:
/// a later item with the same key can add a proven combine action or replace
/// provisional candidates with an unresolved boundary.
fn scan_items(root: impl Into<PathBuf>) -> Vec<ScanItem> {
    scan_projected_items_with_decisions(root.into(), FolderReleaseDecisions::default())
}

fn scan_projected_items_with_decisions(
    root: PathBuf,
    decisions: FolderReleaseDecisions,
) -> Vec<ScanItem> {
    let watched_folder =
        crate::import::WatchedFolder::from_path(root.to_string_lossy().into_owned());
    let mut state = crate::import::handle::ImportCandidateState::default();
    scan_for_candidates_with_decisions(root, &StoredCandidateEdits::none(), &decisions, |item| {
        if !matches!(item, ScanItem::Discovered(_)) {
            state.apply_scan_item(item, false, false);
        }
    })
    .unwrap();
    let snapshot = state.snapshot(vec![watched_folder]);
    snapshot
        .folder_candidates
        .into_iter()
        .map(|candidate| ScanItem::Valid(candidate.candidate))
        .chain(
            snapshot
                .invalid_candidates
                .into_iter()
                .map(ScanItem::Invalid),
        )
        .chain(snapshot.boundaries.into_iter().map(ScanItem::Boundary))
        .collect()
}

/// Only the valid `FolderCandidate`s for `root` — the shape most scanner
/// tests assert against (counts, paths, categorized files).
fn scan_valid(root: impl Into<PathBuf>) -> Vec<FolderCandidate> {
    scan_items(root)
        .into_iter()
        .filter_map(|item| match item {
            ScanItem::Valid(c) => Some(c),
            ScanItem::Invalid(_) => None,
            ScanItem::Discovered(_) | ScanItem::Boundary(_) => None,
        })
        .collect()
}

fn scan_for_candidates_with_decisions_collect(
    root: PathBuf,
    decisions: FolderReleaseDecisions,
) -> Vec<ScanItem> {
    let mut items = Vec::new();
    scan_for_candidates_with_decisions(root, &StoredCandidateEdits::none(), &decisions, |item| {
        if !matches!(item, ScanItem::Discovered(_)) {
            items.push(item);
        }
    })
    .unwrap();
    items
}

#[test]
fn names_do_not_combine_audio_bearing_children() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Wrapped Release");
    for child in ["CD1", "CD2"] {
        let child = root.join(child);
        std::fs::create_dir_all(&child).unwrap();
        std::fs::write(child.join("track.flac"), fake_flac()).unwrap();
    }

    let candidates = scan_valid(&root);

    assert_eq!(candidates.len(), 2);
    assert!(candidates
        .iter()
        .all(|candidate| candidate.scope == ReleaseFileScope::Direct));
}

#[test]
fn direct_audio_and_audio_bearing_child_are_distinct_approximations() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Collection");
    let child = root.join("Nested Release");
    std::fs::create_dir_all(&child).unwrap();
    std::fs::write(root.join("loose.flac"), fake_flac()).unwrap();
    std::fs::write(child.join("track.flac"), fake_flac()).unwrap();

    let candidates = scan_valid(&root);

    assert_eq!(candidates.len(), 2);
    let root_candidate = candidates
        .iter()
        .find(|candidate| candidate.path == root)
        .expect("direct files produce their own release approximation");
    assert_eq!(
        root_candidate.path.to_string_lossy(),
        root.to_string_lossy(),
        "the watched-root candidate key preserves the registered root path"
    );
    assert_eq!(
        root_candidate
            .files
            .release_files()
            .map(|file| file.relative_path.as_str())
            .collect::<Vec<_>>(),
        vec!["loose.flac"],
    );
}

#[test]
fn file_free_group_emits_an_actionable_release_before_later_child_finishes() {
    struct BlockingReader {
        blocked: PathBuf,
        entered: std::sync::mpsc::Sender<()>,
        gate: std::sync::Arc<(std::sync::Mutex<bool>, std::sync::Condvar)>,
    }

    impl DirectoryReader for BlockingReader {
        fn read(
            &self,
            root: &Path,
            directory: &Path,
            cancellation: &ScanCancellation,
        ) -> Result<DirectoryListing, FolderScanError> {
            if directory == self.blocked {
                self.entered.send(()).expect("announce blocked directory");
                let (lock, condition) = &*self.gate;
                let mut open = lock.lock().unwrap();
                while !*open {
                    open = condition.wait(open).unwrap();
                }
            }
            OsDirectoryReader.read(root, directory, cancellation)
        }
    }

    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Queue");
    let group = root.join("Collection");
    let first = group.join("Release 01");
    let later = group.join("Release 99");
    std::fs::create_dir_all(&first).unwrap();
    std::fs::create_dir_all(&later).unwrap();
    std::fs::write(first.join("track.flac"), fake_flac()).unwrap();
    std::fs::write(later.join("track.flac"), fake_flac()).unwrap();

    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (item_tx, item_rx) = std::sync::mpsc::channel();
    let gate = std::sync::Arc::new((std::sync::Mutex::new(false), std::sync::Condvar::new()));
    let thread_gate = gate.clone();
    let scan_root = root.clone();
    let scan = std::thread::spawn(move || {
        let reader = BlockingReader {
            blocked: PathBuf::from("Collection/Release 99"),
            entered: entered_tx,
            gate: thread_gate,
        };
        scan_for_candidates_with_reader(
            &reader,
            scan_root,
            &StoredCandidateEdits::none(),
            &FolderReleaseDecisions::default(),
            |item| item_tx.send(item).expect("receive scan item"),
        )
    });

    let first_item = item_rx
        .recv_timeout(std::time::Duration::from_secs(2))
        .expect("first release emits");
    assert!(matches!(
        first_item,
        ScanItem::Discovered(candidate) if candidate.path == first
    ));
    entered_rx
        .recv_timeout(std::time::Duration::from_secs(2))
        .expect("later directory is suspended");
    assert!(
        item_rx
            .try_iter()
            .any(|item| matches!(item, ScanItem::Valid(candidate) if candidate.path == first)),
        "the immediate listing proves the file-free group cannot own shared release files"
    );

    let (lock, condition) = &*gate;
    *lock.lock().unwrap() = true;
    condition.notify_all();
    scan.join().unwrap().unwrap();
}

#[test]
fn shared_group_files_keep_descendants_non_actionable_until_boundary_is_complete() {
    struct BlockingReader {
        entered: std::sync::mpsc::Sender<()>,
        gate: std::sync::Arc<(std::sync::Mutex<bool>, std::sync::Condvar)>,
    }

    impl DirectoryReader for BlockingReader {
        fn read(
            &self,
            root: &Path,
            directory: &Path,
            cancellation: &ScanCancellation,
        ) -> Result<DirectoryListing, FolderScanError> {
            if directory == Path::new("Group/Release 99") {
                self.entered.send(()).expect("announce blocked directory");
                let (lock, condition) = &*self.gate;
                let mut open = lock.lock().unwrap();
                while !*open {
                    open = condition.wait(open).unwrap();
                }
            }
            OsDirectoryReader.read(root, directory, cancellation)
        }
    }

    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Queue");
    let group = root.join("Group");
    std::fs::create_dir_all(&group).unwrap();
    std::fs::write(group.join("cover.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();
    for release in ["Release 01", "Release 02", "Release 99"] {
        let release = group.join(release);
        std::fs::create_dir_all(&release).unwrap();
        std::fs::write(release.join("track.flac"), fake_flac()).unwrap();
    }

    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (item_tx, item_rx) = std::sync::mpsc::channel();
    let gate = std::sync::Arc::new((std::sync::Mutex::new(false), std::sync::Condvar::new()));
    let scan_gate = gate.clone();
    let scan_root = root.clone();
    let scan = std::thread::spawn(move || {
        scan_for_candidates_with_reader(
            &BlockingReader {
                entered: entered_tx,
                gate: scan_gate,
            },
            scan_root,
            &StoredCandidateEdits::none(),
            &FolderReleaseDecisions::default(),
            |item| item_tx.send(item).expect("receive scan item"),
        )
    });

    entered_rx
        .recv_timeout(std::time::Duration::from_secs(2))
        .expect("final sibling is suspended");
    let available: Vec<_> = item_rx.try_iter().collect();
    assert!(
        !available
            .iter()
            .any(|item| matches!(item, ScanItem::Valid(_))),
        "an unresolved wrapper must not start identification for provisional descendants"
    );

    let (lock, condition) = &*gate;
    *lock.lock().unwrap() = true;
    condition.notify_all();
    scan.join().unwrap().unwrap();
    assert!(item_rx
        .try_iter()
        .any(|item| matches!(item, ScanItem::Boundary(boundary) if boundary.name == "Group")));
}

#[test]
fn direct_audio_parent_keeps_descendant_non_actionable_until_boundary_is_complete() {
    struct BlockingReader {
        entered: std::sync::mpsc::Sender<()>,
        gate: std::sync::Arc<(std::sync::Mutex<bool>, std::sync::Condvar)>,
    }

    impl DirectoryReader for BlockingReader {
        fn read(
            &self,
            root: &Path,
            directory: &Path,
            cancellation: &ScanCancellation,
        ) -> Result<DirectoryListing, FolderScanError> {
            if directory == Path::new("Group/Release 99") {
                self.entered.send(()).expect("announce blocked directory");
                let (lock, condition) = &*self.gate;
                let mut open = lock.lock().unwrap();
                while !*open {
                    open = condition.wait(open).unwrap();
                }
            }
            OsDirectoryReader.read(root, directory, cancellation)
        }
    }

    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Queue");
    let group = root.join("Group");
    std::fs::create_dir_all(group.join("Release 01")).unwrap();
    std::fs::create_dir_all(group.join("Release 99")).unwrap();
    for path in [
        group.join("loose.flac"),
        group.join("Release 01/track.flac"),
        group.join("Release 99/track.flac"),
    ] {
        std::fs::write(path, fake_flac()).unwrap();
    }

    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (item_tx, item_rx) = std::sync::mpsc::channel();
    let gate = std::sync::Arc::new((std::sync::Mutex::new(false), std::sync::Condvar::new()));
    let scan_gate = gate.clone();
    let scan_root = root.clone();
    let scan = std::thread::spawn(move || {
        scan_for_candidates_with_reader(
            &BlockingReader {
                entered: entered_tx,
                gate: scan_gate,
            },
            scan_root,
            &StoredCandidateEdits::none(),
            &FolderReleaseDecisions::default(),
            |item| item_tx.send(item).expect("receive scan item"),
        )
    });

    entered_rx
        .recv_timeout(std::time::Duration::from_secs(2))
        .expect("final sibling is suspended");
    assert!(!item_rx
        .try_iter()
        .any(|item| matches!(item, ScanItem::Valid(_))));

    let (lock, condition) = &*gate;
    *lock.lock().unwrap() = true;
    condition.notify_all();
    scan.join().unwrap().unwrap();
    assert!(item_rx
        .try_iter()
        .any(|item| matches!(item, ScanItem::Boundary(boundary) if boundary.name == "Group")));
}

#[test]
fn discography_and_multidisc_shapes_follow_folder_structure_only() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Media");
    for release in [
        "Collective/Studio Albums/1990 - First",
        "Collective/Studio Albums/1992 - Second",
        "Solo Artist/1971 - Ordinary",
        "Solo Artist/1973 - Box/CD1",
        "Solo Artist/1973 - Box/CD2",
    ] {
        let release = root.join(release);
        std::fs::create_dir_all(&release).unwrap();
        std::fs::write(release.join("track.flac"), fake_flac()).unwrap();
    }

    let items = scan_items(&root);
    let mut candidates = HashMap::new();
    for item in &items {
        if let ScanItem::Valid(candidate) = item {
            candidates.insert(candidate.display_path.as_str(), candidate);
        }
    }
    assert!(candidates.contains_key("Solo Artist/1971 - Ordinary"));
    assert!(candidates.contains_key("Collective/Studio Albums/1990 - First"));
    assert!(candidates.contains_key("Collective/Studio Albums/1992 - Second"));
    assert!(candidates.contains_key("Solo Artist/1973 - Box/CD1"));
    assert!(candidates.contains_key("Solo Artist/1973 - Box/CD2"));
    assert!(!candidates.contains_key("Collective"));
    assert!(!candidates.contains_key("Collective/Studio Albums"));

    assert!(!items
        .iter()
        .any(|item| matches!(item, ScanItem::Boundary(_))));
    for candidate in candidates.values() {
        let expected = if candidate.display_path.starts_with("Collective/") {
            "Collective/Studio Albums"
        } else if candidate.display_path.contains("1973 - Box/") {
            "Solo Artist/1973 - Box"
        } else {
            assert_eq!(candidate.display_path, "Solo Artist/1971 - Ordinary");
            "Solo Artist"
        };
        assert_eq!(
            candidate
                .combine_ancestor_key
                .as_ref()
                .map(|key| key.relative_folder_path.as_str()),
            Some(expected)
        );
    }

    let combined = scan_for_candidates_with_decisions_collect(
        root,
        FolderReleaseDecisions::new(HashMap::from([(
            "Solo Artist/1973 - Box".to_string(),
            FolderReleaseDecision::CombineAsOneRelease,
        )])),
    );
    assert!(combined.iter().any(
        |item| matches!(item, ScanItem::Valid(candidate) if candidate.display_path == "Solo Artist/1973 - Box")
    ));
}

#[test]
fn cancelled_scan_stops_before_the_next_directory_read() {
    #[derive(Default)]
    struct RecordingReader {
        reads: std::sync::Mutex<Vec<PathBuf>>,
    }

    impl DirectoryReader for RecordingReader {
        fn read(
            &self,
            root: &Path,
            directory: &Path,
            cancellation: &ScanCancellation,
        ) -> Result<DirectoryListing, FolderScanError> {
            self.reads.lock().unwrap().push(directory.to_path_buf());
            OsDirectoryReader.read(root, directory, cancellation)
        }
    }

    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Queue");
    for child in ["Release 01", "Release 02"] {
        let child = root.join(child);
        std::fs::create_dir_all(&child).unwrap();
        std::fs::write(child.join("track.flac"), fake_flac()).unwrap();
    }
    let reader = RecordingReader::default();
    let cancellation = ScanCancellation::new();

    let result = scan_for_candidates_with_reader_cancellable(
        &reader,
        root,
        &StoredCandidateEdits::none(),
        &FolderReleaseDecisions::default(),
        &cancellation,
        |item| {
            if matches!(item, ScanItem::Discovered(_)) {
                cancellation.cancel();
            }
        },
    );

    assert!(matches!(result, Err(FolderScanError::Cancelled)));
    assert_eq!(
        reader.reads.lock().unwrap().as_slice(),
        [PathBuf::new(), PathBuf::from("Release 01")]
    );
}

#[test]
fn cancellation_reaches_an_in_progress_directory_read() {
    struct CancellableReader {
        entered: std::sync::mpsc::Sender<()>,
    }

    impl DirectoryReader for CancellableReader {
        fn read(
            &self,
            _root: &Path,
            _directory: &Path,
            cancellation: &ScanCancellation,
        ) -> Result<DirectoryListing, FolderScanError> {
            self.entered.send(()).expect("announce directory read");
            loop {
                cancellation.check()?;
                std::thread::yield_now();
            }
        }
    }

    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let cancellation = ScanCancellation::new();
    let thread_cancellation = cancellation.clone();
    let scan = std::thread::spawn(move || {
        scan_for_candidates_with_reader_cancellable(
            &CancellableReader {
                entered: entered_tx,
            },
            PathBuf::from("/network"),
            &StoredCandidateEdits::none(),
            &FolderReleaseDecisions::default(),
            &thread_cancellation,
            |_| {},
        )
    });
    entered_rx
        .recv_timeout(std::time::Duration::from_secs(2))
        .expect("reader did not start");
    cancellation.cancel();
    assert!(matches!(
        scan.join().unwrap(),
        Err(FolderScanError::Cancelled)
    ));
}

#[test]
fn unresolved_boundary_combines_or_exposes_descendants_by_persisted_decision() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Queue");
    let wrapper = root.join("Collection").join("Release Wrapper");
    for child in ["Part 01", "Part 02"] {
        let child = wrapper.join(child);
        std::fs::create_dir_all(&child).unwrap();
        std::fs::write(child.join("track.flac"), fake_flac()).unwrap();
    }
    std::fs::write(wrapper.join("booklet.txt"), "notes").unwrap();

    let scan = |decisions: FolderReleaseDecisions| {
        scan_projected_items_with_decisions(root.clone(), decisions)
    };

    let unresolved = scan(FolderReleaseDecisions::default());
    let boundary = unresolved
        .iter()
        .find_map(|item| match item {
            ScanItem::Boundary(boundary) => Some(boundary),
            ScanItem::Discovered(_) | ScanItem::Valid(_) | ScanItem::Invalid(_) => None,
        })
        .expect("structure remains unresolved");
    assert_eq!(
        boundary.key.relative_folder_path,
        "Collection/Release Wrapper"
    );
    assert_eq!(boundary.shared_file_count, 1);

    let combined = scan(FolderReleaseDecisions::new(HashMap::from([(
        "Collection/Release Wrapper".to_string(),
        FolderReleaseDecision::CombineAsOneRelease,
    )])));
    let combined = combined
        .iter()
        .find_map(|item| match item {
            ScanItem::Valid(candidate) => Some(candidate),
            ScanItem::Discovered(_) | ScanItem::Invalid(_) | ScanItem::Boundary(_) => None,
        })
        .expect("combined wrapper is actionable");
    assert_eq!(combined.path, wrapper);
    assert_eq!(combined.scope, ReleaseFileScope::Recursive);
    assert_eq!(combined.files.release_files().count(), 3);

    let separate = scan(FolderReleaseDecisions::new(HashMap::from([(
        "Collection/Release Wrapper".to_string(),
        FolderReleaseDecision::KeepAsSeparateReleases,
    )])));
    let separate: Vec<_> = separate
        .iter()
        .filter_map(|item| match item {
            ScanItem::Valid(candidate) => Some(candidate),
            ScanItem::Discovered(_) | ScanItem::Invalid(_) | ScanItem::Boundary(_) => None,
        })
        .collect();
    assert_eq!(separate.len(), 2);
    assert!(separate.iter().all(|candidate| {
        candidate.scope == ReleaseFileScope::Direct
            && matches!(
                candidate.resolved_boundaries.as_slice(),
                [ResolvedFolderReleaseBoundary {
                    decision: FolderReleaseDecision::KeepAsSeparateReleases,
                    ..
                }]
            )
    }));
}

#[test]
fn ambiguity_tree_keeps_a_direct_parent_release_and_its_child() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Queue");
    let parent = root.join("Group").join("Artist");
    let child = parent.join("Album");
    std::fs::create_dir_all(&child).unwrap();
    std::fs::write(parent.join("parent.flac"), fake_flac()).unwrap();
    std::fs::write(child.join("child.flac"), fake_flac()).unwrap();

    let boundary = scan_items(&root)
        .into_iter()
        .find_map(|item| match item {
            ScanItem::Boundary(boundary) if boundary.key.relative_folder_path == "Group/Artist" => {
                Some(boundary)
            }
            _ => None,
        })
        .expect("the parent and child are ambiguous");

    assert!(matches!(
        boundary.tree_rows[0].kind,
        FolderReleaseTreeRowKind::Candidate { .. }
    ));
    assert_eq!(boundary.tree_rows[0].name, "Artist");
    assert_eq!(boundary.tree_rows[0].decision_key, boundary.key);
    assert!(boundary.tree_rows.iter().any(|row| {
        row.name == "Album"
            && matches!(row.kind, FolderReleaseTreeRowKind::Candidate { .. })
            && row.depth == 1
    }));
}

#[test]
fn ambiguity_tree_keeps_an_invalid_direct_parent_and_valid_child() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Queue");
    let parent = root.join("Group").join("Artist");
    let child = parent.join("Album");
    std::fs::create_dir_all(&child).unwrap();
    std::fs::write(parent.join("parent.flac"), fake_flac()).unwrap();
    std::fs::write(parent.join("broken.jpg"), b"not an image").unwrap();
    std::fs::write(child.join("child.flac"), fake_flac()).unwrap();

    let boundary = scan_items(&root)
        .into_iter()
        .find_map(|item| match item {
            ScanItem::Boundary(boundary) if boundary.key.relative_folder_path == "Group/Artist" => {
                Some(boundary)
            }
            _ => None,
        })
        .expect("the invalid parent and valid child are ambiguous");

    assert!(matches!(
        boundary.tree_rows[0].kind,
        FolderReleaseTreeRowKind::Invalid { .. }
    ));
    assert_eq!(boundary.tree_rows[0].name, "Artist");
    assert!(boundary.tree_rows.iter().any(|row| {
        row.name == "Album" && matches!(row.kind, FolderReleaseTreeRowKind::Candidate { .. })
    }));
}

#[test]
fn keep_separate_context_survives_when_every_descendant_is_invalid() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Queue");
    for child in ["Group/Release 1", "Group/Release 2"] {
        let child = root.join(child);
        std::fs::create_dir_all(&child).unwrap();
        std::fs::write(child.join("track.flac"), b"not a flac").unwrap();
    }
    let mut items = Vec::new();
    scan_for_candidates_with_decisions(
        root,
        &StoredCandidateEdits::none(),
        &FolderReleaseDecisions::new(HashMap::from([(
            "Group".to_string(),
            FolderReleaseDecision::KeepAsSeparateReleases,
        )])),
        |item| {
            if !matches!(item, ScanItem::Discovered(_)) {
                items.push(item);
            }
        },
    )
    .unwrap();

    let invalid: Vec<_> = items
        .iter()
        .filter_map(|item| match item {
            ScanItem::Invalid(candidate) => Some(candidate),
            _ => None,
        })
        .collect();
    assert_eq!(invalid.len(), 2);
    assert!(invalid.iter().all(|candidate| {
        matches!(
            candidate.resolved_boundaries.as_slice(),
            [ResolvedFolderReleaseBoundary {
                decision: FolderReleaseDecision::KeepAsSeparateReleases,
                ..
            }]
        )
    }));
}

#[test]
fn unresolved_boundary_counts_shared_files_in_audio_free_subtrees() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Queue");
    let wrapper = root.join("Collection").join("Release Wrapper");
    for child in ["Release 01", "Release 02"] {
        let child = wrapper.join(child);
        std::fs::create_dir_all(&child).unwrap();
        std::fs::write(child.join("track.flac"), fake_flac()).unwrap();
    }
    let scans = wrapper.join("Scans");
    std::fs::create_dir_all(scans.join("Booklet")).unwrap();
    std::fs::write(scans.join("front.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();
    std::fs::write(scans.join("Booklet").join("notes.txt"), "notes").unwrap();

    let boundary = scan_items(root)
        .into_iter()
        .find_map(|item| match item {
            ScanItem::Boundary(boundary) => Some(boundary),
            ScanItem::Discovered(_) | ScanItem::Valid(_) | ScanItem::Invalid(_) => None,
        })
        .expect("collection remains unresolved");

    assert_eq!(boundary.shared_file_count, 2);
}

#[test]
fn nested_collection_candidates_carry_core_issued_combine_keys() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Queue");
    for release in [
        "Collection/Box/Release 01",
        "Collection/Box/Release 02",
        "Collection/Release 03",
    ] {
        let release = root.join(release);
        std::fs::create_dir_all(&release).unwrap();
        std::fs::write(release.join("track.flac"), fake_flac()).unwrap();
    }

    let items = scan_items(&root);
    assert!(!items
        .iter()
        .any(|item| matches!(item, ScanItem::Boundary(_))));
    let box_candidates: Vec<_> = items
        .into_iter()
        .filter_map(|item| match item {
            ScanItem::Valid(candidate) if candidate.display_path.starts_with("Collection/Box/") => {
                Some(candidate)
            }
            _ => None,
        })
        .collect();
    assert_eq!(box_candidates.len(), 2);
    assert!(box_candidates.iter().all(|candidate| candidate
        .combine_ancestor_key
        .as_ref()
        .is_some_and(|key| {
            key.watched_folder_path == root.to_string_lossy()
                && key.relative_folder_path == "Collection/Box"
        })));
}

#[test]
fn test_is_audio_file() {
    assert!(is_audio_file(Path::new("track.flac")));
    assert!(is_audio_file(Path::new("track.FLAC")));
    assert!(is_audio_file(Path::new("track.mp3")));
    assert!(is_audio_file(Path::new("track.MP3")));
    assert!(is_audio_file(Path::new("track.ape")));
    assert!(is_audio_file(Path::new("track.APE")));
    assert!(is_audio_file(Path::new("track.m4a")));
    assert!(is_audio_file(Path::new("track.M4A")));
    assert!(is_audio_file(Path::new("track.wav")));
    assert!(is_audio_file(Path::new("track.aif")));
    assert!(is_audio_file(Path::new("track.aiff")));
    assert!(is_audio_file(Path::new("track.aifc")));
    assert!(is_audio_file(Path::new("track.ogg")));
    assert!(is_audio_file(Path::new("track.oga")));
    assert!(is_audio_file(Path::new("track.opus")));
    assert!(is_audio_file(Path::new("track.wv")));
    assert!(is_audio_file(Path::new("track.dsf")));
    assert!(is_audio_file(Path::new("track.dff")));
    assert!(!is_audio_file(Path::new("track.wma")));
    assert!(!is_audio_file(Path::new("track.mpc")));
    assert!(!is_audio_file(Path::new("track.spx")));
    assert!(!is_audio_file(Path::new("cover.jpg")));
    assert!(!is_audio_file(Path::new("notes.txt")));
}

#[test]
fn test_is_cue_file() {
    assert!(is_cue_file(Path::new("album.cue")));
    assert!(is_cue_file(Path::new("album.CUE")));
    assert!(!is_cue_file(Path::new("album.flac")));
}

#[test]
fn test_cue_parser_counts_audio_tracks_and_captures_file_reference() {
    let tmp = tempfile::tempdir().unwrap();
    let cue = tmp.path().join("album.cue");
    let content = "FILE \"album.flac\" WAVE\n  TRACK 01 AUDIO\n    INDEX 01 00:00:00\n  TRACK 02 AUDIO\n    INDEX 01 05:00:00\n  TRACK 03 AUDIO\n    INDEX 01 10:00:00\n";
    std::fs::write(&cue, content).unwrap();

    let sheet = parse_cue_sheet(&cue).unwrap();
    assert_eq!(sheet.single_file(), Some("album.flac"));
    assert_eq!(sheet.tracks.len(), 3);
}

#[test]
fn test_cue_parser_tolerates_missing_performer_title() {
    // Minimal CUE with no PERFORMER/TITLE — still a valid rip artifact,
    // must parse so the scanner and importer see the same facts.
    let tmp = tempfile::tempdir().unwrap();
    let cue = tmp.path().join("album.cue");
    let content = "FILE \"dummy.flac\" WAVE\n  TRACK 01 AUDIO\n    INDEX 01 00:00:00\n";
    std::fs::write(&cue, content).unwrap();

    let sheet = parse_cue_sheet(&cue).unwrap();
    assert!(sheet.title.is_none());
    assert!(sheet.performer.is_none());
    assert_eq!(sheet.single_file(), Some("dummy.flac"));
    assert_eq!(sheet.tracks.len(), 1);
}

#[test]
fn test_cue_parser_stops_at_data_track() {
    let tmp = tempfile::tempdir().unwrap();
    let cue = tmp.path().join("album.cue");
    let content = "FILE \"album.flac\" WAVE\n  TRACK 01 AUDIO\n    INDEX 01 00:00:00\n  TRACK 02 MODE1/2048\n    INDEX 01 05:00:00\n";
    std::fs::write(&cue, content).unwrap();

    let sheet = parse_cue_sheet(&cue).unwrap();
    assert_eq!(sheet.tracks.len(), 2);
    assert_eq!(sheet.playable_track_count(), 1);
    assert!(matches!(
        sheet.tracks[1].mode,
        CueTrackMode::Other(ref mode) if mode == "MODE1/2048"
    ));
}

#[test]
fn test_collect_release_candidate_files_skips_hidden_and_bae() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path();

    // Create visible files
    std::fs::write(root.join("track.flac"), fake_flac()).unwrap();
    std::fs::write(root.join("cover.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();
    std::fs::write(root.join("back.bmp"), b"BMvalid bmp marker").unwrap();

    // Create hidden file that should be ignored
    std::fs::write(root.join(".DS_Store"), b"mac junk").unwrap();

    // Create .bae directory -- entirely ignored by the scanner
    let bae_dir = root.join(".bae");
    std::fs::create_dir(&bae_dir).unwrap();
    std::fs::write(bae_dir.join("cover-mb.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();
    std::fs::write(bae_dir.join("cover-discogs.jpeg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();

    let files = collect_release_candidate_files_with_scope(
        root,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .unwrap();

    let audio_paths: Vec<_> = files.audio().map(|f| f.relative_path.as_str()).collect();
    assert_eq!(audio_paths, vec!["track.flac"]);

    // Only release artwork, not .bae/ files
    let artwork_paths: Vec<_> = files.artwork().map(|f| f.relative_path.as_str()).collect();
    assert_eq!(artwork_paths, vec!["back.bmp", "cover.jpg"]);

    assert_eq!(files.documents().count(), 0);
}

/// A folder whose only audio is a zero-byte file can't be imported:
/// `collect_release_candidate_files` surfaces the typed
/// `ImportError::InvalidFolder` (carrying the scanner's `InvalidReason`)
/// rather than a stringly error, so the commit caller can distinguish an
/// unimportable folder from an I/O fault.
#[test]
fn collect_release_candidate_files_on_invalid_folder_yields_invalid_folder() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path();
    // Zero-byte audio is corruption, not an I/O fault.
    std::fs::write(root.join("track.flac"), []).unwrap();

    let err = collect_release_candidate_files_with_scope(
        root,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect_err("zero-byte audio makes the folder unimportable");
    assert!(
        matches!(
            err,
            crate::import::ImportError::InvalidFolder(InvalidReason::CorruptAudioFile { .. })
        ),
        "got: {err:?}"
    );
}

#[cfg(unix)]
#[test]
fn unreadable_child_directory_fails_scan() {
    use std::os::unix::fs::PermissionsExt;

    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path();
    let blocked = root.join("blocked");
    std::fs::create_dir(&blocked).unwrap();
    std::fs::set_permissions(&blocked, std::fs::Permissions::from_mode(0o000)).unwrap();

    let err = scan_for_candidates_with_callback(
        root.to_path_buf(),
        &StoredCandidateEdits::none(),
        |_| {},
    )
    .expect_err("unreadable directory should fail the scan");

    std::fs::set_permissions(&blocked, std::fs::Permissions::from_mode(0o700)).unwrap();

    match err {
        FolderScanError::Io { path, source } => {
            assert_eq!(path, blocked);
            assert_eq!(source.kind(), std::io::ErrorKind::PermissionDenied);
        }
        FolderScanError::NotADirectory { path } => {
            panic!(
                "expected IO error, got not-a-directory for {}",
                path.display()
            )
        }
        FolderScanError::Other(message) => panic!("expected IO error, got {message}"),
        FolderScanError::Cancelled => panic!("expected IO error, got cancellation"),
    }
}

#[test]
fn content_hash_is_location_independent_and_size_sensitive() {
    let make = |root: &str, second_size: u64| CategorizedFiles {
        files: vec![
            audio_entry(&format!("{root}/01.flac"), "01.flac", 1000),
            audio_entry(&format!("{root}/02.flac"), "02.flac", second_size),
        ],
        format_label: "FLAC".to_string(),
    };

    // The same relative structure under two different parent folders hashes
    // identically — the fingerprint follows the rip, not where it sits.
    let a = make("/Volumes/Music/Release", 2000);
    let b = make("/tmp/import_source/Release", 2000);
    assert_eq!(a.content_hash(), b.content_hash());

    // A single differing file size flips the hash.
    let c = make("/Volumes/Music/Release", 2001);
    assert_ne!(a.content_hash(), c.content_hash());
}

#[test]
fn content_hash_is_independent_of_discovery_order() {
    let entry = |name: &str, size: u64, role: FileRole| CandidateFile {
        proposed_audio: matches!(role, FileRole::Audio),
        file: ScannedFile::new(PathBuf::from(name), name.to_string(), size),
        role,
    };
    let forward = CategorizedFiles {
        files: vec![
            entry("01.flac", 1, FileRole::Audio),
            entry("02.flac", 2, FileRole::Audio),
            entry("cover.jpg", 3, FileRole::Cover),
            entry("notes.txt", 4, FileRole::Document),
        ],
        format_label: "FLAC".to_string(),
    };
    let shuffled = CategorizedFiles {
        files: vec![
            entry("notes.txt", 4, FileRole::Document),
            entry("02.flac", 2, FileRole::Audio),
            entry("cover.jpg", 3, FileRole::Cover),
            entry("01.flac", 1, FileRole::Audio),
        ],
        format_label: "FLAC".to_string(),
    };
    assert_eq!(forward.content_hash(), shuffled.content_hash());
}

/// Creates a minimal CUE file content that references the given FLAC filename
fn make_cue_content(flac_filename: &str, title: &str) -> String {
    format!(
        r#"PERFORMER "Test Artist"
TITLE "{title}"
FILE "{flac_filename}" WAVE
  TRACK 01 AUDIO
    TITLE "Track One"
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE "Track Two"
    INDEX 01 05:00:00
"#
    )
}

#[test]
fn test_cue_with_corrupt_ape_surfaces_as_invalid_candidate() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("APE Album");
    std::fs::create_dir(&root).unwrap();

    let cue_content = r#"PERFORMER "Test Artist"
TITLE "Test Album"
FILE "album.ape" WAVE
  TRACK 01 AUDIO
    TITLE "Track One"
    INDEX 01 00:00:00
"#;
    std::fs::write(root.join("album.cue"), cue_content).unwrap();
    // APE file with invalid magic bytes (not "MAC ")
    std::fs::write(root.join("album.ape"), b"fake ape data").unwrap();
    std::fs::write(root.join("cover.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();

    // A folder whose only audio is corrupt is not dropped and not a valid
    // candidate — it surfaces as an invalid candidate carrying the reason.
    let items = scan_items(root);
    assert_eq!(items.len(), 1, "exactly one scan item for the leaf");
    match &items[0] {
        ScanItem::Invalid(invalid) => {
            assert!(
                matches!(invalid.reason, InvalidReason::CorruptAudioFile { .. }),
                "reason names the audio fault, got: {}",
                invalid.reason,
            );
        }
        ScanItem::Valid(_) => panic!("corrupt-audio folder must not be a valid candidate"),
        ScanItem::Discovered(_) | ScanItem::Boundary(_) => {
            panic!("terminal scan helper returned a progress item")
        }
    }
}

/// Bytes of a placeholder audio fixture that probes to a specific codec, used to
/// build CUE pairs whose probed codec identity is what a test asserts against.
fn audio_format_fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/test-fixtures/audio-format/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap_or_else(|e| panic!("read audio fixture {name}: {e}"))
}

/// A FLAC with valid `fLaC` magic + well-formed STREAMINFO shape but no audio
/// frames and `total_samples = 0` (streaming-length unknown). It passes the
/// header-only `is_valid_flac` check yet has no usable duration, so the FFmpeg
/// probe can't identify a playable stream — the shape of a download truncated
/// right after the STREAMINFO block.
fn header_only_flac_unprobeable() -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(b"fLaC");
    // STREAMINFO block header: last-block=0, type=0, length=34.
    buf.extend_from_slice(&[0x00, 0x00, 0x00, 34]);
    // STREAMINFO data (34 bytes): 44100 Hz, 2ch, 16-bit, total_samples=0.
    buf.extend_from_slice(&[0x10, 0x00]); // min block size 4096
    buf.extend_from_slice(&[0x10, 0x00]); // max block size 4096
    buf.extend_from_slice(&[0x00, 0x00, 0x00]); // min frame size
    buf.extend_from_slice(&[0x00, 0x00, 0x00]); // max frame size
    buf.push(0x0A); // sample_rate >> 12
    buf.push(0xC4); // (sample_rate >> 4) & 0xFF
    buf.push(0x42); // (sample_rate & 0x0F)<<4 | (ch-1)<<1 | (bps-1)>>4
    buf.push(0xF0); // (bps-1 & 0x0F)<<4 | total_samples high nibble
    buf.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // total_samples low 32 bits
    buf.extend_from_slice(&[0u8; 16]); // MD5 signature
    assert_eq!(buf.len(), 42);
    buf
}

/// A CUE-paired audio file that clears the header-only magic check but can't be
/// probed (no playable stream) surfaces the folder as an invalid candidate — it
/// must NOT abort the whole watched-root walk. Same failure class as an
/// unsupported codec, triggered instead by one corrupt/incomplete file. A
/// sibling FLAC release under the same root still scans.
#[test]
fn cue_with_unprobeable_audio_is_invalid_and_siblings_still_scan() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path();

    let bad = root.join("Truncated Album");
    std::fs::create_dir(&bad).unwrap();
    std::fs::write(
        bad.join("album.cue"),
        make_cue_content("album.flac", "Test Album"),
    )
    .unwrap();
    std::fs::write(bad.join("album.flac"), header_only_flac_unprobeable()).unwrap();

    let good = root.join("FLAC Album");
    std::fs::create_dir(&good).unwrap();
    std::fs::write(good.join("01 Track.flac"), fake_flac()).unwrap();

    let items = scan_items(root.to_path_buf());
    assert_eq!(items.len(), 2, "both leaves surface");

    let invalid = items
        .iter()
        .find_map(|i| match i {
            ScanItem::Invalid(inv) if inv.name == "Truncated Album" => Some(inv),
            _ => None,
        })
        .expect("expected an invalid candidate for the unprobeable CUE audio");
    assert!(
        matches!(invalid.reason, InvalidReason::CorruptAudioFile { .. }),
        "reason names the audio fault, got: {}",
        invalid.reason,
    );

    let sibling_scanned = items
        .iter()
        .any(|i| matches!(i, ScanItem::Valid(c) if c.name == "FLAC Album"));
    assert!(sibling_scanned, "sibling FLAC release still scans");
}

/// A CUE paired with an audio file whose codec can't back single-file CUE
/// playback (MP3, Vorbis) costs the sheet its binding — bae can't carve tracks
/// out of that container — but the folder still imports: the audio keeps its
/// role and becomes one track, labelled by its own format.
#[test]
fn cue_with_unsupported_codec_leaves_the_sheet_unbound() {
    for (folder, audio_name, fixture, codec, label) in [
        (
            "MP3 Album",
            "album.mp3",
            "placeholder-mp3.mp3",
            "MP3",
            "MP3",
        ),
        (
            "Ogg Album",
            "album.ogg",
            "placeholder-vorbis.ogg",
            "Vorbis",
            "OGG",
        ),
    ] {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path();

        let album = root.join(folder);
        std::fs::create_dir(&album).unwrap();
        std::fs::write(
            album.join("album.cue"),
            make_cue_content(audio_name, "Test Album"),
        )
        .unwrap();
        std::fs::write(album.join(audio_name), audio_format_fixture(fixture)).unwrap();

        let candidates = scan_valid(root.to_path_buf());
        assert_eq!(candidates.len(), 1, "{folder}: one valid candidate");
        let files = &candidates[0].files;
        assert!(
            files.bound_sheets().is_empty(),
            "{folder}: the sheet must not bind to a codec bae can't carve",
        );
        // The refusal keeps the file it named and the codec, so the pane can
        // say which file and why instead of leaving the row reading as a bug —
        // and so the editor that makes this binding a user decision can refuse
        // the same pairing up front rather than at commit.
        let sheets: Vec<_> = files.track_sheets().collect();
        assert_eq!(sheets.len(), 1);
        assert_eq!(
            sheets[0].binding,
            &SheetBinding::RefusedCodec {
                file_id: audio_name.to_string(),
                codec: codec.to_string(),
            },
            "{folder}: the refusal names the file and the probed codec",
        );
        assert_eq!(
            files.audio().count(),
            1,
            "{folder}: the audio keeps its role",
        );
        assert_eq!(files.track_count(), 1, "{folder}: it imports as one track");
        assert_eq!(
            files.format_label, label,
            "{folder}: labelled by the file's own format",
        );
    }
}

/// A CUE paired with a codec that CAN back single-file CUE playback yields a
/// valid candidate labeled `CUE+<codec>`. PCM, WavPack, and DSD are otherwise
/// untested positive arms of the codec-label match.
#[test]
fn cue_with_supported_codec_yields_valid_candidate_labeled() {
    for (folder, audio_name, fixture, label) in [
        ("PCM Album", "album.wav", "placeholder-pcm.wav", "CUE+PCM"),
        (
            "WavPack Album",
            "album.wv",
            "placeholder-wavpack.wv",
            "CUE+WavPack",
        ),
        ("DSD Album", "album.dsf", "placeholder-dsd.dsf", "CUE+DSD"),
    ] {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path().join(folder);
        std::fs::create_dir(&root).unwrap();
        std::fs::write(
            root.join("album.cue"),
            make_cue_content(audio_name, "Test Album"),
        )
        .unwrap();
        std::fs::write(root.join(audio_name), audio_format_fixture(fixture)).unwrap();

        let candidates = scan_valid(root);
        assert_eq!(candidates.len(), 1, "{folder}: one valid candidate");
        assert_eq!(
            candidates[0].files.format_label, label,
            "{folder}: CUE+<codec> label",
        );
    }
}

#[test]
fn test_empty_folder_not_detected() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Empty Album");
    std::fs::create_dir(&root).unwrap();

    let candidates = scan_valid(root);

    assert_eq!(candidates.len(), 0, "Empty folder should not be detected");
}

#[test]
fn test_folder_with_only_images_not_detected() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Just Images");
    std::fs::create_dir(&root).unwrap();

    std::fs::write(root.join("cover.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();
    std::fs::write(
        root.join("back.png"),
        [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A],
    )
    .unwrap();

    let candidates = scan_valid(root);

    assert_eq!(
        candidates.len(),
        0,
        "Folder with only images should not be detected"
    );
}

#[test]
fn test_video_ts_folder_not_detected() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Concert DVD");
    std::fs::create_dir(&root).unwrap();

    let video_ts = root.join("VIDEO_TS");
    std::fs::create_dir(&video_ts).unwrap();
    std::fs::write(video_ts.join("VIDEO_TS.VOB"), b"fake video").unwrap();
    std::fs::write(video_ts.join("VTS_01_1.VOB"), b"fake video").unwrap();

    let candidates = scan_valid(root);

    assert_eq!(
        candidates.len(),
        0,
        "VIDEO_TS folder (DVD rip) should not be detected"
    );
}

#[test]
fn test_volume_folders_with_long_names_are_separate() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Compilation Series");
    std::fs::create_dir(&root).unwrap();

    let volumes = ["Vol. 01 (R2 70921 - 1990)", "Vol. 02 (R2 70922 - 1991)"];

    for vol_name in &volumes {
        let vol_dir = root.join(vol_name);
        std::fs::create_dir(&vol_dir).unwrap();
        std::fs::write(vol_dir.join("track.flac"), fake_flac()).unwrap();
    }

    let candidates = scan_valid(root);

    // Names do not affect folder grouping. Each audio-bearing child is its own
    // approximation.
    assert_eq!(candidates.len(), 2, "each volume should be a candidate");
    for c in &candidates {
        let name = c.path.file_name().and_then(|n| n.to_str()).unwrap();
        assert!(
            volumes.contains(&name),
            "unexpected candidate name {:?}",
            name,
        );
    }
}

#[test]
fn test_zero_byte_files_ignored() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Incomplete Download");
    std::fs::create_dir(&root).unwrap();

    std::fs::write(root.join("01 - Track One.flac"), b"").unwrap();
    std::fs::write(root.join("02 - Track Two.flac"), b"").unwrap();
    std::fs::write(root.join("cover.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();

    let candidates = scan_valid(root);

    assert_eq!(
        candidates.len(),
        0,
        "Folder with only 0-byte FLAC files should not be detected"
    );
}

#[test]
fn test_mix_of_real_and_zero_byte_files_skips_candidate() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Partial Download");
    std::fs::create_dir(&root).unwrap();

    std::fs::write(root.join("01 - Track One.flac"), fake_flac()).unwrap();
    std::fs::write(root.join("02 - Track Two.flac"), b"").unwrap();
    std::fs::write(root.join("03 - Track Three.flac"), b"").unwrap();

    let candidates = scan_valid(root.clone());

    assert_eq!(
        candidates.len(),
        0,
        "Candidate with corrupt files should be skipped entirely"
    );
}

#[test]
fn test_corrupt_image_skips_entire_candidate() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Bad Images");
    std::fs::create_dir(&root).unwrap();

    std::fs::write(root.join("track.flac"), fake_flac()).unwrap();
    std::fs::write(root.join("front.jpg"), [0xFF, 0xD8, 0xFF, 0xE0, 0x00]).unwrap();
    std::fs::write(root.join("back.jpg"), b"not a jpeg").unwrap();
    std::fs::write(root.join("inlay.png"), b"").unwrap();

    let candidates = scan_valid(root);

    assert_eq!(
        candidates.len(),
        0,
        "Candidate with corrupt images should be skipped entirely"
    );
}

/// Per-track FLACs plus a sheet naming absent audio: the sheet is a proposal,
/// not the layout, so it stays unbound and the twelve tracks import.
#[test]
fn per_track_flacs_with_missing_cue_audio_still_import() {
    let tmp = tempfile::TempDir::new().unwrap();
    let album = tmp
        .path()
        .join("Collection")
        .join("Artist - Album Title - (1991) {Label CAT-12345}");
    std::fs::create_dir_all(&album).unwrap();

    // 12 per-track FLACs
    for i in 1..=12 {
        std::fs::write(
            album.join(format!("{:02} - Track {i}.flac", i)),
            fake_flac(),
        )
        .unwrap();
    }

    // CUE sheet with missing referenced audio.
    std::fs::write(
        album.join("Album Title.cue"),
        "FILE \"dummy.flac\" WAVE\n  TRACK 01 AUDIO\n    INDEX 01 00:00:00\n",
    )
    .unwrap();

    // LOG file
    std::fs::write(album.join("Artist - Album Title.log"), "EAC log\n").unwrap();

    // Artwork subfolder with images
    let artwork = album.join("Artwork");
    std::fs::create_dir_all(&artwork).unwrap();
    std::fs::write(artwork.join("front.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();
    std::fs::write(artwork.join("back.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();
    std::fs::write(
        artwork.join("disc.png"),
        [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A],
    )
    .unwrap();

    // folder.jpg at album root
    std::fs::write(album.join("folder.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();

    let items = scan_items(tmp.path().join("Collection"));

    assert_eq!(items.len(), 1);
    match &items[0] {
        ScanItem::Valid(candidate) => {
            assert!(candidate.name.contains("Artist - Album Title"));
            assert_eq!(candidate.files.audio().count(), 12);
            assert!(candidate.files.bound_sheets().is_empty());
            let sheets: Vec<_> = candidate.files.track_sheets().collect();
            assert_eq!(sheets.len(), 1);
            assert_eq!(
                sheets[0].binding,
                &SheetBinding::Unresolved,
                "the sheet names absent audio",
            );
            assert_eq!(candidate.files.track_count(), 12);
        }
        ScanItem::Invalid(invalid) => {
            panic!(
                "a sheet naming absent audio must not invalidate: {}",
                invalid.reason
            )
        }
        ScanItem::Discovered(_) | ScanItem::Boundary(_) => {
            panic!("terminal scan helper returned a progress item")
        }
    }
}

/// A CUE paired with an `.m4a` file produces a `CUE+ALAC` format label
/// because FFmpeg probes the actual codec instead of trusting the extension.
#[test]
fn test_collect_release_candidate_files_cue_alac_format_label() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path();

    let stem = "Artist Name - Album Title";
    let m4a_name = format!("{stem}.m4a");
    let cue_name = format!("{stem}.cue");

    std::fs::write(root.join(&m4a_name), fake_m4a()).unwrap();
    std::fs::write(
        root.join(&cue_name),
        make_cue_content_n_tracks(&m4a_name, "Album Title", 8),
    )
    .unwrap();

    let files = collect_release_candidate_files_with_scope(
        root,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect("scan should succeed");

    assert_eq!(files.format_label, "CUE+ALAC");
    let bound = files.bound_sheets();
    assert_eq!(bound.len(), 1);
    assert_eq!(bound[0].sheet.tracks.len(), 8);
}

/// Multi-FILE CUEs resolve as CUE-backed releases, with every referenced audio
/// file attached to the pair. The release's signals — here the CATALOG (UPC) —
/// live on the parsed sheet attached to the pair.
#[test]
fn test_collect_release_candidate_files_resolves_multifile_cue_sheet() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path();

    std::fs::write(root.join("01 - Track One.flac"), fake_flac()).unwrap();
    std::fs::write(root.join("02 - Track Two.flac"), fake_flac()).unwrap();
    let cue = r#"CATALOG 0123456789012
PERFORMER "Artist Name"
TITLE "Album Title"
FILE "01 - Track One.flac" WAVE
  TRACK 01 AUDIO
    TITLE "Track One"
    INDEX 01 00:00:00
FILE "02 - Track Two.flac" WAVE
  TRACK 02 AUDIO
    TITLE "Track Two"
    INDEX 01 00:00:00
"#;
    std::fs::write(root.join("Album.cue"), cue).unwrap();

    let files = collect_release_candidate_files_with_scope(
        root,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect("scan should succeed");

    let bound = files.bound_sheets();
    assert_eq!(bound.len(), 1);
    // The sheet leads with its first FILE directive; both referenced files keep
    // the audio role.
    assert_eq!(bound[0].audio.file_name, "01 - Track One.flac");
    assert_eq!(
        files
            .audio()
            .map(|file| file.file_name.as_str())
            .collect::<Vec<_>>(),
        vec!["01 - Track One.flac", "02 - Track Two.flac"],
    );
    assert_eq!(bound[0].sheet.catalog.as_deref(), Some("0123456789012"));
    assert_eq!(bound[0].sheet.tracks.len(), 2);
}

#[test]
fn test_cue_pair_codec_label_covers_supported_extensions() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let label = |relative: &str| match cue_pair_codec_label(&manifest.join(relative)).unwrap() {
        CueCodecLabel::Supported(label) => label,
        CueCodecLabel::Unsupported(codec) => panic!("expected supported codec, got {codec}"),
        CueCodecLabel::Unprobeable => panic!("expected supported codec, got unprobeable audio"),
    };
    assert_eq!(label("tests/fixtures/flac/01 Test Track 1.flac"), "FLAC");
    assert_eq!(label("tests/fixtures/cue_ape/Test Album.ape"), "APE");
    assert_eq!(label("test-fixtures/alac/cue-alac.m4a"), "ALAC");
}

/// A CUE+APE pair must report the parsed TRACK count from the CUE sheet,
/// not the number of audio files on disk (which for a single-file CUE+APE
/// release is 1).
#[test]
fn test_collect_release_candidate_files_cue_ape_track_count() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path();

    let stem = "Test Artist - Test Album";
    let ape_name = format!("{stem}.ape");
    let cue_name = format!("{stem}.cue");

    std::fs::write(root.join(&ape_name), fake_ape()).unwrap();
    std::fs::write(
        root.join(&cue_name),
        make_cue_content_n_tracks(&ape_name, "Test Album", 15),
    )
    .unwrap();

    let files = collect_release_candidate_files_with_scope(
        root,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect("scan should succeed");

    assert_eq!(files.format_label, "CUE+APE");
    let bound = files.bound_sheets();
    assert_eq!(bound.len(), 1);
    let track_count = bound[0].sheet.tracks.len();
    assert_eq!(
        track_count, 15,
        "CUE with 15 TRACK entries should parse to 15 tracks, got {track_count}",
    );

    assert_eq!(files.track_count(), 15);
}

// ── Folder-scanner shape fixture ────────────────────────────────────────
//
// A declarative taxonomy of the folder shapes the scanner must handle, pinning
// which of them a human would call a release.

// --- Byte stubs ---

/// Minimal valid APE (Monkey's Audio) header — just the "MAC " magic.
fn fake_ape() -> Vec<u8> {
    std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/cue_ape/Test Album.ape"
    ))
    .expect("read APE fixture")
}

/// Minimal valid MP3 with an ID3v2 header.
fn fake_mp3() -> Vec<u8> {
    b"ID3\x04\x00\x00\x00\x00\x00\x00".to_vec()
}

/// Minimal M4A: an ISO base media `ftyp` box with `M4A ` as the major
/// brand. `is_valid_audio` has no m4a-specific validator (dispatches to
/// the unknown-extension fallback `Ok(true)`), so the bytes only need
/// to be non-empty and have a plausible shape for anything downstream
/// that might sniff them.
fn fake_m4a() -> Vec<u8> {
    std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/test-fixtures/alac/cue-alac.m4a"
    ))
    .expect("read ALAC fixture")
}

/// Minimal valid JPEG (only the SOI + APP0 marker — enough for magic check).
fn fake_jpeg() -> Vec<u8> {
    vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10]
}

/// Minimal valid PNG (just the 8-byte signature).
fn fake_png() -> Vec<u8> {
    vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]
}

/// A plausible AVI file. Never validated by the scanner, but should not be
/// mistaken for audio. RIFF header with an `AVI ` form type.
fn fake_avi() -> Vec<u8> {
    let mut v = b"RIFF".to_vec();
    v.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
    v.extend_from_slice(b"AVI ");
    v
}

/// FLAC with valid magic but a malformed STREAMINFO block length.
fn malformed_flac_streaminfo() -> Vec<u8> {
    let mut buf = vec![b'f', b'L', b'a', b'C', 0x00, 0x00, 0x00, 33];
    buf.resize(42, 0x00);
    buf
}

/// FLAC with wrong magic bytes. `is_valid_flac` rejects on the magic
/// check before even looking at STREAMINFO.
fn broken_flac() -> Vec<u8> {
    // Valid size, but the leading four bytes are not `fLaC`.
    let mut buf = b"BROK".to_vec();
    buf.resize(64, 0u8);
    buf
}

/// CUE sheet content referencing `audio_filename` with `n_tracks` entries.
/// Each track is spaced 5 minutes apart so the sheet parses cleanly.
fn make_cue_content_n_tracks(audio_filename: &str, title: &str, n_tracks: usize) -> String {
    let mut s =
        format!("PERFORMER \"Test Artist\"\nTITLE \"{title}\"\nFILE \"{audio_filename}\" WAVE\n");
    for i in 1..=n_tracks {
        let minute = (i - 1) * 5;
        s.push_str(&format!(
            "  TRACK {:02} AUDIO\n    TITLE \"Track {i:02}\"\n    INDEX 01 {:02}:00:00\n",
            i, minute,
        ));
    }
    s
}

/// Like `make_cue_content_n_tracks` but emits an unquoted FILE directive
/// (`FILE name.wav WAVE`). Exercises the unquoted branch of the
/// CUE parser's FILE directive.
fn make_cue_content_unquoted(audio_filename: &str, title: &str, n_tracks: usize) -> String {
    let mut s =
        format!("PERFORMER \"Test Artist\"\nTITLE \"{title}\"\nFILE {audio_filename} WAVE\n");
    for i in 1..=n_tracks {
        let minute = (i - 1) * 5;
        s.push_str(&format!(
            "  TRACK {:02} AUDIO\n    TITLE \"Track {i:02}\"\n    INDEX 01 {:02}:00:00\n",
            i, minute,
        ));
    }
    s
}

/// CUE sheet without PERFORMER / TITLE. The unified CUE parser is lenient
/// enough to accept it — both fields land as `None`.
fn make_cue_content_no_header(audio_filename: &str, n_tracks: usize) -> String {
    let mut s = format!("FILE \"{audio_filename}\" WAVE\n");
    for i in 1..=n_tracks {
        let minute = (i - 1) * 5;
        s.push_str(&format!(
            "  TRACK {:02} AUDIO\n    INDEX 01 {:02}:00:00\n",
            i, minute,
        ));
    }
    s
}

// --- Spec types ---

/// A file the scenario builder writes at `rel_path`. Folder creation is
/// implicit via `create_dir_all` on the parent.
#[derive(Debug)]
enum FixtureEntry {
    File { rel_path: String, kind: FileKind },
}

/// Every file the fixture writes. One variant per distinct byte pattern
/// or semantic role. The walker matches on this to pick the right bytes;
/// the invariant pass matches on this to pick the right validator.
///
/// Audio extensions must stay in lock-step with `ContentTypeHint::is_audio`
/// — if the walker emits `.flac`/`.mp3`/`.ape`/`.m4a`, the scanner must
/// recognize those.
#[derive(Debug, Clone, Copy)]
enum FileKind {
    // Audio formats recognised by the scanner.
    Flac,
    Mp3,
    M4a,
    /// Empty FLAC file (size 0). The scanner must reject the candidate.
    ZeroByteFlac,
    /// Valid `fLaC` magic with malformed STREAMINFO length.
    MalformedFlacStreaminfo,
    /// Wrong magic bytes where a FLAC is expected. `is_valid_flac` must
    /// reject it.
    BrokenFlac,
    // Image formats.
    Jpeg,
    Png,
    /// Empty `.jpg` file (size 0). The scanner's image validator must
    /// reject it, which in practice short-circuits the categorize pass
    /// and drops the enclosing candidate.
    ZeroByteJpeg,
    /// File whose extension is an arbitrary string the scanner does not
    /// recognize (e.g. `"xyz"`, `"sh"`). Used to pin that unknown file
    /// types are silently ignored rather than mis-categorized.
    UnrecognizedFile(&'static str),
    // Non-music video. Scanner must not treat it as audio.
    Avi,
    // Document sidecars — fall into `files.documents`.
    Log,
    M3u,
    Md5,
    Ffp,
    TracklistTxt,
    /// A CUE sheet intended to pair with an audio file sharing its own
    /// path stem in the same directory. `stem` is written into the CUE's
    /// FILE directive as `FILE "<stem>" WAVE`; the scanner's pair
    /// detection keys on path stems, not on the FILE directive content,
    /// so `stem` is only used for CUE content validity.
    CueFor {
        stem: &'static str,
        n_tracks: usize,
    },
    /// Like `CueFor`, but emits an unquoted FILE directive —
    /// `FILE <stem> WAVE` rather than `FILE "<stem>" WAVE`. Exercises
    /// the unquoted branch of the CUE parser's FILE directive.
    CueUnquoted {
        stem: &'static str,
        n_tracks: usize,
    },
    /// A CUE sheet whose path stem deliberately does not match any audio
    /// in the same directory. `file_reference` goes into the FILE
    /// directive; when it names something not on disk and `n_tracks`
    /// exceeds the direct-child audio count, the mismatch guard rejects
    /// the candidate.
    NonPairingCue {
        n_tracks: usize,
        file_reference: &'static str,
    },
    /// A CUE sheet lacking the PERFORMER/TITLE preamble. The unified
    /// CUE parser accepts it (both fields land as `None`) and still
    /// surfaces the file reference + track count the incomplete-rip
    /// guard depends on.
    CueNoHeader {
        n_tracks: usize,
        file_reference: &'static str,
    },
    /// Partial-download marker. The argument is the trailing extension
    /// (e.g. `"part"`, `"crdownload"`, `"aria2"`) — purely self-documenting,
    /// the walker does not inspect it. The full file name lives in the
    /// entry's `rel_path`, so different rippers' conventions (`01.flac.part`,
    /// `01.flac.crdownload`, `01.flac.aria2`) are all expressible.
    PartialMarker(&'static str),
    // Root-level non-music junk.
    Pdf,
    Zip,
    Dmg,
}

// --- Byte writers & validators keyed purely on FileKind ---

/// Bytes to write for each kind. The walker uses this directly.
fn bytes_for(kind: FileKind) -> Vec<u8> {
    match kind {
        FileKind::Flac => fake_flac(),
        FileKind::Mp3 => fake_mp3(),
        FileKind::M4a => fake_m4a(),
        FileKind::ZeroByteFlac => Vec::new(),
        FileKind::MalformedFlacStreaminfo => malformed_flac_streaminfo(),
        FileKind::BrokenFlac => broken_flac(),
        FileKind::Jpeg => fake_jpeg(),
        FileKind::Png => fake_png(),
        FileKind::ZeroByteJpeg => Vec::new(),
        FileKind::UnrecognizedFile(_) => b"opaque contents".to_vec(),
        FileKind::Avi => fake_avi(),
        FileKind::Log => b"EAC log\n".to_vec(),
        FileKind::M3u => b"01.flac\n02.flac\n".to_vec(),
        FileKind::Md5 => b"abc  01.flac\n".to_vec(),
        FileKind::Ffp => b"01.flac:abc\n".to_vec(),
        FileKind::TracklistTxt => b"01. Track One\n02. Track Two\n".to_vec(),
        FileKind::CueFor { stem, n_tracks } => {
            make_cue_content_n_tracks(stem, "Album", n_tracks).into_bytes()
        }
        FileKind::CueUnquoted { stem, n_tracks } => {
            make_cue_content_unquoted(stem, "Album", n_tracks).into_bytes()
        }
        FileKind::NonPairingCue {
            n_tracks,
            file_reference,
        } => make_cue_content_n_tracks(file_reference, "Album", n_tracks).into_bytes(),
        FileKind::CueNoHeader {
            n_tracks,
            file_reference,
        } => make_cue_content_no_header(file_reference, n_tracks).into_bytes(),
        FileKind::PartialMarker(_) => b"partial data".to_vec(),
        FileKind::Pdf => b"%PDF-1.4\n".to_vec(),
        FileKind::Zip => b"PK\x03\x04".to_vec(),
        FileKind::Dmg => b"koly".to_vec(),
    }
}

/// Fixture-builder invariant: validate the written bytes match the kind.
/// Failure here is always a fixture-builder bug, never a scanner bug.
fn assert_kind_invariant(path: &Path, kind: FileKind) {
    assert!(
        path.exists(),
        "fixture builder bug, not scanner bug: file missing at {:?}",
        path,
    );
    match kind {
        FileKind::Flac => {
            assert!(
                file_validation::is_valid_flac(path).unwrap_or(false),
                "fixture builder bug: FLAC at {:?} fails validator",
                path,
            );
        }
        FileKind::Mp3 => {
            assert!(
                file_validation::is_valid_mp3(path).unwrap_or(false),
                "fixture builder bug: MP3 at {:?} fails validator",
                path,
            );
        }
        FileKind::M4a => {
            // is_valid_audio dispatches by extension and falls through to
            // Ok(true) for m4a, so "validation" is really just "file
            // exists, non-empty, extension is .m4a". Pin those.
            let size = std::fs::metadata(path).unwrap().len();
            assert!(
                size > 0,
                "fixture builder bug: M4A at {:?} must be non-empty",
                path,
            );
            assert_eq!(
                path.extension()
                    .and_then(|e| e.to_str())
                    .map(str::to_lowercase),
                Some("m4a".to_string()),
                "fixture builder bug: M4A at {:?} must have .m4a extension",
                path,
            );
        }
        FileKind::ZeroByteFlac => {
            let size = std::fs::metadata(path).unwrap().len();
            assert_eq!(
                size, 0,
                "fixture builder bug: {:?} should be zero-byte, is {}",
                path, size,
            );
        }
        FileKind::MalformedFlacStreaminfo | FileKind::BrokenFlac => {
            // is_valid_flac must reject both — the test matrix depends on
            // these kinds being seen as invalid audio.
            assert!(
                !file_validation::is_valid_flac(path).unwrap_or(true),
                "fixture builder bug: {:?} at {:?} unexpectedly passes is_valid_flac",
                kind,
                path,
            );
        }
        FileKind::Jpeg | FileKind::Png => {
            assert!(
                file_validation::is_valid_image(path).unwrap_or(false),
                "fixture builder bug: image at {:?} fails validator",
                path,
            );
        }
        FileKind::ZeroByteJpeg => {
            let size = std::fs::metadata(path).unwrap().len();
            assert_eq!(
                size, 0,
                "fixture builder bug: {:?} should be zero-byte, is {}",
                path, size,
            );
            assert_eq!(
                path.extension()
                    .and_then(|e| e.to_str())
                    .map(str::to_lowercase),
                Some("jpg".to_string()),
                "fixture builder bug: ZeroByteJpeg at {:?} must have .jpg extension",
                path,
            );
        }
        FileKind::UnrecognizedFile(ext) => {
            // No byte-level validation — the scanner only cares that the
            // extension is unrecognized.
            let got = path
                .extension()
                .and_then(|e| e.to_str())
                .map(str::to_lowercase);
            assert_eq!(
                got,
                Some(ext.to_lowercase()),
                "fixture builder bug: UnrecognizedFile({:?}) at {:?} extension mismatch (got {:?})",
                ext,
                path,
                got,
            );
        }
        FileKind::CueFor { n_tracks, .. }
        | FileKind::CueUnquoted { n_tracks, .. }
        | FileKind::NonPairingCue { n_tracks, .. } => {
            let sheet = parse_cue_sheet(path).unwrap_or_else(|e| {
                panic!(
                    "fixture builder bug: CUE at {:?} fails parse: {:?}",
                    path, e,
                )
            });
            assert_eq!(
                sheet.tracks.len(),
                n_tracks,
                "fixture builder bug: CUE at {:?} declares {} tracks, expected {}",
                path,
                sheet.tracks.len(),
                n_tracks,
            );
        }
        FileKind::CueNoHeader {
            n_tracks,
            file_reference,
        } => {
            let sheet = parse_cue_sheet(path).unwrap_or_else(|e| {
                panic!(
                    "fixture builder bug: headerless CUE at {:?} fails parse: {:?}",
                    path, e,
                )
            });
            assert!(
                sheet.title.is_none() && sheet.performer.is_none(),
                "fixture builder bug: headerless CUE at {:?} unexpectedly has title/performer",
                path,
            );
            assert_eq!(
                sheet.tracks.len(),
                n_tracks,
                "fixture builder bug: headerless CUE at {:?} counts {} tracks, expected {}",
                path,
                sheet.tracks.len(),
                n_tracks,
            );
            assert_eq!(
                sheet.single_file(),
                Some(file_reference as &str),
                "fixture builder bug: headerless CUE at {:?} single_file mismatch",
                path,
            );
        }
        FileKind::PartialMarker(ext) => {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            assert!(
                name.ends_with(&format!(".{ext}")),
                "fixture builder bug: partial marker at {:?} does not end with .{}",
                path,
                ext,
            );
        }
        FileKind::Avi
        | FileKind::Log
        | FileKind::M3u
        | FileKind::Md5
        | FileKind::Ffp
        | FileKind::TracklistTxt
        | FileKind::Pdf
        | FileKind::Zip
        | FileKind::Dmg => {
            // Presence-only kinds: the scanner does not validate their bytes.
        }
    }
}

// --- Sugar: per-track audio ---

/// Produce `n` `File` entries at `{dir}/{i:02}.<ext>`, one per track,
/// with the given audio `kind`. The extension is derived from the kind
/// (Flac / ZeroByteFlac → `flac`, Mp3 → `mp3`). Panics on
/// non-audio kinds — the helper is named for the "per-track audio
/// release" shape and refuses to be repurposed.
fn flat_audio(dir: &str, n: usize, kind: FileKind) -> Vec<FixtureEntry> {
    let ext = match kind {
        FileKind::Flac | FileKind::ZeroByteFlac => "flac",
        FileKind::Mp3 => "mp3",
        FileKind::M4a => "m4a",
        other => panic!(
            "flat_audio: unsupported kind {:?} — this helper only produces audio tracks",
            other,
        ),
    };
    (1..=n)
        .map(|i| FixtureEntry::File {
            rel_path: format!("{dir}/{i:02}.{ext}"),
            kind,
        })
        .collect()
}

// --- Walker: pure dispatch over the spec ---

/// Build a fixture on disk at `root` from a spec. Parent directories for any
/// file path are created implicitly, so container folders need no entries.
fn build_fixture(root: &Path, spec: &[FixtureEntry]) {
    for entry in spec {
        match entry {
            FixtureEntry::File { rel_path, kind } => {
                let path = root.join(rel_path);
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent).unwrap();
                }
                std::fs::write(&path, bytes_for(*kind)).unwrap();
            }
        }
    }
}

// --- Invariant pass: confirm each File entry exists with expected bytes.

fn assert_fixture_invariants(root: &Path, spec: &[FixtureEntry]) {
    for entry in spec {
        match entry {
            FixtureEntry::File { rel_path, kind } => {
                assert_kind_invariant(&root.join(rel_path), *kind);
            }
        }
    }
}

// ── Scenario test library ──────────────────────────────────────────────
//
// Each test builds the minimum tree needed to exercise one rule, scans
// it, and asserts a specific outcome. All tests go through the
// `run_scenario` helper, which handles tempdir creation, fixture
// invariant checking, and top-level path extraction.
//
// Test names name the scenario, not the implementation.

/// Wraps the common "build tempdir, scan, filter top-level" shape. Keeps
/// `_tmp` alive for the lifetime of the result so the tempdir isn't
/// pulled out from under `candidates`.
struct ScenarioResult {
    /// Held for its `Drop`: keeps the tempdir alive so `candidates` paths
    /// remain valid for the result's lifetime. Never read directly.
    _tmp: tempfile::TempDir,
    candidates: Vec<FolderCandidate>,
    root: PathBuf,
}

impl ScenarioResult {
    /// Candidate rel paths, stripped of the tempdir prefix. Rendered with `/`
    /// separators from the path's components rather than `to_string_lossy`, so a
    /// candidate under `A/B` reads `A/B` on Windows too — `candidate.path` is a
    /// real, host-separated filesystem path (backslashes on Windows), and only
    /// this test view flattens it to the OS-neutral form the expectations use.
    fn top_level_paths(&self) -> Vec<String> {
        self.candidates
            .iter()
            .map(|c| {
                c.path
                    .strip_prefix(&self.root)
                    .unwrap()
                    .components()
                    .map(|component| component.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/")
            })
            .collect()
    }

    /// Find a candidate by exact rel path.
    fn candidate(&self, rel_path: &str) -> &FolderCandidate {
        let target = Path::new(rel_path);
        self.candidates
            .iter()
            .find(|c| {
                c.path
                    .strip_prefix(&self.root)
                    .expect("candidate path is under scan root")
                    == target
            })
            .unwrap_or_else(|| {
                panic!(
                    "no candidate at {rel_path:?}; have {:?}",
                    self.top_level_paths()
                )
            })
    }
}

fn run_scenario(entries: Vec<FixtureEntry>) -> ScenarioResult {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    build_fixture(&root, &entries);
    assert_fixture_invariants(&root, &entries);
    let candidates = scan_valid(root.clone());
    ScenarioResult {
        _tmp: tmp,
        candidates,
        root,
    }
}

// ── Layer 1: single-case minimal tests ────────────────────────────────

// --- Completeness signals (A-series) ---

/// A release folder with a real FLAC and a zero-byte FLAC must be
/// rejected. Mixing valid and zero-byte audio poisons the candidate.
#[test]
fn zero_byte_audio_in_release_skips_candidate() {
    let result = run_scenario(vec![
        FixtureEntry::File {
            rel_path: "Album/01.flac".into(),
            kind: FileKind::Flac,
        },
        FixtureEntry::File {
            rel_path: "Album/02.flac".into(),
            kind: FileKind::ZeroByteFlac,
        },
    ]);
    assert!(result.top_level_paths().is_empty());
}

/// A `.flac.part` sidecar next to a real FLAC suppresses the release.
#[test]
fn partial_marker_sidecar_skips_release() {
    let result = run_scenario(vec![
        FixtureEntry::File {
            rel_path: "Album/01.flac".into(),
            kind: FileKind::Flac,
        },
        FixtureEntry::File {
            rel_path: "Album/02.flac.part".into(),
            kind: FileKind::PartialMarker("part"),
        },
    ]);
    assert!(result.top_level_paths().is_empty());
}

/// A folder holding only partial markers (no real audio) must
/// also yield no candidates. Today this passes because no audio is
/// detected at all; the test pins the intent so if marker-only becomes
/// "looks like audio" we notice.
#[test]
fn partial_marker_only_no_real_audio_skips() {
    let result = run_scenario(vec![FixtureEntry::File {
        rel_path: "Album/01.flac.part".into(),
        kind: FileKind::PartialMarker("part"),
    }]);
    assert!(result.top_level_paths().is_empty());
}

/// An I/O fault while validating an audio file (here: the file vanished
/// after the tree was built) is a system error, not "corrupt" — it must
/// surface, not be collapsed to `false` and silently drop the whole
/// release while mis-logging the cause as corruption. The Ok(false)
/// corruption path (covered elsewhere) still skips the candidate.
#[test]
fn io_error_validating_audio_surfaces_not_swallowed() {
    let tree = CandidateFileIndex::new(vec![FileEntry {
        path: PathBuf::from("Album/01.flac"),
        size: 1024,
    }]);
    // fs_root is an empty dir, so Album/01.flac does not exist on disk:
    // is_valid_audio's open fails with a genuine I/O error.
    let temp = tempfile::TempDir::new().unwrap();
    let result = categorize_files_from_tree(
        &tree,
        &PathBuf::from("Album"),
        temp.path(),
        &StoredCandidateEdits::none(),
        &ScanCancellation::new(),
    );
    assert!(
        result.is_err(),
        "an I/O fault during validation must surface as an error"
    );
}

/// A loose partial-download marker sitting directly at the scan root must
/// not abort the whole scan: complete albums in sibling subfolders still
/// import. The root is a collection, not a release, so a loose marker there
/// belongs to no album and shouldn't suppress its neighbours. (A marker
/// that lives inside a release is still caught by the release-level deep
/// check.)
#[test]
fn loose_marker_at_scan_root_does_not_suppress_sibling_albums() {
    let result = run_scenario(vec![
        FixtureEntry::File {
            rel_path: "loose.flac.part".into(),
            kind: FileKind::PartialMarker("part"),
        },
        FixtureEntry::File {
            rel_path: "AlbumA/01.flac".into(),
            kind: FileKind::Flac,
        },
        FixtureEntry::File {
            rel_path: "AlbumB/01.flac".into(),
            kind: FileKind::Flac,
        },
    ]);
    let mut paths = result.top_level_paths();
    paths.sort();
    assert_eq!(paths, vec!["AlbumA".to_string(), "AlbumB".to_string()]);
}

/// Every supported partial-marker extension suppresses the release.
/// Table-driven: one subtest per extension.
#[test]
fn each_partial_marker_extension_skips_release() {
    for ext in ["part", "crdownload", "download", "aria2", "partial"] {
        let result = run_scenario(vec![
            FixtureEntry::File {
                rel_path: "Album/01.flac".into(),
                kind: FileKind::Flac,
            },
            FixtureEntry::File {
                rel_path: format!("Album/02.flac.{ext}"),
                kind: FileKind::PartialMarker(ext),
            },
        ]);
        assert!(
            result.top_level_paths().is_empty(),
            "extension .{ext} should suppress the candidate",
        );
    }
}

/// Partial-marker extension matching is case-insensitive.
#[test]
fn partial_marker_extension_case_insensitive() {
    for name in ["02.FLAC.PART", "03.FLAC.CRDownload"] {
        let ext = name.rsplit('.').next().unwrap();
        let result = run_scenario(vec![
            FixtureEntry::File {
                rel_path: "Album/01.flac".into(),
                kind: FileKind::Flac,
            },
            FixtureEntry::File {
                rel_path: format!("Album/{name}"),
                kind: FileKind::PartialMarker(ext),
            },
        ]);
        assert!(
            result.top_level_paths().is_empty(),
            "marker {name} should suppress the candidate",
        );
    }
}

/// A folder whose only audio is a FLAC the validator rejects — malformed
/// STREAMINFO length, or wrong magic bytes — surfaces no valid candidate.
#[test]
fn invalid_flac_audio_yields_no_candidate() {
    for kind in [FileKind::MalformedFlacStreaminfo, FileKind::BrokenFlac] {
        let result = run_scenario(vec![FixtureEntry::File {
            rel_path: "Album/01.flac".into(),
            kind,
        }]);
        assert!(
            result.top_level_paths().is_empty(),
            "{kind:?} must reject the candidate",
        );
    }
}

/// An audio-free parent emits each audio-bearing child, not the parent.
#[test]
fn sibling_audio_folders_emit_separate_candidates() {
    let mut entries = flat_audio("Collection/First", 3, FileKind::Flac);
    entries.extend(flat_audio("Collection/Second", 3, FileKind::Flac));
    let result = run_scenario(entries);
    let top = result.top_level_paths();
    assert_eq!(top.len(), 2);
    assert!(top.iter().any(|p| p == "Collection/First"));
    assert!(top.iter().any(|p| p == "Collection/Second"));
}

/// A folder with only `.avi` yields no candidates and no diagnostic.
#[test]
fn non_audio_folder_emits_no_candidates() {
    let result = run_scenario(vec![
        FixtureEntry::File {
            rel_path: "Show/S01E01.avi".into(),
            kind: FileKind::Avi,
        },
        FixtureEntry::File {
            rel_path: "Show/S01E02.avi".into(),
            kind: FileKind::Avi,
        },
    ]);
    assert!(result.top_level_paths().is_empty());
}

/// Loose junk at the scan root (.pdf, .zip, .dmg, .jpg) is
/// ignored; a release subfolder still surfaces.
#[test]
fn loose_junk_at_scan_root_ignored() {
    let mut entries = vec![
        FixtureEntry::File {
            rel_path: "loose.pdf".into(),
            kind: FileKind::Pdf,
        },
        FixtureEntry::File {
            rel_path: "loose.zip".into(),
            kind: FileKind::Zip,
        },
        FixtureEntry::File {
            rel_path: "loose.dmg".into(),
            kind: FileKind::Dmg,
        },
    ];
    entries.extend(flat_audio("Album", 3, FileKind::Flac));
    let result = run_scenario(entries);
    assert_eq!(result.top_level_paths(), vec!["Album"]);
}

// --- Within-release intricacies (C-series) ---

/// Per-track FLACs surface as TrackFiles / "FLAC".
#[test]
fn flat_flac_release_surfaces_as_trackfiles() {
    let result = run_scenario(flat_audio("Album", 3, FileKind::Flac));
    let c = result.candidate("Album");
    assert_eq!(c.files.format_label, "FLAC");
    assert_eq!(c.files.audio().count(), 3);
    assert!(c.files.track_sheets().next().is_none());
}

/// A CUE next to the FLAC it names binds, and the folder is labelled
/// "CUE+FLAC".
#[test]
fn cue_flac_pair_binds_and_is_labelled() {
    let result = run_scenario(vec![
        FixtureEntry::File {
            rel_path: "Album/Album.flac".into(),
            kind: FileKind::Flac,
        },
        FixtureEntry::File {
            rel_path: "Album/Album.cue".into(),
            kind: FileKind::CueFor {
                stem: "Album.flac",
                n_tracks: 8,
            },
        },
    ]);
    let c = result.candidate("Album");
    assert_eq!(c.files.format_label, "CUE+FLAC");
    assert_eq!(c.files.bound_sheets().len(), 1);
}

// The CUE/APE pair is covered by
// `test_collect_release_candidate_files_cue_ape_track_count` above; not
// duplicated here.

/// MP3 tracks surface as TrackFiles / "MP3".
#[test]
fn mp3_release_surfaces_as_mp3_trackfiles() {
    let result = run_scenario(flat_audio("Album", 3, FileKind::Mp3));
    let c = result.candidate("Album");
    assert_eq!(c.files.format_label, "MP3");
}

/// M4A tracks surface as TrackFiles / "M4A".
#[test]
fn m4a_release_surfaces_as_trackfiles() {
    let result = run_scenario(flat_audio("Album", 3, FileKind::M4a));
    let c = result.candidate("Album");
    assert_eq!(c.files.format_label, "M4A");
    assert_eq!(c.files.audio().count(), 3);
}

/// A multi-FILE CUE resolves as a CUE-backed release; each referenced track
/// file remains attached as an ordered source for that layout.
#[test]
fn multi_file_cue_surfaces_as_cue_backed_release() {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(&album).unwrap();
    for i in 1..=3 {
        std::fs::write(album.join(format!("0{i}.m4a")), bytes_for(FileKind::M4a)).unwrap();
    }
    std::fs::write(
        album.join("Album.cue"),
        "PERFORMER \"Artist Name\"\nTITLE \"Album Title\"\n\
             FILE \"01.m4a\" WAVE\n  TRACK 01 AUDIO\n    INDEX 01 00:00:00\n\
             FILE \"02.m4a\" WAVE\n  TRACK 02 AUDIO\n    INDEX 01 00:00:00\n\
             FILE \"03.m4a\" WAVE\n  TRACK 03 AUDIO\n    INDEX 01 00:00:00\n",
    )
    .unwrap();
    let candidates = scan_valid(tmp.path().to_path_buf());
    assert_eq!(candidates.len(), 1);
    let c = &candidates[0];
    assert_eq!(c.files.format_label, "CUE+ALAC");
    assert_eq!(c.files.bound_sheets().len(), 1);
    assert_eq!(
        c.files
            .audio()
            .map(|file| file.file_name.as_str())
            .collect::<Vec<_>>(),
        vec!["01.m4a", "02.m4a", "03.m4a"],
    );
    // A sheet is a sheet, never a document.
    assert!(!c.files.documents().any(|d| d.file_name == "Album.cue"));
}

/// Single-FILE CUE whose own stem differs from the audio file it names —
/// pair detection follows the CUE's `FILE` directive, not the CUE's
/// filename stem. The CUE is the source of truth for what it points at.
#[test]
fn single_file_cue_pairs_by_file_directive_not_stem() {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(&album).unwrap();
    std::fs::write(album.join("Audio.flac"), bytes_for(FileKind::Flac)).unwrap();
    std::fs::write(
        album.join("Sheet.cue"),
        "PERFORMER \"X\"\nTITLE \"Y\"\nFILE \"Audio.flac\" WAVE\n  \
             TRACK 01 AUDIO\n    INDEX 01 00:00:00\n  \
             TRACK 02 AUDIO\n    INDEX 01 03:00:00\n",
    )
    .unwrap();
    let candidates = scan_valid(tmp.path().to_path_buf());
    assert_eq!(candidates.len(), 1);
    let bound = candidates[0].files.bound_sheets();
    assert_eq!(bound.len(), 1);
    assert_eq!(bound[0].audio.file_name, "Audio.flac");
    assert_eq!(bound[0].file.file_name, "Sheet.cue");
}

/// A sheet whose `FILE` directive names missing audio leaves the sheet
/// unbound. The folder still imports: the sheet proposes a layout, it does not
/// dictate one.
#[test]
fn cue_referencing_missing_audio_leaves_the_sheet_unbound() {
    let mut entries = flat_audio("Album", 5, FileKind::Flac);
    entries.push(FixtureEntry::File {
        rel_path: "Album/Album.cue".into(),
        kind: FileKind::NonPairingCue {
            n_tracks: 5,
            file_reference: "Album.flac",
        },
    });
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    build_fixture(&root, &entries);
    let items = scan_items(root);
    assert_eq!(items.len(), 1);
    match &items[0] {
        ScanItem::Valid(candidate) => {
            assert!(candidate.files.bound_sheets().is_empty());
            assert_eq!(candidate.files.audio().count(), 5);
        }
        ScanItem::Invalid(invalid) => panic!("must stay importable: {}", invalid.reason),
        ScanItem::Discovered(_) | ScanItem::Boundary(_) => {
            panic!("terminal scan helper returned a progress item")
        }
    }
}

/// A release's sidecar subfolders attach to the candidate by category:
/// `booklet/*.png` becomes artwork (keeping its `booklet/` prefix), and
/// `Info/Tracklist.txt` becomes a document.
#[test]
fn subfolder_sidecars_attach_by_category() {
    let mut entries = flat_audio("Album", 3, FileKind::Flac);
    entries.extend([
        FixtureEntry::File {
            rel_path: "Album/booklet/page1.png".into(),
            kind: FileKind::Png,
        },
        FixtureEntry::File {
            rel_path: "Album/booklet/page2.png".into(),
            kind: FileKind::Png,
        },
        FixtureEntry::File {
            rel_path: "Album/Info/Tracklist.txt".into(),
            kind: FileKind::TracklistTxt,
        },
    ]);
    let result = run_scenario(entries);
    let c = result.candidate("Album");

    let booklet_paths: Vec<_> = c
        .files
        .artwork()
        .filter(|a| a.relative_path.starts_with("booklet/"))
        .map(|a| a.relative_path.as_str())
        .collect();
    assert_eq!(booklet_paths.len(), 2, "booklet artwork: {booklet_paths:?}");

    assert!(
        c.files
            .documents()
            .any(|d| d.relative_path.ends_with("Tracklist.txt")),
        "Info/Tracklist.txt should be a document, got {:?}",
        c.files
            .documents()
            .map(|d| d.relative_path.as_str())
            .collect::<Vec<_>>(),
    );
}

/// `.md5` / `.ffp` sidecars are neither audio nor artwork nor
/// documents; they are omitted from categorization.
#[test]
fn md5_ffp_sidecars_silently_ignored() {
    let mut entries = flat_audio("Album", 3, FileKind::Flac);
    entries.extend([
        FixtureEntry::File {
            rel_path: "Album/checksums.md5".into(),
            kind: FileKind::Md5,
        },
        FixtureEntry::File {
            rel_path: "Album/checksums.ffp".into(),
            kind: FileKind::Ffp,
        },
    ]);
    let result = run_scenario(entries);
    let c = result.candidate("Album");
    assert!(c
        .files
        .documents()
        .all(|d| { !d.file_name.ends_with(".md5") && !d.file_name.ends_with(".ffp") }));
    assert!(c
        .files
        .artwork()
        .all(|a| { !a.file_name.ends_with(".md5") && !a.file_name.ends_with(".ffp") }));
}

/// `.log` and `.m3u` surface as documents.
#[test]
fn log_m3u_attach_as_documents() {
    let mut entries = flat_audio("Album", 3, FileKind::Flac);
    entries.extend([
        FixtureEntry::File {
            rel_path: "Album/rip.log".into(),
            kind: FileKind::Log,
        },
        FixtureEntry::File {
            rel_path: "Album/playlist.m3u".into(),
            kind: FileKind::M3u,
        },
    ]);
    let result = run_scenario(entries);
    let docs: Vec<_> = result
        .candidate("Album")
        .files
        .documents()
        .map(|d| d.file_name.as_str())
        .collect();
    for expected in ["rip.log", "playlist.m3u"] {
        assert!(docs.contains(&expected), "missing {expected} in {docs:?}");
    }
}

/// The `.bae/` subdirectory is entirely hidden from the scanner.
#[test]
fn bae_sidecar_hidden_from_scanner() {
    let mut entries = flat_audio("Album", 3, FileKind::Flac);
    entries.push(FixtureEntry::File {
        rel_path: "Album/.bae/cover-mb.jpg".into(),
        kind: FileKind::Jpeg,
    });
    let result = run_scenario(entries);
    assert_eq!(result.top_level_paths(), vec!["Album"]);
    let c = result.candidate("Album");
    // Nothing under .bae/ should leak into artwork or documents.
    assert!(c
        .files
        .artwork()
        .all(|a| !a.relative_path.contains(".bae/")));
    assert!(c
        .files
        .documents()
        .all(|d| !d.relative_path.contains(".bae/")));
}

/// Cyrillic path components scan cleanly and the name is
/// preserved verbatim.
#[test]
fn cyrillic_path_component_scans_cleanly() {
    let result = run_scenario(flat_audio("Studio \u{0410}lbums/Album", 3, FileKind::Flac));
    assert_eq!(result.top_level_paths(), vec!["Studio \u{0410}lbums/Album"],);
}

// ── Layer 2: combination tests ────────────────────────────────────────

/// Sibling folders with different audio layouts surface independently with
/// their own formats.
#[test]
fn sibling_folders_keep_their_own_audio_layouts() {
    let mut entries = flat_audio("Collection/Track Files", 3, FileKind::Flac);
    entries.push(FixtureEntry::File {
        rel_path: "Collection/Cue Image/Audio.flac".into(),
        kind: FileKind::Flac,
    });
    entries.push(FixtureEntry::File {
        rel_path: "Collection/Cue Image/Audio.cue".into(),
        kind: FileKind::CueFor {
            stem: "Audio.flac",
            n_tracks: 10,
        },
    });
    let result = run_scenario(entries);
    let top = result.top_level_paths();
    assert_eq!(top.len(), 2);
    let track_files = &result.candidate("Collection/Track Files").files;
    assert_eq!(track_files.format_label, "FLAC");
    assert!(track_files.bound_sheets().is_empty());
    let cue_image = &result.candidate("Collection/Cue Image").files;
    assert_eq!(cue_image.format_label, "CUE+FLAC");
    assert_eq!(cue_image.bound_sheets().len(), 1);
}

/// A partial marker nested under a release subdirectory (e.g. in
/// `booklet/`) still suppresses the whole release. This exercises the
/// deep walker check.
#[test]
fn partial_marker_in_nested_subdir_stops_release_candidate() {
    let mut entries = flat_audio("Album", 3, FileKind::Flac);
    entries.push(FixtureEntry::File {
        rel_path: "Album/booklet/02.flac.part".into(),
        kind: FileKind::PartialMarker("part"),
    });
    let result = run_scenario(entries);
    assert!(result.top_level_paths().is_empty());
}

/// A CUE lacking PERFORMER/TITLE still parses. Its `FILE` directive
/// names audio that isn't here, so it stays unbound and the three FLACs import
/// as themselves — the sheet's declared 15 tracks are a claim about audio the
/// folder doesn't have.
#[test]
fn cue_no_header_naming_absent_audio_stays_unbound() {
    let mut entries = flat_audio("Album", 3, FileKind::Flac);
    entries.push(FixtureEntry::File {
        rel_path: "Album/Album.cue".into(),
        kind: FileKind::CueNoHeader {
            n_tracks: 15,
            file_reference: "Album.flac",
        },
    });
    let result = run_scenario(entries);
    assert_eq!(result.top_level_paths(), vec!["Album"]);
    let c = result.candidate("Album");
    assert!(c.files.bound_sheets().is_empty());
    assert_eq!(c.files.track_count(), 3);
}

/// A CUE with an unquoted FILE directive still pairs when it
/// shares a stem; when the track count exceeds on-disk audio, the
/// mismatch guard fires.
#[test]
fn cue_file_reference_with_unquoted_filename_still_parses() {
    // Stem-matched variant: pair detected even with unquoted FILE.
    let result = run_scenario(vec![
        FixtureEntry::File {
            rel_path: "Paired/Album.flac".into(),
            kind: FileKind::Flac,
        },
        FixtureEntry::File {
            rel_path: "Paired/Album.cue".into(),
            kind: FileKind::CueUnquoted {
                stem: "Album.flac",
                n_tracks: 6,
            },
        },
    ]);
    assert_eq!(result.candidate("Paired").files.bound_sheets().len(), 1);

    // Non-pairing variant: the directive names audio that isn't here, so the
    // sheet stays unbound and the folder still imports.
    let mut entries = flat_audio("Mismatch", 3, FileKind::Flac);
    entries.push(FixtureEntry::File {
        rel_path: "Mismatch/Album.cue".into(),
        kind: FileKind::CueUnquoted {
            stem: "Album.flac",
            n_tracks: 15,
        },
    });
    let result = run_scenario(entries);
    assert_eq!(result.top_level_paths(), vec!["Mismatch"]);
    assert!(result.candidate("Mismatch").files.bound_sheets().is_empty());
}

// ── Layer 3: edge / adversarial cases ─────────────────────────────────

/// A multi-FILE CUE referencing a missing audio file describes a layout
/// the folder can't supply, so it doesn't bind at all: a partial binding would
/// carve tracks out of audio that isn't there. The folder still imports, with
/// the present audio as one track.
#[test]
fn multi_file_cue_with_missing_secondary_file_stays_unbound() {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(&album).unwrap();
    std::fs::write(album.join("Album.flac"), bytes_for(FileKind::Flac)).unwrap();
    // Hand-write the CUE because the DSL doesn't model two-FILE sheets.
    std::fs::write(
            album.join("Album.cue"),
            "PERFORMER \"X\"\nTITLE \"Y\"\nFILE \"Album.flac\" WAVE\n  TRACK 01 AUDIO\n    INDEX 01 00:00:00\nFILE \"Missing.flac\" WAVE\n  TRACK 02 AUDIO\n    INDEX 01 05:00:00\n",
        )
        .unwrap();
    let items = scan_items(tmp.path().to_path_buf());
    assert_eq!(items.len(), 1);
    match &items[0] {
        ScanItem::Valid(candidate) => {
            assert!(
                candidate.files.bound_sheets().is_empty(),
                "a sheet binds only when every FILE reference resolves",
            );
            assert_eq!(candidate.files.audio().count(), 1);
            assert_eq!(candidate.files.track_count(), 1);
        }
        ScanItem::Invalid(invalid) => panic!("must stay importable: {}", invalid.reason),
        ScanItem::Discovered(_) | ScanItem::Boundary(_) => {
            panic!("terminal scan helper returned a progress item")
        }
    }
}

/// A sheet that will not parse is a document, not a verdict: its folder still
/// imports, and so does its sibling.
#[test]
fn unparseable_cue_lands_as_a_document_and_siblings_still_scan() {
    let tmp = tempfile::tempdir().unwrap();
    let bad = tmp.path().join("Bad Album");
    let good = tmp.path().join("Good Album");
    std::fs::create_dir_all(&bad).unwrap();
    std::fs::create_dir_all(&good).unwrap();
    std::fs::write(bad.join("Album.flac"), bytes_for(FileKind::Flac)).unwrap();
    std::fs::write(
        bad.join("Album.cue"),
        "FILE \"Album.flac\" WAVE\n  TRACK 01 AUDIO\n    TITLE \"Missing Index\"\n",
    )
    .unwrap();
    std::fs::write(good.join("01.flac"), bytes_for(FileKind::Flac)).unwrap();

    let items = scan_items(tmp.path().to_path_buf());
    assert_eq!(items.len(), 2);
    let bad = items
        .iter()
        .find_map(|item| match item {
            ScanItem::Valid(candidate) if candidate.name == "Bad Album" => Some(candidate),
            _ => None,
        })
        .expect("the folder with the unparseable sheet still imports");
    assert!(
        bad.files.documents().any(|d| d.file_name == "Album.cue"),
        "an unparseable sheet stays a document",
    );
    assert!(bad.files.track_sheets().next().is_none());
    assert_eq!(bad.files.audio().count(), 1);
    assert!(items.iter().any(|item| matches!(
        item,
        ScanItem::Valid(candidate) if candidate.name == "Good Album"
    )));
}

/// A folder with only a CUE and cover, no audio, yields nothing.
#[test]
fn folder_with_only_cue_and_no_audio() {
    let result = run_scenario(vec![
        FixtureEntry::File {
            rel_path: "Album/Album.cue".into(),
            kind: FileKind::CueFor {
                stem: "Album.flac",
                n_tracks: 5,
            },
        },
        FixtureEntry::File {
            rel_path: "Album/cover.jpg".into(),
            kind: FileKind::Jpeg,
        },
    ]);
    assert!(result.top_level_paths().is_empty());
}

/// A folder with audio plus `.avi` extras still produces an audio candidate.
/// surfaces, `.avi` is ignored.
#[test]
fn folder_with_audio_and_video_mixed() {
    let mut entries = flat_audio("Album", 3, FileKind::Flac);
    entries.push(FixtureEntry::File {
        rel_path: "Album/bonus.avi".into(),
        kind: FileKind::Avi,
    });
    let result = run_scenario(entries);
    assert_eq!(result.top_level_paths(), vec!["Album"]);
    let c = result.candidate("Album");
    assert!(c.files.documents().all(|d| !d.file_name.ends_with(".avi")));
    assert!(c.files.artwork().all(|a| !a.file_name.ends_with(".avi")));
}

/// A deeply nested release under a chain of single-child wrappers
/// collapses to the leaf.
#[test]
fn deeply_nested_release_scan_root_two_levels_up() {
    let result = run_scenario(flat_audio("A/B/C/Release", 3, FileKind::Flac));
    let top = result.top_level_paths();
    assert_eq!(top, vec!["A/B/C/Release"]);
    assert_eq!(result.candidate(&top[0]).name, "Release");
}

/// A folder with unexpected file types (`.xyz`, `.sh`) alongside
/// audio: candidate surfaces, extras omitted from categorization.
#[test]
fn unexpected_file_types_silently_ignored() {
    let mut entries = flat_audio("Album", 3, FileKind::Flac);
    entries.extend([
        FixtureEntry::File {
            rel_path: "Album/weird.xyz".into(),
            kind: FileKind::UnrecognizedFile("xyz"),
        },
        FixtureEntry::File {
            rel_path: "Album/script.sh".into(),
            kind: FileKind::UnrecognizedFile("sh"),
        },
    ]);
    let result = run_scenario(entries);
    assert_eq!(result.top_level_paths(), vec!["Album"]);
    let c = result.candidate("Album");
    assert!(c
        .files
        .documents()
        .all(|d| !d.file_name.ends_with(".xyz") && !d.file_name.ends_with(".sh")));
    // They are still listed, under the role for what the scan doesn't
    // recognize — and the release carries them like everything else.
    let other: Vec<_> = c
        .files
        .files
        .iter()
        .filter(|entry| matches!(entry.role, FileRole::Other))
        .map(|entry| entry.file.file_name.as_str())
        .collect();
    assert_eq!(other, vec!["script.sh", "weird.xyz"]);
    assert!(
        c.files.release_files().any(|f| f.file_name == "weird.xyz"),
        "the folder is the release: an unrecognized file is carried, not dropped",
    );
}

/// Zero-byte cover art is an incompleteness signal, so the release is
/// suppressed.
#[test]
fn zero_byte_cover_art_does_not_surface() {
    let mut entries = flat_audio("Album", 3, FileKind::Flac);
    entries.push(FixtureEntry::File {
        rel_path: "Album/cover.jpg".into(),
        kind: FileKind::ZeroByteJpeg,
    });
    let result = run_scenario(entries);
    assert!(result.top_level_paths().is_empty());
}

// ── Files carry roles ───────────────────────────────────────────────────
//
// Rooted in the folders that were broken, not in the model's own shape.

/// The walkthrough's folder: the sheet was written against a WAV that was later
/// encoded to FLAC. The directive names a file that is not here — a question,
/// not a verdict — so the folder imports, the sheet stays unbound, and the FLAC
/// keeps the audio role.
#[test]
fn sheet_naming_absent_audio_does_not_invalidate_the_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(&album).unwrap();
    std::fs::write(album.join("Album.flac"), fake_flac()).unwrap();
    std::fs::write(
        album.join("Album.cue"),
        make_cue_content("Album.wav", "Album Title"),
    )
    .unwrap();

    let items = scan_items(tmp.path().to_path_buf());
    assert_eq!(items.len(), 1);
    let candidate = match &items[0] {
        ScanItem::Valid(candidate) => candidate,
        ScanItem::Invalid(invalid) => {
            panic!(
                "folder must stay importable, got invalid: {}",
                invalid.reason
            )
        }
        ScanItem::Discovered(_) | ScanItem::Boundary(_) => {
            panic!("terminal scan helper returned a progress item")
        }
    };

    let sheets: Vec<_> = candidate.files.track_sheets().collect();
    assert_eq!(sheets.len(), 1);
    assert_eq!(sheets[0].file.file_name, "Album.cue");
    assert_eq!(
        sheets[0].binding,
        &SheetBinding::Unresolved,
        "the directive names audio that is not here",
    );
    assert_eq!(
        candidate
            .files
            .audio()
            .map(|f| f.file_name.as_str())
            .collect::<Vec<_>>(),
        vec!["Album.flac"],
    );
    assert_eq!(
        candidate.files.track_count(),
        1,
        "with no sheet bound, the image is one track",
    );
}

/// Audio no sheet references is kept, not dropped: a bound sheet plus two
/// standalone files leaves all three files on the candidate, hashed and listed.
#[test]
fn audio_no_sheet_references_survives() {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(&album).unwrap();
    std::fs::write(album.join("Album.flac"), fake_flac()).unwrap();
    std::fs::write(
        album.join("Album.cue"),
        make_cue_content("Album.flac", "Album Title"),
    )
    .unwrap();
    std::fs::write(album.join("bonus 1.flac"), fake_flac()).unwrap();
    std::fs::write(album.join("bonus 2.flac"), fake_flac()).unwrap();

    let files = collect_release_candidate_files_with_scope(
        &album,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect("scan should succeed");
    assert_eq!(
        files
            .audio()
            .map(|f| f.relative_path.as_str())
            .collect::<Vec<_>>(),
        vec!["Album.flac", "bonus 1.flac", "bonus 2.flac"],
    );
    let bound = files.bound_sheets();
    assert_eq!(bound.len(), 1);
    assert_eq!(bound[0].audio.file_name, "Album.flac");

    let carried: Vec<_> = files
        .release_files()
        .map(|f| f.relative_path.as_str())
        .collect();
    for expected in ["Album.cue", "Album.flac", "bonus 1.flac", "bonus 2.flac"] {
        assert!(
            carried.contains(&expected),
            "{expected} must survive the scan, got {carried:?}",
        );
    }
}

/// A folder whose only disc-ID source is its rip log becomes a candidate, and
/// the log's TOC still yields the disc ID with the sheet unbound. Before roles
/// the scan refused the folder first, so the log never got the chance.
#[test]
fn folder_identifies_from_its_rip_log_with_the_sheet_unbound() {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(&album).unwrap();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    std::fs::copy(fixtures.join("test_album.log"), album.join("rip.log")).unwrap();
    std::fs::copy(
        fixtures.join("flac/01 Test Track 1.flac"),
        album.join("01 Test Track 1.flac"),
    )
    .unwrap();
    std::fs::copy(
        fixtures.join("flac/02 Test Track 2.flac"),
        album.join("02 Test Track 2.flac"),
    )
    .unwrap();
    std::fs::write(
        album.join("Album.cue"),
        make_cue_content("Album.wav", "Album Title"),
    )
    .unwrap();

    let items = scan_items(tmp.path().to_path_buf());
    assert_eq!(items.len(), 1);
    assert!(
        matches!(&items[0], ScanItem::Valid(_)),
        "folder must be a candidate so its log can identify it",
    );

    let files = collect_release_candidate_files_with_scope(
        &album,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect("scan should succeed");
    assert!(files.bound_sheets().is_empty());
    assert!(
        crate::import::discid::compute_discid_from_categorized(&files).is_some(),
        "the rip log's TOC still yields a disc ID with the sheet unbound",
    );
}

/// A sheet that will not parse is not a sheet: it lands as a document, and its
/// audio keeps its role.
#[test]
fn unparseable_sheet_lands_as_a_document() {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(&album).unwrap();
    std::fs::write(album.join("Album.flac"), fake_flac()).unwrap();
    // No INDEX: the sheet does not parse.
    std::fs::write(
        album.join("Album.cue"),
        "FILE \"Album.flac\" WAVE\n  TRACK 01 AUDIO\n    TITLE \"Missing Index\"\n",
    )
    .unwrap();

    let items = scan_items(tmp.path().to_path_buf());
    assert_eq!(items.len(), 1);
    match &items[0] {
        ScanItem::Valid(candidate) => {
            assert!(
                candidate
                    .files
                    .documents()
                    .any(|d| d.file_name == "Album.cue"),
                "an unparseable sheet stays a document",
            );
            assert!(candidate.files.track_sheets().next().is_none());
            assert_eq!(candidate.files.audio().count(), 1);
        }
        ScanItem::Invalid(invalid) => {
            panic!(
                "an unparseable sheet must not invalidate: {}",
                invalid.reason
            )
        }
        ScanItem::Discovered(_) | ScanItem::Boundary(_) => {
            panic!("terminal scan helper returned a progress item")
        }
    }
}

/// The hash covers every file the release uploads, including audio no sheet
/// references — which the either/or silently dropped from both. This is the
/// test that fails if that omission is ever reintroduced.
#[test]
fn content_hash_covers_audio_no_sheet_references() {
    let build = |dir: &Path, with_bonus: bool| {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("Album.flac"), fake_flac()).unwrap();
        std::fs::write(
            dir.join("Album.cue"),
            make_cue_content("Album.flac", "Album Title"),
        )
        .unwrap();
        if with_bonus {
            std::fs::write(dir.join("bonus.flac"), fake_flac()).unwrap();
        }
        collect_release_candidate_files_with_scope(
            dir,
            crate::import::ReleaseFileScope::Recursive,
            &StoredCandidateEdits::none(),
        )
        .expect("scan should succeed")
        .content_hash()
    };
    let tmp = tempfile::tempdir().unwrap();
    let plain = build(&tmp.path().join("Plain"), false);
    let with_bonus = build(&tmp.path().join("Bonus"), true);
    assert_ne!(
        plain, with_bonus,
        "audio no sheet references must count toward the hash",
    );
}

/// The folder is the release, so an unrecognized sidecar is carried like every
/// other file: it becomes a row the import writes, and it counts toward the
/// hash. The pair that fails if someone narrows either set back — and they must
/// stay one set, or the fingerprint stops describing the payload it identifies.
#[test]
fn an_unrecognized_sidecar_is_carried_and_hashed() {
    const SIDECARS: [&str; 5] = ["rip.accurip", "rip.ffp", "rip.md5", "rip.nfo", "rip.sfv"];

    let tmp = tempfile::tempdir().unwrap();
    let bare = tmp.path().join("Bare");
    let with_sidecars = tmp.path().join("Sidecars");
    for dir in [&bare, &with_sidecars] {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("01.flac"), fake_flac()).unwrap();
    }
    for sidecar in SIDECARS {
        std::fs::write(with_sidecars.join(sidecar), b"scene notes").unwrap();
    }

    let bare_files = collect_release_candidate_files_with_scope(
        &bare,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .unwrap();
    let sidecar_files = collect_release_candidate_files_with_scope(
        &with_sidecars,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .unwrap();
    assert_eq!(
        sidecar_files
            .files
            .iter()
            .filter(|entry| matches!(entry.role, FileRole::Other))
            .map(|entry| entry.file.file_name.as_str())
            .collect::<Vec<_>>(),
        SIDECARS.to_vec(),
        "each sidecar is listed under the role for what the scan doesn't recognize",
    );

    // The rows the import writes come from the same iterator the hash covers.
    let carried: Vec<_> = crate::import::handle::flatten_categorized_files(&sidecar_files)
        .into_iter()
        .map(|file| file.file_name)
        .collect();
    for sidecar in SIDECARS {
        assert!(
            carried.contains(&sidecar.to_string()),
            "{sidecar} must become a file row, got {carried:?}",
        );
    }

    assert_ne!(
        bare_files.content_hash(),
        sidecar_files.content_hash(),
        "a file the release carries must count toward the hash",
    );
}

/// The directive binds, not the filename: a sheet and the audio it names pair
/// even when their names have nothing in common.
#[test]
fn a_binding_survives_a_rename_of_the_sheet() {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(&album).unwrap();
    std::fs::write(album.join("Audio.flac"), fake_flac()).unwrap();
    std::fs::write(
        album.join("Completely Unrelated.cue"),
        make_cue_content("Audio.flac", "Album Title"),
    )
    .unwrap();

    let files = collect_release_candidate_files_with_scope(
        &album,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect("scan should succeed");
    let bound = files.bound_sheets();
    assert_eq!(bound.len(), 1);
    assert_eq!(bound[0].file.file_name, "Completely Unrelated.cue");
    assert_eq!(bound[0].audio.file_name, "Audio.flac");
    assert_eq!(
        bound[0].audio.relative_path, "Audio.flac",
        "`describes` names the audio by its file id",
    );
}

/// Widening the model must not swallow real defects: audio that will not decode
/// and a folder with no audio at all still invalidate.
#[test]
fn corrupt_audio_and_empty_folders_still_invalidate() {
    let tmp = tempfile::tempdir().unwrap();
    let corrupt = tmp.path().join("Corrupt");
    std::fs::create_dir_all(&corrupt).unwrap();
    // Non-empty so the folder is still detected as a leaf, but the bytes are
    // not FLAC — the file will not decode.
    std::fs::write(corrupt.join("01.flac"), b"not a flac at all").unwrap();
    let items = scan_items(corrupt);
    assert_eq!(items.len(), 1);
    assert!(
        matches!(
            &items[0],
            ScanItem::Invalid(InvalidCandidate {
                reason: InvalidReason::CorruptAudioFile { .. },
                ..
            })
        ),
        "corrupt audio is still a real defect",
    );

    let empty = tmp.path().join("Empty");
    std::fs::create_dir_all(&empty).unwrap();
    std::fs::write(empty.join("cover.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();
    assert!(
        scan_items(empty.clone()).is_empty(),
        "a folder with no audio has nothing to import",
    );
    assert!(
        matches!(
            collect_release_candidate_files_with_scope(
                &empty,
                crate::import::ReleaseFileScope::Recursive,
                &StoredCandidateEdits::none()
            ),
            Err(crate::import::ImportError::InvalidFolder(
                InvalidReason::NoValidAudio
            ))
        ),
        "categorizing an audio-less folder names NoValidAudio",
    );
}

/// The scan proposes one cover from the conventional filenames; every other
/// image is artwork, and both are the release's images.
#[test]
fn the_scan_proposes_one_cover_from_the_conventional_names() {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(&album).unwrap();
    std::fs::write(album.join("01.flac"), fake_flac()).unwrap();
    for image in ["back.jpg", "cover.jpg", "folder.jpg"] {
        std::fs::write(album.join(image), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();
    }

    let files = collect_release_candidate_files_with_scope(
        &album,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect("scan should succeed");
    assert_eq!(
        cover_names(&files),
        vec!["cover.jpg"],
        "one image leads the release — the first conventional name in file order",
    );
    assert_eq!(files.artwork().count(), 3, "all three are still images");
}

/// A release-root image outranks a nested one. Sorting by relative path puts
/// `Artwork/front.jpg` ahead of `cover.jpg`, so taking the first conventional
/// name outright would propose the file inside the subfolder.
#[test]
fn a_root_level_cover_outranks_one_in_a_subfolder() {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(album.join("Artwork")).unwrap();
    std::fs::write(album.join("01.flac"), fake_flac()).unwrap();
    std::fs::write(album.join("Artwork/front.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();
    std::fs::write(album.join("cover.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();

    let files = collect_release_candidate_files_with_scope(
        &album,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect("scan should succeed");
    assert_eq!(cover_names(&files), vec!["cover.jpg"]);

    // With nothing at the root, the nested one leads.
    let nested_only = tmp.path().join("Nested");
    std::fs::create_dir_all(nested_only.join("Artwork")).unwrap();
    std::fs::write(nested_only.join("01.flac"), fake_flac()).unwrap();
    std::fs::write(
        nested_only.join("Artwork/front.jpg"),
        [0xFF, 0xD8, 0xFF, 0xE0],
    )
    .unwrap();
    let files = collect_release_candidate_files_with_scope(
        &nested_only,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect("scan should succeed");
    assert_eq!(cover_names(&files), vec!["front.jpg"]);
}

/// File names of every image the scan proposed as the release's cover.
fn cover_names(files: &CategorizedFiles) -> Vec<&str> {
    files
        .files
        .iter()
        .filter(|entry| matches!(entry.role, FileRole::Cover))
        .map(|entry| entry.file.file_name.as_str())
        .collect()
}

// ── The sheet↔audio binding is a user decision ──────────────────────────
//
// The scan proposes; these pin what happens when the user overrules it.

/// The walkthrough folder, one step on from the roles task: a twelve-track
/// sheet written against a WAV, the FLAC it was actually encoded to, and the
/// rip log. Unbound it imports as one track; bound, the slot count comes from
/// the sheet and the disc ID becomes computable from sheet plus audio.
///
/// This is the task's whole point — the information needed to fix the folder
/// was on screen all along and the app had no way to accept it.
#[test]
fn binding_a_sheet_whose_directive_missed_makes_the_folder_a_twelve_track_disc() {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(&album).unwrap();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cue_flac/Test Album.flac"),
        album.join("cd.flac"),
    )
    .unwrap();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/test_album.log"),
        album.join("rip.log"),
    )
    .unwrap();
    std::fs::write(
        album.join("cd.cue"),
        make_cue_content_n_tracks("cd.wav", "Album Title", 12),
    )
    .unwrap();

    let unbound = collect_release_candidate_files_with_scope(
        &album,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect("scan");
    assert_eq!(
        unbound.track_count(),
        1,
        "the directive names a file that is not here, so the image is one track",
    );
    assert_eq!(unbound.format_label, "FLAC");

    let bound = scan_with_binding(&album, &unbound, "cd.cue", Some("cd.flac"));

    assert_eq!(
        bound.track_count(),
        12,
        "bound, the slot count comes from the sheet rather than the file",
    );
    assert_eq!(
        bound.format_label, "CUE+FLAC",
        "the label follows the probed codec of the audio the user named",
    );
    let sheets: Vec<_> = bound.track_sheets().collect();
    assert_eq!(
        sheets[0].binding,
        &SheetBinding::Describes {
            file_id: "cd.flac".to_string()
        },
    );
    assert_eq!(bound.bound_sheets()[0].audio.file_name, "cd.flac");
    assert!(
        crate::import::discid::compute_discid_from_categorized(&bound).is_some(),
        "a bound sheet plus its audio yields a disc ID",
    );
}

/// A codec the CUE path cannot seek inside is refused where the choice is
/// offered, with the codec named — never handed to the user as a choice that
/// fails at commit. The FLAC beside it stays offerable, so this is the refusal
/// and not an empty picker.
#[test]
fn audio_a_sheet_cannot_use_is_refused_at_offer_time_with_the_codec_named() {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(&album).unwrap();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR"));
    std::fs::copy(
        fixtures.join("tests/fixtures/cue_flac/Test Album.flac"),
        album.join("cd.flac"),
    )
    .unwrap();
    std::fs::copy(
        fixtures.join("test-fixtures/audio-format/placeholder-mp3.mp3"),
        album.join("cd.mp3"),
    )
    .unwrap();
    std::fs::write(
        album.join("cd.cue"),
        make_cue_content_n_tracks("cd.wav", "Album Title", 12),
    )
    .unwrap();

    let files = collect_release_candidate_files_with_scope(
        &album,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect("scan");
    let options = files.sheet_binding_options("cd.cue");

    assert_eq!(
        options,
        vec![
            SheetBindingOption {
                file_id: "cd.flac".to_string(),
                offer: SheetBindingOffer::Offered,
            },
            SheetBindingOption {
                file_id: "cd.mp3".to_string(),
                offer: SheetBindingOffer::RefusedCodec {
                    codec: "MP3".to_string()
                },
            },
        ],
        "the MP3 is refused with its codec named, not offered and rejected later",
    );
}

/// Clearing a binding leaves the sheet describing nothing. It does **not**
/// restore the scan's proposal: someone who cleared a binding is saying the
/// guess was wrong, and re-guessing it is the one answer that is certainly not
/// what they asked for.
#[test]
fn clearing_a_binding_leaves_it_unbound_rather_than_re_guessed() {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(&album).unwrap();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cue_flac/Test Album.flac"),
        album.join("cd.flac"),
    )
    .unwrap();
    std::fs::write(
        album.join("cd.cue"),
        make_cue_content_n_tracks("cd.flac", "Album Title", 12),
    )
    .unwrap();

    let proposed = collect_release_candidate_files_with_scope(
        &album,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect("scan");
    assert_eq!(
        proposed.track_count(),
        12,
        "the directive resolves, so the scan proposes the binding on its own",
    );

    let cleared = scan_with_binding(&album, &proposed, "cd.cue", None);

    assert_eq!(
        cleared.track_sheets().next().unwrap().binding,
        &SheetBinding::Unresolved,
        "the sheet the user cleared describes nothing, proposal or not",
    );
    assert_eq!(cleared.track_count(), 1);
    assert_eq!(cleared.format_label, "FLAC");
    assert!(cleared.bound_sheets().is_empty());
}

/// A binding whose audio leaves the folder is not silently kept. Removing the
/// file changes the file set, so it changes the hash the decision is stored
/// under, so the decision is unreachable and the candidate derives from what is
/// actually there. The behaviour is what matters; the hash is only how.
#[test]
fn a_binding_whose_audio_disappears_is_not_kept() {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(&album).unwrap();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cue_flac/Test Album.flac"),
        album.join("cd.flac"),
    )
    .unwrap();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/flac/01 Test Track 1.flac"),
        album.join("bonus.flac"),
    )
    .unwrap();
    std::fs::write(
        album.join("cd.cue"),
        make_cue_content_n_tracks("cd.wav", "Album Title", 12),
    )
    .unwrap();

    let scanned = collect_release_candidate_files_with_scope(
        &album,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect("scan");
    let stored = stored_binding(&scanned, "cd.cue", Some("cd.flac"));
    assert_eq!(
        collect_release_candidate_files_with_scope(
            &album,
            crate::import::ReleaseFileScope::Recursive,
            &stored
        )
        .expect("scan")
        .track_count(),
        12,
        "the binding applies while the audio it names is here",
    );

    std::fs::remove_file(album.join("cd.flac")).unwrap();

    let after = collect_release_candidate_files_with_scope(
        &album,
        crate::import::ReleaseFileScope::Recursive,
        &stored,
    )
    .expect("scan");
    assert_eq!(
        after.track_sheets().next().unwrap().binding,
        &SheetBinding::Unresolved,
        "the folder derives from what is on disk, with no memory of the removed pairing",
    );
    assert_eq!(
        after.track_count(),
        1,
        "one standalone track is all that is left"
    );
}

/// The stored bindings a fresh scan of `folder` would apply, if the user had
/// made this one decision about `files`.
fn stored_binding(
    files: &CategorizedFiles,
    sheet_file_id: &str,
    audio_file_id: Option<&str>,
) -> StoredCandidateEdits {
    let mut edits = SheetBindingEdits::default();
    edits.set(
        sheet_file_id.to_string(),
        match audio_file_id {
            Some(file_id) => UserSheetBinding::Describes {
                file_id: file_id.to_string(),
            },
            None => UserSheetBinding::Cleared,
        },
    );
    StoredCandidateEdits::new(HashMap::from([(
        files.content_hash(),
        CandidateFileEdits {
            sheet_bindings: edits,
            ..Default::default()
        },
    )]))
}

/// Re-scan `folder` as it reads once the user has made one binding decision.
fn scan_with_binding(
    folder: &Path,
    files: &CategorizedFiles,
    sheet_file_id: &str,
    audio_file_id: Option<&str>,
) -> CategorizedFiles {
    collect_release_candidate_files_with_scope(
        folder,
        crate::import::ReleaseFileScope::Recursive,
        &stored_binding(files, sheet_file_id, audio_file_id),
    )
    .expect("scan")
}

// ── What a role makes of a file, and which files are the release's tracks ────

/// A folder holding a disc image, its sheet, and two loose bonus tracks. The
/// "Becomes" column reads off the folder alone — no release has been picked —
/// and it says which slots each file backs: the sheet carves the first eleven,
/// the bonus files take one each, and the container the sheet speaks for backs
/// none of its own.
#[test]
fn becomes_names_the_slots_each_file_backs() {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(&album).unwrap();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cue_flac/Test Album.flac"),
        album.join("CDImage.flac"),
    )
    .unwrap();
    std::fs::write(
        album.join("CDImage.cue"),
        make_cue_content_n_tracks("CDImage.flac", "Album Title", 11),
    )
    .unwrap();
    std::fs::write(album.join("bonus-1.flac"), fake_flac()).unwrap();
    std::fs::write(album.join("bonus-2.flac"), fake_flac()).unwrap();
    std::fs::write(album.join("cover.jpg"), fake_jpeg()).unwrap();

    let files = collect_release_candidate_files_with_scope(
        &album,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect("scan");
    let becomes: Vec<(&str, FileBecomes)> = files
        .files
        .iter()
        .map(|entry| entry.file.relative_path.as_str())
        .zip(files.becomes())
        .collect();

    assert_eq!(
        becomes,
        vec![
            ("CDImage.cue", FileBecomes::Slots { first: 1, last: 11 }),
            ("CDImage.flac", FileBecomes::NoSlots),
            (
                "bonus-1.flac",
                FileBecomes::Slots {
                    first: 12,
                    last: 12
                }
            ),
            (
                "bonus-2.flac",
                FileBecomes::Slots {
                    first: 13,
                    last: 13
                }
            ),
            ("cover.jpg", FileBecomes::NoSlots),
        ],
    );
}

/// A directory of nothing but artwork collapses to one row. A directory that
/// also holds the release's cover does not — the cover has to stay visible on
/// a row of its own — and neither does one holding two different jobs.
#[test]
fn a_homogeneous_directory_collapses_to_one_row() {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(album.join("scans")).unwrap();
    std::fs::create_dir_all(album.join("logs")).unwrap();
    std::fs::write(album.join("01.flac"), fake_flac()).unwrap();
    std::fs::write(album.join("cover.jpg"), fake_jpeg()).unwrap();
    for index in 1..=4 {
        std::fs::write(album.join(format!("scans/page-{index}.jpg")), fake_jpeg()).unwrap();
    }
    std::fs::write(album.join("logs/rip.log"), b"log").unwrap();
    std::fs::write(album.join("logs/notes.txt"), b"notes").unwrap();
    // A directory holding two different jobs stays expanded.
    std::fs::create_dir_all(album.join("extras")).unwrap();
    std::fs::write(album.join("extras/back.jpg"), fake_jpeg()).unwrap();
    std::fs::write(album.join("extras/info.txt"), b"info").unwrap();

    let files = collect_release_candidate_files_with_scope(
        &album,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect("scan");
    let collapsed: Vec<(String, FileRowKind, u32)> = files
        .collapsed_directories()
        .into_iter()
        .map(|dir| (dir.dir_prefix, dir.kind, dir.count))
        .collect();

    assert_eq!(
        collapsed,
        vec![
            ("logs/".to_string(), FileRowKind::Document, 2),
            ("scans/".to_string(), FileRowKind::Image, 4),
        ],
    );
}

/// Taking a file out of the tracklist stops it producing a slot, and the file
/// stays in the release: the folder is the release, so it still imports. The
/// content hash is what the decision is stored under, so it must not move.
#[test]
fn a_file_taken_out_of_the_tracklist_stops_being_a_slot_and_stays_in_the_release() {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(&album).unwrap();
    for index in 1..=3 {
        std::fs::write(album.join(format!("{index:02}.flac")), fake_flac()).unwrap();
    }

    let mut files = collect_release_candidate_files_with_scope(
        &album,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect("scan");
    let hash = files.content_hash();
    assert_eq!(files.track_count(), 3);

    let mut roles = FileRoleEdits::default();
    roles.set("03.flac".to_string(), FileRoleChoice::NotATrack);
    files
        .apply_candidate_file_edits(&CandidateFileEdits {
            file_roles: roles.clone(),
            ..Default::default()
        })
        .expect("taking one of three out is fine");

    assert_eq!(files.track_count(), 2, "it stops being one of the tracks");
    assert_eq!(
        files.becomes().last(),
        Some(&FileBecomes::NoSlots),
        "and it backs no slot",
    );
    assert_eq!(
        files.release_files().count(),
        3,
        "the folder is the release: the file is still carried, uploaded and exported",
    );
    assert_eq!(
        files.content_hash(),
        hash,
        "the hash covers files, never role decisions, so the row stays addressable",
    );

    // And a fresh walk with the decision stored reads the same way, which is
    // what makes an exclusion survive re-picking a release and relaunching.
    let stored = StoredCandidateEdits::new(HashMap::from([(
        hash,
        CandidateFileEdits {
            file_roles: roles,
            ..Default::default()
        },
    )]));
    let reopened = collect_release_candidate_files_with_scope(
        &album,
        crate::import::ReleaseFileScope::Recursive,
        &stored,
    )
    .expect("scan");
    assert_eq!(reopened.track_count(), 2);
    assert_eq!(reopened.release_files().count(), 3);
}

/// Taking out the last audio a folder has is refused, and refused on a copy, so
/// nothing is written and the candidate is left exactly as it was. A release
/// with no tracks is not a state the rest of the import can describe.
#[test]
fn taking_out_the_last_audio_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(&album).unwrap();
    std::fs::write(album.join("01.flac"), fake_flac()).unwrap();

    let mut files = collect_release_candidate_files_with_scope(
        &album,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect("scan");
    let mut roles = FileRoleEdits::default();
    roles.set("01.flac".to_string(), FileRoleChoice::NotATrack);

    let err = files
        .apply_candidate_file_edits(&CandidateFileEdits {
            file_roles: roles,
            ..Default::default()
        })
        .expect_err("there would be nothing left to import");
    assert_eq!(err, InvalidReason::NoValidAudio);
}

/// A decision only ever moves a file the scan read as audio. A stored decision
/// naming an image is ignored rather than applied to whatever now sits at that
/// path, and an image is never offered the choice in the first place.
#[test]
fn only_audio_carries_a_role_decision() {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(&album).unwrap();
    std::fs::write(album.join("01.flac"), fake_flac()).unwrap();
    std::fs::write(album.join("cover.jpg"), fake_jpeg()).unwrap();

    let mut files = collect_release_candidate_files_with_scope(
        &album,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect("scan");
    let alternatives: Vec<(&str, usize)> = files
        .files
        .iter()
        .map(|entry| {
            (
                entry.file.relative_path.as_str(),
                entry.role_alternatives().len(),
            )
        })
        .collect();
    assert_eq!(alternatives, vec![("01.flac", 2), ("cover.jpg", 0)]);

    let mut roles = FileRoleEdits::default();
    roles.set("cover.jpg".to_string(), FileRoleChoice::NotATrack);
    files
        .apply_candidate_file_edits(&CandidateFileEdits {
            file_roles: roles,
            ..Default::default()
        })
        .expect("a decision about a non-audio file changes nothing");

    assert!(matches!(files.files[1].role, FileRole::Cover));
    assert_eq!(files.track_count(), 1);
}
