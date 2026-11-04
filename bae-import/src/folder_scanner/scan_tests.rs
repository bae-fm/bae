//! A walk of a watched folder: where its releases begin and end, a folder
//! read as one release or several, its candidates said as they are found,
//! names that are not UTF-8, and a walk that is cancelled or cannot read a
//! folder.

use super::*;
use crate::folder_scanner::tests::*;
use std::sync::Arc;

/// Reads directories normally, except inside `blocked`, where it is held.
struct BlockingReader {
    blocked: PathBuf,
    held: crate::tests::gate::Held,
}

impl DirectoryReader for BlockingReader {
    fn read(
        &self,
        root: &Path,
        directory: &Path,
        cancellation: &ScanCancellation,
    ) -> Result<DirectoryListing, FolderScanError> {
        if directory == self.blocked {
            self.held.pass();
        }
        OsDirectoryReader.read(root, directory, cancellation)
    }

    fn folder_times(
        &self,
        root: &Path,
        folders: &[PathBuf],
        cancellation: &ScanCancellation,
    ) -> Result<Vec<FolderTimes>, FolderScanError> {
        OsDirectoryReader.folder_times(root, folders, cancellation)
    }
}

/// A scan of `root` running on its own thread, suspended inside `blocked`
/// until [`SuspendedScan::finish`] opens the gate.
struct SuspendedScan {
    gate: crate::tests::gate::Gate,
    /// Announces that the walk reached the blocked directory.
    entered: std::sync::mpsc::Receiver<()>,
    items: std::sync::mpsc::Receiver<WalkItem>,
    thread: std::thread::JoinHandle<Result<(), FolderScanError>>,
}

impl SuspendedScan {
    fn spawn(root: PathBuf, blocked: &str) -> Self {
        let (gate, held, entered) = crate::tests::gate::closed();
        let (item_tx, items) = std::sync::mpsc::channel();
        let reader = BlockingReader { blocked: PathBuf::from(blocked), held };
        let thread = std::thread::spawn(move || {
            scan_with_reader(&reader, root, |item| item_tx.send(item).expect("receive scan item"))
        });
        Self { gate, entered, items, thread }
    }

    /// Whether the scan says an item `wanted` matches while it is held, taking
    /// what it says up to that item. What the walk yields reaches the caller
    /// on the thread that started the scan, so it can arrive after the walk
    /// is held.
    fn says_while_held(&self, wanted: impl Fn(&WalkItem) -> bool) -> bool {
        while let Ok(item) = self.items.recv_timeout(std::time::Duration::from_secs(2)) {
            if wanted(&item) {
                return true;
            }
        }
        false
    }

    /// Let the blocked read proceed, and require the walk to finish cleanly.
    fn finish(self) {
        self.gate.open();
        self.thread.join().unwrap().unwrap();
    }
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
    assert!(candidates.iter().all(|candidate| candidate.scope == ReleaseFileScope::Direct));
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
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Queue");
    let group = root.join("Collection");
    let first = group.join("Release 01");
    // How the folder reads waits on looking into two of its children: until
    // the second is known to hold audio, the folder could be one release. So
    // the child the walk is held in is the third.
    let second = group.join("Release 03");
    let later = group.join("Release 99");
    for release in [&first, &second, &later] {
        std::fs::create_dir_all(release).unwrap();
        std::fs::write(release.join("track.flac"), fake_flac()).unwrap();
    }

    let scan = SuspendedScan::spawn(root.clone(), "Collection/Release 99");

    let mut first_item =
        scan.items.recv_timeout(std::time::Duration::from_secs(2)).expect("first release emits");
    // The folder's own reading comes first — it is what says the releases
    // below it are separate.
    if matches!(first_item, WalkItem::Reading { .. }) {
        first_item = scan
            .items
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("first release emits");
    }
    assert!(matches!(
        first_item,
        WalkItem::Entry(FolderEntry::Release(ScanItem::Discovered(candidate))) if candidate.path == first
    ));
    scan.entered
        .recv_timeout(std::time::Duration::from_secs(2))
        .expect("later directory is suspended");
    assert!(
        scan.says_while_held(
            |item| matches!(item, WalkItem::Entry(FolderEntry::Release(ScanItem::Valid(candidate))) if candidate.path == first)
        ),
        "the immediate listing proves the file-free group cannot own shared release files"
    );

    scan.finish();
}

#[test]
fn a_wrapper_the_scan_reads_makes_its_children_actionable_at_once() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Queue");
    let group = root.join("Group");
    std::fs::create_dir_all(&group).unwrap();
    std::fs::write(group.join("cover.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();
    for release in ["Release 01", "Release 03", "Release 99"] {
        let release = group.join(release);
        std::fs::create_dir_all(&release).unwrap();
        std::fs::write(release.join("track.flac"), fake_flac()).unwrap();
    }

    let scan = SuspendedScan::spawn(root.clone(), "Group/Release 99");

    scan.entered
        .recv_timeout(std::time::Duration::from_secs(2))
        .expect("final sibling is suspended");
    // `Release 01`, `Release 03`, `Release 99` have no part 2, so they are
    // not the discs of one release: once two of them are known to hold audio
    // the scan says so, and the ones it has reached are ready to identify.
    assert!(scan.says_while_held(|item| matches!(
        item,
        WalkItem::Reading {
            key,
            decision: FolderReleaseDecision::KeepAsSeparateReleases,
            ..
        } if key.relative_folder == bae_db::RelativeFolder::test("Group")
    )));
    assert!(
        scan.says_while_held(|item| matches!(
            item,
            WalkItem::Entry(FolderEntry::Release(ScanItem::Valid(_)))
        )),
        "a wrapper the scan has read does not hold its descendants back"
    );

    scan.finish();
}

#[test]
fn a_folder_with_its_own_tracks_beside_children_is_read_as_several_releases() {
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

    let scan = SuspendedScan::spawn(root.clone(), "Group/Release 99");

    scan.entered
        .recv_timeout(std::time::Duration::from_secs(2))
        .expect("final sibling is suspended");
    assert!(scan.says_while_held(|item| matches!(
        item,
        WalkItem::Reading {
            key,
            decision: FolderReleaseDecision::KeepAsSeparateReleases,
            ..
        } if key.relative_folder == bae_db::RelativeFolder::test("Group")
    )));

    scan.finish();
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
            candidates.insert(candidate.relative_folder.text(), candidate);
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
    assert_eq!(candidates["Solo Artist/1973 - Box"].scope, ReleaseFileScope::Recursive);

    // The user's own answer replaces the scan's.
    let separate = scan_for_candidates_with_decisions_collect(
        root,
        readings(&[(
            "Solo Artist/1973 - Box",
            FolderReleaseDecision::KeepAsSeparateReleases,
            FolderReleaseDecisionAuthor::User,
        )]),
    );
    assert!(separate.iter().any(
        |item| matches!(item, WalkItem::Entry(FolderEntry::Release(ScanItem::Valid(candidate))) if candidate.relative_folder.text() == "Solo Artist/1973 - Box/CD1")
    ));
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

        fn folder_times(
            &self,
            _root: &Path,
            folders: &[PathBuf],
            _cancellation: &ScanCancellation,
        ) -> Result<Vec<FolderTimes>, FolderScanError> {
            Ok(undated(folders))
        }
    }

    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let cancellation = ScanCancellation::new();
    let thread_cancellation = cancellation.clone();
    let scan = std::thread::spawn(move || {
        scan_for_candidates_with_reader_cancellable_and_directories(
            &CancellableReader { entered: entered_tx },
            PathBuf::from("/network"),
            &ScanReadings {
                stored: &StoredCandidateEdits::none(),
                decisions: &FolderReleaseDecisions::default(),
                new_grouping_key: &fresh_grouping_key,
            },
            &FileReads::new(Arc::new(crate::file_tag_snapshot::LoftyFileTagReader)),
            &thread_cancellation,
            |_| {},
            |_| {},
        )
    });
    entered_rx.recv_timeout(std::time::Duration::from_secs(2)).expect("reader did not start");
    cancellation.cancel();
    assert!(matches!(scan.join().unwrap(), Err(FolderScanError::Cancelled)));
}

/// A folder with tracks of its own and an album folder beside them is two
/// releases, and the scan says it read the folder that way.
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
    assert!(candidates.iter().all(|candidate| candidate.grouping.is_none()));
    assert!(scan_for_candidates_with_decisions_collect(root, FolderReleaseDecisions::default())
        .iter()
        .any(|item| matches!(
            item,
            WalkItem::Reading {
                key,
                decision: FolderReleaseDecision::KeepAsSeparateReleases,
                ..
            } if key.relative_folder == bae_db::RelativeFolder::test("Group/Artist")
        )));
}

/// The same when the folder's own tracks do not make a valid release: the
/// invalid folder is one of the releases the folder was read as.
#[test]
fn an_invalid_folder_beside_an_album_folder_still_carries_the_reading() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Queue");
    let parent = root.join("Group").join("Artist");
    let child = parent.join("Album");
    std::fs::create_dir_all(&child).unwrap();
    std::fs::write(parent.join("parent.flac"), fake_flac()).unwrap();
    std::fs::write(parent.join("broken.flac"), b"").unwrap();
    std::fs::write(child.join("child.flac"), fake_flac()).unwrap();

    let items = scan_items(&root);
    let invalid = items
        .iter()
        .find_map(|item| match item {
            ScanItem::Invalid(candidate) => Some(candidate),
            _ => None,
        })
        .expect("the folder's own files do not make a release");
    assert!(invalid.grouping.is_none());
    assert_eq!(invalid.path, parent);
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
        &readings(&[(
            "Group",
            FolderReleaseDecision::KeepAsSeparateReleases,
            FolderReleaseDecisionAuthor::User,
        )]),
        |item| {
            if !matches!(
                item,
                WalkItem::Entry(
                    FolderEntry::Release(ScanItem::Discovered(_)) | FolderEntry::Sidecar(_)
                )
            ) {
                items.push(item);
            }
        },
    )
    .unwrap();

    let invalid: Vec<_> = items
        .iter()
        .filter_map(|item| match item {
            WalkItem::Entry(FolderEntry::Release(ScanItem::Invalid(candidate))) => Some(candidate),
            _ => None,
        })
        .collect();
    assert_eq!(invalid.len(), 2);
    assert!(invalid.iter().all(|candidate| candidate.grouping.is_none()));
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
        WalkItem::Reading {
            key,
            decision: FolderReleaseDecision::CombineAsOneRelease,
            ..
        } if key.relative_folder == bae_db::RelativeFolder::test("Collection/Release Wrapper")
    )));
    let releases: Vec<_> = items
        .iter()
        .filter_map(|item| match item {
            WalkItem::Entry(FolderEntry::Release(ScanItem::Valid(candidate))) => {
                Some(candidate.relative_folder.text())
            }
            _ => None,
        })
        .collect();
    assert_eq!(releases, vec!["Collection/Release Wrapper"]);
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
        !items.iter().any(|item| matches!(item, WalkItem::Reading { .. })),
        "one release is nothing to decide about: {items:?}"
    );
    let releases: Vec<_> = items
        .iter()
        .filter_map(|item| match item {
            WalkItem::Entry(FolderEntry::Release(ScanItem::Valid(candidate))) => {
                Some(candidate.relative_folder.text())
            }
            _ => None,
        })
        .collect();
    assert_eq!(releases, vec!["Artist - Album"]);
    assert!(items.iter().any(|item| matches!(
        item,
        WalkItem::Entry(FolderEntry::Release(ScanItem::Valid(candidate)))
            if candidate.grouping.is_none()
    )));
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

    // The first walk decides how each folder reads, and stores it; every walk
    // after reads under what was stored, which is what a pass compares with.
    let stored = FolderReleaseDecisions::new(
        scan_for_candidates_with_decisions_collect(root.clone(), FolderReleaseDecisions::default())
            .into_iter()
            .filter_map(|item| match item {
                WalkItem::Reading { key, decision, grouping } => Some((
                    key.relative_folder,
                    FolderReading {
                        decision,
                        author: FolderReleaseDecisionAuthor::Heuristic,
                        grouping,
                    },
                )),
                _ => None,
            })
            .collect(),
    );
    let first = scan_projected_items_with_decisions(root.clone(), stored.clone());
    let second = scan_projected_items_with_decisions(root.clone(), stored);

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
            ScanItem::Valid(candidate) => Some(candidate.relative_folder.text()),
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
    // None of them is grouped: the wrapper decided nothing, and the list's
    // header over them is where reading the three as one is offered.
    assert!(items.iter().all(|item| match item {
        ScanItem::Valid(candidate) => candidate.grouping.is_none(),
        _ => true,
    }));
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
        crate::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect_err("zero-byte audio makes the folder unimportable");
    assert!(
        matches!(err, crate::ImportError::InvalidFolder(InvalidReason::DamagedAudio { count: 1 })),
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

    let err = scan_for_candidates_with_decisions(
        root.to_path_buf(),
        &StoredCandidateEdits::none(),
        &FolderReleaseDecisions::default(),
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
            panic!("expected IO error, got not-a-directory for {}", path.display())
        }
        FolderScanError::Other(message) => panic!("expected IO error, got {message}"),
        FolderScanError::Cancelled => panic!("expected IO error, got cancellation"),
        FolderScanError::Panicked(failed) => panic!("expected IO error, got {failed}"),
    }
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
        assert!(volumes.contains(&name), "unexpected candidate name {:?}", name,);
    }
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
        file("loose.flac.part", FileKind::PartialMarker("part")),
        file("AlbumA/01.flac", FileKind::Flac),
        file("AlbumB/01.flac", FileKind::Flac),
    ]);
    let mut paths = result.top_level_paths();
    paths.sort();
    assert_eq!(paths, vec!["AlbumA".to_string(), "AlbumB".to_string()]);
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
        file("Show/S01E01.avi", FileKind::Avi),
        file("Show/S01E02.avi", FileKind::Avi),
    ]);
    assert!(result.top_level_paths().is_empty());
}

/// Loose junk at the scan root (.pdf, .zip, .dmg, .jpg) is
/// ignored; a release subfolder still surfaces.
#[test]
fn loose_junk_at_scan_root_ignored() {
    let mut entries = vec![
        file("loose.pdf", FileKind::Pdf),
        file("loose.zip", FileKind::Zip),
        file("loose.dmg", FileKind::Dmg),
    ];
    entries.extend(flat_audio("Album", 3, FileKind::Flac));
    let result = run_scenario(entries);
    assert_eq!(result.top_level_paths(), vec!["Album"]);
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
    entries.push(file("Collection/Cue Image/Audio.flac", FileKind::Flac));
    entries.push(file(
        "Collection/Cue Image/Audio.cue",
        FileKind::CueFor { stem: "Audio.flac", n_tracks: 10 },
    ));
    let result = run_scenario(entries);
    let top = result.top_level_paths();
    assert_eq!(top.len(), 2);
    let track_files = &result.candidate("Collection/Track Files").files;
    assert_uniform_source_audio(track_files, bae_db::source_audio::SourceAudioLayout::File, "FLAC");
    assert!(track_files.bound_sheets().is_empty());
    let cue_image = &result.candidate("Collection/Cue Image").files;
    assert_uniform_source_audio(cue_image, bae_db::source_audio::SourceAudioLayout::Cue, "FLAC");
    assert_eq!(cue_image.bound_sheets().len(), 1);
}

/// A partial marker nested under a release subdirectory (e.g. in
/// `booklet/`) still suppresses the whole release. This exercises the
/// deep walker check.
#[test]
fn partial_marker_in_nested_subdir_stops_release_candidate() {
    let mut entries = flat_audio("Album", 3, FileKind::Flac);
    entries.push(file("Album/booklet/02.flac.part", FileKind::PartialMarker("part")));
    let result = run_scenario(entries);
    assert!(result.top_level_paths().is_empty());
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
