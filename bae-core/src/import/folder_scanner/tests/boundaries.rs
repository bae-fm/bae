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
