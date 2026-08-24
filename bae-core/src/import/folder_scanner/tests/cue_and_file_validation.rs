#[test]
fn test_is_audio_file() {
    assert!(is_audio_file(Path::new("track.flac")));
    assert!(is_audio_file(Path::new("track.FLAC")));
    assert!(is_audio_file(Path::new("track.mp3")));
    assert!(is_audio_file(Path::new("track.MP3")));
    assert!(is_audio_file(Path::new("track.ape")));
    assert!(is_audio_file(Path::new("track.APE")));
    assert!(is_audio_file(Path::new("track.m4a")));
    assert!(is_audio_file(Path::new("track.M4A")));
    assert!(is_audio_file(Path::new("track.wav")));
    assert!(is_audio_file(Path::new("track.aif")));
    assert!(is_audio_file(Path::new("track.aiff")));
    assert!(is_audio_file(Path::new("track.aifc")));
    assert!(is_audio_file(Path::new("track.ogg")));
    assert!(is_audio_file(Path::new("track.oga")));
    assert!(is_audio_file(Path::new("track.opus")));
    assert!(is_audio_file(Path::new("track.wv")));
    assert!(is_audio_file(Path::new("track.dsf")));
    assert!(is_audio_file(Path::new("track.dff")));
    assert!(!is_audio_file(Path::new("track.wma")));
    assert!(!is_audio_file(Path::new("track.mpc")));
    assert!(!is_audio_file(Path::new("track.spx")));
    assert!(!is_audio_file(Path::new("cover.jpg")));
    assert!(!is_audio_file(Path::new("notes.txt")));
}

#[test]
fn test_is_cue_file() {
    assert!(is_cue_file(Path::new("album.cue")));
    assert!(is_cue_file(Path::new("album.CUE")));
    assert!(!is_cue_file(Path::new("album.flac")));
}

#[test]
fn test_cue_parser_counts_audio_tracks_and_captures_file_reference() {
    let tmp = tempfile::tempdir().unwrap();
    let cue = tmp.path().join("album.cue");
    let content = "FILE \"album.flac\" WAVE\n  TRACK 01 AUDIO\n    INDEX 01 00:00:00\n  TRACK 02 AUDIO\n    INDEX 01 05:00:00\n  TRACK 03 AUDIO\n    INDEX 01 10:00:00\n";
    std::fs::write(&cue, content).unwrap();

    let sheet = parse_cue_sheet(&cue).unwrap();
    assert_eq!(sheet.single_file(), Some("album.flac"));
    assert_eq!(sheet.tracks.len(), 3);
}

#[test]
fn test_cue_parser_tolerates_missing_performer_title() {
    // Minimal CUE with no PERFORMER/TITLE — still a valid rip artifact,
    // must parse so the scanner and importer see the same facts.
    let tmp = tempfile::tempdir().unwrap();
    let cue = tmp.path().join("album.cue");
    let content = "FILE \"dummy.flac\" WAVE\n  TRACK 01 AUDIO\n    INDEX 01 00:00:00\n";
    std::fs::write(&cue, content).unwrap();

    let sheet = parse_cue_sheet(&cue).unwrap();
    assert!(sheet.title.is_none());
    assert!(sheet.performer.is_none());
    assert_eq!(sheet.single_file(), Some("dummy.flac"));
    assert_eq!(sheet.tracks.len(), 1);
}

#[test]
fn test_cue_parser_stops_at_data_track() {
    let tmp = tempfile::tempdir().unwrap();
    let cue = tmp.path().join("album.cue");
    let content = "FILE \"album.flac\" WAVE\n  TRACK 01 AUDIO\n    INDEX 01 00:00:00\n  TRACK 02 MODE1/2048\n    INDEX 01 05:00:00\n";
    std::fs::write(&cue, content).unwrap();

    let sheet = parse_cue_sheet(&cue).unwrap();
    assert_eq!(sheet.tracks.len(), 2);
    assert_eq!(sheet.playable_track_count(), 1);
    assert!(matches!(
        sheet.tracks[1].mode,
        CueTrackMode::Other(ref mode) if mode == "MODE1/2048"
    ));
}

#[test]
fn test_collect_release_candidate_files_skips_hidden_and_bae() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path();

    // Create visible files
    std::fs::write(root.join("track.flac"), fake_flac()).unwrap();
    std::fs::write(root.join("cover.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();
    std::fs::write(root.join("back.bmp"), b"BMvalid bmp marker").unwrap();

    // Create hidden file that should be ignored
    std::fs::write(root.join(".DS_Store"), b"mac junk").unwrap();

    // Create .bae directory -- entirely ignored by the scanner
    let bae_dir = root.join(".bae");
    std::fs::create_dir(&bae_dir).unwrap();
    std::fs::write(bae_dir.join("cover-mb.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();
    std::fs::write(bae_dir.join("cover-discogs.jpeg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();

    let files = collect_release_candidate_files_with_scope(
        root,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .unwrap();

    let audio_paths: Vec<_> = files.audio().map(|f| f.relative_path.as_str()).collect();
    assert_eq!(audio_paths, vec!["track.flac"]);

    // Only release artwork, not .bae/ files
    let artwork_paths: Vec<_> = files.artwork().map(|f| f.relative_path.as_str()).collect();
    assert_eq!(artwork_paths, vec!["back.bmp", "cover.jpg"]);

    assert_eq!(files.documents().count(), 0);
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
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect_err("zero-byte audio makes the folder unimportable");
    assert!(
        matches!(
            err,
            crate::import::ImportError::InvalidFolder(InvalidReason::CorruptAudioFile { .. })
        ),
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

    let err = scan_for_candidates_with_callback(
        root.to_path_buf(),
        &StoredCandidateEdits::none(),
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
            panic!(
                "expected IO error, got not-a-directory for {}",
                path.display()
            )
        }
        FolderScanError::Other(message) => panic!("expected IO error, got {message}"),
        FolderScanError::Cancelled => panic!("expected IO error, got cancellation"),
    }
}

#[test]
fn content_hash_is_location_independent_and_size_sensitive() {
    let make = |root: &str, second_size: u64| CategorizedFiles {
        files: vec![
            audio_entry(&format!("{root}/01.flac"), "01.flac", 1000),
            audio_entry(&format!("{root}/02.flac"), "02.flac", second_size),
        ],
        format_label: "FLAC".to_string(),
    };

    // The same relative structure under two different parent folders hashes
    // identically — the fingerprint follows the rip, not where it sits.
    let a = make("/Volumes/Music/Release", 2000);
    let b = make("/tmp/import_source/Release", 2000);
    assert_eq!(a.content_hash(), b.content_hash());

    // A single differing file size flips the hash.
    let c = make("/Volumes/Music/Release", 2001);
    assert_ne!(a.content_hash(), c.content_hash());
}

#[test]
fn content_hash_is_independent_of_discovery_order() {
    let entry = |name: &str, size: u64, role: FileRole| CandidateFile {
        proposed_audio: matches!(role, FileRole::Audio),
        file: ScannedFile::new(PathBuf::from(name), name.to_string(), size),
        role,
    };
    let forward = CategorizedFiles {
        files: vec![
            entry("01.flac", 1, FileRole::Audio),
            entry("02.flac", 2, FileRole::Audio),
            entry("cover.jpg", 3, FileRole::Artwork),
            entry("notes.txt", 4, FileRole::Document),
        ],
        format_label: "FLAC".to_string(),
    };
    let shuffled = CategorizedFiles {
        files: vec![
            entry("notes.txt", 4, FileRole::Document),
            entry("02.flac", 2, FileRole::Audio),
            entry("cover.jpg", 3, FileRole::Artwork),
            entry("01.flac", 1, FileRole::Audio),
        ],
        format_label: "FLAC".to_string(),
    };
    assert_eq!(forward.content_hash(), shuffled.content_hash());
}

/// Creates a minimal CUE file content that references the given FLAC filename
fn make_cue_content(flac_filename: &str, title: &str) -> String {
    format!(
        r#"PERFORMER "Test Artist"
TITLE "{title}"
FILE "{flac_filename}" WAVE
  TRACK 01 AUDIO
    TITLE "Track One"
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE "Track Two"
    INDEX 01 05:00:00
"#
    )
}

#[test]
fn test_cue_with_corrupt_ape_surfaces_as_invalid_candidate() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("APE Album");
    std::fs::create_dir(&root).unwrap();

    let cue_content = r#"PERFORMER "Test Artist"
TITLE "Test Album"
FILE "album.ape" WAVE
  TRACK 01 AUDIO
    TITLE "Track One"
    INDEX 01 00:00:00
"#;
    std::fs::write(root.join("album.cue"), cue_content).unwrap();
    // APE file with invalid magic bytes (not "MAC ")
    std::fs::write(root.join("album.ape"), b"fake ape data").unwrap();
    std::fs::write(root.join("cover.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();

    // A folder whose only audio is corrupt is not dropped and not a valid
    // candidate — it surfaces as an invalid candidate carrying the reason.
    let items = scan_items(root);
    assert_eq!(items.len(), 1, "exactly one scan item for the leaf");
    match &items[0] {
        ScanItem::Invalid(invalid) => {
            assert!(
                matches!(invalid.reason, InvalidReason::CorruptAudioFile { .. }),
                "reason names the audio fault, got: {}",
                invalid.reason,
            );
        }
        ScanItem::Valid(_) => panic!("corrupt-audio folder must not be a valid candidate"),
        ScanItem::Discovered(_) | ScanItem::Boundary(_) | ScanItem::Decided { .. } => {
            panic!("terminal scan helper returned a progress item")
        }
    }
}

/// Bytes of a placeholder audio fixture that probes to a specific codec, used to
/// build CUE pairs whose probed codec identity is what a test asserts against.
fn audio_format_fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/test-fixtures/audio-format/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap_or_else(|e| panic!("read audio fixture {name}: {e}"))
}

/// A FLAC with valid `fLaC` magic + well-formed STREAMINFO shape but no audio
/// frames and `total_samples = 0` (streaming-length unknown). It passes the
/// header-only `is_valid_flac` check yet has no usable duration, so the FFmpeg
/// probe can't identify a playable stream — the shape of a download truncated
/// right after the STREAMINFO block.
fn header_only_flac_unprobeable() -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(b"fLaC");
    // STREAMINFO block header: last-block=0, type=0, length=34.
    buf.extend_from_slice(&[0x00, 0x00, 0x00, 34]);
    // STREAMINFO data (34 bytes): 44100 Hz, 2ch, 16-bit, total_samples=0.
    buf.extend_from_slice(&[0x10, 0x00]); // min block size 4096
    buf.extend_from_slice(&[0x10, 0x00]); // max block size 4096
    buf.extend_from_slice(&[0x00, 0x00, 0x00]); // min frame size
    buf.extend_from_slice(&[0x00, 0x00, 0x00]); // max frame size
    buf.push(0x0A); // sample_rate >> 12
    buf.push(0xC4); // (sample_rate >> 4) & 0xFF
    buf.push(0x42); // (sample_rate & 0x0F)<<4 | (ch-1)<<1 | (bps-1)>>4
    buf.push(0xF0); // (bps-1 & 0x0F)<<4 | total_samples high nibble
    buf.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // total_samples low 32 bits
    buf.extend_from_slice(&[0u8; 16]); // MD5 signature
    assert_eq!(buf.len(), 42);
    buf
}

/// A CUE-paired audio file that clears the header-only magic check but can't be
/// probed (no playable stream) surfaces the folder as an invalid candidate — it
/// must NOT abort the whole watched-root walk. Same failure class as an
/// unsupported codec, triggered instead by one corrupt/incomplete file. A
/// sibling FLAC release under the same root still scans.
#[test]
fn cue_with_unprobeable_audio_is_invalid_and_siblings_still_scan() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path();

    let bad = root.join("Truncated Album");
    std::fs::create_dir(&bad).unwrap();
    std::fs::write(
        bad.join("album.cue"),
        make_cue_content("album.flac", "Test Album"),
    )
    .unwrap();
    std::fs::write(bad.join("album.flac"), header_only_flac_unprobeable()).unwrap();

    let good = root.join("FLAC Album");
    std::fs::create_dir(&good).unwrap();
    std::fs::write(good.join("01 Track.flac"), fake_flac()).unwrap();

    let items = scan_items(root.to_path_buf());
    assert_eq!(items.len(), 2, "both leaves surface");

    let invalid = items
        .iter()
        .find_map(|i| match i {
            ScanItem::Invalid(inv) if inv.name == "Truncated Album" => Some(inv),
            _ => None,
        })
        .expect("expected an invalid candidate for the unprobeable CUE audio");
    assert!(
        matches!(invalid.reason, InvalidReason::CorruptAudioFile { .. }),
        "reason names the audio fault, got: {}",
        invalid.reason,
    );

    let sibling_scanned = items
        .iter()
        .any(|i| matches!(i, ScanItem::Valid(c) if c.name == "FLAC Album"));
    assert!(sibling_scanned, "sibling FLAC release still scans");
}

/// A CUE paired with an audio file whose codec can't back single-file CUE
/// playback (MP3, Vorbis) costs the sheet its binding — bae can't carve tracks
/// out of that container — but the folder still imports: the audio keeps its
/// role and becomes one track, labelled by its own format.
#[test]
fn cue_with_unsupported_codec_leaves_the_sheet_unbound() {
    for (folder, audio_name, fixture, codec, label) in [
        (
            "MP3 Album",
            "album.mp3",
            "placeholder-mp3.mp3",
            "MP3",
            "MP3",
        ),
        (
            "Ogg Album",
            "album.ogg",
            "placeholder-vorbis.ogg",
            "Vorbis",
            "OGG",
        ),
    ] {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path();

        let album = root.join(folder);
        std::fs::create_dir(&album).unwrap();
        std::fs::write(
            album.join("album.cue"),
            make_cue_content(audio_name, "Test Album"),
        )
        .unwrap();
        std::fs::write(album.join(audio_name), audio_format_fixture(fixture)).unwrap();

        let candidates = scan_valid(root.to_path_buf());
        assert_eq!(candidates.len(), 1, "{folder}: one valid candidate");
        let files = &candidates[0].files;
        assert!(
            files.bound_sheets().is_empty(),
            "{folder}: the sheet must not bind to a codec bae can't carve",
        );
        // The refusal keeps the file it named and the codec, so the pane can
        // say which file and why instead of leaving the row reading as a bug —
        // and so the editor that makes this binding a user decision can refuse
        // the same pairing up front rather than at commit.
        let sheets: Vec<_> = files.track_sheets().collect();
        assert_eq!(sheets.len(), 1);
        assert_eq!(
            sheets[0].binding,
            &SheetBinding::RefusedCodec {
                file_id: audio_name.to_string(),
                codec: codec.to_string(),
            },
            "{folder}: the refusal names the file and the probed codec",
        );
        assert_eq!(
            files.audio().count(),
            1,
            "{folder}: the audio keeps its role",
        );
        assert_eq!(files.track_count(), 1, "{folder}: it imports as one track");
        assert_eq!(
            files.format_label, label,
            "{folder}: labelled by the file's own format",
        );
    }
}

/// A CUE paired with a codec that CAN back single-file CUE playback yields a
/// valid candidate labeled `CUE+<codec>`. PCM, WavPack, and DSD are otherwise
/// untested positive arms of the codec-label match.
#[test]
fn cue_with_supported_codec_yields_valid_candidate_labeled() {
    for (folder, audio_name, fixture, label) in [
        ("PCM Album", "album.wav", "placeholder-pcm.wav", "CUE+PCM"),
        (
            "WavPack Album",
            "album.wv",
            "placeholder-wavpack.wv",
            "CUE+WavPack",
        ),
        ("DSD Album", "album.dsf", "placeholder-dsd.dsf", "CUE+DSD"),
    ] {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path().join(folder);
        std::fs::create_dir(&root).unwrap();
        std::fs::write(
            root.join("album.cue"),
            make_cue_content(audio_name, "Test Album"),
        )
        .unwrap();
        std::fs::write(root.join(audio_name), audio_format_fixture(fixture)).unwrap();

        let candidates = scan_valid(root);
        assert_eq!(candidates.len(), 1, "{folder}: one valid candidate");
        assert_eq!(
            candidates[0].files.format_label, label,
            "{folder}: CUE+<codec> label",
        );
    }
}

#[test]
fn test_empty_folder_not_detected() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Empty Album");
    std::fs::create_dir(&root).unwrap();

    let candidates = scan_valid(root);

    assert_eq!(candidates.len(), 0, "Empty folder should not be detected");
}

#[test]
fn test_folder_with_only_images_not_detected() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Just Images");
    std::fs::create_dir(&root).unwrap();

    std::fs::write(root.join("cover.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();
    std::fs::write(
        root.join("back.png"),
        [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A],
    )
    .unwrap();

    let candidates = scan_valid(root);

    assert_eq!(
        candidates.len(),
        0,
        "Folder with only images should not be detected"
    );
}

#[test]
fn test_video_ts_folder_not_detected() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Concert DVD");
    std::fs::create_dir(&root).unwrap();

    let video_ts = root.join("VIDEO_TS");
    std::fs::create_dir(&video_ts).unwrap();
    std::fs::write(video_ts.join("VIDEO_TS.VOB"), b"fake video").unwrap();
    std::fs::write(video_ts.join("VTS_01_1.VOB"), b"fake video").unwrap();

    let candidates = scan_valid(root);

    assert_eq!(
        candidates.len(),
        0,
        "VIDEO_TS folder (DVD rip) should not be detected"
    );
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
        assert!(
            volumes.contains(&name),
            "unexpected candidate name {:?}",
            name,
        );
    }
}

#[test]
fn test_zero_byte_files_ignored() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Incomplete Download");
    std::fs::create_dir(&root).unwrap();

    std::fs::write(root.join("01 - Track One.flac"), b"").unwrap();
    std::fs::write(root.join("02 - Track Two.flac"), b"").unwrap();
    std::fs::write(root.join("cover.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();

    let candidates = scan_valid(root);

    assert_eq!(
        candidates.len(),
        0,
        "Folder with only 0-byte FLAC files should not be detected"
    );
}

#[test]
fn test_mix_of_real_and_zero_byte_files_skips_candidate() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Partial Download");
    std::fs::create_dir(&root).unwrap();

    std::fs::write(root.join("01 - Track One.flac"), fake_flac()).unwrap();
    std::fs::write(root.join("02 - Track Two.flac"), b"").unwrap();
    std::fs::write(root.join("03 - Track Three.flac"), b"").unwrap();

    let candidates = scan_valid(root.clone());

    assert_eq!(
        candidates.len(),
        0,
        "Candidate with corrupt files should be skipped entirely"
    );
}

#[test]
fn test_corrupt_image_skips_entire_candidate() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Bad Images");
    std::fs::create_dir(&root).unwrap();

    std::fs::write(root.join("track.flac"), fake_flac()).unwrap();
    std::fs::write(root.join("front.jpg"), [0xFF, 0xD8, 0xFF, 0xE0, 0x00]).unwrap();
    std::fs::write(root.join("back.jpg"), b"not a jpeg").unwrap();
    std::fs::write(root.join("inlay.png"), b"").unwrap();

    let candidates = scan_valid(root);

    assert_eq!(
        candidates.len(),
        0,
        "Candidate with corrupt images should be skipped entirely"
    );
}

/// Per-track FLACs plus a sheet naming absent audio: the sheet is a proposal,
/// not the layout, so it stays unbound and the twelve tracks import.
#[test]
fn per_track_flacs_with_missing_cue_audio_still_import() {
    let tmp = tempfile::TempDir::new().unwrap();
    let album = tmp
        .path()
        .join("Collection")
        .join("Artist - Album Title - (1991) {Label CAT-12345}");
    std::fs::create_dir_all(&album).unwrap();

    // 12 per-track FLACs
    for i in 1..=12 {
        std::fs::write(
            album.join(format!("{:02} - Track {i}.flac", i)),
            fake_flac(),
        )
        .unwrap();
    }

    // CUE sheet with missing referenced audio.
    std::fs::write(
        album.join("Album Title.cue"),
        "FILE \"dummy.flac\" WAVE\n  TRACK 01 AUDIO\n    INDEX 01 00:00:00\n",
    )
    .unwrap();

    // LOG file
    std::fs::write(album.join("Artist - Album Title.log"), "EAC log\n").unwrap();

    // Artwork subfolder with images
    let artwork = album.join("Artwork");
    std::fs::create_dir_all(&artwork).unwrap();
    std::fs::write(artwork.join("front.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();
    std::fs::write(artwork.join("back.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();
    std::fs::write(
        artwork.join("disc.png"),
        [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A],
    )
    .unwrap();

    // folder.jpg at album root
    std::fs::write(album.join("folder.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();

    let items = scan_items(tmp.path().join("Collection"));

    assert_eq!(items.len(), 1);
    match &items[0] {
        ScanItem::Valid(candidate) => {
            assert!(candidate.name.contains("Artist - Album Title"));
            assert_eq!(candidate.files.audio().count(), 12);
            assert!(candidate.files.bound_sheets().is_empty());
            let sheets: Vec<_> = candidate.files.track_sheets().collect();
            assert_eq!(sheets.len(), 1);
            assert_eq!(
                sheets[0].binding,
                &SheetBinding::Unresolved,
                "the sheet names absent audio",
            );
            assert_eq!(candidate.files.track_count(), 12);
        }
        ScanItem::Invalid(invalid) => {
            panic!(
                "a sheet naming absent audio must not invalidate: {}",
                invalid.reason
            )
        }
        ScanItem::Discovered(_) | ScanItem::Boundary(_) | ScanItem::Decided { .. } => {
            panic!("terminal scan helper returned a progress item")
        }
    }
}

/// A CUE paired with an `.m4a` file produces a `CUE+ALAC` format label
/// because FFmpeg probes the actual codec instead of trusting the extension.
#[test]
fn test_collect_release_candidate_files_cue_alac_format_label() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path();

    let stem = "Artist Name - Album Title";
    let m4a_name = format!("{stem}.m4a");
    let cue_name = format!("{stem}.cue");

    std::fs::write(root.join(&m4a_name), fake_m4a()).unwrap();
    std::fs::write(
        root.join(&cue_name),
        make_cue_content_n_tracks(&m4a_name, "Album Title", 8),
    )
    .unwrap();

    let files = collect_release_candidate_files_with_scope(
        root,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect("scan should succeed");

    assert_eq!(files.format_label, "CUE+ALAC");
    let bound = files.bound_sheets();
    assert_eq!(bound.len(), 1);
    assert_eq!(bound[0].sheet.tracks.len(), 8);
}

/// Multi-FILE CUEs resolve as CUE-backed releases, with every referenced audio
/// file attached to the pair. The release's signals — here the CATALOG (UPC) —
/// live on the parsed sheet attached to the pair.
#[test]
fn test_collect_release_candidate_files_resolves_multifile_cue_sheet() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path();

    std::fs::write(root.join("01 - Track One.flac"), fake_flac()).unwrap();
    std::fs::write(root.join("02 - Track Two.flac"), fake_flac()).unwrap();
    let cue = r#"CATALOG 0123456789012
PERFORMER "Artist Name"
TITLE "Album Title"
FILE "01 - Track One.flac" WAVE
  TRACK 01 AUDIO
    TITLE "Track One"
    INDEX 01 00:00:00
FILE "02 - Track Two.flac" WAVE
  TRACK 02 AUDIO
    TITLE "Track Two"
    INDEX 01 00:00:00
"#;
    std::fs::write(root.join("Album.cue"), cue).unwrap();

    let files = collect_release_candidate_files_with_scope(
        root,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect("scan should succeed");

    let bound = files.bound_sheets();
    assert_eq!(bound.len(), 1);
    // The sheet leads with its first FILE directive; both referenced files keep
    // the audio role.
    assert_eq!(bound[0].audio.file_name, "01 - Track One.flac");
    assert_eq!(
        files
            .audio()
            .map(|file| file.file_name.as_str())
            .collect::<Vec<_>>(),
        vec!["01 - Track One.flac", "02 - Track Two.flac"],
    );
    assert_eq!(bound[0].sheet.catalog.as_deref(), Some("0123456789012"));
    assert_eq!(bound[0].sheet.tracks.len(), 2);
}

/// Nobody has said which cue is which disc, so each bound sheet takes its
/// position among the folder's bound sheets, in path order. An unbound sheet
/// takes no place in that count — it carves nothing either way.
#[test]
fn bound_sheets_take_their_positions_as_discs_by_default() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path();

    std::fs::write(root.join("alpha.flac"), fake_flac()).unwrap();
    std::fs::write(
        root.join("alpha.cue"),
        make_cue_content_n_tracks("alpha.flac", "Album Title", 2),
    )
    .unwrap();
    std::fs::write(root.join("beta.flac"), fake_flac()).unwrap();
    std::fs::write(
        root.join("beta.cue"),
        make_cue_content_n_tracks("beta.flac", "Album Title", 3),
    )
    .unwrap();
    // Names audio the folder does not hold, so it never binds.
    std::fs::write(
        root.join("zeta.cue"),
        make_cue_content_n_tracks("zeta.flac", "Album Title", 4),
    )
    .unwrap();

    let files = collect_release_candidate_files_with_scope(
        root,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect("scan should succeed");

    assert_eq!(
        files
            .track_sheets()
            .map(|sheet| (sheet.file.relative_path.as_str(), sheet.disc))
            .collect::<Vec<_>>(),
        vec![
            ("alpha.cue", SheetDisc::Disc { number: 1 }),
            ("beta.cue", SheetDisc::Disc { number: 2 }),
            ("zeta.cue", SheetDisc::Disc { number: 1 }),
        ],
    );
}

/// A stored assignment wins over the position the folder would hand out, and it
/// survives being read back off disk by a later scan.
#[test]
fn a_stored_disc_assignment_overrules_the_default_position() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path();

    std::fs::write(root.join("alpha.flac"), fake_flac()).unwrap();
    std::fs::write(
        root.join("alpha.cue"),
        make_cue_content_n_tracks("alpha.flac", "Album Title", 2),
    )
    .unwrap();
    std::fs::write(root.join("beta.flac"), fake_flac()).unwrap();
    std::fs::write(
        root.join("beta.cue"),
        make_cue_content_n_tracks("beta.flac", "Album Title", 3),
    )
    .unwrap();

    let scanned = collect_release_candidate_files_with_scope(
        root,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect("scan should succeed");

    let mut sheet_discs = SheetDiscEdits::default();
    sheet_discs.set("alpha.cue".to_string(), SheetDisc::Disc { number: 2 });
    sheet_discs.set("beta.cue".to_string(), SheetDisc::Ignored);
    let edits = CandidateFileEdits {
        sheet_discs,
        ..Default::default()
    };

    let reopened = collect_release_candidate_files_with_scope(
        root,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::new(HashMap::from([(scanned.content_hash(), edits)])),
    )
    .expect("scan should succeed");

    assert_eq!(
        reopened
            .track_sheets()
            .map(|sheet| (sheet.file.relative_path.as_str(), sheet.disc))
            .collect::<Vec<_>>(),
        vec![
            ("alpha.cue", SheetDisc::Disc { number: 2 }),
            ("beta.cue", SheetDisc::Ignored),
        ],
    );
    // The ignored sheet carves nothing, so only the other one's tracks count —
    // and its own container is loose audio again.
    assert_eq!(reopened.carving_sheets().len(), 1);
    assert_eq!(reopened.track_count(), 2);
}

#[test]
fn test_cue_pair_codec_label_covers_supported_extensions() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let label = |relative: &str| match cue_pair_codec_label(&manifest.join(relative)).unwrap() {
        CueCodecLabel::Supported(label) => label,
        CueCodecLabel::Unsupported(codec) => panic!("expected supported codec, got {codec}"),
        CueCodecLabel::Unprobeable => panic!("expected supported codec, got unprobeable audio"),
    };
    assert_eq!(label("tests/fixtures/flac/01 Test Track 1.flac"), "FLAC");
    assert_eq!(label("tests/fixtures/cue_ape/Test Album.ape"), "APE");
    assert_eq!(label("test-fixtures/alac/cue-alac.m4a"), "ALAC");
}

/// A CUE+APE pair must report the parsed TRACK count from the CUE sheet,
/// not the number of audio files on disk (which for a single-file CUE+APE
/// release is 1).
#[test]
fn test_collect_release_candidate_files_cue_ape_track_count() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path();

    let stem = "Test Artist - Test Album";
    let ape_name = format!("{stem}.ape");
    let cue_name = format!("{stem}.cue");

    std::fs::write(root.join(&ape_name), fake_ape()).unwrap();
    std::fs::write(
        root.join(&cue_name),
        make_cue_content_n_tracks(&ape_name, "Test Album", 15),
    )
    .unwrap();

    let files = collect_release_candidate_files_with_scope(
        root,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect("scan should succeed");

    assert_eq!(files.format_label, "CUE+APE");
    let bound = files.bound_sheets();
    assert_eq!(bound.len(), 1);
    let track_count = bound[0].sheet.tracks.len();
    assert_eq!(
        track_count, 15,
        "CUE with 15 TRACK entries should parse to 15 tracks, got {track_count}",
    );

    assert_eq!(files.track_count(), 15);
}

// ── Folder-scanner shape fixture ────────────────────────────────────────
//
// A declarative taxonomy of the folder shapes the scanner must handle, pinning
// which of them a human would call a release.

// --- Byte stubs ---
