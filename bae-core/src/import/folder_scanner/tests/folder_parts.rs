/// Write one playable track into each folder of `folders` under `root`, with
/// a cover in a `Covers` folder beside it.
fn write_albums(root: &Path, folders: &[&str]) {
    for folder in folders {
        let folder = root.join(folder);
        std::fs::create_dir_all(folder.join("Covers")).unwrap();
        std::fs::write(folder.join("track.flac"), fake_flac()).unwrap();
        std::fs::write(folder.join("Covers/front.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();
    }
}

/// The readings a scan of `root` decides for itself, by folder.
fn decided(items: &[ScanItem]) -> Vec<(&str, FolderReleaseDecision)> {
    items
        .iter()
        .filter_map(|item| match item {
            ScanItem::Decided { key, decision, .. } => {
                Some((key.relative_folder_path.as_str(), *decision))
            }
            _ => None,
        })
        .collect()
}

fn valid_paths(items: &[ScanItem]) -> Vec<&str> {
    let mut paths: Vec<_> = items
        .iter()
        .filter_map(|item| match item {
            ScanItem::Valid(candidate) => Some(candidate.display_path.as_str()),
            _ => None,
        })
        .collect();
    paths.sort_unstable();
    paths
}

/// Two numbered folders that wrap whole albums — three in the first, one in
/// the second — number themselves 1 and 2 and are still four releases: a
/// part holding several folders of audio is no disc.
#[test]
fn numbered_folders_of_albums_are_separate_releases() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Queue");
    let collection = "Artist (Originals & Remasters)";
    write_albums(
        &root.join(collection),
        &[
            "01_Artist_Originals/Artist -1984- Album One (Press 11-2X-1001)",
            "01_Artist_Originals/Artist -1985- Album Two (Press 11-2X-1002)",
            "01_Artist_Originals/Artist -1988- Album Three (Press  11-2X-1003)",
            "02_Artist_Remasters/Artist -1992- Album Four (XYZP 104)",
        ],
    );

    let items = scan_for_candidates_with_decisions_collect(root, FolderReleaseDecisions::default());

    assert!(
        decided(&items).contains(&(collection, FolderReleaseDecision::KeepAsSeparateReleases)),
        "{:?}",
        decided(&items)
    );
    assert_eq!(
        valid_paths(&items),
        vec![
            "Artist (Originals & Remasters)/01_Artist_Originals/Artist -1984- Album One (Press 11-2X-1001)",
            "Artist (Originals & Remasters)/01_Artist_Originals/Artist -1985- Album Two (Press 11-2X-1002)",
            "Artist (Originals & Remasters)/01_Artist_Originals/Artist -1988- Album Three (Press  11-2X-1003)",
            "Artist (Originals & Remasters)/02_Artist_Remasters/Artist -1992- Album Four (XYZP 104)",
        ]
    );
    assert!(items.iter().all(|item| match item {
        ScanItem::Valid(candidate) => candidate.grouping.is_none(),
        _ => true,
    }));
}

/// `disc 01` to `disc 12`, each one folder of tracks, are one release.
#[test]
fn twelve_numbered_discs_are_one_release() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Queue");
    let discs: Vec<String> = (1..=12).map(|disc| format!("Box Set/disc {disc:02}")).collect();
    write_albums(&root, &discs.iter().map(String::as_str).collect::<Vec<_>>());

    let items = scan_for_candidates_with_decisions_collect(root, FolderReleaseDecisions::default());

    assert_eq!(
        decided(&items),
        vec![("Box Set", FolderReleaseDecision::CombineAsOneRelease)]
    );
    assert_eq!(valid_paths(&items), vec!["Box Set"]);
    let release = items
        .iter()
        .find_map(|item| match item {
            ScanItem::Valid(candidate) => Some(candidate),
            _ => None,
        })
        .expect("the set is one release");
    assert_eq!(release.files.parts.len(), 12);
}

/// A disc whose tracks sit one folder down, under a folder with no audio of
/// its own, still holds one folder's audio.
#[test]
fn discs_whose_tracks_sit_one_folder_down_are_one_release() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Queue");
    write_albums(&root, &["Box Set/CD1/FLAC", "Box Set/CD2/FLAC"]);

    let items = scan_for_candidates_with_decisions_collect(root, FolderReleaseDecisions::default());

    assert_eq!(
        decided(&items),
        vec![("Box Set", FolderReleaseDecision::CombineAsOneRelease)]
    );
    assert_eq!(valid_paths(&items), vec!["Box Set"]);
    let release = items
        .iter()
        .find_map(|item| match item {
            ScanItem::Valid(candidate) => Some(candidate),
            _ => None,
        })
        .expect("the set is one release");
    assert_eq!(
        release
            .files
            .parts
            .iter()
            .map(|part| part.prefix.as_str())
            .collect::<Vec<_>>(),
        ["CD1/FLAC/", "CD2/FLAC/"]
    );
}

/// Numbered folders that hold no audio are no discs: an album with artwork in
/// `Disc 1` and `Disc 2` beside its tracks is one release with nothing to
/// decide, and a box of artwork alone yields nothing and decides nothing.
#[test]
fn numbered_folders_without_audio_are_not_parts() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Queue");
    let album = root.join("Artist - Album");
    std::fs::create_dir_all(&album).unwrap();
    std::fs::write(album.join("track.flac"), fake_flac()).unwrap();
    for folder in ["Artist - Album/Disc 1", "Artist - Album/Disc 2", "Box/Disc 1", "Box/Disc 2"] {
        let folder = root.join(folder);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("scan.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();
    }

    let items = scan_for_candidates_with_decisions_collect(root, FolderReleaseDecisions::default());

    assert_eq!(decided(&items), vec![]);
    assert_eq!(valid_paths(&items), vec!["Artist - Album"]);
}

/// The listings a folder's reading looks at are the ones the walk goes on
/// to use: every folder is listed once.
#[test]
fn reading_a_folder_lists_nothing_twice() {
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
    write_albums(
        &root,
        &[
            "Collection/01 Originals/Album One",
            "Collection/01 Originals/Album Two",
            "Collection/02 Remasters/Album Three",
            "Box Set/CD1/FLAC",
            "Box Set/CD2",
            "Wrapper/Live One",
            "Wrapper/Live Two",
        ],
    );
    std::fs::create_dir_all(root.join("Box Set/Scans/Booklet")).unwrap();
    let reader = RecordingReader::default();

    scan_with_reader(&reader, root.clone(), |_| {}).unwrap();

    fn folders_below(root: &Path, relative: PathBuf, into: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(root.join(&relative)).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                folders_below(root, relative.join(entry.file_name()), into);
            }
        }
        into.push(relative);
    }
    let mut every_folder = Vec::new();
    folders_below(&root, PathBuf::new(), &mut every_folder);
    every_folder.sort();
    let mut reads = reader.reads.into_inner().unwrap();
    reads.sort();
    assert_eq!(reads, every_folder);
}
