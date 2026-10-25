/// One valid release at `/music/Album` holding the track `01.flac`, measured
/// as `audio`.
fn stored_release(audio: ScannedAudio) -> ScanItem {
    let mut file = ScannedFile::new(
        PathBuf::from("/music/Album/01.flac"),
        "01.flac".to_string(),
        10,
        1,
    );
    file.source_audio = Some(audio);
    ScanItem::Valid(FolderCandidate {
        path: PathBuf::from("/music/Album"),
        file_root: PathBuf::from("/music/Album"),
        name: "Album".to_string(),
        files: CategorizedFiles {
            files: vec![CandidateFile {
                file,
                role: FileRole::Audio,
            }],
            parts: Vec::new(),
        },
        watched_folder_path: "/music".to_string(),
        scope: ReleaseFileScope::Direct,
        file_edit_revision: 0,
        display_path: "Album".to_string(),
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
            stored_release(measured(5_000)),
            stored_release(measured(6_000)),
        ],
        &[],
        Arc::new(crate::import::file_tag_snapshot::LoftyFileTagReader),
    )
    .err()
    .expect("the rows disagree");
    assert!(
        error.to_string().contains("/music/Album/01.flac"),
        "{error}"
    );
    assert!(FileReads::stored(
        &[
            stored_release(measured(5_000)),
            stored_release(measured(5_000))
        ],
        &[],
        Arc::new(crate::import::file_tag_snapshot::LoftyFileTagReader),
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
        Arc::new(Self {
            size,
            reads: Default::default(),
        })
    }

    fn reads_of(&self, path: &Path) -> u64 {
        self.reads.lock().unwrap().get(path).copied().unwrap_or(0)
    }
}

impl crate::import::file_tag_snapshot::FileTagReader for PictureReader {
    fn read(
        &self,
        path: &Path,
    ) -> Result<crate::import::file_tag_snapshot::FileTagRead, crate::import::ImportError> {
        *self
            .reads
            .lock()
            .unwrap()
            .entry(path.to_path_buf())
            .or_default() += 1;
        let mut read = crate::import::file_tag_snapshot::LoftyFileTagReader.read(path)?;
        read.embedded_cover = Some((
            vec![0; self.size],
            crate::util::content_type::ContentType::Jpeg,
        ));
        Ok(read)
    }
}

/// The releases a scan of `root` through `reads` reads, as it last says them.
fn scan_reading(root: &Path, reads: &FileReads) -> Vec<FolderCandidate> {
    let mut releases = BTreeMap::new();
    scan_for_candidates_with_reader_cancellable_and_directories(
        &OsDirectoryReader,
        root.to_path_buf(),
        &ScanReadings {
            stored: &StoredCandidateEdits::none(),
            decisions: &FolderReleaseDecisions::default(),
            new_grouping_key: &fresh_grouping_key,
        },
        reads,
        &ScanCancellation::new(),
        |_| {},
        |item| {
            if let ScanItem::Valid(candidate) = item {
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
    let reads = FileReads::new(reader.clone());
    let [release] = scan_reading(tmp.path(), &reads).try_into().unwrap();
    let audio = audio_of(&release);

    let reading = reads.tag_snapshot(&audio, 0, 0, None).unwrap();
    assert_eq!(
        reading.embedded_cover.unwrap().source_relative_path,
        "01.flac"
    );
    for file in &audio {
        assert_eq!(reader.reads_of(&file.path), 1, "{}", file.relative_path);
    }

    let second = reads.tag_snapshot(&audio[1..], 0, 0, None).unwrap();
    assert_eq!(
        second.embedded_cover.unwrap().source_relative_path,
        "02.flac"
    );
    assert_eq!(reader.reads_of(&audio[1].path), 2);
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
    let reads = FileReads::new(reader.clone());
    let [first, second] = scan_reading(tmp.path(), &reads).try_into().unwrap();

    for release in [&second, &first] {
        let audio = audio_of(release);
        let reading = reads.tag_snapshot(&audio, 0, 0, None).unwrap();
        assert!(reading.embedded_cover.is_some());
    }
    assert_eq!(reader.reads_of(&audio_of(&second)[0].path), 1);
    assert_eq!(reader.reads_of(&audio_of(&first)[0].path), 2);
}
