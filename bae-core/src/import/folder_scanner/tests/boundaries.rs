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

    let mut first_item = item_rx
        .recv_timeout(std::time::Duration::from_secs(2))
        .expect("first release emits");
    // The folder's own reading comes first — it is what says the releases
    // below it are separate.
    if matches!(first_item, ScanItem::Decided { .. }) {
        first_item = item_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("first release emits");
    }
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
fn a_wrapper_the_scan_reads_makes_its_children_actionable_at_once() {
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
    // `Release 01`, `Release 02`, `Release 99` are not the parts of one
    // release, so the scan says so before it walks them and the ones it has
    // reached are ready to identify.
    assert!(available.iter().any(|item| matches!(
        item,
        ScanItem::Decided {
            key,
            decision: FolderReleaseDecision::KeepAsSeparateReleases,
        } if key.relative_folder_path == "Group"
    )));
    assert!(
        available
            .iter()
            .any(|item| matches!(item, ScanItem::Valid(_))),
        "a wrapper the scan has read does not hold its descendants back"
    );

    let (lock, condition) = &*gate;
    *lock.lock().unwrap() = true;
    condition.notify_all();
    scan.join().unwrap().unwrap();
}

#[test]
fn a_folder_with_its_own_tracks_beside_children_is_read_as_several_releases() {
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
    assert!(item_rx.try_iter().any(|item| matches!(
        item,
        ScanItem::Decided {
            key,
            decision: FolderReleaseDecision::KeepAsSeparateReleases,
        } if key.relative_folder_path == "Group"
    )));

    let (lock, condition) = &*gate;
    *lock.lock().unwrap() = true;
    condition.notify_all();
    scan.join().unwrap().unwrap();
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
    // Years are not disc numbers: an album folder per year is several
    // releases, and each release keeps its own folder.
    assert!(candidates.contains_key("Solo Artist/1971 - Ordinary"));
    assert!(candidates.contains_key("Collective/Studio Albums/1990 - First"));
    assert!(candidates.contains_key("Collective/Studio Albums/1992 - Second"));
    assert!(!candidates.contains_key("Collective"));
    assert!(!candidates.contains_key("Collective/Studio Albums"));

    // `CD1` and `CD2` are the parts of one release, so the box is one
    // candidate over both of them and neither disc stands on its own.
    assert!(candidates.contains_key("Solo Artist/1973 - Box"));
    assert!(!candidates.contains_key("Solo Artist/1973 - Box/CD1"));
    assert!(!candidates.contains_key("Solo Artist/1973 - Box/CD2"));
    assert_eq!(
        candidates["Solo Artist/1973 - Box"].scope,
        ReleaseFileScope::Recursive
    );

    // The user's own answer replaces the scan's.
    let separate = scan_for_candidates_with_decisions_collect(
        root,
        FolderReleaseDecisions::new(HashMap::from([(
            "Solo Artist/1973 - Box".to_string(),
            (
                FolderReleaseDecision::KeepAsSeparateReleases,
                FolderReleaseDecisionAuthor::User,
            ),
        )])),
    );
    assert!(separate.iter().any(
        |item| matches!(item, ScanItem::Valid(candidate) if candidate.display_path == "Solo Artist/1973 - Box/CD1")
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
fn a_wrapper_of_numbered_parts_combines_unless_the_user_says_otherwise() {
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

    // `Part 01` and `Part 02` number themselves 1 and 2, so with nothing
    // stored the scan reads the wrapper as one release and says so.
    let read_by_the_scan =
        scan_for_candidates_with_decisions_collect(root.clone(), FolderReleaseDecisions::default());
    assert!(read_by_the_scan.iter().any(|item| matches!(
        item,
        ScanItem::Decided {
            key,
            decision: FolderReleaseDecision::CombineAsOneRelease,
        } if key.relative_folder_path == "Collection/Release Wrapper"
    )));

    let combined = read_by_the_scan
        .iter()
        .find_map(|item| match item {
            ScanItem::Valid(candidate) => Some(candidate),
            ScanItem::Discovered(_) | ScanItem::Invalid(_) | ScanItem::Decided { .. } => None,
        })
        .expect("combined wrapper is actionable");
    assert_eq!(combined.path, wrapper);
    assert_eq!(combined.scope, ReleaseFileScope::Recursive);
    assert_eq!(combined.files.release_files().count(), 3);

    let separate = scan(FolderReleaseDecisions::new(HashMap::from([(
        "Collection/Release Wrapper".to_string(),
        (
            FolderReleaseDecision::KeepAsSeparateReleases,
            FolderReleaseDecisionAuthor::User,
        ),
    )])));
    let separate: Vec<_> = separate
        .iter()
        .filter_map(|item| match item {
            ScanItem::Valid(candidate) => Some(candidate),
            ScanItem::Discovered(_) | ScanItem::Invalid(_) | ScanItem::Decided { .. } => None,
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
fn labeled_disc_numbers_win_over_other_numbers_in_part_names() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Queue");
    let wrapper = root.join("Collection").join("Release Wrapper");
    for child in [
        "1935 Archive - CD 1 - Location A",
        "CD 2 - 1935-36 - Location A",
        "1936 Archive - CD 3 - 1937 - Location B",
        "Session 1937-40 - CD 4 - Location C",
        "Archive 1940 - CD 5 - 1941 - Location D",
    ] {
        let child = wrapper.join(child);
        std::fs::create_dir_all(&child).unwrap();
        std::fs::write(child.join("track.flac"), fake_flac()).unwrap();
    }

    let items = scan_for_candidates_with_decisions_collect(
        root,
        FolderReleaseDecisions::default(),
    );

    assert!(items.iter().any(|item| matches!(
        item,
        ScanItem::Decided {
            key,
            decision: FolderReleaseDecision::CombineAsOneRelease,
        } if key.relative_folder_path == "Collection/Release Wrapper"
    )));
    let releases: Vec<_> = items
        .iter()
        .filter_map(|item| match item {
            ScanItem::Valid(candidate) => Some(candidate),
            _ => None,
        })
        .collect();
    assert_eq!(releases.len(), 1);
    assert_eq!(releases[0].path, wrapper);
    assert_eq!(releases[0].scope, ReleaseFileScope::Recursive);
}

/// A folder with tracks of its own and an album folder beside them is two
/// releases, and each one is a candidate carrying the reading that made it
/// one — which is what the flip control on the row rewrites.
#[test]
fn tracks_beside_an_album_folder_are_two_releases_that_say_so() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Queue");
    let parent = root.join("Group").join("Artist");
    let child = parent.join("Album");
    std::fs::create_dir_all(&child).unwrap();
    std::fs::write(parent.join("parent.flac"), fake_flac()).unwrap();
    std::fs::write(child.join("child.flac"), fake_flac()).unwrap();

    let items = scan_items(&root);
    let candidates: Vec<_> = items
        .iter()
        .filter_map(|item| match item {
            ScanItem::Valid(candidate) => Some(candidate),
            _ => None,
        })
        .collect();
    assert_eq!(candidates.len(), 2);
    assert!(candidates.iter().all(|candidate| {
        candidate.resolved_boundaries.iter().any(|resolved| {
            resolved.key.relative_folder_path == "Group/Artist"
                && resolved.decision == FolderReleaseDecision::KeepAsSeparateReleases
        })
    }));
}

/// The same when the folder's own tracks do not make a valid release: the
/// invalid folder still carries the reading, so the row it draws offers the
/// same flip.
#[test]
fn an_invalid_folder_beside_an_album_folder_still_carries_the_reading() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Queue");
    let parent = root.join("Group").join("Artist");
    let child = parent.join("Album");
    std::fs::create_dir_all(&child).unwrap();
    std::fs::write(parent.join("parent.flac"), fake_flac()).unwrap();
    std::fs::write(parent.join("broken.jpg"), b"not an image").unwrap();
    std::fs::write(child.join("child.flac"), fake_flac()).unwrap();

    let items = scan_items(&root);
    let invalid = items
        .iter()
        .find_map(|item| match item {
            ScanItem::Invalid(candidate) => Some(candidate),
            _ => None,
        })
        .expect("the folder's own files do not make a release");
    assert!(invalid.resolved_boundaries.iter().any(|resolved| {
        resolved.key.relative_folder_path == "Group/Artist"
            && resolved.decision == FolderReleaseDecision::KeepAsSeparateReleases
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
            (
                FolderReleaseDecision::KeepAsSeparateReleases,
                FolderReleaseDecisionAuthor::User,
            ),
        )])),
        |item| {
            if !matches!(item, ScanItem::Discovered(_) | ScanItem::Decided { .. }) {
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

/// A folder of scans beside the numbered parts is a sidecar the release
/// carries, not a third part that failed to number itself. It yields no
/// candidate, so it is not read as one of the parts and the parts still number
/// themselves 1..=N.
#[test]
fn a_folder_that_yields_nothing_is_not_one_of_the_parts() {
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

    let items = scan_for_candidates_with_decisions_collect(root, FolderReleaseDecisions::default());
    assert!(items.iter().any(|item| matches!(
        item,
        ScanItem::Decided {
            key,
            decision: FolderReleaseDecision::CombineAsOneRelease,
        } if key.relative_folder_path == "Collection/Release Wrapper"
    )));
    let releases: Vec<_> = items
        .iter()
        .filter_map(|item| match item {
            ScanItem::Valid(candidate) => Some(candidate.display_path.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(releases, vec!["Collection/Release Wrapper"]);
}

/// A child that does hold audio and carries no number still breaks the run:
/// the folder holds several releases, not one release's parts.
#[test]
fn a_part_with_no_number_makes_it_several_releases() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Queue");
    let wrapper = root.join("Collection").join("Release Wrapper");
    for child in ["Release 01", "Release 02", "Bonus"] {
        let child = wrapper.join(child);
        std::fs::create_dir_all(&child).unwrap();
        std::fs::write(child.join("track.flac"), fake_flac()).unwrap();
    }

    let items = scan_for_candidates_with_decisions_collect(root, FolderReleaseDecisions::default());
    assert!(items.iter().any(|item| matches!(
        item,
        ScanItem::Decided {
            key,
            decision: FolderReleaseDecision::KeepAsSeparateReleases,
        } if key.relative_folder_path == "Collection/Release Wrapper"
    )));
    let releases: Vec<_> = items
        .iter()
        .filter_map(|item| match item {
            ScanItem::Valid(candidate) => Some(candidate.display_path.as_str()),
            _ => None,
        })
        .collect();
    assert!(releases.contains(&"Collection/Release Wrapper/Release 01"));
    assert!(releases.contains(&"Collection/Release Wrapper/Bonus"));
}

/// An album folder whose only subfolder is artwork yields one release, so
/// there is nothing to combine it with and no decision to record — the pane
/// used to offer the album a choice between itself and its own artwork.
#[test]
fn a_folder_that_yields_one_release_decides_nothing() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Queue");
    let album = root.join("Artist - Album");
    std::fs::create_dir_all(album.join("Artwork")).unwrap();
    std::fs::write(album.join("track.flac"), fake_flac()).unwrap();
    std::fs::write(album.join("Artwork").join("front.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();

    let items = scan_for_candidates_with_decisions_collect(root, FolderReleaseDecisions::default());
    assert!(
        !items
            .iter()
            .any(|item| matches!(item, ScanItem::Decided { .. })),
        "one release is nothing to decide about: {items:?}"
    );
    let releases: Vec<_> = items
        .iter()
        .filter_map(|item| match item {
            ScanItem::Valid(candidate) => Some(candidate.display_path.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(releases, vec!["Artist - Album"]);
    assert!(items.iter().any(|item| matches!(
        item,
        ScanItem::Valid(candidate)
            if candidate.resolved_boundaries.is_empty()
    )));
}

/// A box of numbered parts inside a collection of albums: the box is one
/// release, its sibling album is another, and the collection around them is
/// not one release just because it holds both.
#[test]
fn a_numbered_box_inside_a_collection_is_one_release() {
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
    let mut paths: Vec<_> = items
        .iter()
        .filter_map(|item| match item {
            ScanItem::Valid(candidate) => Some(candidate.display_path.as_str()),
            _ => None,
        })
        .collect();
    paths.sort_unstable();
    assert_eq!(paths, vec!["Collection/Box", "Collection/Release 03"]);
}

/// Two walks of one unchanged tree produce the same items, in the same order.
///
/// This is what lets a pass write and announce only what changed: the
/// comparison it makes is between the item it just built and the row it stored
/// last time, so anything the walk decides differently from one pass to the
/// next — a set iterated in whatever order it felt like, a list left unsorted —
/// would read as a change forever and rewrite the row on every pass.
#[test]
fn two_walks_of_one_tree_produce_the_same_items() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Queue");
    for release in [
        "Artist - Album",
        "Box/Disc 1",
        "Box/Disc 2",
        "Wrapper/Live in Tokyo",
        "Wrapper/Live in Osaka",
    ] {
        let release = root.join(release);
        std::fs::create_dir_all(&release).unwrap();
        for track in ["01.flac", "02.flac", "03.flac"] {
            std::fs::write(release.join(track), fake_flac()).unwrap();
        }
        std::fs::write(release.join("cover.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();
        std::fs::write(release.join("rip.log"), "log").unwrap();
    }
    std::fs::create_dir_all(root.join("Box/Scans")).unwrap();
    std::fs::write(root.join("Box/Scans/front.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();

    let first = scan_items(&root);
    let second = scan_items(&root);

    assert_eq!(first.len(), second.len(), "{first:?} vs {second:?}");
    for (left, right) in first.iter().zip(second.iter()) {
        assert_eq!(left, right);
    }
}

/// A folder of loose files whose releases all sit under one child folder is a
/// wrapper, not a question. Nothing about it is the user's to answer: the child
/// holding the releases has already been read, and this folder is only where
/// reading them as one is offered.
#[test]
fn a_wrapper_over_one_folder_of_releases_asks_nothing() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Queue");
    let wrapper = root.join("Freddie Roach");
    let inner = wrapper.join("Sound");
    for album in ["Brown Sugar", "Mocha Motion", "Good Move"] {
        let album = inner.join(album);
        std::fs::create_dir_all(&album).unwrap();
        std::fs::write(album.join("01.flac"), fake_flac()).unwrap();
    }
    // The loose files beside the child folder, which is what used to make this
    // shape a card rather than a reading.
    std::fs::create_dir_all(&wrapper).unwrap();
    std::fs::write(wrapper.join("front.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();
    std::fs::write(wrapper.join("notes.txt"), "notes").unwrap();

    let items = scan_items(&root);

    let mut releases: Vec<_> = items
        .iter()
        .filter_map(|item| match item {
            ScanItem::Valid(candidate) => Some(candidate.display_path.as_str()),
            _ => None,
        })
        .collect();
    releases.sort_unstable();
    assert_eq!(
        releases,
        vec![
            "Freddie Roach/Sound/Brown Sugar",
            "Freddie Roach/Sound/Good Move",
            "Freddie Roach/Sound/Mocha Motion",
        ]
    );
    // Every one of them names the wrapper, so the header over them offers to
    // read the three as one release.
    assert!(items.iter().all(|item| match item {
        ScanItem::Valid(candidate) => candidate
            .combine_ancestor_key
            .as_ref()
            .is_some_and(|key| key.relative_folder_path == "Freddie Roach"),
        _ => true,
    }));
}
