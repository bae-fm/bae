/// A pass that reads two disc folders as one release probes each disc's audio
/// once: the release reuses the facts the disc folders were read with.
#[tokio::test]
async fn regrouping_disc_folders_probes_each_audio_file_once() {
    let test = setup_import_service().await;
    let root = test.temp.path().join("watched");
    let album = root.join("Album");
    let mut tracks = Vec::new();
    for disc in ["CD1", "CD2"] {
        std::fs::create_dir_all(album.join(disc)).unwrap();
        for n in 1..=2 {
            let path = album.join(disc).join(format!("0{n}.flac"));
            std::fs::write(&path, flac()).unwrap();
            tracks.push(path);
        }
    }
    let root = PathBuf::from(
        crate::import::watched_folder::canonical_absolute_root(&root.to_string_lossy()).unwrap(),
    );
    test.service
        .library_manager
        .add_watched_import_folder(&root.to_string_lossy())
        .await
        .unwrap();

    let (scan, _) = test.scan();
    scan.rescan(&root).await.unwrap();

    let releases: Vec<usize> = test
        .service
        .library_manager
        .load_folder_scan_items(&root.to_string_lossy())
        .await
        .unwrap()
        .into_iter()
        .filter_map(|item| match item {
            ScanItem::Valid(candidate) => Some(candidate.files.audio().count()),
            _ => None,
        })
        .collect();
    assert_eq!(releases, vec![4], "the discs read as one release");
    for track in &tracks {
        assert_eq!(
            crate::test_reads::count(crate::test_reads::FileRead::Probe, track),
            1,
            "{} is probed once",
            track.display()
        );
    }
}

/// The files of a watched root a rescan could read, by what reads them.
struct ReadableFiles {
    root: PathBuf,
    /// The release's folder, its key.
    album: PathBuf,
    tracks: Vec<PathBuf>,
    images: Vec<PathBuf>,
    sheet: PathBuf,
}

impl ReadableFiles {
    /// How many times each of them has been read, by each read.
    fn reads(&self, tags: &CountingTagReader) -> Vec<(PathBuf, &'static str, u64)> {
        use crate::test_reads::{count, FileRead};
        let mut reads = Vec::new();
        for track in &self.tracks {
            reads.push((track.clone(), "probe", count(FileRead::Probe, track)));
            reads.push((track.clone(), "tags", tags.reads_of(track)));
        }
        for image in &self.images {
            reads.push((image.clone(), "image", count(FileRead::ImageCheck, image)));
        }
        reads.push((
            self.sheet.clone(),
            "sheet",
            count(FileRead::CueParse, &self.sheet),
        ));
        reads
    }
}

/// A watched root holding a release — two tracks, a cover, a sheet, and an
/// image that does not decode — a folder that can't be imported — a playable
/// track, an empty one, and one FFmpeg finds nothing in — and two releases
/// beside a booklet neither owns.
async fn readable_root(test: &TestService) -> ReadableFiles {
    let root = test.temp.path().join("watched");
    let album = root.join("Album");
    let broken = root.join("Broken");
    let set = root.join("Set");
    for folder in [
        &album,
        &broken,
        &set.join("Live One"),
        &set.join("Live Two"),
    ] {
        std::fs::create_dir_all(folder).unwrap();
    }
    let mut tracks = Vec::new();
    for track in [
        album.join("01.flac"),
        album.join("02.flac"),
        broken.join("01.flac"),
        set.join("Live One").join("01.flac"),
        set.join("Live Two").join("01.flac"),
    ] {
        std::fs::write(&track, flac()).unwrap();
        tracks.push(track);
    }
    std::fs::write(broken.join("02.flac"), b"").unwrap();
    std::fs::write(broken.join("03.flac"), vec![7u8; 4096]).unwrap();
    tracks.push(broken.join("03.flac"));
    write_test_jpeg(&album.join("cover.jpg"));
    std::fs::write(album.join("back.png"), vec![0u8; 64]).unwrap();
    write_test_jpeg(&set.join("booklet.jpg"));
    let sheet = album.join("album.cue");
    std::fs::write(
        &sheet,
        "FILE \"01.flac\" WAVE\n  TRACK 01 AUDIO\n    INDEX 01 00:00:00\n",
    )
    .unwrap();
    let root = PathBuf::from(
        crate::import::watched_folder::canonical_absolute_root(&root.to_string_lossy()).unwrap(),
    );
    test.service
        .library_manager
        .add_watched_import_folder(&root.to_string_lossy())
        .await
        .unwrap();
    ReadableFiles {
        album: root.join("Album"),
        tracks: tracks
            .iter()
            .map(|track| {
                root.join(
                    track
                        .strip_prefix(test.temp.path().join("watched"))
                        .unwrap(),
                )
            })
            .collect(),
        images: ["Album/cover.jpg", "Album/back.png", "Set/booklet.jpg"]
            .iter()
            .map(|image| root.join(image))
            .collect(),
        sheet: root.join("Album/album.cue"),
        root,
    }
}

/// A first scan reads each file once — each track's stream and tags, each
/// image's header, each sheet — and a rescan of a folder nothing in has
/// changed opens none of them: every one of those, and every release's tags,
/// is what the last scan stored. What it stores is what was there.
#[tokio::test]
async fn a_rescan_of_an_unchanged_folder_reads_none_of_its_files() {
    let test = setup_import_service().await;
    let files = readable_root(&test).await;
    let tags = Arc::new(CountingTagReader::default());
    let (scan, _) = test.scan_with(
        tags.clone(),
        Arc::new(crate::import::folder_scanner::OsDirectoryReader),
    );
    let root_key = files.root.to_string_lossy().into_owned();
    scan.rescan(&files.root).await.unwrap();
    let first = files.reads(&tags);
    for (path, read, count) in &first {
        // A track FFmpeg finds nothing in has no tags to read.
        let expected = u64::from(!(*read == "tags" && path.ends_with("Broken/03.flac")));
        assert_eq!(*count, expected, "{read} of {}", path.display());
    }
    let stored = test
        .service
        .library_manager
        .load_folder_scan_items(&root_key)
        .await
        .unwrap();
    assert!(
        stored
            .iter()
            .any(|item| matches!(item, ScanItem::Invalid(_))),
        "the broken folder is stored as one that can't be imported: {stored:?}"
    );
    let reading = test
        .service
        .library_manager
        .load_candidate_file_tag_snapshot(&root_key, &files.album.to_string_lossy())
        .await
        .unwrap()
        .unwrap()
        .snapshot
        .unwrap();

    scan.rescan(&files.root).await.unwrap();

    assert_eq!(files.reads(&tags), first, "the rescan reads no file again");
    assert_eq!(
        test.service
            .library_manager
            .load_folder_scan_items(&root_key)
            .await
            .unwrap(),
        stored,
        "the rescan stores what the scan before it stored"
    );
    let reread = test
        .service
        .library_manager
        .load_candidate_file_tag_snapshot(&root_key, &files.album.to_string_lossy())
        .await
        .unwrap()
        .unwrap()
        .snapshot
        .unwrap();
    assert!(reread.same_reading(&reading), "{reread:?} != {reading:?}");
}

/// An album whose disc folders the scan reads as one release is, on a rescan
/// that finds nothing changed, what the last scan stored: no track's stream or
/// tags are read again, and the disc folders, reported on their own before
/// the album is read as one, leave nothing stored beside the album.
#[tokio::test]
async fn a_rescan_of_an_unchanged_multi_disc_album_reads_none_of_its_files() {
    use crate::test_reads::{count, FileRead};
    let test = setup_import_service().await;
    let watched = test.temp.path().join("watched");
    for disc in ["Disc 1", "Disc 2"] {
        let folder = watched.join("Album").join(disc);
        std::fs::create_dir_all(&folder).unwrap();
        for n in 1..=2 {
            std::fs::write(folder.join(format!("0{n}.flac")), flac()).unwrap();
        }
    }
    let root = PathBuf::from(
        crate::import::watched_folder::canonical_absolute_root(&watched.to_string_lossy()).unwrap(),
    );
    let root_key = root.to_string_lossy().into_owned();
    test.service
        .library_manager
        .add_watched_import_folder(&root_key)
        .await
        .unwrap();
    let tracks: Vec<PathBuf> = ["Disc 1/01", "Disc 1/02", "Disc 2/01", "Disc 2/02"]
        .iter()
        .map(|track| root.join("Album").join(format!("{track}.flac")))
        .collect();
    let tags = Arc::new(CountingTagReader::default());
    let (scan, _) = test.scan_with(
        tags.clone(),
        Arc::new(crate::import::folder_scanner::OsDirectoryReader),
    );
    let reads = || {
        tracks
            .iter()
            .map(|track| (count(FileRead::Probe, track), tags.reads_of(track)))
            .collect::<Vec<_>>()
    };
    scan.rescan(&root).await.unwrap();
    let stored = test
        .service
        .library_manager
        .load_folder_scan_items(&root_key)
        .await
        .unwrap();
    let releases: Vec<usize> = stored
        .iter()
        .filter_map(|item| match item {
            ScanItem::Valid(candidate) => Some(candidate.files.audio().count()),
            _ => None,
        })
        .collect();
    assert_eq!(releases, vec![4], "the discs read as one release");
    assert_eq!(stored.len(), 1, "nothing else is stored: {stored:?}");
    assert_eq!(reads(), vec![(1, 1); 4], "the first scan reads each track once");

    scan.rescan(&root).await.unwrap();

    assert_eq!(reads(), vec![(1, 1); 4], "the rescan reads no track again");
    assert_eq!(
        test.service
            .library_manager
            .load_folder_scan_items(&root_key)
            .await
            .unwrap(),
        stored,
        "the rescan stores what the scan before it stored"
    );
}

/// A track whose modification time moved is read again; the files beside it
/// are not.
#[tokio::test]
async fn a_rescan_reads_again_only_the_file_that_changed() {
    let test = setup_import_service().await;
    let files = readable_root(&test).await;
    let tags = Arc::new(CountingTagReader::default());
    let (scan, _) = test.scan_with(
        tags.clone(),
        Arc::new(crate::import::folder_scanner::OsDirectoryReader),
    );
    scan.rescan(&files.root).await.unwrap();
    let changed = files.album.join("02.flac");
    std::fs::File::options()
        .write(true)
        .open(&changed)
        .unwrap()
        .set_times(
            std::fs::FileTimes::new()
                .set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(5)),
        )
        .unwrap();
    let before = files.reads(&tags);

    scan.rescan(&files.root).await.unwrap();

    for ((path, read, before), (_, _, after)) in before.iter().zip(files.reads(&tags)) {
        let expected = match *read {
            "probe" | "tags" if *path == changed => before + 1,
            _ => *before,
        };
        assert_eq!(after, expected, "{read} of {}", path.display());
    }
}

/// A release whose tracks are the ones its stored tags were read from takes
/// those tags for its draft, though another file beside them is new.
#[tokio::test]
async fn a_release_whose_tracks_did_not_change_keeps_their_stored_tags() {
    let test = setup_import_service().await;
    let files = readable_root(&test).await;
    let tags = Arc::new(CountingTagReader::default());
    let (scan, _) = test.scan_with(
        tags.clone(),
        Arc::new(crate::import::folder_scanner::OsDirectoryReader),
    );
    let root_key = files.root.to_string_lossy().into_owned();
    let album_key = files.album.to_string_lossy().into_owned();
    scan.rescan(&files.root).await.unwrap();
    let reading = test
        .service
        .library_manager
        .load_candidate_file_tag_snapshot(&root_key, &album_key)
        .await
        .unwrap()
        .unwrap();
    std::fs::write(files.album.join("notes.txt"), "Rip notes").unwrap();
    let before = files.reads(&tags);

    scan.rescan(&files.root).await.unwrap();

    assert_eq!(files.reads(&tags), before, "no file is read again");
    let stored = test
        .service
        .library_manager
        .load_candidate_file_tag_snapshot(&root_key, &album_key)
        .await
        .unwrap()
        .unwrap();
    assert_ne!(
        stored.candidate.files.content_hash(),
        reading.candidate.files.content_hash(),
        "the release holds a new file"
    );
    let preparation = test
        .service
        .library_manager
        .load_import_candidate_preparation(&stored.candidate.files.content_hash())
        .await
        .unwrap()
        .expect("the release with the new file is stored");
    assert_eq!(preparation.draft.tracks.len(), 2);
    assert!(stored
        .snapshot
        .unwrap()
        .same_reading(&reading.snapshot.unwrap()));
}

/// Two folders holding the same names at the same sizes and modification
/// times hash alike, so they share one draft, and each still keeps the tags
/// read from its own tracks: a rescan reads neither folder's files again.
#[tokio::test]
async fn folders_whose_files_hash_alike_each_keep_their_tags() {
    let test = setup_import_service().await;
    let watched = test.temp.path().join("watched");
    let modified =
        std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
    for folder in ["First", "Second"] {
        std::fs::create_dir_all(watched.join(folder)).unwrap();
        let track = watched.join(folder).join("01.flac");
        std::fs::write(&track, flac()).unwrap();
        std::fs::File::options()
            .write(true)
            .open(&track)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(modified))
            .unwrap();
    }
    let root = PathBuf::from(
        crate::import::watched_folder::canonical_absolute_root(&watched.to_string_lossy()).unwrap(),
    );
    let root_key = root.to_string_lossy().into_owned();
    test.service
        .library_manager
        .add_watched_import_folder(&root_key)
        .await
        .unwrap();
    let tags = Arc::new(CountingTagReader::default());
    let (scan, _) = test.scan_with(
        tags.clone(),
        Arc::new(crate::import::folder_scanner::OsDirectoryReader),
    );
    scan.rescan(&root).await.unwrap();
    let mut hashes = Vec::new();
    for folder in ["First", "Second"] {
        let stored = test
            .service
            .library_manager
            .load_candidate_file_tag_snapshot(&root_key, &root.join(folder).to_string_lossy())
            .await
            .unwrap()
            .unwrap();
        assert!(stored.snapshot.is_some(), "{folder} keeps the tags read from its track");
        hashes.push(stored.candidate.files.content_hash());
    }
    assert_eq!(hashes[0], hashes[1], "the two folders hash alike");
    let tracks = [root.join("First/01.flac"), root.join("Second/01.flac")];
    let reads = || tracks.iter().map(|track| tags.reads_of(track)).collect::<Vec<_>>();
    assert_eq!(reads(), vec![1, 1], "the first scan reads each track's tags once");

    scan.rescan(&root).await.unwrap();

    assert_eq!(reads(), vec![1, 1], "the rescan reads neither track's tags again");
}

/// Reads tags as [`CountingTagReader`] does, and says each track embeds
/// `picture`.
struct EmbeddingTagReader {
    counted: CountingTagReader,
    picture: Vec<u8>,
}

impl crate::import::file_tag_snapshot::FileTagReader for EmbeddingTagReader {
    fn read(
        &self,
        path: &Path,
    ) -> Result<crate::import::file_tag_snapshot::FileTagRead, crate::import::ImportError> {
        let mut read = self.counted.read(path)?;
        read.embedded_cover = Some((
            self.picture.clone(),
            crate::util::content_type::ContentType::Jpeg,
        ));
        Ok(read)
    }
}

/// A first scan reads each track's tags once, though it reads each disc
/// folder as a release of its own before reading the set as one: the walk's
/// reading places the tracks, and every one of those releases' drafts takes
/// its tags and its first track's picture from it.
#[tokio::test]
async fn a_first_scan_reads_each_tracks_tags_once() {
    let test = setup_import_service().await;
    let watched = test.temp.path().join("watched");
    let mut tracks = Vec::new();
    for disc in ["CD1", "CD2"] {
        let folder = watched.join("Album").join(disc);
        std::fs::create_dir_all(&folder).unwrap();
        for n in 1..=2 {
            std::fs::write(folder.join(format!("0{n}.flac")), flac()).unwrap();
            tracks.push(format!("Album/{disc}/0{n}.flac"));
        }
    }
    let picture_path = test.temp.path().join("picture.jpg");
    write_test_jpeg(&picture_path);
    let root = PathBuf::from(
        crate::import::watched_folder::canonical_absolute_root(&watched.to_string_lossy()).unwrap(),
    );
    let root_key = root.to_string_lossy().into_owned();
    test.service
        .library_manager
        .add_watched_import_folder(&root_key)
        .await
        .unwrap();
    let tags = Arc::new(EmbeddingTagReader {
        counted: CountingTagReader::default(),
        picture: std::fs::read(&picture_path).unwrap(),
    });
    let (scan, _) = test.scan_with(
        tags.clone(),
        Arc::new(crate::import::folder_scanner::OsDirectoryReader),
    );

    scan.rescan(&root).await.unwrap();

    for track in &tracks {
        assert_eq!(tags.counted.reads_of(&root.join(track)), 1, "{track}");
    }
    let release = test
        .service
        .library_manager
        .load_folder_scan_items(&root_key)
        .await
        .unwrap()
        .into_iter()
        .find_map(|item| match item {
            ScanItem::Valid(candidate) => Some(candidate),
            _ => None,
        })
        .expect("the discs read as one release");
    assert_eq!(release.files.audio().count(), 4);
    let reading = test
        .service
        .library_manager
        .load_candidate_file_tag_snapshot(&root_key, &release.key())
        .await
        .unwrap()
        .unwrap()
        .snapshot
        .expect("the release's draft was seeded from its tags");
    assert_eq!(reading.files.len(), 4);
    assert_eq!(
        reading
            .embedded_cover
            .map(|cover| cover.source_relative_path),
        Some("CD1/01.flac".to_string())
    );
}
