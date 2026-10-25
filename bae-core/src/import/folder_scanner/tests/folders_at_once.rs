/// Everything a pass over `root` said, in the order it said it.
struct SaidByPass {
    directories: Vec<PathBuf>,
    items: Vec<ScanItem>,
}

/// One grouping key for every grouping a pass proposes, so two passes over
/// the same folders say the same items.
fn same_grouping_key() -> String {
    "grouping:proposed".to_string()
}

/// A pass over all of `root` through `reader`, with nothing stored.
fn pass_over_root<R: DirectoryReader>(
    reader: &R,
    root: &Path,
    cancellation: &ScanCancellation,
) -> (Result<(), FolderScanError>, SaidByPass) {
    let mut said = SaidByPass {
        directories: Vec::new(),
        items: Vec::new(),
    };
    let result = scan_for_candidates_with_reader_cancellable_and_directories(
        reader,
        root.to_path_buf(),
        &ScanReadings {
            stored: &StoredCandidateEdits::none(),
            decisions: &FolderReleaseDecisions::default(),
            new_grouping_key: &same_grouping_key,
        },
        &FileReads::new(Arc::new(crate::import::file_tag_snapshot::LoftyFileTagReader)),
        cancellation,
        |directory| said.directories.push(directory),
        |item| said.items.push(item),
    );
    (result, said)
}

/// What reading `folder` under `root` on its own says.
fn reading_of_one_folder(root: &Path, folder: &str) -> SaidByPass {
    let mut said = SaidByPass {
        directories: Vec::new(),
        items: Vec::new(),
    };
    scan_top_level_folder_with_reader(
        &OsDirectoryReader,
        root,
        Path::new(folder),
        &ScanReadings {
            stored: &StoredCandidateEdits::none(),
            decisions: &FolderReleaseDecisions::default(),
            new_grouping_key: &same_grouping_key,
        },
        &FileReads::new(Arc::new(crate::import::file_tag_snapshot::LoftyFileTagReader)),
        &ScanCancellation::new(),
        |directory| said.directories.push(directory),
        |item| said.items.push(item),
    )
    .unwrap();
    said
}

/// The folder directly under `root` that `path` is in, or `""` for the root.
fn top_level_folder_of(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .expect("the pass says only what is under its root")
        .components()
        .next()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The folder directly under `root` that `item` is read from.
fn top_level_folder_of_item(root: &Path, item: &ScanItem) -> String {
    match item {
        ScanItem::Discovered(candidate) | ScanItem::Valid(candidate) => {
            top_level_folder_of(root, &candidate.path)
        }
        ScanItem::Invalid(candidate) => top_level_folder_of(root, &candidate.path),
        ScanItem::Sidecar(sidecar) => top_level_folder_of(root, &sidecar.folder),
        ScanItem::Decided { key, .. } => key
            .relative_folder_path
            .split('/')
            .next()
            .expect("split yields at least one part")
            .to_string(),
    }
}

/// Reads directories normally, except that `held`'s listing waits until
/// `awaited` has been listed.
struct ListsOneFolderAfterAnother {
    held: PathBuf,
    awaited: PathBuf,
    awaited_listed: (std::sync::Mutex<bool>, std::sync::Condvar),
}

impl DirectoryReader for ListsOneFolderAfterAnother {
    fn read(
        &self,
        root: &Path,
        directory: &Path,
        cancellation: &ScanCancellation,
    ) -> Result<DirectoryListing, FolderScanError> {
        if directory == self.held {
            let (listed, changed) = &self.awaited_listed;
            let (listed, timeout) = changed
                .wait_timeout_while(
                    listed.lock().unwrap(),
                    std::time::Duration::from_secs(10),
                    |listed| !*listed,
                )
                .unwrap();
            if timeout.timed_out() && !*listed {
                return Err(FolderScanError::Other(format!(
                    "{} was not listed while {} waited for it",
                    self.awaited.display(),
                    self.held.display()
                )));
            }
        }
        let listing = OsDirectoryReader.read(root, directory, cancellation)?;
        if directory == self.awaited {
            let (listed, changed) = &self.awaited_listed;
            *listed.lock().unwrap() = true;
            changed.notify_all();
        }
        Ok(listing)
    }
}

#[test]
fn a_pass_reads_folders_at_once_and_says_what_each_says_on_its_own() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Library");
    let write = |relative: &str, bytes: &[u8]| {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    };
    write("loose.flac", &fake_flac());
    // No audio: its files are the root's own release's.
    write("Folder 01/Extras/notes.txt", b"notes");
    // Two numbered discs, which the pass reads as one release.
    write("Folder 02/CD1/01 Track.flac", &fake_flac());
    write("Folder 02/CD2/01 Track.flac", &fake_flac());
    // Two releases with a booklet beside them that neither owns.
    write("Folder 03/booklet.txt", b"booklet");
    write("Folder 03/Release A/01 Track.flac", &fake_flac());
    write("Folder 03/Release B/01 Track.flac", &fake_flac());
    write("Folder 04/readme.txt", b"readme");
    // A track nothing plays.
    write("Folder 05/01 Track.flac", b"not audio");
    for folder in 6..=12 {
        write(&format!("Folder {folder:02}/01 Track.flac"), &fake_flac());
        write(&format!("Folder {folder:02}/cover.jpg"), &[0xFF, 0xD8, 0xFF, 0xE0]);
    }
    let folders: Vec<String> = (1..=12).map(|folder| format!("Folder {folder:02}")).collect();
    assert!(folders.len() > FOLDERS_AT_ONCE);

    // The first folder waits on the last, which the pass reaches only once
    // folders after the first have been read through.
    let reader = ListsOneFolderAfterAnother {
        held: PathBuf::from("Folder 01"),
        awaited: PathBuf::from("Folder 12"),
        awaited_listed: Default::default(),
    };
    let (result, said) = pass_over_root(&reader, &root, &ScanCancellation::new());
    result.unwrap();

    let position = |directory: &str| {
        said.directories
            .iter()
            .position(|said| *said == root.join(directory))
            .unwrap_or_else(|| panic!("{directory} was reached"))
    };
    assert!(
        position("Folder 12") < position("Folder 01/Extras"),
        "a folder's releases arrive as it is read, not in the order the folders' names sort in"
    );

    // Each folder says exactly what reading it on its own says, in the order
    // reading it on its own says it.
    for folder in &folders {
        let alone = reading_of_one_folder(&root, folder);
        let directories: Vec<PathBuf> = said
            .directories
            .iter()
            .filter(|directory| top_level_folder_of(&root, directory) == *folder)
            .cloned()
            .collect();
        assert_eq!(directories, alone.directories, "the directories of {folder}");
        let items: Vec<ScanItem> = said
            .items
            .iter()
            .filter(|item| top_level_folder_of_item(&root, item) == *folder)
            .cloned()
            .collect();
        assert_eq!(items, alone.items, "the items of {folder}");
    }

    // The root's own release comes last, and takes the files of the folders
    // that hold no audio, the one read last among them.
    assert_eq!(said.directories.first(), Some(&root));
    let root_items: Vec<&ScanItem> = said
        .items
        .iter()
        .filter(|item| top_level_folder_of_item(&root, item).is_empty())
        .collect();
    assert_eq!(root_items.len(), 1);
    let Some(ScanItem::Valid(root_release)) = said.items.last() else {
        panic!("the root's own release is said last: {:?}", said.items.last());
    };
    assert_eq!(root_release.path, root);
    assert_eq!(
        root_release
            .files
            .release_files()
            .map(|file| file.relative_path.as_str())
            .collect::<Vec<_>>(),
        ["Folder 01/Extras/notes.txt", "Folder 04/readme.txt", "loose.flac"]
    );
}

/// Lists the root; inside `failing`, fails; inside every other folder, waits
/// until the walk reading it is stopped. Records each directory it was asked
/// to list.
struct FailsInOneFolder {
    failing: PathBuf,
    asked: std::sync::Mutex<Vec<PathBuf>>,
}

impl DirectoryReader for FailsInOneFolder {
    fn read(
        &self,
        root: &Path,
        directory: &Path,
        cancellation: &ScanCancellation,
    ) -> Result<DirectoryListing, FolderScanError> {
        self.asked.lock().unwrap().push(directory.to_path_buf());
        if directory.as_os_str().is_empty() {
            return OsDirectoryReader.read(root, directory, cancellation);
        }
        if directory == self.failing {
            return Err(FolderScanError::Other(format!(
                "{} cannot be listed",
                directory.display()
            )));
        }
        loop {
            cancellation.check()?;
            std::thread::yield_now();
        }
    }
}

#[test]
fn a_folder_that_fails_stops_the_others_and_fails_the_pass() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Library");
    for folder in 1..=20 {
        std::fs::create_dir_all(root.join(format!("Folder {folder:02}"))).unwrap();
    }
    let reader = Arc::new(FailsInOneFolder {
        failing: PathBuf::from("Folder 05"),
        asked: Default::default(),
    });

    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let pass_reader = reader.clone();
    let pass_root = root.clone();
    std::thread::spawn(move || {
        let (result, said) = pass_over_root(&*pass_reader, &pass_root, &ScanCancellation::new());
        done_tx.send((result, said)).unwrap();
    });
    let (result, said) = done_rx
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("the failing folder stops the folders read beside it");

    match result {
        Err(FolderScanError::Other(message)) => assert_eq!(message, "Folder 05 cannot be listed"),
        other => panic!("the pass fails with the folder's error: {other:?}"),
    }
    assert!(said.items.is_empty());
    // No folder is begun once one has failed: only those already in flight
    // were asked for.
    let asked = reader.asked.lock().unwrap().clone();
    let first_in_flight: Vec<PathBuf> = std::iter::once(PathBuf::new())
        .chain((1..=FOLDERS_AT_ONCE).map(|folder| PathBuf::from(format!("Folder {folder:02}"))))
        .collect();
    assert!(
        asked.iter().all(|directory| first_in_flight.contains(directory)),
        "{asked:?}"
    );
}

/// Lists the root and `cancelling`, cancelling `pass` as it lists the
/// latter; inside every other folder, waits until the walk reading it is
/// stopped. Records each directory it was asked to list.
struct CancelsInOneFolder {
    cancelling: PathBuf,
    pass: ScanCancellation,
    asked: std::sync::Mutex<Vec<PathBuf>>,
}

impl DirectoryReader for CancelsInOneFolder {
    fn read(
        &self,
        root: &Path,
        directory: &Path,
        cancellation: &ScanCancellation,
    ) -> Result<DirectoryListing, FolderScanError> {
        self.asked.lock().unwrap().push(directory.to_path_buf());
        if directory == self.cancelling {
            self.pass.cancel();
        }
        if directory.as_os_str().is_empty() || directory == self.cancelling {
            return OsDirectoryReader.read(root, directory, cancellation);
        }
        loop {
            cancellation.check()?;
            std::thread::yield_now();
        }
    }
}

#[test]
fn cancelling_a_pass_stops_every_folder_it_is_reading() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Library");
    for folder in 1..=20 {
        let folder = root.join(format!("Release {folder:02}"));
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("01 Track.flac"), fake_flac()).unwrap();
    }
    let cancellation = ScanCancellation::new();
    let reader = Arc::new(CancelsInOneFolder {
        cancelling: PathBuf::from("Release 01"),
        pass: cancellation.clone(),
        asked: Default::default(),
    });

    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let pass_reader = reader.clone();
    let pass_root = root.clone();
    std::thread::spawn(move || {
        let (result, said) = pass_over_root(&*pass_reader, &pass_root, &cancellation);
        done_tx.send((result, said)).unwrap();
    });
    let (result, said) = done_rx
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("cancelling reaches every folder being read");

    assert!(matches!(result, Err(FolderScanError::Cancelled)), "{result:?}");
    // The cancelled folder's walk reads none of its files into a release.
    assert!(
        !said
            .items
            .iter()
            .any(|item| matches!(item, ScanItem::Valid(_) | ScanItem::Invalid(_))),
        "{:?}",
        said.items
    );
    // No folder is begun once the pass is cancelled: only those already in
    // flight were asked for.
    let asked = reader.asked.lock().unwrap().clone();
    let first_in_flight: Vec<PathBuf> = std::iter::once(PathBuf::new())
        .chain((1..=FOLDERS_AT_ONCE).map(|folder| PathBuf::from(format!("Release {folder:02}"))))
        .collect();
    assert!(
        asked.iter().all(|directory| first_in_flight.contains(directory)),
        "{asked:?}"
    );
}
