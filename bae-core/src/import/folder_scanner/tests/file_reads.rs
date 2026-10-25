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
        &[]
    )
    .is_ok());
}
