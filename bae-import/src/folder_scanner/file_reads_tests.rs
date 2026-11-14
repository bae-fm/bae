//! What a pass knows it has read of each file, and the reads it takes again
//! only where a file changed.

use super::*;
use crate::folder_scanner::tests::*;
use std::sync::Arc;

/// One valid release at `/music/Album` holding the track `01.flac`, measured
/// as `audio`.
fn stored_release(audio: ScannedAudio) -> ScanItem {
    let mut file =
        ScannedFile::new(PathBuf::from("/music/Album/01.flac"), "01.flac".to_string(), 10, 1);
    file.source_audio = Some(audio);
    ScanItem::Valid(FolderCandidate {
        path: PathBuf::from("/music/Album"),
        file_root: PathBuf::from("/music/Album"),
        name: "Album".to_string(),
        files: CategorizedFiles {
            files: vec![CandidateFile { file, role: FileRole::Audio }],
            parts: Vec::new(),
        },
        watched_folder_path: std::path::PathBuf::from("/music"),
        scope: ReleaseFileScope::Direct,
        file_edit_revision: 0,
        relative_folder: crate::RelativeFolder::test("Album"),
        grouping: None,
    })
}

fn measured(duration_ms: u64) -> ScannedAudio {
    let mut audio = ScannedFile::new(PathBuf::from("/x"), "x".to_string(), 1, 1)
        .with_test_flac_audio()
        .source_audio
        .unwrap();
    audio.duration_ms = duration_ms;
    audio
}

/// Stored rows that measure one version of a file two ways are a store no
/// scan wrote: the pass reading from them fails rather than take either.
#[test]
fn stored_rows_that_disagree_about_a_file_fail_the_pass() {
    let error = FileReads::stored(
        &[
            FolderEntry::Release(stored_release(measured(5_000))),
            FolderEntry::Release(stored_release(measured(6_000))),
        ],
        &[],
        Arc::new(crate::file_tag_reader::LoftyFileTagReader),
        bae_store::FileLeases::for_test_default(),
    )
    .err()
    .expect("the rows disagree");
    assert!(error.to_string().contains("/music/Album/01.flac"), "{error}");
    assert!(FileReads::stored(
        &[
            FolderEntry::Release(stored_release(measured(5_000))),
            FolderEntry::Release(stored_release(measured(5_000))),
        ],
        &[],
        Arc::new(crate::file_tag_reader::LoftyFileTagReader),
        bae_store::FileLeases::for_test_default()
    )
    .is_ok());
}

/// Reads tags as a scan does, says each track embeds a picture of `size`
/// bytes, and counts each read of each file.
struct PictureReader {
    size: usize,
    reads: std::sync::Mutex<HashMap<PathBuf, u64>>,
}

impl PictureReader {
    fn new(size: usize) -> Arc<Self> {
        Arc::new(Self { size, reads: Default::default() })
    }

    fn reads_of(&self, path: &Path) -> u64 {
        self.reads.lock().unwrap().get(path).copied().unwrap_or(0)
    }
}

impl crate::file_tag_reader::FileTagReader for PictureReader {
    fn read(
        &self,
        path: &Path,
        place: coven::LeasePlace,
    ) -> Result<crate::file_tag_reader::FileTagRead, crate::file_tag_reader::TagReadFailure> {
        *self.reads.lock().unwrap().entry(path.to_path_buf()).or_default() += 1;
        let mut read = crate::file_tag_reader::LoftyFileTagReader.read(path, place)?;
        read.embedded_cover = Some((vec![0; self.size], bae_util::content_type::ContentType::Jpeg));
        Ok(read)
    }
}

/// The releases a scan of `root` through `reads` reads, as it last says them.
fn scan_reading(root: &Path, reads: &FileReads) -> Vec<FolderCandidate> {
    let mut releases = BTreeMap::new();
    let ids = coven::SequentialIds::new();
    let new_key = || new_grouping_key(&ids);
    scan_for_candidates_with_reader_cancellable_and_directories(
        &OsDirectoryReader,
        root.to_path_buf(),
        &ScanReadings {
            stored: &StoredCandidateEdits::none(),
            decisions: &FolderReleaseDecisions::default(),
            new_grouping_key: &new_key,
        },
        reads,
        &ScanCancellation::new(),
        |_| {},
        |item| {
            if let WalkItem::Entry(FolderEntry::Release(ScanItem::Valid(candidate))) = item {
                releases.insert(candidate.key(), candidate);
            }
        },
    )
    .unwrap();
    releases.into_values().collect()
}

fn audio_of(release: &FolderCandidate) -> Vec<ScannedFile> {
    release.files.audio().cloned().collect()
}

/// A pass holds, of the pictures a release's tracks embed, only the one its
/// draft takes — its first track's — so a reading led by another of its
/// tracks reads that one's tags again, while the release's own reads none.
#[test]
fn a_pass_holds_only_the_picture_a_release_takes() {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(&album).unwrap();
    std::fs::write(album.join("01.flac"), fake_flac()).unwrap();
    std::fs::write(album.join("02.flac"), fake_flac()).unwrap();
    let reader = PictureReader::new(16);
    let reads = FileReads::new(reader.clone(), bae_store::FileLeases::for_test_default());
    let [release] = scan_reading(tmp.path(), &reads).try_into().unwrap();
    let audio = audio_of(&release);

    let reading = reads.tag_snapshot(&audio, 0, 0, None).unwrap();
    assert_eq!(reading.embedded_cover.unwrap().source_relative_path, "01.flac");
    for file in &audio {
        assert_eq!(reader.reads_of(&file.version.path), 1, "{}", file.relative_path);
    }

    let second = reads.tag_snapshot(&audio[1..], 0, 0, None).unwrap();
    assert_eq!(second.embedded_cover.unwrap().source_relative_path, "02.flac");
    assert_eq!(reader.reads_of(&audio[1].version.path), 2);
}

/// A pass holds no more than [`file_reads::HELD_COVER_BYTES`] of pictures:
/// past that it lets the oldest go, and a draft that would take it reads its
/// track's tags again.
#[test]
fn a_pass_lets_the_oldest_picture_go_past_what_it_holds() {
    let tmp = tempfile::tempdir().unwrap();
    // Both under one folder of the root, which one walk reads in order, so
    // `First`'s picture is the older.
    for folder in ["Releases/First", "Releases/Second"] {
        std::fs::create_dir_all(tmp.path().join(folder)).unwrap();
        std::fs::write(tmp.path().join(folder).join("01.flac"), fake_flac()).unwrap();
    }
    let reader = PictureReader::new(file_reads::HELD_COVER_BYTES / 2 + 1);
    let reads = FileReads::new(reader.clone(), bae_store::FileLeases::for_test_default());
    let [first, second] = scan_reading(tmp.path(), &reads).try_into().unwrap();

    for release in [&second, &first] {
        let audio = audio_of(release);
        let reading = reads.tag_snapshot(&audio, 0, 0, None).unwrap();
        assert!(reading.embedded_cover.is_some());
    }
    assert_eq!(reader.reads_of(&audio_of(&second)[0].version.path), 1);
    assert_eq!(reader.reads_of(&audio_of(&first)[0].version.path), 2);
}

#[test]
fn same_size_same_mtime_corrupt_replacement_is_not_served_from_probe_cache() {
    let result = run_scenario(vec![file("Album/01.flac", FileKind::Flac)]);
    let path = result.root.join("Album/01.flac");
    let original_metadata = std::fs::metadata(&path).unwrap();
    std::fs::write(&path, vec![0; original_metadata.len() as usize]).unwrap();
    std::fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(original_metadata.modified().unwrap()))
        .unwrap();

    let items = scan_items(result.root);
    assert!(items.iter().any(|item| matches!(
        item,
        ScanItem::Invalid(invalid)
            if invalid.reason() == InvalidReason::DamagedAudio { count: 1 }
    )));
    assert!(!items
        .iter()
        .any(|item| matches!(item, ScanItem::Valid(candidate) if candidate.name == "Album")));
}

/// A track whose bytes can't be read says nothing of its audio: the scan
/// fails with the read's error instead of reading the folder as damaged audio.
#[cfg(unix)]
#[test]
fn an_unreadable_track_fails_the_scan_instead_of_reading_as_damaged_audio() {
    use std::os::unix::fs::PermissionsExt;

    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(&album).unwrap();
    let track = album.join("01.flac");
    std::fs::write(&track, fake_flac()).unwrap();
    std::fs::set_permissions(&track, std::fs::Permissions::from_mode(0o000)).unwrap();

    let result = scan_for_candidates_with_decisions(
        tmp.path().to_path_buf(),
        &StoredCandidateEdits::none(),
        &FolderReleaseDecisions::default(),
        &coven::SequentialIds::new(),
        |_| {},
    );

    std::fs::set_permissions(&track, std::fs::Permissions::from_mode(0o600)).unwrap();
    match result {
        Err(FolderScanError::Io { path, source }) => {
            assert_eq!(path, track);
            assert_eq!(source.kind(), std::io::ErrorKind::PermissionDenied);
        }
        other => panic!("expected the track's read error, got {other:?}"),
    }
}

/// Run `read` over a pass's reads on a thread of its own while a test holds
/// the only place of a budget of one: the read waits for the budget, and
/// finishes once the place goes back. Returns what it read.
async fn read_after_waiting_for_the_only_place<T: Send + 'static>(
    read: impl FnOnce(FileReads) -> T + Send + 'static,
) -> T {
    let budget = coven::FileBudget::new(std::num::NonZeroUsize::MIN);
    let watch = budget.watch();
    let reads = FileReads::new(
        Arc::new(crate::file_tag_reader::LoftyFileTagReader),
        bae_store::FileLeases::for_test(budget.clone()),
    );
    let held = budget.lease(1).await.unwrap();
    let waited = watch.lease_waits_from_now(1);

    let diagnostics = bae_diagnostics::Diagnostics::noop();
    let reading = diagnostics.blocking("read a file", move || read(reads));
    tokio::pin!(reading);
    tokio::select! {
        _ = &mut reading => panic!("the read opened its file without a place"),
        () = waited => {}
    }
    drop(held);

    let read = reading.await.unwrap();
    assert_eq!(watch.holding(), 0);
    assert_eq!(watch.leased(), 0);
    read
}

/// One track of a scanned folder.
fn one_scanned_track() -> (tempfile::TempDir, ScannedFile) {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(&album).unwrap();
    std::fs::write(album.join("01.flac"), fake_flac()).unwrap();
    let reads = FileReads::new(
        Arc::new(crate::file_tag_reader::LoftyFileTagReader),
        bae_store::FileLeases::for_test_default(),
    );
    let [release] = scan_reading(tmp.path(), &reads).try_into().unwrap();
    let track = audio_of(&release).remove(0);
    (tmp, track)
}

/// `path` as a scan lists it.
fn scanned(path: &Path) -> ScannedFile {
    let metadata = std::fs::metadata(path).unwrap();
    ScannedFile::new(
        path.to_path_buf(),
        path.file_name().unwrap().to_string_lossy().into_owned(),
        metadata.len(),
        file_modified_at_ns(path, &metadata).unwrap(),
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn a_probe_waits_for_a_place() {
    let (_tmp, track) = one_scanned_track();

    let playability =
        read_after_waiting_for_the_only_place(move |reads| reads.stream(&track).unwrap()).await;

    assert!(matches!(playability, Playability::Measured(_)), "{playability:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_tag_read_waits_for_a_place() {
    let (_tmp, track) = one_scanned_track();

    let tags =
        read_after_waiting_for_the_only_place(move |reads| reads.tags(&track).unwrap()).await;

    assert!(tags.is_ok(), "{tags:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_image_check_waits_for_a_place() {
    let tmp = tempfile::tempdir().unwrap();
    let cover = tmp.path().join("cover.png");
    std::fs::write(&cover, b"\x89PNG\r\n\x1a\n and a few more bytes of a picture").unwrap();
    let version = scanned(&cover).version;

    let decodes =
        read_after_waiting_for_the_only_place(move |reads| reads.image_decodes(version).unwrap())
            .await;

    assert!(decodes);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cue_sheet_read_waits_for_a_place() {
    let tmp = tempfile::tempdir().unwrap();
    let sheet = tmp.path().join("Album.cue");
    std::fs::write(
        &sheet,
        "FILE \"Album.flac\" WAVE\n  TRACK 01 AUDIO\n    TITLE \"One\"\n    INDEX 01 00:00:00\n",
    )
    .unwrap();
    let file = scanned(&sheet);

    let parsed = read_after_waiting_for_the_only_place(move |reads| reads.sheet(&file)).await;

    assert!(parsed.is_some(), "the sheet parses");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_folder_listing_waits_for_a_place() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("Album")).unwrap();
    std::fs::write(tmp.path().join("Album/01.flac"), fake_flac()).unwrap();
    let root = tmp.path().to_path_buf();

    let releases =
        read_after_waiting_for_the_only_place(move |reads| scan_reading(&root, &reads)).await;

    assert_eq!(releases.len(), 1, "the scan reads the folder once it has a place");
}
