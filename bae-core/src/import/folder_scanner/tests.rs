use super::*;

/// Minimal valid FLAC header (42 bytes) with total_samples=0 (unknown length).
/// This passes is_valid_flac without needing a realistic file size.
fn fake_flac() -> Vec<u8> {
    let mut buf = vec![
        b'f', b'L', b'a', b'C', // magic
        0x00, 0x00, 0x00, 34, // STREAMINFO block: type=0, length=34
    ];
    // STREAMINFO: 34 bytes, all zeros -> sample_rate=0 so size check is skipped
    buf.extend_from_slice(&[0u8; 34]);
    buf
}

/// Every scan item (valid + invalid) for `root`.
fn scan_items(root: impl Into<PathBuf>) -> Vec<ScanItem> {
    let mut items = Vec::new();
    scan_for_candidates_with_callback(root.into(), |item| items.push(item)).unwrap();
    items
}

/// Only the valid `FolderCandidate`s for `root` — the shape most scanner
/// tests assert against (counts, paths, categorized files).
fn scan_valid(root: impl Into<PathBuf>) -> Vec<FolderCandidate> {
    scan_items(root)
        .into_iter()
        .filter_map(|item| match item {
            ScanItem::Valid(c) => Some(c),
            ScanItem::Invalid(_) => None,
        })
        .collect()
}

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
fn test_is_disc_indicator_name_basics() {
    // Bare numeric.
    assert!(is_disc_indicator_name("1"));
    assert!(is_disc_indicator_name("02"));
    assert!(is_disc_indicator_name("003"));
    assert!(!is_disc_indicator_name(""));

    // Space / no-separator forms.
    assert!(is_disc_indicator_name("Disc 1"));
    assert!(is_disc_indicator_name("DISC 1"));
    assert!(is_disc_indicator_name("CD2"));
    assert!(is_disc_indicator_name("disk 03"));
    assert!(is_disc_indicator_name("Part 2"));

    // Side with separator (single alpha char).
    assert!(is_disc_indicator_name("Side A"));
    assert!(is_disc_indicator_name("side b"));
}

#[test]
fn test_is_disc_indicator_name_alt_separators() {
    // -, _, . must all work as separators (N1).
    assert!(is_disc_indicator_name("Disc-1"));
    assert!(is_disc_indicator_name("Disk_2"));
    assert!(is_disc_indicator_name("CD.3"));
    assert!(is_disc_indicator_name("Part-04"));
    assert!(is_disc_indicator_name("Side-A"));
    assert!(is_disc_indicator_name("Side_B"));
    assert!(is_disc_indicator_name("Side.C"));
}

#[test]
fn test_is_disc_indicator_name_rejects_sider_family() {
    // Must require a separator between `Side` and the alpha char so
    // `Sider`, `Sideshow`, etc. do NOT match (N2).
    assert!(!is_disc_indicator_name("Sider"));
    assert!(!is_disc_indicator_name("Sideshow"));
    assert!(!is_disc_indicator_name("SideProject"));
}

#[test]
fn test_is_disc_indicator_name_rejects_descriptive_names() {
    // Album names, year-prefixed folders, etc. must NOT match.
    assert!(!is_disc_indicator_name("1991 - Album A2"));
    assert!(!is_disc_indicator_name("Vol. 01 (catalog)"));
    assert!(!is_disc_indicator_name("Artist - Album"));
    assert!(!is_disc_indicator_name("Disc One"));
    assert!(!is_disc_indicator_name("Side Alpha"));
}

#[test]
fn test_is_disc_indicator_name_accepts_descriptive_suffix() {
    // Prefix + digits terminated by any non-alphanumeric char: match.
    assert!(is_disc_indicator_name("Disc 1 - suffix text"));
    assert!(is_disc_indicator_name("CD1 (suffix)"));
    assert!(is_disc_indicator_name("CD 4 - suffix • more"));
    assert!(is_disc_indicator_name("Part 2: suffix"));
    // Prefix + digits followed by alphanumeric: no match — the digit
    // run is not terminated.
    assert!(!is_disc_indicator_name("Discography"));
    assert!(!is_disc_indicator_name("Disc 1A"));
    assert!(!is_disc_indicator_name("CD1Remaster"));
}

#[test]
fn test_cue_parser_counts_audio_tracks_and_captures_file_reference() {
    let tmp = tempfile::tempdir().unwrap();
    let cue = tmp.path().join("album.cue");
    let content = "FILE \"album.flac\" WAVE\n  TRACK 01 AUDIO\n    INDEX 01 00:00:00\n  TRACK 02 AUDIO\n    INDEX 01 05:00:00\n  TRACK 03 AUDIO\n    INDEX 01 10:00:00\n";
    std::fs::write(&cue, content).unwrap();

    let sheet = CueFlacProcessor::parse_cue_sheet(&cue).unwrap();
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

    let sheet = CueFlacProcessor::parse_cue_sheet(&cue).unwrap();
    assert!(sheet.title.is_none());
    assert!(sheet.performer.is_none());
    assert_eq!(sheet.single_file(), Some("dummy.flac"));
    assert_eq!(sheet.tracks.len(), 1);
}

#[test]
fn test_cue_parser_stops_at_data_track() {
    // Only TRACK NN AUDIO is parsed; MODE1 data tracks terminate the
    // track sequence (they sit at the end of rip CUE files).
    let tmp = tempfile::tempdir().unwrap();
    let cue = tmp.path().join("album.cue");
    let content = "FILE \"album.flac\" WAVE\n  TRACK 01 AUDIO\n    INDEX 01 00:00:00\n  TRACK 02 MODE1/2048\n    INDEX 01 05:00:00\n";
    std::fs::write(&cue, content).unwrap();

    let sheet = CueFlacProcessor::parse_cue_sheet(&cue).unwrap();
    assert_eq!(sheet.tracks.len(), 1);
}

#[test]
fn test_collect_release_candidate_files_skips_hidden_and_bae() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path();

    // Create visible files
    std::fs::write(root.join("track.flac"), fake_flac()).unwrap();
    std::fs::write(root.join("cover.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();

    // Create hidden file that should be ignored
    std::fs::write(root.join(".DS_Store"), b"mac junk").unwrap();

    // Create .bae directory -- entirely ignored by the scanner
    let bae_dir = root.join(".bae");
    std::fs::create_dir(&bae_dir).unwrap();
    std::fs::write(bae_dir.join("cover-mb.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();
    std::fs::write(bae_dir.join("cover-discogs.jpeg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();

    let files = collect_release_candidate_files(root).unwrap();

    let audio_paths: Vec<_> = match &files.audio {
        AudioContent::TrackFiles { tracks, .. } => {
            tracks.iter().map(|f| f.relative_path.as_str()).collect()
        }
        AudioContent::CueFlacPairs { .. } => vec![],
    };
    assert_eq!(audio_paths, vec!["track.flac"]);

    // Only release artwork, not .bae/ files
    let artwork_paths: Vec<_> = files
        .artwork
        .iter()
        .map(|f| f.relative_path.as_str())
        .collect();
    assert_eq!(artwork_paths, vec!["cover.jpg"]);

    assert!(files.documents.is_empty());
}

#[test]
fn content_hash_is_location_independent_and_size_sensitive() {
    let make = |root: &str, second_size: u64| CategorizedFiles {
        audio: AudioContent::TrackFiles {
            tracks: vec![
                ScannedFile::new(
                    PathBuf::from(format!("{root}/01.flac")),
                    "01.flac".to_string(),
                    1000,
                ),
                ScannedFile::new(
                    PathBuf::from(format!("{root}/02.flac")),
                    "02.flac".to_string(),
                    second_size,
                ),
            ],
            format_label: "FLAC".to_string(),
        },
        artwork: vec![],
        documents: vec![],
        unpaired_cue_sheets: vec![],
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
    let file =
        |name: &str, size: u64| ScannedFile::new(PathBuf::from(name), name.to_string(), size);
    let forward = CategorizedFiles {
        audio: AudioContent::TrackFiles {
            tracks: vec![file("01.flac", 1), file("02.flac", 2)],
            format_label: "FLAC".to_string(),
        },
        artwork: vec![file("cover.jpg", 3)],
        documents: vec![file("notes.txt", 4)],
        unpaired_cue_sheets: vec![],
    };
    let shuffled = CategorizedFiles {
        audio: AudioContent::TrackFiles {
            tracks: vec![file("02.flac", 2), file("01.flac", 1)],
            format_label: "FLAC".to_string(),
        },
        artwork: vec![file("cover.jpg", 3)],
        documents: vec![file("notes.txt", 4)],
        unpaired_cue_sheets: vec![],
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
fn test_collection_of_albums_detected_as_separate_candidates() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Artist Collection");
    std::fs::create_dir(&root).unwrap();

    let albums = [
        ("2020 - Album One [CAT001]", "Artist - Album One"),
        ("2021 - Album Two [CAT002]", "Artist - Album Two"),
        ("2022 - Album Three [CAT003]", "Artist - Album Three"),
    ];

    for (folder_name, file_base) in &albums {
        let album_dir = root.join(folder_name);
        std::fs::create_dir(&album_dir).unwrap();

        let flac_name = format!("{}.flac", file_base);
        let cue_name = format!("{}.cue", file_base);

        std::fs::write(album_dir.join(&flac_name), fake_flac()).unwrap();
        std::fs::write(
            album_dir.join(&cue_name),
            make_cue_content(&flac_name, file_base),
        )
        .unwrap();
        std::fs::write(album_dir.join("cover.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();
    }

    let candidates = scan_valid(root);

    // An artist/discography container is NOT a candidate — each album
    // inside it is its own candidate.
    assert_eq!(
        candidates.len(),
        3,
        "each album should be its own candidate",
    );
    for c in &candidates {
        let name = c.path.file_name().and_then(|n| n.to_str()).unwrap();
        assert!(
            albums.iter().any(|(folder, _)| *folder == name),
            "unexpected candidate name {:?}",
            name,
        );
    }
}

#[test]
fn test_multi_disc_release_detected_as_single_candidate() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Multi Disc Album");
    std::fs::create_dir(&root).unwrap();

    let discs = [("CD1", "Artist - Album CD1"), ("CD2", "Artist - Album CD2")];

    for (folder_name, file_base) in &discs {
        let disc_dir = root.join(folder_name);
        std::fs::create_dir(&disc_dir).unwrap();

        let flac_name = format!("{}.flac", file_base);
        let cue_name = format!("{}.cue", file_base);

        std::fs::write(disc_dir.join(&flac_name), fake_flac()).unwrap();
        std::fs::write(
            disc_dir.join(&cue_name),
            make_cue_content(&flac_name, file_base),
        )
        .unwrap();
    }

    let candidates = scan_valid(root);

    // The multi-disc album is the sole candidate. Its CUE/FLAC pairs
    // cover both discs.
    assert_eq!(candidates.len(), 1, "Expected 1 multi-disc candidate");

    match &candidates[0].files.audio {
        AudioContent::CueFlacPairs { pairs, .. } => {
            assert_eq!(
                pairs.len(),
                2,
                "Multi-disc release should have 2 CUE/FLAC pairs"
            );
        }
        AudioContent::TrackFiles { .. } => {
            panic!("Expected CUE/FLAC pairs for multi-disc release");
        }
    }
}

/// Helper to create a multi-disc test structure and verify a single
/// candidate is emitted covering every disc (via `dir_prefix` on the
/// audio entries).
fn assert_multi_disc_detected(folder_names: &[&str]) {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Test Album");
    std::fs::create_dir(&root).unwrap();

    for folder_name in folder_names {
        let disc_dir = root.join(folder_name);
        std::fs::create_dir(&disc_dir).unwrap();
        std::fs::write(disc_dir.join("track.flac"), fake_flac()).unwrap();
    }

    let candidates = scan_valid(root);

    assert_eq!(
        candidates.len(),
        1,
        "Folders {:?} should produce 1 multi-disc candidate",
        folder_names,
    );

    let prefixes: BTreeSet<Option<&str>> = match &candidates[0].files.audio {
        AudioContent::TrackFiles { tracks, .. } => {
            tracks.iter().map(|t| t.dir_prefix.as_deref()).collect()
        }
        AudioContent::CueFlacPairs { pairs, .. } => pairs
            .iter()
            .map(|p| p.audio_file.dir_prefix.as_deref())
            .collect(),
    };
    assert_eq!(
        prefixes.len(),
        folder_names.len(),
        "Folders {:?} should contribute {} distinct dir_prefix values, got {:?}",
        folder_names,
        folder_names.len(),
        prefixes,
    );
}

/// Helper for collection shapes: a folder whose audio-bearing subdirs do
/// NOT match disc-indicator patterns is a navigation container, not a
/// candidate. Each child surfaces as its own top-level candidate.
fn assert_collection_detected(folder_names: &[&str]) {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Collection");
    std::fs::create_dir(&root).unwrap();

    for folder_name in folder_names {
        let album_dir = root.join(folder_name);
        std::fs::create_dir(&album_dir).unwrap();
        std::fs::write(album_dir.join("track.flac"), fake_flac()).unwrap();
    }

    let candidates = scan_valid(root);

    assert_eq!(
        candidates.len(),
        folder_names.len(),
        "Folders {:?} should each be a candidate (got {})",
        folder_names,
        candidates.len(),
    );
    for c in &candidates {
        assert!(
            c.path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| folder_names.contains(&n))
                .unwrap_or(false),
            "unexpected candidate path {:?}",
            c.path,
        );
    }
}

#[test]
fn test_multi_disc_disc_1_disc_2() {
    assert_multi_disc_detected(&["Disc 1", "Disc 2"]);
}

#[test]
fn test_multi_disc_side_a_side_b() {
    assert_multi_disc_detected(&["Side A", "Side B"]);
}

#[test]
fn test_multi_disc_numbered() {
    assert_multi_disc_detected(&["1", "2", "3"]);
}

#[test]
fn test_multi_disc_zero_padded() {
    assert_multi_disc_detected(&["01", "02"]);
}

#[test]
fn test_collection_year_prefixed() {
    assert_collection_detected(&["2020 - Album One", "2021 - Album Two", "2022 - Album Three"]);
}

#[test]
fn test_collection_artist_prefixed() {
    assert_collection_detected(&[
        "Artist - First Album",
        "Artist - Second Album",
        "Artist - Third Album",
    ]);
}

#[test]
fn test_collection_with_catalog_numbers() {
    assert_collection_detected(&[
        "Album One [CAT001]",
        "Album Two [CAT002]",
        "Album Three [CAT003]",
    ]);
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

    // `Vol. NN (...)` names carry descriptive text — they do NOT match
    // the disc-indicator pattern, so the parent is a navigation container
    // and each volume is its own candidate.
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

/// Partial markers anywhere under a release (e.g. `Disc 2/*.flac.part`
/// under a multi-disc album) must stop the candidate from surfacing.
/// The walker-level direct check wouldn't catch this because markers
/// live one level below the leaf directory; the leaf-emission-time deep
/// check does.
#[test]
fn test_partial_markers_in_sub_subfolder_stops_multi_disc_candidate() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("Album - Multi Disc With Marker");
    let disc1 = root.join("Disc 1");
    let disc2 = root.join("Disc 2");
    std::fs::create_dir_all(&disc1).unwrap();
    std::fs::create_dir_all(&disc2).unwrap();
    std::fs::write(disc1.join("01.flac"), fake_flac()).unwrap();
    std::fs::write(disc2.join("01.flac"), fake_flac()).unwrap();
    // Marker one level below the multi-disc leaf.
    std::fs::write(disc2.join("02.flac.part"), b"in progress").unwrap();

    let candidates = scan_valid(root);

    assert_eq!(
        candidates.len(),
        0,
        "markers anywhere under the release must suppress it, got {:?}",
        candidates.iter().map(|c| &c.name).collect::<Vec<_>>(),
    );
}

/// A multi-disc release with one disc containing a zero-byte file is an
/// incomplete release. The whole album is suppressed — the clean sibling
/// disc is NOT surfaced as a standalone candidate, because a disc is a
/// slice of a release, not a release on its own. Symmetric with how
/// partial-marker sidecars are handled (see
/// `multi_disc_with_partial_marker_in_one_disc_suppresses_whole_album`).
#[test]
fn multi_disc_with_partial_disc_suppresses_whole_album() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("Album X - Multi Disc");
    let disc1 = root.join("Disc 1");
    let disc2 = root.join("Disc 2");
    std::fs::create_dir_all(&disc1).unwrap();
    std::fs::create_dir_all(&disc2).unwrap();
    std::fs::write(disc1.join("01.flac"), fake_flac()).unwrap();
    std::fs::write(disc2.join("01.flac"), fake_flac()).unwrap();
    // Zero-byte file marks this disc as in-progress / abandoned.
    std::fs::write(disc2.join("02.flac"), b"").unwrap();

    let candidates = scan_valid(root);

    assert!(
        candidates.is_empty(),
        "incomplete multi-disc release must be suppressed; got {:?}",
        candidates.iter().map(|c| &c.name).collect::<Vec<_>>(),
    );
}

// ── FileTree unit tests ─────────────────────────────────────────────────

#[test]
fn test_file_tree_files_in_dir() {
    let tree = FileTree::new(vec![
        FileEntry {
            path: PathBuf::from("a/1.flac"),
            size: 100,
        },
        FileEntry {
            path: PathBuf::from("a/2.flac"),
            size: 200,
        },
        FileEntry {
            path: PathBuf::from("b/3.flac"),
            size: 300,
        },
        FileEntry {
            path: PathBuf::from("root.txt"),
            size: 10,
        },
    ]);

    let a_files: Vec<_> = tree
        .files_in_dir(Path::new("a"))
        .map(|f| f.path.to_string_lossy().to_string())
        .collect();
    assert_eq!(a_files.len(), 2);

    let root_files: Vec<_> = tree
        .files_in_dir(Path::new(""))
        .map(|f| f.path.to_string_lossy().to_string())
        .collect();
    assert_eq!(root_files.len(), 1);
    assert_eq!(root_files[0], "root.txt");
}

#[test]
fn test_file_tree_immediate_subdirs() {
    let tree = FileTree::new(vec![
        FileEntry {
            path: PathBuf::from("a/1.flac"),
            size: 100,
        },
        FileEntry {
            path: PathBuf::from("b/2.flac"),
            size: 200,
        },
        FileEntry {
            path: PathBuf::from("root.txt"),
            size: 10,
        },
    ]);

    let subdirs = tree.immediate_subdirs(Path::new(""));
    assert_eq!(subdirs.len(), 2);
    assert!(subdirs.contains(&PathBuf::from("a")));
    assert!(subdirs.contains(&PathBuf::from("b")));
}

#[test]
fn test_file_tree_all_files_under() {
    let tree = FileTree::new(vec![
        FileEntry {
            path: PathBuf::from("a/1.flac"),
            size: 100,
        },
        FileEntry {
            path: PathBuf::from("a/sub/2.flac"),
            size: 200,
        },
        FileEntry {
            path: PathBuf::from("b/3.flac"),
            size: 300,
        },
    ]);

    let a_all: Vec<_> = tree.all_files_under(Path::new("a")).collect();
    assert_eq!(a_all.len(), 2);

    let root_all: Vec<_> = tree.all_files_under(Path::new("")).collect();
    assert_eq!(root_all.len(), 3);
}

/// Regression: per-track FLACs + CUE + LOG + artwork subfolder + folder.jpg
/// must be detected as a candidate. This mimics a typical CD-rip folder.
#[test]
fn test_per_track_flacs_with_cue_and_artwork_subfolder() {
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

    // CUE sheet
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

    let candidates = scan_valid(tmp.path().join("Collection"));

    assert_eq!(
        candidates.len(),
        1,
        "Expected 1 candidate, got {}",
        candidates.len()
    );

    let c = &candidates[0];
    assert!(
        c.name.contains("Artist - Album Title"),
        "Candidate name should contain album title, got: {}",
        c.name
    );

    // Should have per-track audio (not CUE+FLAC pairs, since tracks are individual files)
    match &c.files.audio {
        AudioContent::TrackFiles { tracks, .. } => {
            assert_eq!(tracks.len(), 12, "Expected 12 track files");
        }
        AudioContent::CueFlacPairs { .. } => {
            panic!("Expected TrackFiles, got CueFlacPairs");
        }
    }

    // Should have artwork from both root and subfolder
    assert!(
        !c.files.artwork.is_empty(),
        "Expected artwork files, got none"
    );

    // Should have documents (CUE + LOG)
    assert!(
        c.files.documents.len() >= 2,
        "Expected at least 2 documents (CUE + LOG), got {}",
        c.files.documents.len()
    );
}

/// A CUE paired with an `.m4a` file produces a `CUE+ALAC` format label —
/// the label is driven by the codec name (`ContentType::display_name()`),
/// not the raw uppercased extension. Detection is extension-only; the
/// container is not decoded in this test (that requires Step 6 fixtures).
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

    let files = collect_release_candidate_files(root).expect("scan should succeed");

    match &files.audio {
        AudioContent::CueFlacPairs {
            pairs,
            format_label,
        } => {
            assert_eq!(format_label, "CUE+ALAC");
            assert_eq!(pairs.len(), 1);
            assert_eq!(
                pairs[0]
                    .cue_sheet
                    .as_ref()
                    .expect("folder scan parses the CUE")
                    .tracks
                    .len(),
                8
            );
        }
        AudioContent::TrackFiles { .. } => {
            panic!("Expected CueFlacPairs for CUE+ALAC, got TrackFiles");
        }
    }
}

/// Multi-FILE CUEs (one FILE per TRACK) never pair, so their parsed sheet
/// would otherwise be discarded. The release's signals — here the CATALOG
/// (UPC) — must survive on `unpaired_cue_sheets`.
#[test]
fn test_collect_release_candidate_files_retains_unpaired_multifile_cue_sheet() {
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

    let files = collect_release_candidate_files(root).expect("scan should succeed");

    // A multi-FILE CUE doesn't pair → audio is the per-track files.
    assert!(
        matches!(files.audio, AudioContent::TrackFiles { .. }),
        "multi-FILE CUE must not pair; got {:?}",
        files.audio,
    );
    // The parsed sheet — and its CATALOG/UPC — is retained, not discarded.
    assert_eq!(files.unpaired_cue_sheets.len(), 1);
    let (_, sheet) = &files.unpaired_cue_sheets[0];
    assert_eq!(sheet.catalog.as_deref(), Some("0123456789012"));
    assert_eq!(sheet.tracks.len(), 2);
}

#[test]
fn test_cue_pair_codec_label_covers_supported_extensions() {
    // Mirrors the detector's allowed extensions in `cue_flac.rs`.
    assert_eq!(cue_pair_codec_label("flac"), "FLAC");
    assert_eq!(cue_pair_codec_label("FLAC"), "FLAC");
    assert_eq!(cue_pair_codec_label("ape"), "APE");
    assert_eq!(cue_pair_codec_label("APE"), "APE");
    assert_eq!(cue_pair_codec_label("m4a"), "ALAC");
    assert_eq!(cue_pair_codec_label("M4A"), "ALAC");
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

    let files = collect_release_candidate_files(root).expect("scan should succeed");

    match &files.audio {
        AudioContent::CueFlacPairs {
            pairs,
            format_label,
        } => {
            assert_eq!(format_label, "CUE+APE");
            assert_eq!(pairs.len(), 1);
            let track_count = pairs[0]
                .cue_sheet
                .as_ref()
                .expect("folder scan parses the CUE")
                .tracks
                .len();
            assert_eq!(
                track_count, 15,
                "CUE with 15 TRACK entries should parse to 15 tracks, got {track_count}",
            );
        }
        AudioContent::TrackFiles { .. } => {
            panic!("Expected CueFlacPairs for CUE+APE, got TrackFiles");
        }
    }

    assert_eq!(files.audio.track_count(), Some(15));
}

// ── Folder-scanner shape fixture ────────────────────────────────────────
//
// A declarative taxonomy of folder shapes the scanner must handle, used to
// pin the human-intent contract (see `plans/folder-scanner-test.md`).
// Several assertions are expected to fail today — that is the point.
// Do NOT adjust the scanner to make them pass; instead, this test
// defines what "correct" means so the follow-up fix can land cleanly.

// --- Byte stubs ---

/// Minimal valid APE (Monkey's Audio) header — just the "MAC " magic.
fn fake_ape() -> Vec<u8> {
    b"MAC \x00\x00\x00\x00".to_vec()
}

/// Minimal valid MP3 with an ID3v2 header.
fn fake_mp3() -> Vec<u8> {
    b"ID3\x04\x00\x00\x00\x00\x00\x00".to_vec()
}

/// Minimal M4A: an ISO base media `ftyp` box with `M4A ` as the major
/// brand. `is_valid_audio` has no m4a-specific validator (dispatches to
/// the unknown-extension fallback `Ok(true)`), so the bytes only need
/// to be non-empty and have a plausible shape for anything downstream
/// that might sniff them.
fn fake_m4a() -> Vec<u8> {
    let mut buf = Vec::new();
    // Box size: 28 bytes, big-endian u32.
    // Layout: size(4) + type(4) + major(4) + minor(4) + brands(12).
    buf.extend_from_slice(&28u32.to_be_bytes());
    // Box type: `ftyp`.
    buf.extend_from_slice(b"ftyp");
    // Major brand: `M4A ` (space-padded).
    buf.extend_from_slice(b"M4A ");
    // Minor version: 0.
    buf.extend_from_slice(&0u32.to_be_bytes());
    // Compatible brands: `M4A `, `mp42`, `isom`.
    buf.extend_from_slice(b"M4A mp42isom");
    assert_eq!(buf.len(), 28);
    buf
}

/// Minimal valid JPEG (only the SOI + APP0 marker — enough for magic check).
fn fake_jpeg() -> Vec<u8> {
    vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10]
}

/// Minimal valid PNG (just the 8-byte signature).
fn fake_png() -> Vec<u8> {
    vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]
}

/// A plausible AVI file. Never validated by the scanner, but should not be
/// mistaken for audio. RIFF header with an `AVI ` form type.
fn fake_avi() -> Vec<u8> {
    let mut v = b"RIFF".to_vec();
    v.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
    v.extend_from_slice(b"AVI ");
    v
}

/// FLAC that passes the magic/STREAMINFO shape check but declares more
/// samples than the file holds — `is_valid_flac` must reject it via the
/// truncation size check.
///
/// STREAMINFO encodes 44100 Hz / 2ch / 16-bit / 10_000_000 samples. Raw
/// PCM = 40 MB, 10% threshold = 4 MB. We cap the file at 1 KB so
/// `file_size < min_expected` fires.
fn truncated_flac() -> Vec<u8> {
    let mut buf = vec![b'f', b'L', b'a', b'C', 0x00, 0x00, 0x00, 34];

    // STREAMINFO bytes 0..10: block sizes + frame sizes (zero is fine).
    buf.extend_from_slice(&[0u8; 10]);

    // Sample rate (20 bits) | channels-1 (3 bits) | bps-1 (5 bits) | total_samples high (4 bits).
    let sample_rate = 44100u32;
    let ch_minus_1 = 2u32 - 1;
    let bps_minus_1 = 16u32 - 1;
    let total_samples: u64 = 10_000_000;
    let ts_high = ((total_samples >> 32) & 0x0F) as u32;
    buf.push((sample_rate >> 12) as u8);
    buf.push(((sample_rate >> 4) & 0xFF) as u8);
    buf.push(
        (((sample_rate & 0x0F) as u8) << 4)
            | ((ch_minus_1 as u8) << 1)
            | ((bps_minus_1 >> 4) as u8),
    );
    buf.push(((bps_minus_1 & 0x0F) as u8) << 4 | ts_high as u8);

    // total_samples low 32 bits.
    let ts_low = (total_samples & 0xFFFF_FFFF) as u32;
    buf.push((ts_low >> 24) as u8);
    buf.push(((ts_low >> 16) & 0xFF) as u8);
    buf.push(((ts_low >> 8) & 0xFF) as u8);
    buf.push((ts_low & 0xFF) as u8);

    // MD5 (16 bytes). Header ends at offset 42.
    buf.extend_from_slice(&[0u8; 16]);
    assert_eq!(buf.len(), 42);

    // Pad to 1 KB — well below the 4 MB threshold.
    buf.resize(1024, 0xAA);
    buf
}

/// FLAC with wrong magic bytes. `is_valid_flac` rejects on the magic
/// check before even looking at STREAMINFO.
fn broken_flac() -> Vec<u8> {
    // Valid size, but the leading four bytes are not `fLaC`.
    let mut buf = b"BROK".to_vec();
    buf.resize(64, 0u8);
    buf
}

/// CUE sheet content referencing `audio_filename` with `n_tracks` entries.
/// Each track is spaced 5 minutes apart so the sheet parses cleanly.
fn make_cue_content_n_tracks(audio_filename: &str, title: &str, n_tracks: usize) -> String {
    let mut s =
        format!("PERFORMER \"Test Artist\"\nTITLE \"{title}\"\nFILE \"{audio_filename}\" WAVE\n");
    for i in 1..=n_tracks {
        let minute = (i - 1) * 5;
        s.push_str(&format!(
            "  TRACK {:02} AUDIO\n    TITLE \"Track {i:02}\"\n    INDEX 01 {:02}:00:00\n",
            i, minute,
        ));
    }
    s
}

/// Like `make_cue_content_n_tracks` but emits an unquoted FILE directive
/// (`FILE name.wav WAVE`). Exercises the unquoted branch of the
/// CUE parser's FILE directive.
fn make_cue_content_unquoted(audio_filename: &str, title: &str, n_tracks: usize) -> String {
    let mut s =
        format!("PERFORMER \"Test Artist\"\nTITLE \"{title}\"\nFILE {audio_filename} WAVE\n");
    for i in 1..=n_tracks {
        let minute = (i - 1) * 5;
        s.push_str(&format!(
            "  TRACK {:02} AUDIO\n    TITLE \"Track {i:02}\"\n    INDEX 01 {:02}:00:00\n",
            i, minute,
        ));
    }
    s
}

/// CUE sheet without PERFORMER / TITLE. The unified CUE parser is lenient
/// enough to accept it — both fields land as `None`.
fn make_cue_content_no_header(audio_filename: &str, n_tracks: usize) -> String {
    let mut s = format!("FILE \"{audio_filename}\" WAVE\n");
    for i in 1..=n_tracks {
        let minute = (i - 1) * 5;
        s.push_str(&format!(
            "  TRACK {:02} AUDIO\n    INDEX 01 {:02}:00:00\n",
            i, minute,
        ));
    }
    s
}

// --- Spec types ---

/// Top-level spec entry. Either writes a file at `rel_path` (all folder
/// creation is implicit via `create_dir_all` on the parent), or pins the
/// scanner's top-level-candidate behavior on a folder.
///
/// Folders that aren't explicitly pinned still exist on disk via the
/// parent-creation path — they just don't participate in the candidate
/// set-equality check. That's the right default for the dozens of
/// container folders the fixture creates implicitly.
#[derive(Debug)]
enum FixtureEntry {
    File {
        rel_path: String,
        kind: FileKind,
    },
    Expect {
        rel_path: String,
        top_level_candidate: bool,
    },
}

/// Every file the fixture writes. One variant per distinct byte pattern
/// or semantic role. The walker matches on this to pick the right bytes;
/// the invariant pass matches on this to pick the right validator.
///
/// Audio extensions must stay in lock-step with `ContentTypeHint::is_audio`
/// — if the walker emits `.flac`/`.mp3`/`.ape`/`.m4a`, the scanner must
/// recognize those.
#[derive(Debug, Clone, Copy)]
enum FileKind {
    // Audio formats recognised by the scanner.
    Flac,
    Mp3,
    Ape,
    M4a,
    /// Empty FLAC file (size 0). The scanner must reject the candidate.
    ZeroByteFlac,
    /// Valid `fLaC` magic + STREAMINFO that declares far more samples
    /// than the file actually contains. `file_validation::is_valid_flac`
    /// must reject it.
    TruncatedFlac,
    /// Wrong magic bytes where a FLAC is expected. `is_valid_flac` must
    /// reject it.
    BrokenFlac,
    // Image formats.
    Jpeg,
    Png,
    /// Empty `.jpg` file (size 0). The scanner's image validator must
    /// reject it, which in practice short-circuits the categorize pass
    /// and drops the enclosing candidate.
    ZeroByteJpeg,
    /// File whose extension is an arbitrary string the scanner does not
    /// recognize (e.g. `"xyz"`, `"sh"`). Used to pin that unknown file
    /// types are silently ignored rather than mis-categorized.
    UnrecognizedFile(&'static str),
    // Non-music video. Scanner must not treat it as audio.
    Avi,
    // Document sidecars — fall into `files.documents`.
    Log,
    M3u,
    Md5,
    Ffp,
    TracklistTxt,
    /// A CUE sheet intended to pair with an audio file sharing its own
    /// path stem in the same directory. `stem` is written into the CUE's
    /// FILE directive as `FILE "<stem>" WAVE`; the scanner's pair
    /// detection keys on path stems, not on the FILE directive content,
    /// so `stem` is only used for CUE content validity.
    CueFor {
        stem: &'static str,
        n_tracks: usize,
    },
    /// Like `CueFor`, but emits an unquoted FILE directive —
    /// `FILE <stem> WAVE` rather than `FILE "<stem>" WAVE`. Exercises
    /// the unquoted branch of the CUE parser's FILE directive.
    CueUnquoted {
        stem: &'static str,
        n_tracks: usize,
    },
    /// A CUE sheet whose path stem deliberately does not match any audio
    /// in the same directory. `file_reference` goes into the FILE
    /// directive; when it names something not on disk and `n_tracks`
    /// exceeds the direct-child audio count, the mismatch guard rejects
    /// the candidate.
    NonPairingCue {
        n_tracks: usize,
        file_reference: &'static str,
    },
    /// A CUE sheet lacking the PERFORMER/TITLE preamble. The unified
    /// CUE parser accepts it (both fields land as `None`) and still
    /// surfaces the file reference + track count the incomplete-rip
    /// guard depends on.
    CueNoHeader {
        n_tracks: usize,
        file_reference: &'static str,
    },
    /// Partial-download marker. The argument is the trailing extension
    /// (e.g. `"part"`, `"crdownload"`, `"aria2"`) — purely self-documenting,
    /// the walker does not inspect it. The full file name lives in the
    /// entry's `rel_path`, so different rippers' conventions (`01.flac.part`,
    /// `01.flac.crdownload`, `01.flac.aria2`) are all expressible.
    PartialMarker(&'static str),
    // Root-level non-music junk.
    Pdf,
    Zip,
    Dmg,
}

// --- Byte writers & validators keyed purely on FileKind ---

/// Bytes to write for each kind. The walker uses this directly.
fn bytes_for(kind: FileKind) -> Vec<u8> {
    match kind {
        FileKind::Flac => fake_flac(),
        FileKind::Mp3 => fake_mp3(),
        FileKind::Ape => fake_ape(),
        FileKind::M4a => fake_m4a(),
        FileKind::ZeroByteFlac => Vec::new(),
        FileKind::TruncatedFlac => truncated_flac(),
        FileKind::BrokenFlac => broken_flac(),
        FileKind::Jpeg => fake_jpeg(),
        FileKind::Png => fake_png(),
        FileKind::ZeroByteJpeg => Vec::new(),
        FileKind::UnrecognizedFile(_) => b"opaque contents".to_vec(),
        FileKind::Avi => fake_avi(),
        FileKind::Log => b"EAC log\n".to_vec(),
        FileKind::M3u => b"01.flac\n02.flac\n".to_vec(),
        FileKind::Md5 => b"abc  01.flac\n".to_vec(),
        FileKind::Ffp => b"01.flac:abc\n".to_vec(),
        FileKind::TracklistTxt => b"01. Track One\n02. Track Two\n".to_vec(),
        FileKind::CueFor { stem, n_tracks } => {
            make_cue_content_n_tracks(stem, "Album", n_tracks).into_bytes()
        }
        FileKind::CueUnquoted { stem, n_tracks } => {
            make_cue_content_unquoted(stem, "Album", n_tracks).into_bytes()
        }
        FileKind::NonPairingCue {
            n_tracks,
            file_reference,
        } => make_cue_content_n_tracks(file_reference, "Album", n_tracks).into_bytes(),
        FileKind::CueNoHeader {
            n_tracks,
            file_reference,
        } => make_cue_content_no_header(file_reference, n_tracks).into_bytes(),
        FileKind::PartialMarker(_) => b"partial data".to_vec(),
        FileKind::Pdf => b"%PDF-1.4\n".to_vec(),
        FileKind::Zip => b"PK\x03\x04".to_vec(),
        FileKind::Dmg => b"koly".to_vec(),
    }
}

/// Fixture-builder invariant: validate the written bytes match the kind.
/// Failure here is always a fixture-builder bug, never a scanner bug.
fn assert_kind_invariant(path: &Path, kind: FileKind) {
    assert!(
        path.exists(),
        "fixture builder bug, not scanner bug: file missing at {:?}",
        path,
    );
    match kind {
        FileKind::Flac => {
            assert!(
                file_validation::is_valid_flac(path).unwrap_or(false),
                "fixture builder bug: FLAC at {:?} fails validator",
                path,
            );
        }
        FileKind::Mp3 => {
            assert!(
                file_validation::is_valid_mp3(path).unwrap_or(false),
                "fixture builder bug: MP3 at {:?} fails validator",
                path,
            );
        }
        FileKind::Ape => {
            assert!(
                file_validation::is_valid_ape(path).unwrap_or(false),
                "fixture builder bug: APE at {:?} fails validator",
                path,
            );
        }
        FileKind::M4a => {
            // is_valid_audio dispatches by extension and falls through to
            // Ok(true) for m4a, so "validation" is really just "file
            // exists, non-empty, extension is .m4a". Pin those.
            let size = std::fs::metadata(path).unwrap().len();
            assert!(
                size > 0,
                "fixture builder bug: M4A at {:?} must be non-empty",
                path,
            );
            assert_eq!(
                path.extension()
                    .and_then(|e| e.to_str())
                    .map(str::to_lowercase),
                Some("m4a".to_string()),
                "fixture builder bug: M4A at {:?} must have .m4a extension",
                path,
            );
        }
        FileKind::ZeroByteFlac => {
            let size = std::fs::metadata(path).unwrap().len();
            assert_eq!(
                size, 0,
                "fixture builder bug: {:?} should be zero-byte, is {}",
                path, size,
            );
        }
        FileKind::TruncatedFlac | FileKind::BrokenFlac => {
            // is_valid_flac must reject both — the test matrix depends on
            // these kinds being seen as invalid audio.
            assert!(
                !file_validation::is_valid_flac(path).unwrap_or(true),
                "fixture builder bug: {:?} at {:?} unexpectedly passes is_valid_flac",
                kind,
                path,
            );
        }
        FileKind::Jpeg | FileKind::Png => {
            assert!(
                file_validation::is_valid_image(path).unwrap_or(false),
                "fixture builder bug: image at {:?} fails validator",
                path,
            );
        }
        FileKind::ZeroByteJpeg => {
            let size = std::fs::metadata(path).unwrap().len();
            assert_eq!(
                size, 0,
                "fixture builder bug: {:?} should be zero-byte, is {}",
                path, size,
            );
            assert_eq!(
                path.extension()
                    .and_then(|e| e.to_str())
                    .map(str::to_lowercase),
                Some("jpg".to_string()),
                "fixture builder bug: ZeroByteJpeg at {:?} must have .jpg extension",
                path,
            );
        }
        FileKind::UnrecognizedFile(ext) => {
            // No byte-level validation — the scanner only cares that the
            // extension is unrecognized.
            let got = path
                .extension()
                .and_then(|e| e.to_str())
                .map(str::to_lowercase);
            assert_eq!(
                got,
                Some(ext.to_lowercase()),
                "fixture builder bug: UnrecognizedFile({:?}) at {:?} extension mismatch (got {:?})",
                ext,
                path,
                got,
            );
        }
        FileKind::CueFor { n_tracks, .. }
        | FileKind::CueUnquoted { n_tracks, .. }
        | FileKind::NonPairingCue { n_tracks, .. } => {
            let sheet = CueFlacProcessor::parse_cue_sheet(path).unwrap_or_else(|e| {
                panic!(
                    "fixture builder bug: CUE at {:?} fails parse: {:?}",
                    path, e,
                )
            });
            assert_eq!(
                sheet.tracks.len(),
                n_tracks,
                "fixture builder bug: CUE at {:?} declares {} tracks, expected {}",
                path,
                sheet.tracks.len(),
                n_tracks,
            );
        }
        FileKind::CueNoHeader {
            n_tracks,
            file_reference,
        } => {
            let sheet = CueFlacProcessor::parse_cue_sheet(path).unwrap_or_else(|e| {
                panic!(
                    "fixture builder bug: headerless CUE at {:?} fails parse: {:?}",
                    path, e,
                )
            });
            assert!(
                sheet.title.is_none() && sheet.performer.is_none(),
                "fixture builder bug: headerless CUE at {:?} unexpectedly has title/performer",
                path,
            );
            assert_eq!(
                sheet.tracks.len(),
                n_tracks,
                "fixture builder bug: headerless CUE at {:?} counts {} tracks, expected {}",
                path,
                sheet.tracks.len(),
                n_tracks,
            );
            assert_eq!(
                sheet.single_file(),
                Some(file_reference as &str),
                "fixture builder bug: headerless CUE at {:?} single_file mismatch",
                path,
            );
        }
        FileKind::PartialMarker(ext) => {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            assert!(
                name.ends_with(&format!(".{ext}")),
                "fixture builder bug: partial marker at {:?} does not end with .{}",
                path,
                ext,
            );
        }
        FileKind::Avi
        | FileKind::Log
        | FileKind::M3u
        | FileKind::Md5
        | FileKind::Ffp
        | FileKind::TracklistTxt
        | FileKind::Pdf
        | FileKind::Zip
        | FileKind::Dmg => {
            // Presence-only kinds: the scanner does not validate their bytes.
        }
    }
}

// --- Sugar: per-track audio ---

/// Produce `n` `File` entries at `{dir}/{i:02}.<ext>`, one per track,
/// with the given audio `kind`. The extension is derived from the kind
/// (Flac / ZeroByteFlac → `flac`, Mp3 → `mp3`, Ape → `ape`). Panics on
/// non-audio kinds — the helper is named for the "per-track audio
/// release" shape and refuses to be repurposed.
fn flat_audio(dir: &str, n: usize, kind: FileKind) -> Vec<FixtureEntry> {
    let ext = match kind {
        FileKind::Flac | FileKind::ZeroByteFlac => "flac",
        FileKind::Mp3 => "mp3",
        FileKind::Ape => "ape",
        FileKind::M4a => "m4a",
        other => panic!(
            "flat_audio: unsupported kind {:?} — this helper only produces audio tracks",
            other,
        ),
    };
    (1..=n)
        .map(|i| FixtureEntry::File {
            rel_path: format!("{dir}/{i:02}.{ext}"),
            kind,
        })
        .collect()
}

// --- Walker: pure dispatch over the spec ---

/// Build a fixture on disk at `root` from a spec. Parent directories for
/// any file path are created implicitly — so container folders don't need
/// their own entries. `Expect` entries create the folder they reference so
/// assertions can run against empty-but-expected containers.
fn build_fixture(root: &Path, spec: &[FixtureEntry]) {
    for entry in spec {
        match entry {
            FixtureEntry::File { rel_path, kind } => {
                let path = root.join(rel_path);
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent).unwrap();
                }
                std::fs::write(&path, bytes_for(*kind)).unwrap();
            }
            FixtureEntry::Expect { rel_path, .. } => {
                let dir = root.join(rel_path);
                std::fs::create_dir_all(&dir).unwrap();
            }
        }
    }
}

// --- Invariant pass: confirm each File entry exists with expected bytes.

fn assert_fixture_invariants(root: &Path, spec: &[FixtureEntry]) {
    for entry in spec {
        match entry {
            FixtureEntry::File { rel_path, kind } => {
                assert_kind_invariant(&root.join(rel_path), *kind);
            }
            FixtureEntry::Expect { rel_path, .. } => {
                let dir = root.join(rel_path);
                assert!(
                    dir.is_dir(),
                    "fixture builder bug, not scanner bug: expected folder {:?} missing",
                    dir,
                );
            }
        }
    }
}

// --- Per-primitive unit tests ---
//
// One test per FileKind variant: build a tempdir, write one file via the
// walker's `bytes_for` primitive, and confirm validation sees what the
// walker meant to emit. These catch walker drift before the integration
// test has a chance to fingerpoint at the scanner.

fn write_one(dir: &Path, name: &str, kind: FileKind) -> std::path::PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, bytes_for(kind)).unwrap();
    assert_kind_invariant(&path, kind);
    path
}

#[test]
fn test_write_flac() {
    let tmp = tempfile::tempdir().unwrap();
    write_one(tmp.path(), "01.flac", FileKind::Flac);
}

#[test]
fn test_write_mp3() {
    let tmp = tempfile::tempdir().unwrap();
    write_one(tmp.path(), "01.mp3", FileKind::Mp3);
}

#[test]
fn test_write_ape() {
    let tmp = tempfile::tempdir().unwrap();
    write_one(tmp.path(), "Album.ape", FileKind::Ape);
}

#[test]
fn test_write_m4a() {
    let tmp = tempfile::tempdir().unwrap();
    let path = write_one(tmp.path(), "01.m4a", FileKind::M4a);
    // is_audio_file recognises the extension; is_valid_audio falls through
    // to the unknown-extension fallback, so non-zero bytes pass.
    assert!(is_audio_file(&path));
    assert!(file_validation::is_valid_audio(&path).unwrap());
}

#[test]
fn test_write_zero_byte_flac() {
    let tmp = tempfile::tempdir().unwrap();
    let path = write_one(tmp.path(), "01.flac", FileKind::ZeroByteFlac);
    assert_eq!(std::fs::metadata(&path).unwrap().len(), 0);
    assert!(!file_validation::is_valid_flac(&path).unwrap());
}

#[test]
fn test_write_truncated_flac() {
    let tmp = tempfile::tempdir().unwrap();
    let path = write_one(tmp.path(), "01.flac", FileKind::TruncatedFlac);
    // Magic is valid but the size-vs-samples check fires.
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(&bytes[..4], b"fLaC");
    assert!(!file_validation::is_valid_flac(&path).unwrap());
}

#[test]
fn test_write_broken_flac() {
    let tmp = tempfile::tempdir().unwrap();
    let path = write_one(tmp.path(), "01.flac", FileKind::BrokenFlac);
    let bytes = std::fs::read(&path).unwrap();
    assert_ne!(&bytes[..4], b"fLaC");
    assert!(!file_validation::is_valid_flac(&path).unwrap());
}

#[test]
fn test_write_jpeg() {
    let tmp = tempfile::tempdir().unwrap();
    write_one(tmp.path(), "cover.jpg", FileKind::Jpeg);
}

#[test]
fn test_write_png() {
    let tmp = tempfile::tempdir().unwrap();
    write_one(tmp.path(), "scan.png", FileKind::Png);
}

#[test]
fn test_write_avi() {
    let tmp = tempfile::tempdir().unwrap();
    let path = write_one(tmp.path(), "S01E01.avi", FileKind::Avi);
    // The scanner must not mistake AVI for audio.
    assert!(!is_audio_file(&path));
}

#[test]
fn test_write_document_kinds() {
    let tmp = tempfile::tempdir().unwrap();
    write_one(tmp.path(), "rip.log", FileKind::Log);
    write_one(tmp.path(), "playlist.m3u", FileKind::M3u);
    write_one(tmp.path(), "checksums.md5", FileKind::Md5);
    write_one(tmp.path(), "checksums.ffp", FileKind::Ffp);
    write_one(tmp.path(), "Tracklist.txt", FileKind::TracklistTxt);
}

#[test]
fn test_write_cue_for() {
    let tmp = tempfile::tempdir().unwrap();
    let path = write_one(
        tmp.path(),
        "Album.cue",
        FileKind::CueFor {
            stem: "Album.flac",
            n_tracks: 12,
        },
    );
    let sheet = CueFlacProcessor::parse_cue_sheet(&path).unwrap();
    assert_eq!(sheet.tracks.len(), 12);
}

#[test]
fn test_write_non_pairing_cue() {
    let tmp = tempfile::tempdir().unwrap();
    // Emit a FLAC with a different stem so the CUE cannot pair.
    write_one(tmp.path(), "01.flac", FileKind::Flac);
    let cue_path = write_one(
        tmp.path(),
        "Album.cue",
        FileKind::NonPairingCue {
            n_tracks: 5,
            file_reference: "Album.flac",
        },
    );
    let paths = vec![tmp.path().join("01.flac"), cue_path];
    let pairs = CueFlacProcessor::detect_cue_flac_from_paths(&paths).unwrap();
    assert!(pairs.is_empty(), "CUE stem must not match any FLAC");
}

#[test]
fn test_write_cue_unquoted() {
    let tmp = tempfile::tempdir().unwrap();
    let path = write_one(
        tmp.path(),
        "Album.cue",
        FileKind::CueUnquoted {
            stem: "Album.flac",
            n_tracks: 7,
        },
    );
    // Sheet content uses the unquoted form.
    let raw = std::fs::read_to_string(&path).unwrap();
    assert!(
        raw.contains("FILE Album.flac WAVE"),
        "unquoted FILE directive missing: {raw}",
    );
    // The unified parser extracts the reference from the unquoted form.
    let sheet = CueFlacProcessor::parse_cue_sheet(&path).unwrap();
    assert_eq!(sheet.single_file(), Some("Album.flac"));
    assert_eq!(sheet.tracks.len(), 7);
}

#[test]
fn test_write_cue_no_header() {
    let tmp = tempfile::tempdir().unwrap();
    let path = write_one(
        tmp.path(),
        "Album.cue",
        FileKind::CueNoHeader {
            n_tracks: 4,
            file_reference: "Album.flac",
        },
    );
    // The unified parser accepts missing PERFORMER/TITLE; both land as None.
    let sheet = CueFlacProcessor::parse_cue_sheet(&path).unwrap();
    assert!(sheet.title.is_none());
    assert!(sheet.performer.is_none());
    assert_eq!(sheet.tracks.len(), 4);
    assert_eq!(sheet.single_file(), Some("Album.flac"));
}

#[test]
fn test_write_partial_marker() {
    let tmp = tempfile::tempdir().unwrap();
    for (name, ext) in [
        ("01.flac.part", "part"),
        ("02.flac.crdownload", "crdownload"),
        ("03.flac.aria2", "aria2"),
    ] {
        let path = write_one(tmp.path(), name, FileKind::PartialMarker(ext));
        // Markers must NOT register as audio.
        assert!(!is_audio_file(&path));
    }
}

#[test]
fn test_write_junk_kinds() {
    let tmp = tempfile::tempdir().unwrap();
    write_one(tmp.path(), "loose.pdf", FileKind::Pdf);
    write_one(tmp.path(), "loose.zip", FileKind::Zip);
    write_one(tmp.path(), "loose.dmg", FileKind::Dmg);
}

// --- Integration smoke test ---
//
// Builds the full reference tree from `notes/folder-scanner-cases.md` and
// asserts what `scan_for_candidates_with_callback` produces against the
// 13-candidate human-intent set. The fixture exercises every case in the
// doc (release shapes, container shapes, completeness/mismatch signals).
// Any regression — bogus parents, sibling taint, partial-marker leaks,
// CUE-mismatch surfacing — produces a set-diff failure with the exact
// offending paths.

/// The full reference fixture spec, in minimal vocabulary. Each release
/// is a sequence of `File` entries plus one `Expect`. Navigation folders
/// (artist, discography, reissue containers) get an `Expect { top_level_candidate:
/// false }` so the scanner is pinned against surfacing them. Folders with
/// no pinning exist implicitly via `create_dir_all` on child paths.
fn reference_fixture() -> Vec<FixtureEntry> {
    const ARTIST_A: &str = "Artist A - Discography [FLAC]";
    const STUDIO: &str = "Artist A - Discography [FLAC]/Studio \u{0410}lbums";
    const A1: &str = "Artist A - Discography [FLAC]/Studio \u{0410}lbums/1990 - Album A1";
    const A1_ORIG: &str =
            "Artist A - Discography [FLAC]/Studio \u{0410}lbums/1990 - Album A1/1990 - Album A1 [Original Release]";
    const A2: &str = "Artist A - Discography [FLAC]/Studio \u{0410}lbums/1991 - Album A2";
    const A2_ORIG: &str =
            "Artist A - Discography [FLAC]/Studio \u{0410}lbums/1991 - Album A2/1991 - Album A2 [Original Release]";
    const A2_JAPAN: &str =
            "Artist A - Discography [FLAC]/Studio \u{0410}lbums/1991 - Album A2/1997 - Album A2 [Japan Reissue]";
    const A2_REISSUE: &str =
            "Artist A - Discography [FLAC]/Studio \u{0410}lbums/1991 - Album A2/2003 - Album A2 [Reissue]";
    const A3: &str = "Artist A - Discography [FLAC]/Studio \u{0410}lbums/1993 - Album A3";
    const A3_ORIG: &str =
            "Artist A - Discography [FLAC]/Studio \u{0410}lbums/1993 - Album A3/1993 - Album A3 [Original Release]";
    const A3_ABANDONED: &str =
            "Artist A - Discography [FLAC]/Studio \u{0410}lbums/1993 - Album A3/2006 - Album A3 [Abandoned Download]";

    let mut entries: Vec<FixtureEntry> = Vec::new();

    // --- Root-level loose junk (B9). ---
    entries.extend([
        FixtureEntry::File {
            rel_path: "loose.pdf".into(),
            kind: FileKind::Pdf,
        },
        FixtureEntry::File {
            rel_path: "loose.zip".into(),
            kind: FileKind::Zip,
        },
        FixtureEntry::File {
            rel_path: "loose.dmg".into(),
            kind: FileKind::Dmg,
        },
        FixtureEntry::File {
            rel_path: "loose.jpg".into(),
            kind: FileKind::Jpeg,
        },
    ]);

    // --- Artist A discography wrappers (B1, B2, B3, B4 + C11). ---
    // The wrappers must not surface as candidates. They exist on disk
    // because children live under them — we just pin the behavior.
    entries.push(FixtureEntry::Expect {
        rel_path: ARTIST_A.into(),
        top_level_candidate: false,
    });
    entries.push(FixtureEntry::Expect {
        rel_path: STUDIO.into(),
        top_level_candidate: false,
    });
    entries.push(FixtureEntry::Expect {
        rel_path: A1.into(),
        top_level_candidate: false,
    });
    entries.push(FixtureEntry::Expect {
        rel_path: A2.into(),
        top_level_candidate: false,
    });
    entries.push(FixtureEntry::Expect {
        rel_path: A3.into(),
        top_level_candidate: false,
    });

    // --- CANDIDATE 1 — flat FLAC + log + cover (C1, C9). ---
    entries.push(FixtureEntry::Expect {
        rel_path: A1_ORIG.into(),
        top_level_candidate: true,
    });
    entries.extend(flat_audio(A1_ORIG, 3, FileKind::Flac));
    entries.push(FixtureEntry::File {
        rel_path: format!("{A1_ORIG}/cover.jpg"),
        kind: FileKind::Jpeg,
    });
    entries.push(FixtureEntry::File {
        rel_path: format!("{A1_ORIG}/rip.log"),
        kind: FileKind::Log,
    });

    // --- CANDIDATE 2 — CUE+FLAC pair (C2). ---
    entries.push(FixtureEntry::Expect {
        rel_path: A2_ORIG.into(),
        top_level_candidate: true,
    });
    entries.push(FixtureEntry::File {
        rel_path: format!("{A2_ORIG}/Album.flac"),
        kind: FileKind::Flac,
    });
    entries.push(FixtureEntry::File {
        rel_path: format!("{A2_ORIG}/Album.cue"),
        kind: FileKind::CueFor {
            stem: "Album.flac",
            n_tracks: 8,
        },
    });
    entries.push(FixtureEntry::File {
        rel_path: format!("{A2_ORIG}/rip.log"),
        kind: FileKind::Log,
    });

    // --- CANDIDATE 3 — CUE+APE pair with Info/Tracklist.txt (C3, C7). ---
    entries.push(FixtureEntry::Expect {
        rel_path: A2_JAPAN.into(),
        top_level_candidate: true,
    });
    entries.push(FixtureEntry::File {
        rel_path: format!("{A2_JAPAN}/Album.ape"),
        kind: FileKind::Ape,
    });
    entries.push(FixtureEntry::File {
        rel_path: format!("{A2_JAPAN}/Album.cue"),
        kind: FileKind::CueFor {
            stem: "Album.ape",
            n_tracks: 11,
        },
    });
    entries.push(FixtureEntry::File {
        rel_path: format!("{A2_JAPAN}/rip.log"),
        kind: FileKind::Log,
    });
    entries.push(FixtureEntry::File {
        rel_path: format!("{A2_JAPAN}/cover.jpg"),
        kind: FileKind::Jpeg,
    });
    entries.push(FixtureEntry::File {
        rel_path: format!("{A2_JAPAN}/Info/Tracklist.txt"),
        kind: FileKind::TracklistTxt,
    });

    // --- CANDIDATE 4 — flat FLAC + booklet/ subfolder (C1, C6). ---
    entries.push(FixtureEntry::Expect {
        rel_path: A2_REISSUE.into(),
        top_level_candidate: true,
    });
    entries.extend(flat_audio(A2_REISSUE, 4, FileKind::Flac));
    entries.push(FixtureEntry::File {
        rel_path: format!("{A2_REISSUE}/rip.log"),
        kind: FileKind::Log,
    });
    entries.push(FixtureEntry::File {
        rel_path: format!("{A2_REISSUE}/booklet/page1.png"),
        kind: FileKind::Png,
    });
    entries.push(FixtureEntry::File {
        rel_path: format!("{A2_REISSUE}/booklet/page2.png"),
        kind: FileKind::Png,
    });

    // --- CANDIDATE 5 — complete, today lost to sibling taint. ---
    entries.push(FixtureEntry::Expect {
        rel_path: A3_ORIG.into(),
        top_level_candidate: true,
    });
    entries.extend(flat_audio(A3_ORIG, 3, FileKind::Flac));
    entries.push(FixtureEntry::File {
        rel_path: format!("{A3_ORIG}/rip.log"),
        kind: FileKind::Log,
    });
    entries.push(FixtureEntry::File {
        rel_path: format!("{A3_ORIG}/cover.jpg"),
        kind: FileKind::Jpeg,
    });

    // --- Abandoned download sibling — 1 real + 2 zero-byte (A1 + A5). ---
    // A real track makes the subdir "look like audio" at the container
    // level; the zero-byte tracks poison the container's categorize pass.
    entries.push(FixtureEntry::Expect {
        rel_path: A3_ABANDONED.into(),
        top_level_candidate: false,
    });
    entries.push(FixtureEntry::File {
        rel_path: format!("{A3_ABANDONED}/01.flac"),
        kind: FileKind::Flac,
    });
    entries.push(FixtureEntry::File {
        rel_path: format!("{A3_ABANDONED}/02.flac"),
        kind: FileKind::ZeroByteFlac,
    });
    entries.push(FixtureEntry::File {
        rel_path: format!("{A3_ABANDONED}/03.flac"),
        kind: FileKind::ZeroByteFlac,
    });

    // --- Artist B (B5) — navigation container. ---
    entries.push(FixtureEntry::Expect {
        rel_path: "Artist B".into(),
        top_level_candidate: false,
    });

    // --- CANDIDATE 6 — flat FLAC + cover + log + non-pairing CUE (C4). ---
    entries.push(FixtureEntry::Expect {
        rel_path: "Artist B/1986 - Album B1".into(),
        top_level_candidate: true,
    });
    entries.extend(flat_audio("Artist B/1986 - Album B1", 5, FileKind::Flac));
    entries.push(FixtureEntry::File {
        rel_path: "Artist B/1986 - Album B1/cover.jpg".into(),
        kind: FileKind::Jpeg,
    });
    entries.push(FixtureEntry::File {
        rel_path: "Artist B/1986 - Album B1/rip.log".into(),
        kind: FileKind::Log,
    });
    entries.push(FixtureEntry::File {
        rel_path: "Artist B/1986 - Album B1/Album.cue".into(),
        kind: FileKind::NonPairingCue {
            n_tracks: 5,
            file_reference: "Album.flac",
        },
    });

    // --- CANDIDATE 7 — plain. ---
    entries.push(FixtureEntry::Expect {
        rel_path: "Artist B/1989 - Album B2".into(),
        top_level_candidate: true,
    });
    entries.extend(flat_audio("Artist B/1989 - Album B2", 6, FileKind::Flac));
    entries.push(FixtureEntry::File {
        rel_path: "Artist B/1989 - Album B2/cover.jpg".into(),
        kind: FileKind::Jpeg,
    });

    // --- CANDIDATE 8 — with .bae/ sidecar (C10). ---
    entries.push(FixtureEntry::Expect {
        rel_path: "Artist B/1992 - Album B3".into(),
        top_level_candidate: true,
    });
    entries.extend(flat_audio("Artist B/1992 - Album B3", 7, FileKind::Flac));
    entries.push(FixtureEntry::File {
        rel_path: "Artist B/1992 - Album B3/cover.jpg".into(),
        kind: FileKind::Jpeg,
    });
    // `.bae/` sidecar — scanner must ignore the hidden dir entirely.
    entries.push(FixtureEntry::File {
        rel_path: "Artist B/1992 - Album B3/.bae/cover-mb.jpg".into(),
        kind: FileKind::Jpeg,
    });

    // --- CANDIDATE 9 — .md5, .ffp (C8). ---
    entries.push(FixtureEntry::Expect {
        rel_path: "Artist B/1994 - Album B4".into(),
        top_level_candidate: true,
    });
    entries.extend(flat_audio("Artist B/1994 - Album B4", 4, FileKind::Flac));
    entries.push(FixtureEntry::File {
        rel_path: "Artist B/1994 - Album B4/checksums.md5".into(),
        kind: FileKind::Md5,
    });
    entries.push(FixtureEntry::File {
        rel_path: "Artist B/1994 - Album B4/checksums.ffp".into(),
        kind: FileKind::Ffp,
    });

    // --- CANDIDATE 10 — .log, .m3u (C9). ---
    entries.push(FixtureEntry::Expect {
        rel_path: "Artist B/1998 - Album B5".into(),
        top_level_candidate: true,
    });
    entries.extend(flat_audio("Artist B/1998 - Album B5", 8, FileKind::Flac));
    entries.push(FixtureEntry::File {
        rel_path: "Artist B/1998 - Album B5/rip.log".into(),
        kind: FileKind::Log,
    });
    entries.push(FixtureEntry::File {
        rel_path: "Artist B/1998 - Album B5/playlist.m3u".into(),
        kind: FileKind::M3u,
    });
    // --- Compilation folder (B6) — navigation container. ---
    entries.push(FixtureEntry::Expect {
        rel_path: "Artist C - Albums & Singles [mp3]".into(),
        top_level_candidate: false,
    });

    // --- CANDIDATE 11 — MP3 flat + multi-cover (C5). ---
    let c1 = "Artist C - Albums & Singles [mp3]/1974 - Album C1 [320]";
    entries.push(FixtureEntry::Expect {
        rel_path: c1.into(),
        top_level_candidate: true,
    });
    entries.extend(flat_audio(c1, 10, FileKind::Mp3));
    for name in ["front.jpg", "back.jpg", "inlay.jpg"] {
        entries.push(FixtureEntry::File {
            rel_path: format!("{c1}/{name}"),
            kind: FileKind::Jpeg,
        });
    }

    // --- CANDIDATE 12 — MP3 flat + multi-cover (C5). ---
    let c2 = "Artist C - Albums & Singles [mp3]/1982 - Album C2 [320]";
    entries.push(FixtureEntry::Expect {
        rel_path: c2.into(),
        top_level_candidate: true,
    });
    entries.extend(flat_audio(c2, 9, FileKind::Mp3));
    for name in ["front.jpg", "back.jpg", "inlay.jpg"] {
        entries.push(FixtureEntry::File {
            rel_path: format!("{c2}/{name}"),
            kind: FileKind::Jpeg,
        });
    }

    // --- CANDIDATE 13 — true multi-disc release (B7). ---
    entries.push(FixtureEntry::Expect {
        rel_path: "Album D - Multi-Disc".into(),
        top_level_candidate: true,
    });
    entries.push(FixtureEntry::File {
        rel_path: "Album D - Multi-Disc/cover.jpg".into(),
        kind: FileKind::Jpeg,
    });
    entries.push(FixtureEntry::Expect {
        rel_path: "Album D - Multi-Disc/Disc 1".into(),
        top_level_candidate: false,
    });
    entries.extend(flat_audio("Album D - Multi-Disc/Disc 1", 5, FileKind::Flac));
    entries.push(FixtureEntry::Expect {
        rel_path: "Album D - Multi-Disc/Disc 2".into(),
        top_level_candidate: false,
    });
    entries.extend(flat_audio("Album D - Multi-Disc/Disc 2", 6, FileKind::Flac));

    // --- In-progress — zero-byte audio (A1). SKIP. ---
    entries.push(FixtureEntry::Expect {
        rel_path: "In-Progress - Zero Byte".into(),
        top_level_candidate: false,
    });
    entries.push(FixtureEntry::File {
        rel_path: "In-Progress - Zero Byte/01.flac".into(),
        kind: FileKind::Flac,
    });
    entries.push(FixtureEntry::File {
        rel_path: "In-Progress - Zero Byte/02.flac".into(),
        kind: FileKind::ZeroByteFlac,
    });
    entries.push(FixtureEntry::File {
        rel_path: "In-Progress - Zero Byte/03.flac".into(),
        kind: FileKind::ZeroByteFlac,
    });

    // --- In-progress — browser markers only, no real audio (A3). SKIP. ---
    entries.push(FixtureEntry::Expect {
        rel_path: "In-Progress - Browser".into(),
        top_level_candidate: false,
    });
    entries.push(FixtureEntry::File {
        rel_path: "In-Progress - Browser/01.flac.part".into(),
        kind: FileKind::PartialMarker("part"),
    });
    entries.push(FixtureEntry::File {
        rel_path: "In-Progress - Browser/02.flac.crdownload".into(),
        kind: FileKind::PartialMarker("crdownload"),
    });
    entries.push(FixtureEntry::File {
        rel_path: "In-Progress - Browser/cover.jpg".into(),
        kind: FileKind::Jpeg,
    });

    // --- In-progress — aria2 marker only. SKIP. ---
    entries.push(FixtureEntry::Expect {
        rel_path: "In-Progress - aria2".into(),
        top_level_candidate: false,
    });
    entries.push(FixtureEntry::File {
        rel_path: "In-Progress - aria2/01.flac.aria2".into(),
        kind: FileKind::PartialMarker("aria2"),
    });

    // --- In-progress — generic ".download" marker only. SKIP. ---
    entries.push(FixtureEntry::Expect {
        rel_path: "In-Progress - Safari".into(),
        top_level_candidate: false,
    });
    entries.push(FixtureEntry::File {
        rel_path: "In-Progress - Safari/01.flac.download".into(),
        kind: FileKind::PartialMarker("download"),
    });

    // --- In-progress — generic ".partial" marker only. SKIP. ---
    entries.push(FixtureEntry::Expect {
        rel_path: "In-Progress - Generic".into(),
        top_level_candidate: false,
    });
    entries.push(FixtureEntry::File {
        rel_path: "In-Progress - Generic/01.flac.partial".into(),
        kind: FileKind::PartialMarker("partial"),
    });

    // --- CUE-Mismatch — 10 real FLACs + CUE claiming 15. SKIP. ---
    entries.push(FixtureEntry::Expect {
        rel_path: "CUE-Mismatch".into(),
        top_level_candidate: false,
    });
    entries.extend(flat_audio("CUE-Mismatch", 10, FileKind::Flac));
    entries.push(FixtureEntry::File {
        rel_path: "CUE-Mismatch/Album.cue".into(),
        kind: FileKind::NonPairingCue {
            n_tracks: 15,
            file_reference: "Album.flac",
        },
    });

    // --- Non-music video folders (B8). ---
    entries.push(FixtureEntry::Expect {
        rel_path: "Video Series".into(),
        top_level_candidate: false,
    });
    entries.push(FixtureEntry::Expect {
        rel_path: "Video Series/Season 1".into(),
        top_level_candidate: false,
    });
    for i in 1..=3 {
        entries.push(FixtureEntry::File {
            rel_path: format!("Video Series/Season 1/S01E{:02}.avi", i),
            kind: FileKind::Avi,
        });
    }
    entries.push(FixtureEntry::Expect {
        rel_path: "Video Series/Season 2".into(),
        top_level_candidate: false,
    });
    for i in 1..=2 {
        entries.push(FixtureEntry::File {
            rel_path: format!("Video Series/Season 2/S02E{:02}.avi", i),
            kind: FileKind::Avi,
        });
    }

    entries
}

/// Integration smoke test: the full reference fixture vs the real scanner.
///
/// Encodes the human-intent contract as a set-equality check against the
/// top-level candidates the scanner produces, plus per-release content
/// sanity assertions. Any regression — bogus parents, sibling taint,
/// partial-marker leaks, CUE-mismatch surfacing — produces a set-diff
/// failure with the exact offending paths.
#[test]
fn scan_reference_tree_matches_human_intent() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();

    let spec = reference_fixture();
    build_fixture(root, &spec);
    assert_fixture_invariants(root, &spec);

    // Run the real scanner.
    let candidates = scan_valid(root.to_path_buf());

    // --- Assertion 1: exact top-level candidate set ---

    let top_level: std::collections::BTreeSet<String> = candidates
        .iter()
        .map(|c| {
            c.path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .to_string()
        })
        .collect();

    let expected: std::collections::BTreeSet<String> = spec
        .iter()
        .filter_map(|e| match e {
            FixtureEntry::Expect {
                rel_path,
                top_level_candidate: true,
            } => Some(rel_path.to_string()),
            _ => None,
        })
        .collect();

    let unexpected: Vec<_> = top_level.difference(&expected).collect();
    let missing: Vec<_> = expected.difference(&top_level).collect();
    assert!(
            unexpected.is_empty() && missing.is_empty(),
            "top-level candidate set mismatch: {} unexpected, {} missing\n  unexpected (bogus): {:#?}\n  missing: {:#?}",
            unexpected.len(),
            missing.len(),
            unexpected,
            missing,
        );

    // --- Assertion 2: multi-disc parent shape ---

    assert!(
        top_level.iter().any(|p| p == "Album D - Multi-Disc"),
        "Album D - Multi-Disc should be a top-level candidate",
    );
    assert!(
        !top_level.iter().any(|p| p == "Album D - Multi-Disc/Disc 1"),
        "Disc 1 should not be a top-level candidate",
    );
    assert!(
        !top_level.iter().any(|p| p == "Album D - Multi-Disc/Disc 2"),
        "Disc 2 should not be a top-level candidate",
    );

    // --- Assertion 3: per-release content sanity ---

    let find = |suffix: &str| -> &FolderCandidate {
        candidates
            .iter()
            .find(|c| c.path.to_string_lossy().ends_with(suffix))
            .unwrap_or_else(|| {
                panic!(
                    "expected candidate ending in {:?}, got candidate set {:#?}",
                    suffix, top_level,
                )
            })
    };

    // 3a. 1991 - Album A2 [Original Release] → CueFlacPairs / "CUE+FLAC".
    let a2_original = find("1991 - Album A2 [Original Release]");
    match &a2_original.files.audio {
        AudioContent::CueFlacPairs { format_label, .. } => {
            assert_eq!(format_label, "CUE+FLAC");
        }
        other => panic!(
            "Album A2 [Original Release] should be CueFlacPairs / CUE+FLAC, got {:?}",
            other,
        ),
    }

    // 3b. 1997 - Album A2 [Japan Reissue] → CueFlacPairs / "CUE+APE",
    //     with Info/Tracklist.txt as a document.
    let a2_japan = find("1997 - Album A2 [Japan Reissue]");
    match &a2_japan.files.audio {
        AudioContent::CueFlacPairs { format_label, .. } => {
            assert_eq!(format_label, "CUE+APE");
        }
        other => panic!(
            "Album A2 [Japan Reissue] should be CueFlacPairs / CUE+APE, got {:?}",
            other,
        ),
    }
    assert!(
        a2_japan
            .files
            .documents
            .iter()
            .any(|d| d.relative_path.ends_with("Tracklist.txt")),
        "Album A2 [Japan Reissue] documents should include Info/Tracklist.txt, got {:?}",
        a2_japan
            .files
            .documents
            .iter()
            .map(|d| d.relative_path.as_str())
            .collect::<Vec<_>>(),
    );

    // 3c. 2003 - Album A2 [Reissue] → artwork pulled from booklet/.
    let a2_reissue = find("2003 - Album A2 [Reissue]");
    assert!(
        a2_reissue
            .files
            .artwork
            .iter()
            .any(|a| a.relative_path.contains("booklet/")),
        "Album A2 [Reissue] artwork should include booklet/*.png, got {:?}",
        a2_reissue
            .files
            .artwork
            .iter()
            .map(|a| a.relative_path.as_str())
            .collect::<Vec<_>>(),
    );

    // 3d. 1974 - Album C1 [320] → TrackFiles / "MP3".
    let c1 = find("1974 - Album C1 [320]");
    match &c1.files.audio {
        AudioContent::TrackFiles { format_label, .. } => {
            assert_eq!(format_label, "MP3");
        }
        other => panic!("Album C1 [320] should be TrackFiles / MP3, got {:?}", other,),
    }
}

// ── Scenario test library ──────────────────────────────────────────────
//
// Each test builds the minimum tree needed to exercise one rule, scans
// it, and asserts a specific outcome. All tests go through the
// `run_scenario` helper, which handles tempdir creation, fixture
// invariant checking, and top-level path extraction.
//
// Test names name the scenario, not the implementation. The taxonomy
// in `plans/folder-scanner-scenario-tests.md` is the map.

/// Wraps the common "build tempdir, scan, filter top-level" shape. Keeps
/// `_tmp` alive for the lifetime of the result so the tempdir isn't
/// pulled out from under `candidates`.
struct ScenarioResult {
    /// Held for its `Drop`: keeps the tempdir alive so `candidates` paths
    /// remain valid for the result's lifetime. Never read directly.
    _tmp: tempfile::TempDir,
    candidates: Vec<FolderCandidate>,
    root: PathBuf,
}

impl ScenarioResult {
    /// Candidate rel paths, stripped of the tempdir prefix.
    fn top_level_paths(&self) -> Vec<String> {
        self.candidates
            .iter()
            .map(|c| {
                c.path
                    .strip_prefix(&self.root)
                    .unwrap()
                    .to_string_lossy()
                    .to_string()
            })
            .collect()
    }

    /// Find a candidate by exact rel path.
    fn candidate(&self, rel_path: &str) -> &FolderCandidate {
        let target = Path::new(rel_path);
        self.candidates
            .iter()
            .find(|c| {
                c.path
                    .strip_prefix(&self.root)
                    .expect("candidate path is under scan root")
                    == target
            })
            .unwrap_or_else(|| {
                panic!(
                    "no candidate at {rel_path:?}; have {:?}",
                    self.top_level_paths()
                )
            })
    }
}

fn run_scenario(entries: Vec<FixtureEntry>) -> ScenarioResult {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    build_fixture(&root, &entries);
    assert_fixture_invariants(&root, &entries);
    let candidates = scan_valid(root.clone());
    ScenarioResult {
        _tmp: tmp,
        candidates,
        root,
    }
}

// ── Layer 1: single-case minimal tests ────────────────────────────────

// --- Completeness signals (A-series) ---

/// L1.1 — Release folder with a real FLAC and a zero-byte FLAC must be
/// rejected. Mixing valid and zero-byte audio poisons the candidate.
#[test]
fn zero_byte_audio_in_release_skips_candidate() {
    let result = run_scenario(vec![
        FixtureEntry::File {
            rel_path: "Album/01.flac".into(),
            kind: FileKind::Flac,
        },
        FixtureEntry::File {
            rel_path: "Album/02.flac".into(),
            kind: FileKind::ZeroByteFlac,
        },
    ]);
    assert!(result.top_level_paths().is_empty());
}

/// L1.2 — `.flac.part` sidecar next to a real FLAC suppresses the release.
#[test]
fn partial_marker_sidecar_skips_release() {
    let result = run_scenario(vec![
        FixtureEntry::File {
            rel_path: "Album/01.flac".into(),
            kind: FileKind::Flac,
        },
        FixtureEntry::File {
            rel_path: "Album/02.flac.part".into(),
            kind: FileKind::PartialMarker("part"),
        },
    ]);
    assert!(result.top_level_paths().is_empty());
}

/// L1.3 — A folder holding only partial markers (no real audio) must
/// also yield no candidates. Today this passes because no audio is
/// detected at all; the test pins the intent so if marker-only becomes
/// "looks like audio" we notice.
#[test]
fn partial_marker_only_no_real_audio_skips() {
    let result = run_scenario(vec![FixtureEntry::File {
        rel_path: "Album/01.flac.part".into(),
        kind: FileKind::PartialMarker("part"),
    }]);
    assert!(result.top_level_paths().is_empty());
}

/// An I/O fault while validating an audio file (here: the file vanished
/// after the tree was built) is a system error, not "corrupt" — it must
/// surface, not be collapsed to `false` and silently drop the whole
/// release while mis-logging the cause as corruption. The Ok(false)
/// corruption path (covered elsewhere) still skips the candidate.
#[test]
fn io_error_validating_audio_surfaces_not_swallowed() {
    let tree = FileTree::new(vec![FileEntry {
        path: PathBuf::from("Album/01.flac"),
        size: 1024,
    }]);
    // fs_root is an empty dir, so Album/01.flac does not exist on disk:
    // is_valid_audio's open fails with a genuine I/O error.
    let temp = tempfile::TempDir::new().unwrap();
    let result = categorize_files_from_tree(&tree, &PathBuf::from("Album"), temp.path());
    assert!(
        result.is_err(),
        "an I/O fault during validation must surface as an error"
    );
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
        FixtureEntry::File {
            rel_path: "loose.flac.part".into(),
            kind: FileKind::PartialMarker("part"),
        },
        FixtureEntry::File {
            rel_path: "AlbumA/01.flac".into(),
            kind: FileKind::Flac,
        },
        FixtureEntry::File {
            rel_path: "AlbumB/01.flac".into(),
            kind: FileKind::Flac,
        },
    ]);
    let mut paths = result.top_level_paths();
    paths.sort();
    assert_eq!(paths, vec!["AlbumA".to_string(), "AlbumB".to_string()]);
}

/// L1.4 — Every supported partial-marker extension suppresses the release.
/// Table-driven: one subtest per extension.
#[test]
fn each_partial_marker_extension_skips_release() {
    for ext in ["part", "crdownload", "download", "aria2", "partial"] {
        let result = run_scenario(vec![
            FixtureEntry::File {
                rel_path: "Album/01.flac".into(),
                kind: FileKind::Flac,
            },
            FixtureEntry::File {
                rel_path: format!("Album/02.flac.{ext}"),
                kind: FileKind::PartialMarker(ext),
            },
        ]);
        assert!(
            result.top_level_paths().is_empty(),
            "extension .{ext} should suppress the candidate",
        );
    }
}

/// L1.5 — Partial-marker extension matching is case-insensitive.
#[test]
fn partial_marker_extension_case_insensitive() {
    for name in ["02.FLAC.PART", "03.FLAC.CRDownload"] {
        let ext = name.rsplit('.').next().unwrap();
        let result = run_scenario(vec![
            FixtureEntry::File {
                rel_path: "Album/01.flac".into(),
                kind: FileKind::Flac,
            },
            FixtureEntry::File {
                rel_path: format!("Album/{name}"),
                kind: FileKind::PartialMarker(ext),
            },
        ]);
        assert!(
            result.top_level_paths().is_empty(),
            "marker {name} should suppress the candidate",
        );
    }
}

/// L1.6 — Valid FLAC magic but declared samples >> file size ⇒ rejected.
#[test]
fn truncated_flac_rejected() {
    let result = run_scenario(vec![FixtureEntry::File {
        rel_path: "Album/01.flac".into(),
        kind: FileKind::TruncatedFlac,
    }]);
    assert!(result.top_level_paths().is_empty());
}

/// L1.7 — Wrong magic bytes where a FLAC is expected ⇒ rejected.
#[test]
fn broken_flac_header_rejected() {
    let result = run_scenario(vec![FixtureEntry::File {
        rel_path: "Album/01.flac".into(),
        kind: FileKind::BrokenFlac,
    }]);
    assert!(result.top_level_paths().is_empty());
}

// --- Container shape signals (B-series) ---

/// L1.8 — Discography wrapper plus grouping folder chain collapses down
/// to the single inner release. Neither wrapper surfaces.
#[test]
fn discography_wrapper_recurses_does_not_surface() {
    let result = run_scenario(flat_audio(
        "Artist - Discography/Studio Albums/Album",
        3,
        FileKind::Flac,
    ));
    assert_eq!(
        result.top_level_paths(),
        vec!["Artist - Discography/Studio Albums/Album"],
    );
}

/// L1.9 — Deeper single-child wrapper chains still flatten to the leaf.
#[test]
fn grouping_folder_recurses_does_not_surface() {
    let result = run_scenario(flat_audio("A/B/C/Album", 3, FileKind::Flac));
    assert_eq!(result.top_level_paths(), vec!["A/B/C/Album"]);
}

/// L1.10 — Single-child wrapper with one release surfaces the inner folder.
#[test]
fn single_child_audio_wrapper_surfaces_inner() {
    let result = run_scenario(flat_audio("Wrapper/Release", 3, FileKind::Flac));
    assert_eq!(result.top_level_paths(), vec!["Wrapper/Release"]);
    assert_eq!(result.candidate("Wrapper/Release").name, "Release");
}

/// L1.11 — A year-prefixed reissue container emits each child, not the parent.
#[test]
fn multi_child_reissue_container_emits_children_not_parent() {
    let mut entries = flat_audio("Album/1991 - Original", 3, FileKind::Flac);
    entries.extend(flat_audio("Album/1997 - Japan Reissue", 3, FileKind::Flac));
    entries.extend(flat_audio("Album/2003 - Reissue", 3, FileKind::Flac));
    let result = run_scenario(entries);
    let top = result.top_level_paths();
    let expected = vec![
        "Album/1991 - Original",
        "Album/1997 - Japan Reissue",
        "Album/2003 - Reissue",
    ];
    for e in &expected {
        assert!(top.iter().any(|p| p == e), "missing {e} in {top:?}");
    }
    assert!(!top.iter().any(|p| p == "Album"), "parent leaked: {top:?}");
}

/// L1.12 — Artist folder emits per-album candidates, not the artist folder.
#[test]
fn artist_folder_emits_per_album_candidates() {
    let mut entries = flat_audio("Artist/Album A", 3, FileKind::Flac);
    entries.extend(flat_audio("Artist/Album B", 3, FileKind::Flac));
    let result = run_scenario(entries);
    let top = result.top_level_paths();
    assert_eq!(top.len(), 2);
    assert!(top.iter().any(|p| p == "Artist/Album A"));
    assert!(top.iter().any(|p| p == "Artist/Album B"));
}

/// L1.13 — Compilation-style artist folder emits per-item candidates.
#[test]
fn compilation_folder_emits_per_item_candidates() {
    let mut entries = flat_audio("Compilation [mp3]/1974 - Album One", 3, FileKind::Mp3);
    entries.extend(flat_audio(
        "Compilation [mp3]/1982 - Album Two",
        3,
        FileKind::Mp3,
    ));
    let result = run_scenario(entries);
    let top = result.top_level_paths();
    assert_eq!(top.len(), 2);
    assert!(top
        .iter()
        .any(|p| p == "Compilation [mp3]/1974 - Album One"));
    assert!(top
        .iter()
        .any(|p| p == "Compilation [mp3]/1982 - Album Two"));
}

/// L1.14 — `Disc 1` / `Disc 2` subdirs emit the parent as a multi-disc.
#[test]
fn multi_disc_release_with_disc_prefix_emits_parent() {
    let mut entries = flat_audio("Album/Disc 1", 3, FileKind::Flac);
    entries.extend(flat_audio("Album/Disc 2", 3, FileKind::Flac));
    let result = run_scenario(entries);
    assert_eq!(result.top_level_paths(), vec!["Album"]);
}

/// L1.15 — Bare numeric subdirs (`1`, `2`) still trigger multi-disc.
#[test]
fn multi_disc_release_with_bare_numeric_emits_parent() {
    let mut entries = flat_audio("Album/1", 3, FileKind::Flac);
    entries.extend(flat_audio("Album/2", 3, FileKind::Flac));
    let result = run_scenario(entries);
    assert_eq!(result.top_level_paths(), vec!["Album"]);
}

/// L1.16 — `CD1` / `CD2` subdirs trigger multi-disc.
#[test]
fn multi_disc_release_with_cd_prefix_emits_parent() {
    let mut entries = flat_audio("Box/CD1", 3, FileKind::Flac);
    entries.extend(flat_audio("Box/CD2", 3, FileKind::Flac));
    let result = run_scenario(entries);
    assert_eq!(result.top_level_paths(), vec!["Box"]);
}

/// L1.17 — `Side A` / `Side B` vinyl-style subdirs trigger multi-disc.
#[test]
fn multi_disc_release_with_side_prefix_emits_parent() {
    let mut entries = flat_audio("LP/Side A", 3, FileKind::Flac);
    entries.extend(flat_audio("LP/Side B", 3, FileKind::Flac));
    let result = run_scenario(entries);
    assert_eq!(result.top_level_paths(), vec!["LP"]);
}

/// L1.18 — `Part 1` / `Part 2` subdirs trigger multi-disc.
#[test]
fn multi_disc_release_with_part_prefix_emits_parent() {
    let mut entries = flat_audio("Suite/Part 1", 3, FileKind::Flac);
    entries.extend(flat_audio("Suite/Part 2", 3, FileKind::Flac));
    let result = run_scenario(entries);
    assert_eq!(result.top_level_paths(), vec!["Suite"]);
}

/// L1.19 — `Disk 1` / `Disk 2` subdirs trigger multi-disc.
#[test]
fn multi_disc_release_with_disk_prefix_emits_parent() {
    let mut entries = flat_audio("Album/Disk 1", 3, FileKind::Flac);
    entries.extend(flat_audio("Album/Disk 2", 3, FileKind::Flac));
    let result = run_scenario(entries);
    assert_eq!(result.top_level_paths(), vec!["Album"]);
}

/// Box-set with per-disc descriptive suffixes still surfaces as a single
/// multi-disc candidate. Separators varied on purpose (hyphen, bullet,
/// parens, colon) — every one of these appears as a disc-name suffix in
/// real rips, and the predicate must accept any non-alphanumeric
/// character after the digit run.
#[test]
fn multi_disc_release_with_descriptive_disc_suffix_emits_parent() {
    let discs = [
        "Box Set/CD 1 - part one",
        "Box Set/CD 2 - part two",
        "Box Set/CD 3 • part three",
        "Box Set/CD 4 (part four)",
        "Box Set/CD 5: part five",
    ];
    let mut entries = Vec::new();
    for disc in &discs {
        entries.extend(flat_audio(disc, 3, FileKind::Flac));
    }
    let result = run_scenario(entries);
    assert_eq!(result.top_level_paths(), vec!["Box Set"]);
    let c = result.candidate("Box Set");
    let prefixes: BTreeSet<Option<&str>> = match &c.files.audio {
        AudioContent::TrackFiles { tracks, .. } => {
            tracks.iter().map(|t| t.dir_prefix.as_deref()).collect()
        }
        AudioContent::CueFlacPairs { pairs, .. } => pairs
            .iter()
            .map(|p| p.audio_file.dir_prefix.as_deref())
            .collect(),
    };
    assert_eq!(
        prefixes.len(),
        5,
        "5 distinct disc prefixes, got {prefixes:?}"
    );
}

/// L1.20 — Hyphen separator (`Disc-1`) is accepted.
#[test]
fn disc_indicator_separator_hyphen_matches() {
    let mut entries = flat_audio("Album/Disc-1", 3, FileKind::Flac);
    entries.extend(flat_audio("Album/Disc-2", 3, FileKind::Flac));
    let result = run_scenario(entries);
    assert_eq!(result.top_level_paths(), vec!["Album"]);
}

/// L1.21 — Underscore separator (`CD_1`) is accepted.
#[test]
fn disc_indicator_separator_underscore_matches() {
    let mut entries = flat_audio("Album/CD_1", 3, FileKind::Flac);
    entries.extend(flat_audio("Album/CD_2", 3, FileKind::Flac));
    let result = run_scenario(entries);
    assert_eq!(result.top_level_paths(), vec!["Album"]);
}

/// L1.22 — Dot separator (`Side.A`) is accepted.
#[test]
fn disc_indicator_separator_dot_matches() {
    let mut entries = flat_audio("LP/Side.A", 3, FileKind::Flac);
    entries.extend(flat_audio("LP/Side.B", 3, FileKind::Flac));
    let result = run_scenario(entries);
    assert_eq!(result.top_level_paths(), vec!["LP"]);
}

/// L1.23 — `Side A` matches; `Side AB` does not (single-alpha rule).
#[test]
fn side_indicator_accepts_single_alpha_only() {
    assert!(is_disc_indicator_name("Side A"));
    assert!(!is_disc_indicator_name("Side AB"));
}

/// L1.24 — `Sider` (no separator, more than single alpha) must not match,
/// guarding against the false-positive class.
#[test]
fn sider_not_matched_as_side_indicator() {
    assert!(!is_disc_indicator_name("Sider"));
    assert!(!is_disc_indicator_name("Sideshow"));
}

/// L1.25 — Folder with only `.avi` yields no candidates, no diagnostic.
#[test]
fn non_audio_folder_emits_no_candidates() {
    let result = run_scenario(vec![
        FixtureEntry::File {
            rel_path: "Show/S01E01.avi".into(),
            kind: FileKind::Avi,
        },
        FixtureEntry::File {
            rel_path: "Show/S01E02.avi".into(),
            kind: FileKind::Avi,
        },
    ]);
    assert!(result.top_level_paths().is_empty());
}

/// L1.26 — Loose junk at the scan root (.pdf, .zip, .dmg, .jpg) is
/// ignored; a release subfolder still surfaces.
#[test]
fn loose_junk_at_scan_root_ignored() {
    let mut entries = vec![
        FixtureEntry::File {
            rel_path: "loose.pdf".into(),
            kind: FileKind::Pdf,
        },
        FixtureEntry::File {
            rel_path: "loose.zip".into(),
            kind: FileKind::Zip,
        },
        FixtureEntry::File {
            rel_path: "loose.dmg".into(),
            kind: FileKind::Dmg,
        },
    ];
    entries.extend(flat_audio("Album", 3, FileKind::Flac));
    let result = run_scenario(entries);
    assert_eq!(result.top_level_paths(), vec!["Album"]);
}

// --- Within-release intricacies (C-series) ---

/// L1.27 — Per-track FLACs surface as TrackFiles / "FLAC".
#[test]
fn flat_flac_release_surfaces_as_trackfiles() {
    let result = run_scenario(flat_audio("Album", 3, FileKind::Flac));
    let c = result.candidate("Album");
    match &c.files.audio {
        AudioContent::TrackFiles {
            tracks,
            format_label,
        } => {
            assert_eq!(format_label, "FLAC");
            assert_eq!(tracks.len(), 3);
        }
        other => panic!("expected TrackFiles / FLAC, got {other:?}"),
    }
}

/// L1.28 — CUE+FLAC pair surfaces as CueFlacPairs / "CUE+FLAC".
#[test]
fn cue_flac_pair_surfaces_as_cue_pairs() {
    let result = run_scenario(vec![
        FixtureEntry::File {
            rel_path: "Album/Album.flac".into(),
            kind: FileKind::Flac,
        },
        FixtureEntry::File {
            rel_path: "Album/Album.cue".into(),
            kind: FileKind::CueFor {
                stem: "Album.flac",
                n_tracks: 8,
            },
        },
    ]);
    let c = result.candidate("Album");
    match &c.files.audio {
        AudioContent::CueFlacPairs {
            pairs,
            format_label,
        } => {
            assert_eq!(format_label, "CUE+FLAC");
            assert_eq!(pairs.len(), 1);
        }
        other => panic!("expected CueFlacPairs / CUE+FLAC, got {other:?}"),
    }
}

// L1.29 (cue_ape pair) — covered by
// `test_collect_release_candidate_files_cue_ape_track_count` above; not
// duplicated here.

/// L1.30 — MP3 tracks surface as TrackFiles / "MP3".
#[test]
fn mp3_release_surfaces_as_mp3_trackfiles() {
    let result = run_scenario(flat_audio("Album", 3, FileKind::Mp3));
    let c = result.candidate("Album");
    match &c.files.audio {
        AudioContent::TrackFiles { format_label, .. } => assert_eq!(format_label, "MP3"),
        other => panic!("expected TrackFiles / MP3, got {other:?}"),
    }
}

/// L1.30b — M4A tracks surface as TrackFiles / "M4A".
#[test]
fn m4a_release_surfaces_as_trackfiles() {
    let result = run_scenario(flat_audio("Album", 3, FileKind::M4a));
    let c = result.candidate("Album");
    match &c.files.audio {
        AudioContent::TrackFiles {
            tracks,
            format_label,
        } => {
            assert_eq!(format_label, "M4A");
            assert_eq!(tracks.len(), 3);
        }
        other => panic!("expected TrackFiles / M4A, got {other:?}"),
    }
}

/// A multi-FILE CUE (one FILE per TRACK, the lossy-rip shape) does not
/// pair on purpose — each track has its own file. The per-track audio
/// surfaces as `TrackFiles`; the CUE lands in `documents`.
#[test]
fn multi_file_cue_surfaces_as_trackfiles() {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(&album).unwrap();
    for i in 1..=3 {
        std::fs::write(album.join(format!("0{i}.m4a")), bytes_for(FileKind::M4a)).unwrap();
    }
    std::fs::write(
        album.join("Album.cue"),
        "PERFORMER \"Artist Name\"\nTITLE \"Album Title\"\n\
             FILE \"01.m4a\" WAVE\n  TRACK 01 AUDIO\n    INDEX 01 00:00:00\n\
             FILE \"02.m4a\" WAVE\n  TRACK 02 AUDIO\n    INDEX 01 00:00:00\n\
             FILE \"03.m4a\" WAVE\n  TRACK 03 AUDIO\n    INDEX 01 00:00:00\n",
    )
    .unwrap();
    let candidates = scan_valid(tmp.path().to_path_buf());
    assert_eq!(candidates.len(), 1);
    let c = &candidates[0];
    match &c.files.audio {
        AudioContent::TrackFiles {
            tracks,
            format_label,
        } => {
            assert_eq!(tracks.len(), 3);
            assert_eq!(format_label, "M4A");
        }
        other => panic!("expected TrackFiles, got {other:?}"),
    }
    assert!(
        c.files.documents.iter().any(|d| d.file_name == "Album.cue"),
        "multi-FILE CUE should land in documents",
    );
}

/// Single-FILE CUE whose own stem differs from the audio file it names —
/// pair detection follows the CUE's `FILE` directive, not the CUE's
/// filename stem. The CUE is the source of truth for what it points at.
#[test]
fn single_file_cue_pairs_by_file_directive_not_stem() {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(&album).unwrap();
    std::fs::write(album.join("Audio.flac"), bytes_for(FileKind::Flac)).unwrap();
    std::fs::write(
        album.join("Sheet.cue"),
        "PERFORMER \"X\"\nTITLE \"Y\"\nFILE \"Audio.flac\" WAVE\n  \
             TRACK 01 AUDIO\n    INDEX 01 00:00:00\n  \
             TRACK 02 AUDIO\n    INDEX 01 03:00:00\n",
    )
    .unwrap();
    let candidates = scan_valid(tmp.path().to_path_buf());
    assert_eq!(candidates.len(), 1);
    match &candidates[0].files.audio {
        AudioContent::CueFlacPairs { pairs, .. } => {
            assert_eq!(pairs.len(), 1);
            assert_eq!(pairs[0].audio_file.file_name, "Audio.flac");
            assert_eq!(pairs[0].cue_file.file_name, "Sheet.cue");
        }
        other => panic!("expected CueFlacPairs, got {other:?}"),
    }
}

/// L1.31 — A CUE whose stem does not match any audio in the dir lands in
/// `documents`; the audio stays as TrackFiles.
#[test]
fn non_pairing_cue_becomes_document() {
    let mut entries = flat_audio("Album", 5, FileKind::Flac);
    entries.push(FixtureEntry::File {
        rel_path: "Album/Album.cue".into(),
        kind: FileKind::NonPairingCue {
            n_tracks: 5,
            file_reference: "Album.flac",
        },
    });
    let result = run_scenario(entries);
    let c = result.candidate("Album");
    match &c.files.audio {
        AudioContent::TrackFiles { tracks, .. } => assert_eq!(tracks.len(), 5),
        other => panic!("expected TrackFiles, got {other:?}"),
    }
    assert!(
        c.files.documents.iter().any(|d| d.file_name == "Album.cue"),
        "Album.cue should land in documents, got {:?}",
        c.files
            .documents
            .iter()
            .map(|d| d.file_name.as_str())
            .collect::<Vec<_>>(),
    );
}

/// L1.32 — Candidate 6 shape: on-disk track count equals the non-pairing
/// CUE's declared count, so the mismatch guard stays quiet. Candidate
/// surfaces with the CUE as a document.
#[test]
fn non_pairing_cue_with_matching_track_count_keeps_release() {
    let mut entries = flat_audio("Album", 5, FileKind::Flac);
    entries.push(FixtureEntry::File {
        rel_path: "Album/Album.cue".into(),
        kind: FileKind::NonPairingCue {
            n_tracks: 5,
            file_reference: "Album.flac",
        },
    });
    let result = run_scenario(entries);
    assert_eq!(result.top_level_paths(), vec!["Album"]);
    assert!(result
        .candidate("Album")
        .files
        .documents
        .iter()
        .any(|d| d.file_name == "Album.cue"),);
}

/// L1.33 — `booklet/*.png` attaches as artwork with the `booklet/` prefix.
#[test]
fn booklet_subfolder_attaches_as_artwork() {
    let mut entries = flat_audio("Album", 3, FileKind::Flac);
    entries.extend([
        FixtureEntry::File {
            rel_path: "Album/booklet/page1.png".into(),
            kind: FileKind::Png,
        },
        FixtureEntry::File {
            rel_path: "Album/booklet/page2.png".into(),
            kind: FileKind::Png,
        },
    ]);
    let result = run_scenario(entries);
    let c = result.candidate("Album");
    let booklet_paths: Vec<_> = c
        .files
        .artwork
        .iter()
        .filter(|a| a.relative_path.starts_with("booklet/"))
        .map(|a| a.relative_path.as_str())
        .collect();
    assert_eq!(booklet_paths.len(), 2, "got {:?}", booklet_paths);
}

/// L1.34 — `Info/Tracklist.txt` attaches as a document.
#[test]
fn info_subfolder_txt_attaches_as_document() {
    let mut entries = flat_audio("Album", 3, FileKind::Flac);
    entries.push(FixtureEntry::File {
        rel_path: "Album/Info/Tracklist.txt".into(),
        kind: FileKind::TracklistTxt,
    });
    let result = run_scenario(entries);
    let c = result.candidate("Album");
    assert!(
        c.files
            .documents
            .iter()
            .any(|d| d.relative_path.ends_with("Tracklist.txt")),
        "Tracklist.txt should be a document, got {:?}",
        c.files
            .documents
            .iter()
            .map(|d| d.relative_path.as_str())
            .collect::<Vec<_>>(),
    );
}

/// L1.35 — `.md5` / `.ffp` sidecars are neither audio nor artwork nor
/// documents; they are omitted from categorization.
#[test]
fn md5_ffp_sidecars_silently_ignored() {
    let mut entries = flat_audio("Album", 3, FileKind::Flac);
    entries.extend([
        FixtureEntry::File {
            rel_path: "Album/checksums.md5".into(),
            kind: FileKind::Md5,
        },
        FixtureEntry::File {
            rel_path: "Album/checksums.ffp".into(),
            kind: FileKind::Ffp,
        },
    ]);
    let result = run_scenario(entries);
    let c = result.candidate("Album");
    assert!(c
        .files
        .documents
        .iter()
        .all(|d| { !d.file_name.ends_with(".md5") && !d.file_name.ends_with(".ffp") }));
    assert!(c
        .files
        .artwork
        .iter()
        .all(|a| { !a.file_name.ends_with(".md5") && !a.file_name.ends_with(".ffp") }));
}

/// L1.36 — `.log` and `.m3u` surface as documents.
#[test]
fn log_m3u_attach_as_documents() {
    let mut entries = flat_audio("Album", 3, FileKind::Flac);
    entries.extend([
        FixtureEntry::File {
            rel_path: "Album/rip.log".into(),
            kind: FileKind::Log,
        },
        FixtureEntry::File {
            rel_path: "Album/playlist.m3u".into(),
            kind: FileKind::M3u,
        },
    ]);
    let result = run_scenario(entries);
    let docs: Vec<_> = result
        .candidate("Album")
        .files
        .documents
        .iter()
        .map(|d| d.file_name.as_str())
        .collect();
    for expected in ["rip.log", "playlist.m3u"] {
        assert!(docs.contains(&expected), "missing {expected} in {docs:?}");
    }
}

/// L1.37 — `.bae/` subdir is entirely hidden from the scanner.
#[test]
fn bae_sidecar_hidden_from_scanner() {
    let mut entries = flat_audio("Album", 3, FileKind::Flac);
    entries.push(FixtureEntry::File {
        rel_path: "Album/.bae/cover-mb.jpg".into(),
        kind: FileKind::Jpeg,
    });
    let result = run_scenario(entries);
    assert_eq!(result.top_level_paths(), vec!["Album"]);
    let c = result.candidate("Album");
    // Nothing under .bae/ should leak into artwork or documents.
    assert!(c
        .files
        .artwork
        .iter()
        .all(|a| !a.relative_path.contains(".bae/")));
    assert!(c
        .files
        .documents
        .iter()
        .all(|d| !d.relative_path.contains(".bae/")));
}

/// L1.38 — Cyrillic path components scan cleanly and the name is
/// preserved verbatim.
#[test]
fn cyrillic_path_component_scans_cleanly() {
    let result = run_scenario(flat_audio("Studio \u{0410}lbums/Album", 3, FileKind::Flac));
    assert_eq!(result.top_level_paths(), vec!["Studio \u{0410}lbums/Album"],);
}

/// L1.39 — Audio files at the root of a container alongside release
/// subdirectories are ignored. We don't model a release that contains
/// other releases, so if the subdirs are already releases the root audio
/// is treated as noise: the subdirs surface as top-level candidates and
/// the container itself does not.
#[test]
fn loose_audio_beside_release_subdirs_is_ignored() {
    let mut entries = flat_audio("Mixed", 2, FileKind::Flac);
    entries.extend(flat_audio("Mixed/Album A", 5, FileKind::Flac));
    entries.extend(flat_audio("Mixed/Album B", 5, FileKind::Flac));
    let result = run_scenario(entries);
    let top = result.top_level_paths();
    for expected in &["Mixed/Album A", "Mixed/Album B"] {
        assert!(
            top.iter().any(|p| p == expected),
            "missing {expected} in {top:?}"
        );
    }
    assert!(
        !top.iter().any(|p| p == "Mixed"),
        "Mixed leaked as top-level: {top:?}"
    );
}

/// L1.40 — Same rule as L1.39 but the release is nested under a chain
/// of non-audio wrapper dirs instead of sitting one level below the
/// container. The container has direct audio and no immediate subdir
/// with direct audio; the release must still surface and the container
/// must not leak. Depth 3 rules out any fix that only peeks a bounded
/// number of levels into `tree_has_subdirs_with_audio`.
#[test]
fn loose_audio_beside_deeply_nested_release_is_ignored() {
    let mut entries = flat_audio("Mixed", 2, FileKind::Flac);
    entries.extend(flat_audio("Mixed/Wrapper/Sub/Album", 5, FileKind::Flac));
    let result = run_scenario(entries);
    let top = result.top_level_paths();
    assert_eq!(
        top,
        vec!["Mixed/Wrapper/Sub/Album"],
        "only the nested release should surface"
    );
}

// ── Layer 2: combination tests ────────────────────────────────────────

// L2.1 is covered by `multi_disc_with_partial_disc_suppresses_whole_album`
// above. L2.3 and L2.4 are dropped (aggregate-CUE case dropped with G1).

/// L2.2 — A partial marker anywhere under a multi-disc album suppresses
/// the whole album. Parallel to
/// `multi_disc_with_partial_disc_suppresses_whole_album` above — both
/// incompleteness signals (zero-byte audio, partial-marker sidecar)
/// produce the same outcome: the whole release is suppressed, individual
/// discs do not surface on their own.
#[test]
fn multi_disc_with_partial_marker_in_one_disc_suppresses_whole_album() {
    let mut entries = flat_audio("Album/Disc 1", 3, FileKind::Flac);
    entries.push(FixtureEntry::File {
        rel_path: "Album/Disc 2/01.flac".into(),
        kind: FileKind::Flac,
    });
    entries.push(FixtureEntry::File {
        rel_path: "Album/Disc 2/02.flac.part".into(),
        kind: FileKind::PartialMarker("part"),
    });
    let result = run_scenario(entries);
    assert!(
        result.top_level_paths().is_empty(),
        "current scanner suppresses the whole album; got {:?}",
        result.top_level_paths(),
    );
}

/// L2.5 — Reissue container with mixed CUE+FLAC and per-track rips:
/// each child surfaces as its own top-level with its own format.
#[test]
fn reissue_container_with_mixed_cue_and_track_rips() {
    let mut entries = flat_audio("Album/1991 - Original", 3, FileKind::Flac);
    entries.push(FixtureEntry::File {
        rel_path: "Album/1997 - Reissue/Album.flac".into(),
        kind: FileKind::Flac,
    });
    entries.push(FixtureEntry::File {
        rel_path: "Album/1997 - Reissue/Album.cue".into(),
        kind: FileKind::CueFor {
            stem: "Album.flac",
            n_tracks: 10,
        },
    });
    let result = run_scenario(entries);
    let top = result.top_level_paths();
    assert_eq!(top.len(), 2);
    match &result.candidate("Album/1991 - Original").files.audio {
        AudioContent::TrackFiles { format_label, .. } => assert_eq!(format_label, "FLAC"),
        other => panic!("1991 should be TrackFiles, got {other:?}"),
    }
    match &result.candidate("Album/1997 - Reissue").files.audio {
        AudioContent::CueFlacPairs { format_label, .. } => {
            assert_eq!(format_label, "CUE+FLAC")
        }
        other => panic!("1997 should be CueFlacPairs, got {other:?}"),
    }
}

/// L2.6 — Multi-disc release with a booklet under one disc still emits
/// the parent; booklet attaches to that disc's artwork.
#[test]
fn multi_disc_with_booklet_in_one_disc_still_emits_parent() {
    let mut entries = flat_audio("Box/CD1", 3, FileKind::Flac);
    entries.extend(flat_audio("Box/CD2", 3, FileKind::Flac));
    entries.extend([
        FixtureEntry::File {
            rel_path: "Box/CD2/booklet/page1.png".into(),
            kind: FileKind::Png,
        },
        FixtureEntry::File {
            rel_path: "Box/CD2/booklet/page2.png".into(),
            kind: FileKind::Png,
        },
    ]);
    let result = run_scenario(entries);
    assert_eq!(result.top_level_paths(), vec!["Box"]);
    let parent = result.candidate("Box");
    assert!(
        parent
            .files
            .artwork
            .iter()
            .any(|a| a.relative_path.contains("CD2/booklet/")),
        "parent artwork should include CD2 booklet pages",
    );
}

/// Multi-disc release with artwork files directly at the parent level
/// (cover/back/inlay JPEGs next to the disc subdirs, not inside them).
/// Common real-world shape — parent JPEGs should attach as artwork on
/// the multi-disc candidate.
#[test]
fn multi_disc_with_parent_level_artwork_attaches_to_release() {
    let mut entries = flat_audio("Album/Disc 1", 3, FileKind::Flac);
    entries.extend(flat_audio("Album/Disc 2", 3, FileKind::Flac));
    entries.extend([
        FixtureEntry::File {
            rel_path: "Album/cover.jpg".into(),
            kind: FileKind::Jpeg,
        },
        FixtureEntry::File {
            rel_path: "Album/back.jpg".into(),
            kind: FileKind::Jpeg,
        },
        FixtureEntry::File {
            rel_path: "Album/inlay.jpg".into(),
            kind: FileKind::Jpeg,
        },
    ]);
    let result = run_scenario(entries);
    assert_eq!(result.top_level_paths(), vec!["Album"]);
    let parent = result.candidate("Album");
    let art_names: Vec<_> = parent
        .files
        .artwork
        .iter()
        .map(|a| a.file_name.as_str())
        .collect();
    for expected in ["cover.jpg", "back.jpg", "inlay.jpg"] {
        assert!(
            art_names.contains(&expected),
            "missing parent-level {expected} in artwork {art_names:?}",
        );
    }
}

/// L2.7 — Multi-disc with an Info/ subdir under one disc still emits
/// the parent and doesn't break disc-indicator matching.
#[test]
fn multi_disc_with_info_in_one_disc_still_emits_parent() {
    let mut entries = flat_audio("Album/Disc 1", 3, FileKind::Flac);
    entries.extend(flat_audio("Album/Disc 2", 3, FileKind::Flac));
    entries.push(FixtureEntry::File {
        rel_path: "Album/Disc 1/Info/notes.txt".into(),
        kind: FileKind::TracklistTxt,
    });
    let result = run_scenario(entries);
    assert_eq!(result.top_level_paths(), vec!["Album"]);
}

/// L2.8 — Partial marker nested under a release subdir (e.g. in
/// `booklet/`) still suppresses the whole release. This exercises the
/// deep walker check.
#[test]
fn partial_marker_in_nested_subdir_stops_multi_disc_candidate() {
    let mut entries = flat_audio("Album", 3, FileKind::Flac);
    entries.push(FixtureEntry::File {
        rel_path: "Album/booklet/02.flac.part".into(),
        kind: FileKind::PartialMarker("part"),
    });
    let result = run_scenario(entries);
    assert!(result.top_level_paths().is_empty());
}

/// L2.9 — One disc-indicator sibling plus one descriptive-name sibling
/// (no `Disc`/`CD`/`Part`/`Side` prefix) disqualifies the multi-disc
/// shape; both children surface.
#[test]
fn single_weird_sibling_prevents_multidisc_classification() {
    let mut entries = flat_audio("Album/Disc 1", 3, FileKind::Flac);
    entries.extend(flat_audio("Album/Bonus Tracks", 3, FileKind::Flac));
    let result = run_scenario(entries);
    let top = result.top_level_paths();
    assert_eq!(top.len(), 2);
    assert!(top.iter().any(|p| p == "Album/Disc 1"));
    assert!(top.iter().any(|p| p == "Album/Bonus Tracks"));
}

/// L2.11 — CUE lacking PERFORMER/TITLE is rejected by the nom parser,
/// but the scanner's line-based summary still extracts the declared
/// track count. When that count exceeds on-disk audio and the CUE
/// stem doesn't pair with any FLAC, the mismatch guard rejects the
/// candidate.
#[test]
fn cue_no_header_track_count_mismatch_rejects_release() {
    let mut entries = flat_audio("Album", 3, FileKind::Flac);
    entries.push(FixtureEntry::File {
        rel_path: "Album/Album.cue".into(),
        kind: FileKind::CueNoHeader {
            n_tracks: 15,
            file_reference: "Album.flac",
        },
    });
    let result = run_scenario(entries);
    assert!(result.top_level_paths().is_empty());
}

/// L2.12 — CUE with an unquoted FILE directive still pairs when it
/// shares a stem; when the track count exceeds on-disk audio, the
/// mismatch guard fires.
#[test]
fn cue_file_reference_with_unquoted_filename_still_parses() {
    // Stem-matched variant: pair detected even with unquoted FILE.
    let result = run_scenario(vec![
        FixtureEntry::File {
            rel_path: "Paired/Album.flac".into(),
            kind: FileKind::Flac,
        },
        FixtureEntry::File {
            rel_path: "Paired/Album.cue".into(),
            kind: FileKind::CueUnquoted {
                stem: "Album.flac",
                n_tracks: 6,
            },
        },
    ]);
    match &result.candidate("Paired").files.audio {
        AudioContent::CueFlacPairs { pairs, .. } => assert_eq!(pairs.len(), 1),
        other => panic!("expected CueFlacPairs, got {other:?}"),
    }

    // Non-pairing / over-declared variant: guard fires.
    let mut entries = flat_audio("Mismatch", 3, FileKind::Flac);
    entries.push(FixtureEntry::File {
        rel_path: "Mismatch/Album.cue".into(),
        kind: FileKind::CueUnquoted {
            stem: "Album.flac",
            n_tracks: 15,
        },
    });
    let result = run_scenario(entries);
    assert!(result.top_level_paths().is_empty());
}

// ── Layer 3: edge / adversarial cases ─────────────────────────────────

/// L3.1 — Zero-padded numeric suffixes (`CD01`, `CD02`) are accepted.
#[test]
fn disc_indicator_with_leading_zeros() {
    let mut entries = flat_audio("Album/CD01", 3, FileKind::Flac);
    entries.extend(flat_audio("Album/CD02", 3, FileKind::Flac));
    let result = run_scenario(entries);
    assert_eq!(result.top_level_paths(), vec!["Album"]);
}

/// L3.2 — Mixed prefixes (`Disc 1`, `CD 2`) still qualify as multi-disc
/// because both match `is_disc_indicator_name`.
#[test]
fn disc_indicator_mixed_prefixes_in_same_release() {
    let mut entries = flat_audio("Album/Disc 1", 3, FileKind::Flac);
    entries.extend(flat_audio("Album/CD 2", 3, FileKind::Flac));
    let result = run_scenario(entries);
    assert_eq!(result.top_level_paths(), vec!["Album"]);
}

/// L3.3 — Five-disc box set surfaces as one candidate whose audio list
/// covers all five discs (one `dir_prefix` per disc, 15 tracks total).
#[test]
fn five_disc_boxset_emits_single_candidate_covering_all_discs() {
    let mut entries = Vec::new();
    for i in 1..=5 {
        entries.extend(flat_audio(&format!("Boxset/Disc {i}"), 3, FileKind::Flac));
    }
    let result = run_scenario(entries);
    assert_eq!(result.top_level_paths(), vec!["Boxset"]);
    let c = result.candidate("Boxset");
    match &c.files.audio {
        AudioContent::TrackFiles { tracks, .. } => {
            assert_eq!(tracks.len(), 15, "5 discs × 3 tracks = 15 audio entries");
            let prefixes: BTreeSet<Option<&str>> =
                tracks.iter().map(|t| t.dir_prefix.as_deref()).collect();
            assert_eq!(
                prefixes.len(),
                5,
                "5 distinct disc prefixes, got {prefixes:?}"
            );
        }
        other => panic!("expected TrackFiles, got {other:?}"),
    }
}

/// L3.4 — Descriptive text after the disc number (`Disc 1 (CAT-001)`)
/// still matches the disc-indicator pattern — the digit run is
/// terminated by a non-alphanumeric character. One candidate whose
/// audio entries carry both disc prefixes.
#[test]
fn multi_disc_with_descriptive_disc_names_emits_parent() {
    let mut entries = flat_audio("Box/Disc 1 (CAT-001)", 3, FileKind::Flac);
    entries.extend(flat_audio("Box/Disc 2 (CAT-002)", 3, FileKind::Flac));
    let result = run_scenario(entries);
    assert_eq!(result.top_level_paths(), vec!["Box"]);
    let c = result.candidate("Box");
    let prefixes: BTreeSet<Option<&str>> = match &c.files.audio {
        AudioContent::TrackFiles { tracks, .. } => {
            tracks.iter().map(|t| t.dir_prefix.as_deref()).collect()
        }
        AudioContent::CueFlacPairs { pairs, .. } => pairs
            .iter()
            .map(|p| p.audio_file.dir_prefix.as_deref())
            .collect(),
    };
    assert_eq!(
        prefixes.len(),
        2,
        "2 distinct disc prefixes, got {prefixes:?}"
    );
}

/// L3.5 — A multi-FILE CUE referencing a missing audio file is an
/// incomplete rip and yields no candidate. The CUE's per-track FILEs
/// describe where audio lives; a missing FILE means the audio for
/// those tracks is unreachable.
#[test]
fn multi_file_cue_with_missing_secondary_file_is_incomplete() {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(&album).unwrap();
    std::fs::write(album.join("Album.flac"), bytes_for(FileKind::Flac)).unwrap();
    // Hand-write the CUE because the DSL doesn't model two-FILE sheets.
    std::fs::write(
            album.join("Album.cue"),
            "PERFORMER \"X\"\nTITLE \"Y\"\nFILE \"Album.flac\" WAVE\n  TRACK 01 AUDIO\n    INDEX 01 00:00:00\nFILE \"Missing.flac\" WAVE\n  TRACK 02 AUDIO\n    INDEX 01 05:00:00\n",
        )
        .unwrap();
    let candidates = scan_valid(tmp.path().to_path_buf());
    assert_eq!(
        candidates.len(),
        0,
        "multi-FILE CUE with a missing FILE reference is an incomplete rip"
    );
}

/// L3.6 — Folder with only a CUE and cover, no audio, yields nothing.
#[test]
fn folder_with_only_cue_and_no_audio() {
    let result = run_scenario(vec![
        FixtureEntry::File {
            rel_path: "Album/Album.cue".into(),
            kind: FileKind::CueFor {
                stem: "Album.flac",
                n_tracks: 5,
            },
        },
        FixtureEntry::File {
            rel_path: "Album/cover.jpg".into(),
            kind: FileKind::Jpeg,
        },
    ]);
    assert!(result.top_level_paths().is_empty());
}

/// L3.7 — A folder with audio plus `.avi` extras: audio candidate still
/// surfaces, `.avi` is ignored.
#[test]
fn folder_with_audio_and_video_mixed() {
    let mut entries = flat_audio("Album", 3, FileKind::Flac);
    entries.push(FixtureEntry::File {
        rel_path: "Album/bonus.avi".into(),
        kind: FileKind::Avi,
    });
    let result = run_scenario(entries);
    assert_eq!(result.top_level_paths(), vec!["Album"]);
    let c = result.candidate("Album");
    assert!(c
        .files
        .documents
        .iter()
        .all(|d| !d.file_name.ends_with(".avi")));
    assert!(c
        .files
        .artwork
        .iter()
        .all(|a| !a.file_name.ends_with(".avi")));
}

/// L3.8 — Deeply nested release under a chain of single-child wrappers
/// collapses to the leaf.
#[test]
fn deeply_nested_release_scan_root_two_levels_up() {
    let result = run_scenario(flat_audio("A/B/C/Release", 3, FileKind::Flac));
    let top = result.top_level_paths();
    assert_eq!(top, vec!["A/B/C/Release"]);
    assert_eq!(result.candidate(&top[0]).name, "Release");
}

/// L3.9 — Folder with unexpected file types (`.xyz`, `.sh`) alongside
/// audio: candidate surfaces, extras omitted from categorization.
#[test]
fn unexpected_file_types_silently_ignored() {
    let mut entries = flat_audio("Album", 3, FileKind::Flac);
    entries.extend([
        FixtureEntry::File {
            rel_path: "Album/weird.xyz".into(),
            kind: FileKind::UnrecognizedFile("xyz"),
        },
        FixtureEntry::File {
            rel_path: "Album/script.sh".into(),
            kind: FileKind::UnrecognizedFile("sh"),
        },
    ]);
    let result = run_scenario(entries);
    assert_eq!(result.top_level_paths(), vec!["Album"]);
    let c = result.candidate("Album");
    assert!(c
        .files
        .documents
        .iter()
        .all(|d| !d.file_name.ends_with(".xyz") && !d.file_name.ends_with(".sh")));
}

/// Zero-byte cover art is an incompleteness signal — the release is
/// suppressed for the same reason a multi-disc album with a zero-byte
/// FLAC is suppressed (any incompleteness signal drops the whole
/// release). Parallel to
/// `multi_disc_with_partial_disc_suppresses_whole_album` and
/// `zero_byte_cover_at_multi_disc_parent_suppresses_release`.
#[test]
fn zero_byte_cover_art_does_not_surface() {
    let mut entries = flat_audio("Album", 3, FileKind::Flac);
    entries.push(FixtureEntry::File {
        rel_path: "Album/cover.jpg".into(),
        kind: FileKind::ZeroByteJpeg,
    });
    let result = run_scenario(entries);
    assert!(result.top_level_paths().is_empty());
}

/// L3.11 — Zero-byte cover at the parent of a multi-disc release poisons
/// the merged categorize pass that scans the whole release tree. The
/// multi-disc parent therefore fails to surface even though both discs
/// contain valid audio. Symmetric with L3.10 for the flat shape.
#[test]
fn zero_byte_cover_at_multi_disc_parent_suppresses_release() {
    let mut entries = flat_audio("Album/Disc 1", 1, FileKind::Flac);
    entries.extend(flat_audio("Album/Disc 2", 1, FileKind::Flac));
    entries.push(FixtureEntry::File {
        rel_path: "Album/cover.jpg".into(),
        kind: FileKind::ZeroByteJpeg,
    });
    let result = run_scenario(entries);
    assert!(result.top_level_paths().is_empty());
}
