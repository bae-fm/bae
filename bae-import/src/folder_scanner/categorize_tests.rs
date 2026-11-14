//! What a folder's files make of it: each file's role, the files a scan
//! skips or that keep a folder from being a release, a folder still
//! downloading, and the files a folder that no release owns carries beside
//! its releases.

use super::*;
use crate::folder_scanner::tests::*;
use std::sync::Arc;

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
            assert_eq!(invalid.reason(), InvalidReason::DamagedAudio { count: 1 });
            assert!(
                invalid.files.iter().any(|file| file.relative_path == "album.ape"
                    && file.role == ProposedRole::DamagedAudio { damage: AudioDamage::Unreadable }),
                "the unreadable audio is marked as such: {:?}",
                invalid.files,
            );
        }
        ScanItem::Valid(_) => panic!("corrupt-audio folder must not be a valid candidate"),
        ScanItem::Discovered(_) => {
            panic!("terminal scan helper returned a progress item")
        }
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
    std::fs::write(root.join("back.png"), [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A])
        .unwrap();

    let candidates = scan_valid(root);

    assert_eq!(candidates.len(), 0, "Folder with only images should not be detected");
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

    assert_eq!(candidates.len(), 0, "VIDEO_TS folder (DVD rip) should not be detected");
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

    assert_eq!(candidates.len(), 0, "Folder with only 0-byte FLAC files should not be detected");
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

    assert_eq!(candidates.len(), 0, "Candidate with corrupt files should be skipped entirely");
}

/// A folder whose only defect is a damaged image is its audio: it is found,
/// not skipped, and the damaged image is carried as one — never its artwork,
/// so never its cover — while the image that decodes is.
#[test]
fn a_damaged_image_leaves_the_release_found_without_it_as_cover() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Bad Front");
    std::fs::create_dir(&root).unwrap();

    std::fs::write(root.join("track.flac"), fake_flac()).unwrap();
    // Zero-filled, the way an interrupted copy leaves it.
    std::fs::write(root.join("front.png"), [0u8; 512]).unwrap();
    std::fs::write(root.join("back.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();

    let items = scan_items(root);
    let [ScanItem::Valid(candidate)] = items.as_slice() else {
        panic!("the folder is one found release, not a skipped one: {items:?}");
    };
    let roles: Vec<_> = candidate
        .files
        .files
        .iter()
        .map(|entry| (entry.file.relative_path.as_str(), &entry.role))
        .collect();
    assert!(roles.contains(&("front.png", &FileRole::DamagedImage)), "{roles:?}");
    assert!(
        candidate.files.artwork().all(|file| file.relative_path != "front.png"),
        "a damaged image is no artwork"
    );
    assert!(
        candidate.files.cover_files().all(|entry| entry.file.relative_path != "front.png"),
        "a damaged image is offered to no cover picker"
    );
    assert_eq!(
        crate::cover_art::folder_cover(None, candidate.files.artwork()),
        Some(crate::cover_art::PreparedCover::Local("back.jpg".to_string())),
        "the folder's cover is the image that decodes, though the damaged one is named front"
    );
}

/// Per-track FLACs plus a sheet naming absent audio: the sheet is a proposal,
/// not the layout, so it stays unbound and the twelve tracks import.
#[test]
fn per_track_flacs_with_missing_cue_audio_still_import() {
    let tmp = tempfile::TempDir::new().unwrap();
    let album =
        tmp.path().join("Collection").join("Artist - Album Title - (1991) {Label CAT-12345}");
    std::fs::create_dir_all(&album).unwrap();

    // 12 per-track FLACs
    for i in 1..=12 {
        std::fs::write(album.join(format!("{:02} - Track {i}.flac", i)), fake_flac()).unwrap();
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
    std::fs::write(artwork.join("disc.png"), [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A])
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
                &SheetBinding::Unresolved { files: Vec::new() },
                "the sheet names absent audio",
            );
            assert_eq!(candidate.files.track_count(), 12);
        }
        ScanItem::Invalid(invalid) => {
            panic!("a sheet naming absent audio must not invalidate: {}", invalid.reason())
        }
        ScanItem::Discovered(_) => {
            panic!("terminal scan helper returned a progress item")
        }
    }
}

/// A CUE paired with an `.m4a` file records a CUE-backed ALAC descriptor
/// because the scan probes the actual codec instead of trusting the extension.
#[test]
fn test_collect_release_candidate_files_cue_alac_source_audio() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path();

    let stem = "Artist Name - Album Title";
    let m4a_name = format!("{stem}.m4a");
    let cue_name = format!("{stem}.cue");

    std::fs::write(root.join(&m4a_name), fake_m4a()).unwrap();
    std::fs::write(root.join(&cue_name), make_cue_content_n_tracks(&m4a_name, "Album Title", 8))
        .unwrap();

    let files = scan_files(root);

    assert_uniform_source_audio(&files, bae_records::source_audio::SourceAudioLayout::Cue, "ALAC");
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

    let files = scan_files(root);

    let bound = files.bound_sheets();
    assert_eq!(bound.len(), 1);
    assert_eq!(
        bound[0].audio_files.iter().map(|(_, audio)| audio.file_name.as_str()).collect::<Vec<_>>(),
        vec!["01 - Track One.flac", "02 - Track Two.flac"],
    );
    assert_eq!(
        files.audio().map(|file| file.file_name.as_str()).collect::<Vec<_>>(),
        vec!["01 - Track One.flac", "02 - Track Two.flac"],
    );
    assert_eq!(bound[0].sheet.catalog.as_deref(), Some("0123456789012"));
    assert_eq!(bound[0].sheet.tracks.len(), 2);
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
    std::fs::write(root.join(&cue_name), make_cue_content_n_tracks(&ape_name, "Test Album", 15))
        .unwrap();

    let files = scan_files(root);

    assert_uniform_source_audio(&files, bae_records::source_audio::SourceAudioLayout::Cue, "APE");
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

/// A folder whose audio is damaged is read through before it is judged: every
/// file is listed with what the scan made of it, each damaged track with why,
/// and what its playable tracks' tags call the release — what the Skipped tab
/// shows of it.
#[test]
fn a_skipped_folder_lists_every_damaged_track_and_what_its_tags_say() {
    use lofty::config::WriteOptions;
    use lofty::prelude::*;
    use lofty::tag::{Tag, TagType};

    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("Half Copied");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("01.flac"), fake_flac()).unwrap();
    let mut tagged = lofty::read_from_path(root.join("01.flac")).unwrap();
    let mut tag = Tag::new(TagType::VorbisComments);
    tag.set_album("Album Title".to_string());
    tag.set_artist("Track Artist".to_string());
    tag.insert_text(ItemKey::AlbumArtist, "Artist Name".to_string());
    tagged.insert_tag(tag);
    tagged.save_to_path(root.join("01.flac"), WriteOptions::default()).unwrap();
    // Zero-filled, the way an interrupted copy leaves it.
    std::fs::write(root.join("02.flac"), vec![0u8; 4096]).unwrap();
    std::fs::write(root.join("03.flac"), b"").unwrap();
    std::fs::write(root.join("front.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();

    let items = scan_items(root);
    let [ScanItem::Invalid(invalid)] = items.as_slice() else {
        panic!("the folder is skipped: {items:?}");
    };
    assert_eq!(invalid.reason(), InvalidReason::DamagedAudio { count: 2 });
    assert_eq!(
        invalid
            .files
            .iter()
            .map(|file| (file.relative_path.as_str(), file.role))
            .collect::<Vec<_>>(),
        [
            ("01.flac", ProposedRole::Audio),
            ("02.flac", ProposedRole::DamagedAudio { damage: AudioDamage::Unreadable }),
            ("03.flac", ProposedRole::DamagedAudio { damage: AudioDamage::Empty }),
            ("front.jpg", ProposedRole::Image),
        ]
    );
    assert_eq!(
        invalid.tags,
        Some(SkippedTags {
            album_title: "Album Title".to_string(),
            artist: Some(bae_types::credits::CreditLine::of_name("Artist Name")),
        })
    );
    assert_eq!(invalid.cover().map(|file| file.relative_path.as_str()), Some("front.jpg"));
}

/// A skipped folder's tags name the release as a draft read from them would:
/// the title its playable tracks agree on, a disc marker taken off, and none
/// where they title it differently.
#[test]
fn a_skipped_folder_names_the_album_its_tracks_agree_on() {
    use lofty::config::WriteOptions;
    use lofty::prelude::*;
    use lofty::tag::{Tag, TagType};

    let skipped_tags = |albums: [&str; 2]| {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path().join("Half Copied");
        std::fs::create_dir(&root).unwrap();
        for (name, album) in ["01.flac", "02.flac"].into_iter().zip(albums) {
            std::fs::write(root.join(name), fake_flac()).unwrap();
            let mut tagged = lofty::read_from_path(root.join(name)).unwrap();
            let mut tag = Tag::new(TagType::VorbisComments);
            tag.set_album(album.to_string());
            tag.insert_text(ItemKey::AlbumArtist, "Artist Name".to_string());
            tagged.insert_tag(tag);
            tagged.save_to_path(root.join(name), WriteOptions::default()).unwrap();
        }
        std::fs::write(root.join("03.flac"), b"").unwrap();
        let items = scan_items(&root);
        let [ScanItem::Invalid(invalid)] = items.as_slice() else {
            panic!("the folder is skipped: {items:?}");
        };
        invalid.tags.clone()
    };

    assert_eq!(
        skipped_tags(["Album Title (Disc 1)", "Album Title (Disc 2)"]),
        Some(SkippedTags {
            album_title: "Album Title".to_string(),
            artist: Some(bae_types::credits::CreditLine::of_name("Artist Name")),
        })
    );
    assert_eq!(skipped_tags(["Album Alpha", "Album Beta"]), None);
}

/// A folder's files are in bae's one natural order, the order the library
/// lists a release's files in: names alike but for case or spaces, which
/// natural order alone counts as equal, fall in byte order, not in the order
/// the folder happened to give them.
#[test]
fn files_alike_but_for_case_or_spaces_sort_as_the_library_sorts_them() {
    let names = ["cover.jpg", "Cover.jpg", "Track1.flac", "Track 1.flac"];
    let proposed: Vec<(ScannedFile, ProposedRole)> = names
        .iter()
        .map(|name| {
            let file = ScannedFile::new(Path::new("/Album").join(name), name.to_string(), 1, 1);
            (file, ProposedRole::Other)
        })
        .collect();

    let listed: Vec<String> =
        skipped_files(&proposed).into_iter().map(|file| file.relative_path).collect();

    let mut shared = names.map(str::to_string);
    shared.sort_by(|left, right| bae_util::natural_order::compare(left, right));
    assert_eq!(listed, shared);
    assert_eq!(listed, ["Cover.jpg", "cover.jpg", "Track 1.flac", "Track1.flac"]);
}

/// A release folder with a real FLAC and a zero-byte FLAC must be
/// rejected. Mixing valid and zero-byte audio poisons the candidate.
#[test]
fn zero_byte_audio_in_release_skips_candidate() {
    let result = run_scenario(vec![
        file("Album/01.flac", FileKind::Flac),
        file("Album/02.flac", FileKind::ZeroByteFlac),
    ]);
    assert!(result.top_level_paths().is_empty());
}

/// A `.flac.part` sidecar next to a real FLAC suppresses the release.
#[test]
fn partial_marker_sidecar_skips_release() {
    let result = run_scenario(vec![
        file("Album/01.flac", FileKind::Flac),
        file("Album/02.flac.part", FileKind::PartialMarker("part")),
    ]);
    assert!(result.top_level_paths().is_empty());
}

/// A folder holding only partial markers (no real audio) must
/// also yield no candidates. Today this passes because no audio is
/// detected at all; the test pins the intent so if marker-only becomes
/// "looks like audio" we notice.
#[test]
fn partial_marker_only_no_real_audio_skips() {
    let result = run_scenario(vec![file("Album/01.flac.part", FileKind::PartialMarker("part"))]);
    assert!(result.top_level_paths().is_empty());
}

/// An I/O fault while validating an audio file (here: the file vanished
/// after the tree was built) is a system error, not "corrupt" — it must
/// surface, not be collapsed to `false` and silently drop the whole
/// release while mis-logging the cause as corruption. The Ok(false)
/// corruption path (covered elsewhere) still skips the candidate.
#[test]
fn io_error_validating_audio_surfaces_not_swallowed() {
    let tree = CandidateFileIndex::new(vec![FileEntry {
        path: PathBuf::from("Album/01.flac"),
        size: 1024,
        modified_at_ns: 1,
    }]);
    // fs_root is an empty dir, so Album/01.flac does not exist on disk: the
    // audio probe's open fails with a genuine I/O error.
    let temp = tempfile::TempDir::new().unwrap();
    let result = categorize_files_from_tree(
        &tree,
        &PathBuf::from("Album"),
        temp.path(),
        &StoredCandidateEdits::none(),
        &[],
        &ScanCancellation::new(),
        &FileReads::new(
            Arc::new(crate::file_tag_reader::LoftyFileTagReader),
            bae_store::FileLeases::for_test_default(),
        ),
    );
    assert!(result.is_err(), "an I/O fault during validation must surface as an error");
}

/// Every supported partial-marker extension suppresses the release.
/// Table-driven: one subtest per extension.
#[test]
fn each_partial_marker_extension_skips_release() {
    for ext in ["part", "crdownload", "download", "aria2", "partial"] {
        let result = run_scenario(vec![
            file("Album/01.flac", FileKind::Flac),
            file(format!("Album/02.flac.{ext}"), FileKind::PartialMarker(ext)),
        ]);
        assert!(
            result.top_level_paths().is_empty(),
            "extension .{ext} should suppress the candidate",
        );
    }
}

/// Partial-marker extension matching is case-insensitive.
#[test]
fn partial_marker_extension_case_insensitive() {
    for name in ["02.FLAC.PART", "03.FLAC.CRDownload"] {
        let ext = name.rsplit('.').next().unwrap();
        let result = run_scenario(vec![
            file("Album/01.flac", FileKind::Flac),
            file(format!("Album/{name}"), FileKind::PartialMarker(ext)),
        ]);
        assert!(result.top_level_paths().is_empty(), "marker {name} should suppress the candidate",);
    }
}

/// A folder whose only audio is a FLAC the validator rejects — malformed
/// STREAMINFO length, or wrong magic bytes — surfaces no valid candidate.
#[test]
fn invalid_flac_audio_yields_no_candidate() {
    for kind in [FileKind::MalformedFlacStreaminfo, FileKind::BrokenFlac] {
        let result = run_scenario(vec![file("Album/01.flac", kind)]);
        assert!(result.top_level_paths().is_empty(), "{kind:?} must reject the candidate",);
    }
}

// --- Within-release intricacies (C-series) ---

/// Per-track FLACs surface as TrackFiles / "FLAC".
#[test]
fn flat_flac_release_surfaces_as_trackfiles() {
    let result = run_scenario(flat_audio("Album", 3, FileKind::Flac));
    let c = result.candidate("Album");
    assert_uniform_source_audio(
        &c.files,
        bae_records::source_audio::SourceAudioLayout::File,
        "FLAC",
    );
    assert_eq!(c.files.audio().count(), 3);
    assert!(c.files.track_sheets().next().is_none());
}

// The CUE/APE pair is covered by
// `test_collect_release_candidate_files_cue_ape_track_count` above; not
// duplicated here.

/// MP3 tracks surface as TrackFiles / "MP3".
#[test]
fn mp3_release_surfaces_as_mp3_trackfiles() {
    let result = run_scenario(flat_audio("Album", 3, FileKind::Mp3));
    let c = result.candidate("Album");
    assert_uniform_source_audio(
        &c.files,
        bae_records::source_audio::SourceAudioLayout::File,
        "MP3",
    );
}

/// M4A tracks surface as physical files with their probed codec.
#[test]
fn m4a_release_surfaces_as_trackfiles() {
    let result = run_scenario(flat_audio("Album", 3, FileKind::M4a));
    let c = result.candidate("Album");
    assert_uniform_source_audio(
        &c.files,
        bae_records::source_audio::SourceAudioLayout::File,
        "ALAC",
    );
    assert_eq!(c.files.audio().count(), 3);
}

/// A multi-FILE CUE resolves as a CUE-backed release; each referenced track
/// file remains attached as an ordered source for that layout.
#[test]
fn multi_file_cue_surfaces_as_cue_backed_release() {
    let (tmp, album) = album_dir();
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
    assert_uniform_source_audio(
        &c.files,
        bae_records::source_audio::SourceAudioLayout::Cue,
        "ALAC",
    );
    assert_eq!(c.files.bound_sheets().len(), 1);
    assert_eq!(
        c.files.audio().map(|file| file.file_name.as_str()).collect::<Vec<_>>(),
        vec!["01.m4a", "02.m4a", "03.m4a"],
    );
    // A sheet is a sheet, never a document.
    assert!(!c.files.documents().any(|d| d.file_name == "Album.cue"));
}

/// `.md5` / `.ffp` sidecars are neither audio nor artwork nor
/// documents; they are omitted from categorization.
#[test]
fn md5_ffp_sidecars_silently_ignored() {
    let mut entries = flat_audio("Album", 3, FileKind::Flac);
    entries.extend([
        file("Album/checksums.md5", FileKind::Md5),
        file("Album/checksums.ffp", FileKind::Ffp),
    ]);
    let result = run_scenario(entries);
    let c = result.candidate("Album");
    assert!(c
        .files
        .documents()
        .all(|d| { !d.file_name.ends_with(".md5") && !d.file_name.ends_with(".ffp") }));
    assert!(c
        .files
        .artwork()
        .all(|a| { !a.file_name.ends_with(".md5") && !a.file_name.ends_with(".ffp") }));
}

/// `.log` and `.m3u` surface as documents.
#[test]
fn log_m3u_attach_as_documents() {
    let mut entries = flat_audio("Album", 3, FileKind::Flac);
    entries
        .extend([file("Album/rip.log", FileKind::Log), file("Album/playlist.m3u", FileKind::M3u)]);
    let result = run_scenario(entries);
    let docs: Vec<_> =
        result.candidate("Album").files.documents().map(|d| d.file_name.as_str()).collect();
    for expected in ["rip.log", "playlist.m3u"] {
        assert!(docs.contains(&expected), "missing {expected} in {docs:?}");
    }
}

/// A sheet that will not parse is a document, not a verdict: its folder still
/// imports, and so does its sibling.
#[test]
fn unparseable_cue_lands_as_a_document_and_siblings_still_scan() {
    let tmp = tempfile::tempdir().unwrap();
    let bad = tmp.path().join("Bad Album");
    let good = tmp.path().join("Good Album");
    std::fs::create_dir_all(&bad).unwrap();
    std::fs::create_dir_all(&good).unwrap();
    std::fs::write(bad.join("Album.flac"), bytes_for(FileKind::Flac)).unwrap();
    std::fs::write(
        bad.join("Album.cue"),
        "FILE \"Album.flac\" WAVE\n  TRACK 01 AUDIO\n    TITLE \"Missing Index\"\n",
    )
    .unwrap();
    std::fs::write(good.join("01.flac"), bytes_for(FileKind::Flac)).unwrap();

    let items = scan_items(tmp.path().to_path_buf());
    assert_eq!(items.len(), 2);
    let bad = items
        .iter()
        .find_map(|item| match item {
            ScanItem::Valid(candidate) if candidate.name == "Bad Album" => Some(candidate),
            _ => None,
        })
        .expect("the folder with the unparseable sheet still imports");
    assert!(
        bad.files.documents().any(|d| d.file_name == "Album.cue"),
        "an unparseable sheet stays a document",
    );
    assert!(bad.files.track_sheets().next().is_none());
    assert_eq!(bad.files.audio().count(), 1);
    assert!(items.iter().any(|item| matches!(
        item,
        ScanItem::Valid(candidate) if candidate.name == "Good Album"
    )));
}

/// A folder with only a CUE and cover, no audio, yields nothing.
#[test]
fn folder_with_only_cue_and_no_audio() {
    let result = run_scenario(vec![
        file("Album/Album.cue", FileKind::CueFor { stem: "Album.flac", n_tracks: 5 }),
        file("Album/cover.jpg", FileKind::Jpeg),
    ]);
    assert!(result.top_level_paths().is_empty());
}

/// A folder with audio plus `.avi` extras still produces an audio candidate.
/// surfaces, `.avi` is ignored.
#[test]
fn folder_with_audio_and_video_mixed() {
    let mut entries = flat_audio("Album", 3, FileKind::Flac);
    entries.push(file("Album/bonus.avi", FileKind::Avi));
    let result = run_scenario(entries);
    assert_eq!(result.top_level_paths(), vec!["Album"]);
    let c = result.candidate("Album");
    assert!(c.files.documents().all(|d| !d.file_name.ends_with(".avi")));
    assert!(c.files.artwork().all(|a| !a.file_name.ends_with(".avi")));
}

/// A folder with unexpected file types (`.xyz`, `.sh`) alongside
/// audio: candidate surfaces, extras omitted from categorization.
#[test]
fn unexpected_file_types_silently_ignored() {
    let mut entries = flat_audio("Album", 3, FileKind::Flac);
    entries.extend([
        file("Album/weird.xyz", FileKind::UnrecognizedFile("xyz")),
        file("Album/script.sh", FileKind::UnrecognizedFile("sh")),
    ]);
    let result = run_scenario(entries);
    assert_eq!(result.top_level_paths(), vec!["Album"]);
    let c = result.candidate("Album");
    assert!(c
        .files
        .documents()
        .all(|d| !d.file_name.ends_with(".xyz") && !d.file_name.ends_with(".sh")));
    // They are still listed, under the role for what the scan doesn't
    // recognize — and the release carries them like everything else.
    let other: Vec<_> = c
        .files
        .files
        .iter()
        .filter(|entry| matches!(entry.role, FileRole::Other))
        .map(|entry| entry.file.file_name.as_str())
        .collect();
    assert_eq!(other, vec!["script.sh", "weird.xyz"]);
    assert!(
        c.files.release_files().any(|f| f.file_name == "weird.xyz"),
        "the folder is the release: an unrecognized file is carried, not dropped",
    );
}

/// Zero-byte cover art is a damaged image, not a damaged release: the release
/// surfaces, carrying the image as damaged and never as its artwork.
#[test]
fn zero_byte_cover_art_is_carried_as_a_damaged_image() {
    let mut entries = flat_audio("Album", 3, FileKind::Flac);
    entries.push(file("Album/cover.jpg", FileKind::ZeroByteJpeg));
    let result = run_scenario(entries);
    assert_eq!(result.top_level_paths(), ["Album"]);
    let album = result.candidate("Album");
    assert!(album.files.files.iter().any(|entry| {
        entry.file.relative_path == "cover.jpg" && entry.role == FileRole::DamagedImage
    }));
    assert_eq!(album.files.artwork().count(), 0);
}

/// A sheet that will not parse is not a sheet: it lands as a document, and its
/// audio keeps its role.
#[test]
fn unparseable_sheet_lands_as_a_document() {
    let (tmp, album) = album_dir();
    std::fs::write(album.join("Album.flac"), fake_flac()).unwrap();
    // No INDEX: the sheet does not parse.
    std::fs::write(
        album.join("Album.cue"),
        "FILE \"Album.flac\" WAVE\n  TRACK 01 AUDIO\n    TITLE \"Missing Index\"\n",
    )
    .unwrap();

    let items = scan_items(tmp.path().to_path_buf());
    assert_eq!(items.len(), 1);
    match &items[0] {
        ScanItem::Valid(candidate) => {
            assert!(
                candidate.files.documents().any(|d| d.file_name == "Album.cue"),
                "an unparseable sheet stays a document",
            );
            assert!(candidate.files.track_sheets().next().is_none());
            assert_eq!(candidate.files.audio().count(), 1);
        }
        ScanItem::Invalid(invalid) => {
            panic!("an unparseable sheet must not invalidate: {}", invalid.reason())
        }
        ScanItem::Discovered(_) => {
            panic!("terminal scan helper returned a progress item")
        }
    }
}

/// Widening the model must not swallow real defects: audio that will not decode
/// and a folder with no audio at all still invalidate.
#[test]
fn corrupt_audio_and_empty_folders_still_invalidate() {
    let tmp = tempfile::tempdir().unwrap();
    let corrupt = tmp.path().join("Corrupt");
    std::fs::create_dir_all(&corrupt).unwrap();
    // Non-empty so the folder is still detected as a leaf, but the bytes are
    // not FLAC — the file will not decode.
    std::fs::write(corrupt.join("01.flac"), b"not a flac at all").unwrap();
    let items = scan_items(corrupt);
    assert_eq!(items.len(), 1);
    assert!(
        matches!(
            &items[0],
            ScanItem::Invalid(invalid)
                if invalid.reason() == InvalidReason::DamagedAudio { count: 1 }
        ),
        "corrupt audio is still a real defect",
    );

    let empty = tmp.path().join("Empty");
    std::fs::create_dir_all(&empty).unwrap();
    std::fs::write(empty.join("cover.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();
    assert!(scan_items(empty.clone()).is_empty(), "a folder with no audio has nothing to import",);
    assert!(
        matches!(
            collect_release_candidate_files_with_scope(
                &empty,
                crate::ReleaseFileScope::Recursive,
                &StoredCandidateEdits::none()
            ),
            Err(crate::ImportError::InvalidFolder(InvalidReason::NoValidAudio))
        ),
        "categorizing an audio-less folder names NoValidAudio",
    );
}
