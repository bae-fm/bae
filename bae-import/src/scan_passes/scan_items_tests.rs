//! What a pass stores and announces of what it read: each file read only
//! when it is new or changed, the tags stored for unchanged tracks kept, a
//! candidate seeded from its files' tags, and an unchanged folder announcing
//! nothing. A release is identified on its own only when a pass first finds
//! it while automatic identification is on, and changed files retire its
//! result.

use super::*;
use crate::extraction::ArtworkAnalysis;
use crate::extraction::ArtworkAnalyzer;
use crate::identification::tests::*;
use crate::scan_passes::tests::*;
use crate::service::tests::*;
use bae_catalogs::test_releases::mb_release_json;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::Mutex;
use std::time::Duration;

/// A pass that reads two disc folders as one release probes each disc's audio
/// once: the release reuses the facts the disc folders were read with.
#[tokio::test]
async fn regrouping_disc_folders_probes_each_audio_file_once() {
    let test = setup_import_service().await;
    let tables = test.tables().clone();
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
    let root = bae_db::watched_path::canonical_absolute_root(&root).unwrap();
    tables.add_watched_import_folder(&root).await.unwrap();

    let (scan, _) = test.scan();
    scan.rescan(&root).await.unwrap();

    let releases: Vec<usize> = tables
        .load_folder_scan_items(&root)
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
        assert_eq!(bae_audio::test_reads::probes(track), 1, "{} is probed once", track.display());
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
        use crate::tests::reads::{count, FileRead};
        let mut reads = Vec::new();
        for track in &self.tracks {
            reads.push((track.clone(), "probe", bae_audio::test_reads::probes(track)));
            reads.push((track.clone(), "tags", tags.reads_of(track)));
        }
        for image in &self.images {
            reads.push((image.clone(), "image", count(FileRead::ImageCheck, image)));
        }
        reads.push((self.sheet.clone(), "sheet", count(FileRead::CueParse, &self.sheet)));
        reads
    }
}

/// A watched root holding a release — two tracks, a cover, a sheet, and an
/// image that does not decode — a folder that can't be imported — a playable
/// track, an empty one, and one FFmpeg finds nothing in — and two releases
/// beside a booklet neither owns.
async fn readable_root(test: &TestService) -> ReadableFiles {
    let tables = test.tables().clone();
    let root = test.temp.path().join("watched");
    let album = root.join("Album");
    let broken = root.join("Broken");
    let set = root.join("Set");
    for folder in [&album, &broken, &set.join("Live One"), &set.join("Live Two")] {
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
    std::fs::write(&sheet, "FILE \"01.flac\" WAVE\n  TRACK 01 AUDIO\n    INDEX 01 00:00:00\n")
        .unwrap();
    let root = bae_db::watched_path::canonical_absolute_root(&root).unwrap();
    tables.add_watched_import_folder(&root).await.unwrap();
    ReadableFiles {
        album: root.join("Album"),
        tracks: tracks
            .iter()
            .map(|track| root.join(track.strip_prefix(test.temp.path().join("watched")).unwrap()))
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
    let tables = test.tables().clone();
    let files = readable_root(&test).await;
    let tags = Arc::new(CountingTagReader::default());
    let (scan, _) =
        test.scan_with(tags.clone(), Arc::new(crate::folder_scanner::OsDirectoryReader));
    let root_key = files.root.to_path_buf();
    scan.rescan(&files.root).await.unwrap();
    let first = files.reads(&tags);
    for (path, read, count) in &first {
        // A track FFmpeg finds nothing in has no tags to read.
        let expected = u64::from(!(*read == "tags" && path.ends_with("Broken/03.flac")));
        assert_eq!(*count, expected, "{read} of {}", path.display());
    }
    let stored = tables.load_folder_scan_items(std::path::Path::new(&root_key)).await.unwrap();
    assert!(
        stored.iter().any(|item| matches!(item, ScanItem::Invalid(_))),
        "the broken folder is stored as one that can't be imported: {stored:?}"
    );
    let reading = tables
        .load_candidate_file_tag_snapshot(&root_key, &crate::CandidateKey::folder(&files.album))
        .await
        .unwrap()
        .unwrap()
        .snapshot
        .unwrap();

    scan.rescan(&files.root).await.unwrap();

    assert_eq!(files.reads(&tags), first, "the rescan reads no file again");
    assert_eq!(
        tables.load_folder_scan_items(std::path::Path::new(&root_key)).await.unwrap(),
        stored,
        "the rescan stores what the scan before it stored"
    );
    let reread = tables
        .load_candidate_file_tag_snapshot(&root_key, &crate::CandidateKey::folder(&files.album))
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
    let test = setup_import_service().await;
    let tables = test.tables().clone();
    let watched = test.temp.path().join("watched");
    for disc in ["Disc 1", "Disc 2"] {
        let folder = watched.join("Album").join(disc);
        std::fs::create_dir_all(&folder).unwrap();
        for n in 1..=2 {
            std::fs::write(folder.join(format!("0{n}.flac")), flac()).unwrap();
        }
    }
    let root = bae_db::watched_path::canonical_absolute_root(&watched).unwrap();
    let root_key = root.to_path_buf();
    tables.add_watched_import_folder(std::path::Path::new(&root_key)).await.unwrap();
    let tracks: Vec<PathBuf> = ["Disc 1/01", "Disc 1/02", "Disc 2/01", "Disc 2/02"]
        .iter()
        .map(|track| root.join("Album").join(format!("{track}.flac")))
        .collect();
    let tags = Arc::new(CountingTagReader::default());
    let (scan, _) =
        test.scan_with(tags.clone(), Arc::new(crate::folder_scanner::OsDirectoryReader));
    let reads = || {
        tracks
            .iter()
            .map(|track| (bae_audio::test_reads::probes(track), tags.reads_of(track)))
            .collect::<Vec<_>>()
    };
    scan.rescan(&root).await.unwrap();
    let stored = tables.load_folder_scan_items(std::path::Path::new(&root_key)).await.unwrap();
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
        tables.load_folder_scan_items(std::path::Path::new(&root_key)).await.unwrap(),
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
    let (scan, _) =
        test.scan_with(tags.clone(), Arc::new(crate::folder_scanner::OsDirectoryReader));
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
    let tables = test.tables().clone();
    let files = readable_root(&test).await;
    let tags = Arc::new(CountingTagReader::default());
    let (scan, _) =
        test.scan_with(tags.clone(), Arc::new(crate::folder_scanner::OsDirectoryReader));
    let root_key = files.root.to_path_buf();
    let album_key = crate::CandidateKey::folder(&files.album);
    scan.rescan(&files.root).await.unwrap();
    let reading =
        tables.load_candidate_file_tag_snapshot(&root_key, &album_key).await.unwrap().unwrap();
    std::fs::write(files.album.join("notes.txt"), "Rip notes").unwrap();
    let before = files.reads(&tags);

    scan.rescan(&files.root).await.unwrap();

    assert_eq!(files.reads(&tags), before, "no file is read again");
    let stored =
        tables.load_candidate_file_tag_snapshot(&root_key, &album_key).await.unwrap().unwrap();
    assert_ne!(
        stored.candidate.files.content_hash(),
        reading.candidate.files.content_hash(),
        "the release holds a new file"
    );
    let preparation = crate::preparations::stored_preparation(
        test.tables(),
        &stored.candidate.files.content_hash(),
    )
    .await
    .unwrap()
    .expect("the release with the new file is stored");
    assert_eq!(preparation.metadata.draft.tracks.len(), 2);
    assert!(stored.snapshot.unwrap().same_reading(&reading.snapshot.unwrap()));
}

/// Two folders holding the same names at the same sizes and modification
/// times hash alike, so they share one draft, and each still keeps the tags
/// read from its own tracks: a rescan reads neither folder's files again.
#[tokio::test]
async fn folders_whose_files_hash_alike_each_keep_their_tags() {
    let test = setup_import_service().await;
    let tables = test.tables().clone();
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
    let root = bae_db::watched_path::canonical_absolute_root(&watched).unwrap();
    let root_key = root.to_path_buf();
    tables.add_watched_import_folder(std::path::Path::new(&root_key)).await.unwrap();
    let tags = Arc::new(CountingTagReader::default());
    let (scan, _) =
        test.scan_with(tags.clone(), Arc::new(crate::folder_scanner::OsDirectoryReader));
    scan.rescan(&root).await.unwrap();
    let mut hashes = Vec::new();
    for folder in ["First", "Second"] {
        let stored = tables
            .load_candidate_file_tag_snapshot(
                &root_key,
                &crate::CandidateKey::folder(&root.join(folder)),
            )
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

impl crate::file_tag_reader::FileTagReader for EmbeddingTagReader {
    fn read(&self, path: &Path) -> Result<crate::file_tag_reader::FileTagRead, crate::ImportError> {
        let mut read = self.counted.read(path)?;
        read.embedded_cover =
            Some((self.picture.clone(), bae_util::content_type::ContentType::Jpeg));
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
    let tables = test.tables().clone();
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
    let root = bae_db::watched_path::canonical_absolute_root(&watched).unwrap();
    let root_key = root.to_path_buf();
    tables.add_watched_import_folder(std::path::Path::new(&root_key)).await.unwrap();
    let tags = Arc::new(EmbeddingTagReader {
        counted: CountingTagReader::default(),
        picture: std::fs::read(&picture_path).unwrap(),
    });
    let (scan, _) =
        test.scan_with(tags.clone(), Arc::new(crate::folder_scanner::OsDirectoryReader));

    scan.rescan(&root).await.unwrap();

    for track in &tracks {
        assert_eq!(tags.counted.reads_of(&root.join(track)), 1, "{track}");
    }
    let release = tables
        .load_folder_scan_items(std::path::Path::new(&root_key))
        .await
        .unwrap()
        .into_iter()
        .find_map(|item| match item {
            ScanItem::Valid(candidate) => Some(candidate),
            _ => None,
        })
        .expect("the discs read as one release");
    assert_eq!(release.files.audio().count(), 4);
    let reading = tables
        .load_candidate_file_tag_snapshot(std::path::Path::new(&root_key), &release.key())
        .await
        .unwrap()
        .unwrap()
        .snapshot
        .expect("the release's draft was seeded from its tags");
    assert_eq!(reading.files.len(), 4);
    assert_eq!(
        reading.embedded_cover.map(|cover| cover.source_relative_path),
        Some("CD1/01.flac".to_string())
    );
}

/// Re-reading a folder nothing has touched is a scan that finds what it found
/// last time. It must write nothing and announce nothing: a watched folder is
/// re-read on a timer, and a pass that rewrites and re-announces every row it
/// already holds is work the whole app pays for — a database transaction, an
/// event, and a list rebuilt — once per row, forever, over a folder that did
/// not change.
#[tokio::test]
async fn a_second_pass_over_an_unchanged_folder_announces_nothing() {
    let test = setup_import_service().await;
    let tables = test.tables().clone();
    let root = test.temp.path().join("watched");
    for album in ["Artist - One", "Artist - Two"] {
        let album = root.join(album);
        std::fs::create_dir_all(&album).unwrap();
        std::fs::write(album.join("01.flac"), flac()).unwrap();
        std::fs::write(album.join("02.flac"), flac()).unwrap();
    }
    tables.add_watched_import_folder(&root).await.unwrap();

    let (scan, mut events) = test.scan();
    let pass = async || scan.rescan(&root).await.expect("the pass reads the folder");
    pass().await;
    while events.try_recv().is_ok() {}
    pass().await;

    assert_eq!(announced_candidates(&mut events), Vec::<String>::new());

    // And a folder that did change still announces — once, and only itself.
    std::fs::write(root.join("Artist - Two").join("03.flac"), flac()).unwrap();
    pass().await;

    assert_eq!(announced_candidates(&mut events), vec!["Artist - Two"]);
}

/// Discovery seeds the draft from what the folder's own files say, in the
/// same write that stores the candidate: the album, its artist and its tracks
/// are there the first time anyone looks, and the reading they came from is
/// stored beside them.
/// Counts what reading tags opens, file by file, and answers with what the
/// files really say.
#[derive(Default)]
struct CountingTagReader {
    reads: Mutex<std::collections::HashMap<PathBuf, u64>>,
}

impl CountingTagReader {
    /// Every read, of every file.
    fn reads(&self) -> u64 {
        self.reads.lock().unwrap().values().sum()
    }

    fn reads_of(&self, path: &Path) -> u64 {
        self.reads.lock().unwrap().get(path).copied().unwrap_or(0)
    }
}

impl crate::file_tag_reader::FileTagReader for CountingTagReader {
    fn read(&self, path: &Path) -> Result<crate::file_tag_reader::FileTagRead, crate::ImportError> {
        *self.reads.lock().unwrap().entry(path.to_path_buf()).or_default() += 1;
        crate::file_tag_reader::LoftyFileTagReader.read(path)
    }
}

#[tokio::test]
async fn pre_fill_seeds_the_discovered_candidate_from_its_file_tags() {
    let test = setup_import_service().await;
    let tables = test.tables().clone();
    let root = test.temp.path().join("watched");
    let album = root.join("Candidate");
    std::fs::create_dir_all(&album).unwrap();
    for name in TAGGED_FLAC_FIXTURES {
        std::fs::copy(bae_audio::test_fixtures::cue_flac_dir().join(name), album.join(name))
            .unwrap();
    }
    let root_text = root.to_string_lossy().into_owned();
    tables.add_watched_import_folder(std::path::Path::new(&root_text)).await.unwrap();

    let reader = std::sync::Arc::new(CountingTagReader::default());
    let (scan, mut events) =
        test.scan_with(reader.clone(), Arc::new(crate::folder_scanner::OsDirectoryReader));
    scan.rescan(&root).await.expect("the candidate is read and stored");

    // Two writes store one candidate — it arrives tentative, then valid — and
    // the second finds the draft the first seeded, so the folder's two tracks
    // are opened once between them.
    assert_eq!(reader.reads(), TAGGED_FLAC_FIXTURES.len() as u64);

    let key = crate::CandidateKey::folder(&album);
    let detail =
        tables.load_import_candidate(&key).await.unwrap().expect("the candidate is stored");
    assert_eq!(
        detail.draft().unwrap().metadata_provenance,
        Some(crate::MetadataProvenance::FileMetadata)
    );
    assert_eq!(detail.draft().unwrap().metadata_draft.album_title, "Test Album");
    assert_eq!(
        detail
            .draft()
            .unwrap()
            .metadata_draft
            .tracks
            .iter()
            .map(|track| track.title.clone())
            .collect::<Vec<_>>(),
        vec!["Track Two (White Noise)".to_string(), "Track Three (Brown Noise)".to_string(),]
    );
    let snapshot = tables
        .load_candidate_file_tag_snapshot(std::path::Path::new(&root_text), &key)
        .await
        .unwrap()
        .expect("the candidate has a snapshot")
        .snapshot
        .expect("the reading the draft was projected from is stored");
    assert_eq!(snapshot.file_edit_revision, 0);

    // A scan of a folder nobody touched re-reads no tags: the candidate holds
    // its draft, and the reading it was projected from is carried forward.
    scan.rescan(&root).await.expect("the folder is read again");
    assert_eq!(reader.reads(), TAGGED_FLAC_FIXTURES.len() as u64);
    assert!(tables
        .load_candidate_file_tag_snapshot(std::path::Path::new(&root_text), &key)
        .await
        .unwrap()
        .expect("the candidate is still stored")
        .snapshot
        .is_some());

    while let Ok(event) = events.try_recv() {
        if let crate::ImportEvent::Scan(crate::ScanEvent::FolderCandidate { candidate, .. }) = event
        {
            assert_eq!(candidate.path, album);
            return;
        }
    }
    panic!("the seeded candidate was not announced");
}

// A release is queued on its own only when a scan first finds it while the
// setting is on.

/// A release found while the setting is on is identified; one found after it
/// was turned off is not.
#[tokio::test(flavor = "multi_thread")]
async fn only_a_release_found_while_automatic_is_on_is_identified_on_its_own() {
    let fixture = Fixture::new("found-while-on").await;
    let on = fixture.disc_id_candidate("Found While On");
    fixture.provider.route("/discid/", 200, "{}");
    fixture.scan(1).await;

    fixture.set_identify_automatically(false).await;
    let off = fixture.disc_id_candidate("Found While Off");
    std::fs::write(off.join("notes.txt"), "a candidate of its own").unwrap();
    fixture.rescan(&fixture.import, 2).await;
    fixture.drain_automatic().await;

    assert!(
        fixture.identified_for(&on).await.is_some(),
        "the release found while identification ran on its own is identified"
    );
    assert!(
        fixture.identified_for(&off).await.is_none(),
        "the release found after it was turned off waits for a person"
    );
    assert_eq!(
        fixture.provider.count_containing("/discid/"),
        1,
        "one release was asked about: {:?}",
        fixture.provider.requests()
    );
}

/// Turning the setting on queues no candidate found while it was off.
#[tokio::test(flavor = "multi_thread")]
async fn turning_automatic_identification_on_queues_no_candidate_already_found() {
    let fixture = Fixture::new("on-reaches-back-to-nothing").await;
    let dir = fixture.disc_id_candidate("Album");
    fixture.provider.route("/discid/", 200, "{}");
    fixture.set_identify_automatically(false).await;
    fixture.scan(1).await;
    fixture.drain_automatic().await;

    fixture.set_identify_automatically(true).await;
    fixture.drain_automatic().await;

    assert_eq!(
        fixture.provider.count_containing("/discid/"),
        0,
        "nothing was asked about the candidate found while it was off: {:?}",
        fixture.provider.requests()
    );
    assert!(fixture.identified_for(&dir).await.is_none());
}

/// A rescan queues no candidate it had found, even with new files in its
/// folder.
#[tokio::test(flavor = "multi_thread")]
async fn a_rescan_queues_no_candidate_it_had_found() {
    let fixture = Fixture::new("rescan-queues-nothing").await;
    let dir = fixture.disc_id_candidate("Album");
    let key = crate::CandidateKey::folder(&dir);
    fixture.provider.route("/discid/", 200, "{}");
    fixture.set_identify_automatically(false).await;
    fixture.scan(1).await;
    let files_before = fixture
        .import
        .answerable_candidate(&key)
        .await
        .unwrap()
        .expect("the scanned candidate is answerable")
        .files
        .content_hash();
    fixture.set_identify_automatically(true).await;
    fixture.drain_automatic().await;

    fixture.rescan(&fixture.import, 1).await;
    std::fs::write(dir.join("notes.txt"), "a file added under the same folder").unwrap();
    fixture.rescan(&fixture.import, 1).await;
    fixture.drain_automatic().await;

    let files_after = fixture
        .import
        .answerable_candidate(&key)
        .await
        .unwrap()
        .expect("the rescanned candidate is answerable")
        .files
        .content_hash();
    assert_ne!(
        files_before, files_after,
        "the rescan stored the folder's new files under the same candidate"
    );
    assert_eq!(
        fixture.provider.count_containing("/discid/"),
        0,
        "no rescan queued the candidate: {:?}",
        fixture.provider.requests()
    );
    assert!(fixture.identified_for(&dir).await.is_none());
}

/// The launch scan queues no stored candidate, even one found while the
/// setting was on whose run the last session never started.
#[tokio::test(flavor = "multi_thread")]
async fn a_relaunch_queues_no_candidate_the_library_stored() {
    let fixture = Fixture::new("relaunch-queues-nothing").await;
    let dir = fixture.disc_id_candidate("Album");
    fixture.provider.route("/discid/", 200, "{}");
    fixture.scan(1).await;
    fixture.import.stop_and_join();

    let import = fixture.parts.start().await.unwrap();
    let identification = import.start_identification_queue();
    fixture.rescan(&import, 1).await;
    tokio::time::timeout(Duration::from_secs(30), identification.automatic_drained_for_test())
        .await
        .expect("the relaunched queue drains");

    assert_eq!(
        fixture.provider.count_containing("/discid/"),
        0,
        "the relaunch queued nothing: {:?}",
        fixture.provider.requests()
    );
    assert!(fixture.identified_for(&dir).await.is_none());
    assert!(
        import.candidate_runtime(&crate::CandidateKey::folder(&dir)).await.unwrap().is_none(),
        "nothing is waiting either"
    );
    identification.stop();
    import.stop_and_join();
}

/// A takeover queues only a release new to the parent.
#[tokio::test(flavor = "multi_thread")]
async fn a_takeover_queues_only_a_release_new_to_the_parent() {
    let fixture = Fixture::new("takeover-queues-only-new").await;
    let inner = fixture.root.join("Inner");
    let known = fixture.disc_id_candidate("Inner/Album");
    fixture.provider.route("/discid/", 200, "{}");
    fixture.set_identify_automatically(false).await;
    fixture.import.add_watched_folder(inner.clone()).await.unwrap();
    fixture.read_root(&fixture.import, &inner).await;
    fixture.await_listed(&fixture.import, 1).await;
    fixture.set_identify_automatically(true).await;

    let new = fixture.disc_id_candidate("New Album");
    std::fs::write(new.join("notes.txt"), "a candidate of its own").unwrap();
    fixture.import.add_watched_folder(fixture.root.clone()).await.unwrap();
    // Answered once the parent's read, which follows the takeover, is over.
    fixture.read_root(&fixture.import, &fixture.root).await;
    fixture.await_listed(&fixture.import, 2).await;
    fixture.drain_automatic().await;

    assert!(
        fixture.identified_for(&known).await.is_none(),
        "the candidate the inner folder had found waits for a person"
    );
    assert!(
        fixture.identified_for(&new).await.is_some(),
        "the release new to the parent is identified on its own"
    );
    assert_eq!(
        fixture.provider.count_containing("/discid/"),
        1,
        "one release was asked about: {:?}",
        fixture.provider.requests()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn automatic_lookup_off_runs_none_of_the_identification_pipeline() {
    let fixture = Fixture::new("automatic-off").await;
    let calls = Arc::new(AtomicUsize::new(0));
    fixture
        .import
        .register_artwork_analyzer(Arc::new(CountingAnalyzer { calls: Arc::clone(&calls) }));
    let dir = fixture.barcode_candidate("Candidate");
    fixture.set_identify_automatically(false).await;
    fixture.scan(1).await;

    fixture.drain_automatic().await;

    assert_eq!(calls.load(Ordering::Relaxed), 0, "OCR must not run");
    assert!(fixture.provider.requests().is_empty(), "no provider request may run");
    assert!(fixture.identified_for(&dir).await.is_none());
}

/// Files that changed retire the result they were read from. The candidate is
/// the one the library already found, so nothing identifies it again on its
/// own: it waits, unanswered, for a person to ask.
#[tokio::test(flavor = "multi_thread")]
async fn changed_files_retire_the_result_and_wait_for_a_person() {
    let fixture = Fixture::new("changed-files-wait").await;
    let dir = fixture.disc_id_candidate("Candidate");
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route("/discid/", 200, discid_json("mb-changed", "rg-changed", &[probed, 0]));
    fixture.provider.route(
        "/release/mb-changed?",
        200,
        mb_release_json("mb-changed", "rg-changed", &[probed, 0]),
    );
    fixture.scan(1).await;
    fixture.drain_automatic().await;
    assert!(fixture.identified_for(&dir).await.is_some());

    let asked = fixture.provider.count_containing("/discid/");

    std::fs::write(dir.join("notes.txt"), "the folder changed").unwrap();
    fixture.rescan(&fixture.import, 1).await;
    fixture.drain_automatic().await;

    assert!(fixture.identified_for(&dir).await.is_none(), "the files it has now have no result");
    assert_eq!(
        fixture.provider.count_containing("/discid/"),
        asked,
        "nothing asked about the changed candidate on its own: {:?}",
        fixture.provider.requests()
    );

    fixture.start_explicit_lookup(&dir);
    fixture.await_identified_row(&dir).await;
}

struct CountingAnalyzer {
    calls: Arc<AtomicUsize>,
}

impl ArtworkAnalyzer for CountingAnalyzer {
    fn analyze(&self, _path: &Path) -> ArtworkAnalysis {
        self.calls.fetch_add(1, Ordering::Relaxed);
        ArtworkAnalysis::empty()
    }
}
