use super::*;
use std::path::PathBuf;

/// Resolve a checked-in test fixture by path relative to the crate root, so
/// tests don't depend on the process working directory.
fn fixture(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(rel)
}

fn assert_invalid_data(result: Result<String, MetadataDetectionError>) {
    let error = result.expect_err("out-of-range sector should return an error");
    match error {
        MetadataDetectionError::Io(error) => {
            assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        }
    }
}

fn write_log_with_toc(end_sector: i32) -> tempfile::NamedTempFile {
    let file = tempfile::NamedTempFile::new().expect("test LOG file should be created");
    let content = format!(
        "TOC of the extracted CD\n\
         \n\
         Track | Start | Length | Start sector | End sector\n\
         ---------------------------------------------------\n\
         1 | 0:00.00 | 1:00.00 | 0 | {end_sector}\n\
         \n\
         Range status and errors\n"
    );
    std::fs::write(file.path(), content).expect("test LOG file should be written");
    file
}

#[test]
fn discid_rejects_track_start_sector_that_overflows_pregap_offset() {
    assert_invalid_data(discid_from_raw_offsets(
        "LOG",
        &[i32::MAX],
        100,
        "from test lead-out",
    ));
}

#[test]
fn discid_rejects_leadout_sector_that_overflows_pregap_offset() {
    assert_invalid_data(discid_from_raw_offsets(
        "LOG",
        &[0],
        i32::MAX,
        "from test lead-out",
    ));
}

#[test]
fn discid_from_log_rejects_end_sector_that_overflows_leadout_derivation() {
    let log_file = write_log_with_toc(i32::MAX);

    assert_invalid_data(calculate_mb_discid_from_log(log_file.path()));
}

#[test]
fn test_extract_leadout_from_log() {
    let log_content = crate::text_encoding::read_text_file(&fixture("test_album.log"))
        .expect("LOG fixture should be readable")
        .text;
    let toc_sectors = extract_log_toc_sectors(&log_content).expect("LOG TOC should parse");
    let last_end_sector = toc_sectors
        .last()
        .expect("LOG TOC should include at least one track")
        .1;
    let raw_sector = last_end_sector + 1;
    let final_offset = raw_sector + 150;
    assert_eq!(
        final_offset, 188965,
        "Expected lead-out to be 188965 (188814 + 1 + 150)",
    );
    assert_eq!(
        raw_sector, 188815,
        "Expected raw lead-out sector to be 188815 (188814 + 1)",
    );
}

#[test]
fn test_calculate_mb_discid_from_log() {
    let discid = calculate_mb_discid_from_log(&fixture("test_album.log"))
        .expect("disc ID should compute from the LOG fixture");
    assert_eq!(discid.len(), 28, "DiscID should be 28 characters");
    assert!(
        discid
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
        "DiscID should contain only alphanumeric characters, dashes, and underscores",
    );
}

/// The shared duration-based disc ID path is container-agnostic: given a
/// CUE sheet and a container duration in seconds, the FLAC and probe
/// wrappers produce the same disc ID.
#[test]
fn test_cue_duration_discid_matches_across_codecs() {
    use crate::cue_flac::{CueIndex, CuePregap, CueSheet, CueTrack, CueTrackMode};

    // Three tracks, 75 CUE frames/sec → minute 0, minute 3, minute 6.
    let sheet = CueSheet {
        title: Some("Album Title".to_string()),
        performer: Some("Artist Name".to_string()),
        catalog: None,
        date: None,
        tracks: vec![
            CueTrack {
                number: 1,
                mode: CueTrackMode::Audio,
                title: Some("Track 01".to_string()),
                performer: None,
                indexes: vec![CueIndex {
                    number: 1,
                    frames: 0,
                    file_reference: "Album.flac".to_string(),
                }],
                file_reference: "Album.flac".to_string(),
                start_cue_frames: 0,
                pregap: CuePregap::None,
                end_cue_frames: Some(3 * 60 * 75),
            },
            CueTrack {
                number: 2,
                mode: CueTrackMode::Audio,
                title: Some("Track 02".to_string()),
                performer: None,
                indexes: vec![CueIndex {
                    number: 1,
                    frames: 3 * 60 * 75,
                    file_reference: "Album.flac".to_string(),
                }],
                file_reference: "Album.flac".to_string(),
                start_cue_frames: 3 * 60 * 75,
                pregap: CuePregap::None,
                end_cue_frames: Some(6 * 60 * 75),
            },
            CueTrack {
                number: 3,
                mode: CueTrackMode::Audio,
                title: Some("Track 03".to_string()),
                performer: None,
                indexes: vec![CueIndex {
                    number: 1,
                    frames: 6 * 60 * 75,
                    file_reference: "Album.flac".to_string(),
                }],
                file_reference: "Album.flac".to_string(),
                start_cue_frames: 6 * 60 * 75,
                pregap: CuePregap::None,
                end_cue_frames: None,
            },
        ],
    };

    let audio = [SheetAudioDuration {
        file_reference: "Album.flac",
        duration_ms: 9 * 60 * 1000,
    }];
    let id_a = calculate_mb_discid_from_cue(&sheet, &audio, "CUE/FLAC")
        .expect("disc ID from FLAC path should compute");
    let id_b = calculate_mb_discid_from_cue(&sheet, &audio, "CUE/probe")
        .expect("disc ID from probe path should compute");

    assert_eq!(
        id_a, id_b,
        "duration-based disc ID is codec-agnostic: same sheet + same duration \
         must produce the same disc ID regardless of which method label is logged"
    );
    assert_eq!(id_a.len(), 28, "MusicBrainz disc IDs are 28 chars");
}

#[test]
fn cue_duration_discid_ignores_non_audio_tracks() {
    use crate::cue_flac::{CueIndex, CuePregap, CueSheet, CueTrack, CueTrackMode};

    fn track(number: u32, mode: CueTrackMode, start_cue_frames: u64) -> CueTrack {
        CueTrack {
            number,
            mode,
            title: Some(format!("Track {number:02}")),
            performer: None,
            indexes: vec![CueIndex {
                number: 1,
                frames: start_cue_frames,
                file_reference: "disc-image.bin".to_string(),
            }],
            file_reference: "disc-image.bin".to_string(),
            start_cue_frames,
            pregap: CuePregap::None,
            end_cue_frames: None,
        }
    }

    let audio_tracks = vec![
        track(1, CueTrackMode::Audio, 0),
        track(2, CueTrackMode::Audio, 3 * 60 * 75),
    ];
    let audio_only = CueSheet {
        title: Some("Album Title".to_string()),
        performer: Some("Artist Name".to_string()),
        catalog: None,
        date: None,
        tracks: audio_tracks.clone(),
    };
    let with_data_track = CueSheet {
        title: Some("Album Title".to_string()),
        performer: Some("Artist Name".to_string()),
        catalog: None,
        date: None,
        tracks: audio_tracks
            .into_iter()
            .chain(std::iter::once(track(
                3,
                CueTrackMode::Other("MODE1/2352".to_string()),
                6 * 60 * 75,
            )))
            .collect(),
    };

    let audio = [SheetAudioDuration {
        file_reference: "disc-image.bin",
        duration_ms: 6 * 60 * 1000,
    }];
    let audio_only_id = calculate_mb_discid_from_cue(&audio_only, &audio, "CUE/probe")
        .expect("audio-only CUE should compute a disc ID");
    let with_data_track_id = calculate_mb_discid_from_cue(&with_data_track, &audio, "CUE/probe")
        .expect("CUE with a data track should compute a disc ID");

    assert_eq!(with_data_track_id, audio_only_id);
}

/// One disc, ripped two ways: a single image with tracks at 0, 3 and 6
/// minutes, and one file per track of 3 minutes each. The one-per-track
/// rip lays its files end to end, so both describe the same TOC.
#[test]
fn multi_file_cue_lays_its_files_end_to_end() {
    let image = cue_sheet_with(vec![
        audio_cue_track_at(1, "Album.flac", 0),
        audio_cue_track_at(2, "Album.flac", 3 * 60 * 75),
        audio_cue_track_at(3, "Album.flac", 6 * 60 * 75),
    ]);
    let image_id = calculate_mb_discid_from_cue(
        &image,
        &[SheetAudioDuration {
            file_reference: "Album.flac",
            duration_ms: 9 * 60 * 1000,
        }],
        "image",
    )
    .expect("the image computes");

    let per_track = cue_sheet_with(vec![
        audio_cue_track(1, "01.flac"),
        audio_cue_track(2, "02.flac"),
        audio_cue_track(3, "03.flac"),
    ]);
    let per_track_id = calculate_mb_discid_from_cue(
        &per_track,
        &["01.flac", "02.flac", "03.flac"].map(|file_reference| SheetAudioDuration {
            file_reference,
            duration_ms: 3 * 60 * 1000,
        }),
        "per track",
    )
    .expect("the one-per-track rip computes");

    assert_eq!(per_track_id, image_id);
}

/// A `PREGAP` directive generates silence that is on the disc and in no
/// file: the tracks after it, and the lead-out, sit that much later.
#[test]
fn generated_pregap_shifts_every_later_offset() {
    let with_gap_on_disc = cue_sheet_with(vec![
        audio_cue_track_at(1, "Album.flac", 0),
        audio_cue_track_at(2, "Album.flac", 3 * 60 * 75 + 150),
    ]);
    let expected = calculate_mb_discid_from_cue(
        &with_gap_on_disc,
        &[SheetAudioDuration {
            file_reference: "Album.flac",
            duration_ms: 6 * 60 * 1000 + 2_000,
        }],
        "image with gap",
    )
    .expect("the image computes");

    let mut second = audio_cue_track(2, "02.flac");
    second.pregap = crate::cue_flac::CuePregap::Silence { frames: 150 };
    let gap_left_out = cue_sheet_with(vec![audio_cue_track(1, "01.flac"), second]);
    let computed = calculate_mb_discid_from_cue(
        &gap_left_out,
        &["01.flac", "02.flac"].map(|file_reference| SheetAudioDuration {
            file_reference,
            duration_ms: 3 * 60 * 1000,
        }),
        "gap left out",
    )
    .expect("the gap-less rip computes");

    assert_eq!(computed, expected);
}

/// A pregap ripped onto the tail of the previous file (INDEX 00 in the
/// file before, INDEX 01 at the head of the track's own) is already
/// inside that file's length, so it adds nothing of its own.
#[test]
fn audio_pregap_in_the_previous_file_is_already_laid() {
    let mut second = audio_cue_track(2, "02.flac");
    second.pregap = crate::cue_flac::CuePregap::Audio(crate::cue_flac::CueIndex {
        number: 0,
        frames: 3 * 60 * 75 - 150,
        file_reference: "01.flac".to_string(),
    });
    let gaps_appended = cue_sheet_with(vec![audio_cue_track(1, "01.flac"), second]);
    let durations = ["01.flac", "02.flac"].map(|file_reference| SheetAudioDuration {
        file_reference,
        duration_ms: 3 * 60 * 1000,
    });
    let computed = calculate_mb_discid_from_cue(&gaps_appended, &durations, "gaps appended")
        .expect("the gaps-appended rip computes");

    let plain = cue_sheet_with(vec![
        audio_cue_track(1, "01.flac"),
        audio_cue_track(2, "02.flac"),
    ]);
    let expected =
        calculate_mb_discid_from_cue(&plain, &durations, "plain").expect("the plain rip computes");
    assert_eq!(computed, expected);
}

/// A sheet naming a file whose length nobody measured cannot be laid out.
#[test]
fn a_file_without_a_measured_length_is_an_error() {
    let sheet = cue_sheet_with(vec![
        audio_cue_track(1, "01.flac"),
        audio_cue_track(2, "02.flac"),
    ]);
    assert_invalid_data(calculate_mb_discid_from_cue(
        &sheet,
        &[SheetAudioDuration {
            file_reference: "01.flac",
            duration_ms: 3 * 60 * 1000,
        }],
        "half measured",
    ));
}

#[test]
fn test_cue_duration_discid_empty_tracks_is_error() {
    use crate::cue_flac::CueSheet;

    let sheet = CueSheet {
        title: None,
        performer: None,
        catalog: None,
        date: None,
        tracks: vec![],
    };

    let result = calculate_mb_discid_from_cue(&sheet, &[], "CUE/probe");
    assert!(result.is_err(), "empty track list must return an error");
}

/// A single-FILE rip with `.cue` + `.ape` produces a disc ID — the
/// dispatcher routes APE through the FFmpeg-probe path.
#[test]
fn test_compute_discid_routes_cue_ape() {
    use tempfile::TempDir;
    let fixture_dir = fixture("cue_ape");
    let tmp = TempDir::new().unwrap();
    let folder = tmp.path();
    std::fs::copy(
        fixture_dir.join("Test Album.ape"),
        folder.join("Test Album.ape"),
    )
    .unwrap();
    std::fs::copy(
        fixture_dir.join("Test Album.cue"),
        folder.join("Test Album.cue"),
    )
    .unwrap();

    let categorized = crate::import::folder_scanner::collect_release_candidate_files_with_scope(
        folder,
        crate::import::ReleaseFileScope::Recursive,
        &crate::import::folder_scanner::StoredCandidateEdits::none(),
    )
    .unwrap();
    let audio_path = folder.join("Test Album.ape");
    let opens_before = crate::audio_codec::probe_opens_for(&audio_path);
    let computed =
        compute_discid_from_categorized(&categorized).expect("CUE+APE pair must compute a disc ID");
    assert_eq!(
        crate::audio_codec::probe_opens_for(&audio_path),
        opens_before,
        "DiscID must reuse the duration retained by the folder scan"
    );
    assert_eq!(
        computed.disc_id.len(),
        28,
        "MusicBrainz disc IDs are 28 chars"
    );
    assert!(
        computed.source_file.ends_with(".cue"),
        "the sheet it was carved from rides with it, got {:?}",
        computed.source_file
    );
}

/// A single-FILE rip with `.cue` + `.mp3` produces a disc ID — the dispatcher
/// routes MP3 through the FFmpeg-probe path.
///
/// Drives `compute_discid_from_paths` with constructed paths rather than
/// `compute_discid_from_categorized`, which would go through the folder
/// scanner's CUE+audio pair detection — a separate concern for MP3.
#[test]
fn test_compute_discid_routes_cue_mp3() {
    use tempfile::TempDir;

    crate::audio_codec::init();
    let tmp = TempDir::new().unwrap();
    let folder = tmp.path();
    let mp3_path = folder.join("Test Album.mp3");
    // 9s silent stereo MP3 — short enough to keep the test fast, long
    // enough to span the CUE's three 3-second tracks.
    let samples = vec![0i32; 44_100 * 9 * 2];
    let mp3_bytes = crate::audio_codec::encode_i32(
        crate::audio_codec::EncodeFormat::Mp3 { bitrate_kbps: 320 },
        &samples,
        44_100,
        2,
    )
    .expect("encode mp3");
    std::fs::write(&mp3_path, mp3_bytes).unwrap();

    let cue_body = "PERFORMER \"Artist Name\"\n\
                    TITLE \"Album Title\"\n\
                    FILE \"Test Album.mp3\" WAVE\n  \
                    TRACK 01 AUDIO\n    INDEX 01 00:00:00\n  \
                    TRACK 02 AUDIO\n    INDEX 01 00:03:00\n  \
                    TRACK 03 AUDIO\n    INDEX 01 00:06:00\n";
    let cue_path = folder.join("Test Album.cue");
    std::fs::write(&cue_path, cue_body).unwrap();

    let disc_id = compute_discid_from_paths(&[], &[cue_path], &[(mp3_path, 9_000)])
        .expect("CUE+MP3 pair must compute a disc ID");
    assert_eq!(disc_id.len(), 28, "MusicBrainz disc IDs are 28 chars");
}

#[test]
fn parse_log_toc_row_accepts_valid_and_rejects_malformed() {
    // A well-formed EAC/XLD TOC row: track | start | length | start | end.
    assert_eq!(
        parse_log_toc_row("   1 | 0:00.00 | 1:00.00 |      0 |  188814"),
        Some((0, 188814))
    );
    // Fewer than five pipe-separated columns.
    assert_eq!(parse_log_toc_row("1 | 0:00.00 | 1:00.00 | 0"), None);
    // Track number outside 1..=99.
    assert_eq!(parse_log_toc_row("0 | 0:00.00 | 1:00.00 | 0 | 100"), None);
    assert_eq!(parse_log_toc_row("100 | 0:00.00 | 1:00.00 | 0 | 100"), None);
    // Non-numeric track / sector cells.
    assert_eq!(parse_log_toc_row("x | 0:00.00 | 1:00.00 | 0 | 100"), None);
    assert_eq!(parse_log_toc_row("1 | 0:00.00 | 1:00.00 | a | 100"), None);
    // Negative start sector, or non-positive end sector.
    assert_eq!(parse_log_toc_row("1 | 0:00.00 | 1:00.00 | -1 | 100"), None);
    assert_eq!(parse_log_toc_row("1 | 0:00.00 | 1:00.00 | 0 | 0"), None);
}

#[test]
fn extract_log_toc_sectors_errors_without_toc_rows() {
    let log = "Some header line\nRandom prose\nNo TOC table anywhere\n";
    let err = extract_log_toc_sectors(log).expect_err("a LOG with no TOC rows must error");
    match err {
        MetadataDetectionError::Io(e) => {
            assert_eq!(e.kind(), std::io::ErrorKind::InvalidData);
        }
    }
}

#[test]
fn extract_log_toc_sectors_parses_headerless_table() {
    // A bare TOC table with no "TOC of the extracted CD" header still parses:
    // the first parseable row opens the section.
    let log = "   1 | 0:00.00 | 1:00.00 | 0 | 100\n   2 | 1:00.00 | 1:00.00 | 100 | 200\n";
    let rows = extract_log_toc_sectors(log).expect("headerless TOC table should parse");
    assert_eq!(rows, vec![(0, 100), (100, 200)]);
}

#[test]
fn extract_log_toc_sectors_skips_unparseable_rows() {
    // A header, a malformed row (track 0), then a valid row: the bad row is
    // dropped rather than aborting the parse.
    let log = "TOC of the extracted CD\n\
               Track | Start | Length | Start sector | End sector\n\
               ---------------------------------------------------\n\
               0 | 0:00.00 | 1:00.00 | 0 | 100\n\
               1 | 0:00.00 | 1:00.00 | 10 | 200\n\
               \n\
               Range status and errors\n";
    let rows = extract_log_toc_sectors(log).expect("the valid row should parse");
    assert_eq!(rows, vec![(10, 200)], "the invalid track-0 row is dropped");
}

fn audio_cue_track(number: u32, file_reference: &str) -> crate::cue_flac::CueTrack {
    audio_cue_track_at(number, file_reference, 0)
}

fn audio_cue_track_at(
    number: u32,
    file_reference: &str,
    start_cue_frames: u64,
) -> crate::cue_flac::CueTrack {
    use crate::cue_flac::{CueIndex, CuePregap, CueTrack, CueTrackMode};
    CueTrack {
        number,
        mode: CueTrackMode::Audio,
        title: Some(format!("Track {number:02}")),
        performer: None,
        indexes: vec![CueIndex {
            number: 1,
            frames: start_cue_frames,
            file_reference: file_reference.to_string(),
        }],
        file_reference: file_reference.to_string(),
        start_cue_frames,
        pregap: CuePregap::None,
        end_cue_frames: None,
    }
}

fn cue_sheet_with(tracks: Vec<crate::cue_flac::CueTrack>) -> CueSheet {
    CueSheet {
        title: Some("Album Title".to_string()),
        performer: Some("Artist Name".to_string()),
        catalog: None,
        date: None,
        tracks,
    }
}

/// A single-FILE CUE matches the unique same-stem audio beside the sheet.
#[test]
fn resolve_cue_audio_paths_matches_by_stem() {
    let sheet = cue_sheet_with(vec![
        audio_cue_track(1, "Album Image.flac"),
        audio_cue_track(2, "Album Image.flac"),
    ]);
    let audio_files = vec![
        PathBuf::from("/rip/Some Other.mp3"),
        PathBuf::from("/rip/Album Image.flac"),
    ];

    let matched = resolve_cue_audio_paths(Path::new("/rip/Album.cue"), &sheet, &audio_files)
        .expect("the unique same-stem audio should resolve");
    assert_eq!(matched[0].1, &PathBuf::from("/rip/Album Image.flac"));
}

/// No audio file shares the FILE reference's stem → no match.
#[test]
fn resolve_cue_audio_paths_no_stem_match_is_none() {
    let sheet = cue_sheet_with(vec![audio_cue_track(1, "Album Image.flac")]);
    let audio_files = vec![PathBuf::from("/rip/Different Name.flac")];

    assert!(resolve_cue_audio_paths(Path::new("/rip/Album.cue"), &sheet, &audio_files).is_none());
}

#[test]
fn resolve_cue_audio_paths_ambiguous_same_stem_is_none() {
    let sheet = cue_sheet_with(vec![audio_cue_track(1, "Album Image.wav")]);
    let audio_files = vec![
        PathBuf::from("/rip/Album Image.flac"),
        PathBuf::from("/rip/Album Image.ape"),
    ];

    assert!(resolve_cue_audio_paths(Path::new("/rip/Album.cue"), &sheet, &audio_files).is_none());
}

#[test]
fn resolve_cue_audio_paths_exact_path_wins_over_same_stem_audio() {
    let sheet = cue_sheet_with(vec![audio_cue_track(1, "Album Image.flac")]);
    let audio_files = vec![
        PathBuf::from("/rip/Album Image.ape"),
        PathBuf::from("/rip/Album Image.flac"),
    ];

    assert_eq!(
        resolve_cue_audio_paths(Path::new("/rip/Album.cue"), &sheet, &audio_files)
            .expect("the exact path should resolve")[0]
            .1,
        &PathBuf::from("/rip/Album Image.flac"),
    );
}

#[test]
fn resolve_cue_audio_paths_other_directory_is_none() {
    let sheet = cue_sheet_with(vec![audio_cue_track(1, "Album Image.wav")]);
    let audio_files = vec![PathBuf::from("/rip/audio/Album Image.flac")];

    assert!(resolve_cue_audio_paths(Path::new("/rip/Album.cue"), &sheet, &audio_files).is_none());
}

#[test]
/// Same-stem fallback follows the reference's directory, not only the
/// directory containing the CUE.
fn resolve_cue_audio_paths_matches_by_stem_in_referenced_subdirectory() {
    let sheet = cue_sheet_with(vec![audio_cue_track(1, "audio/Album Image.wav")]);
    let audio_files = vec![PathBuf::from("/rip/audio/Album Image.flac")];

    let matched = resolve_cue_audio_paths(Path::new("/rip/Album.cue"), &sheet, &audio_files)
        .expect("same-stem audio beside the referenced path should resolve");
    assert_eq!(matched[0].1, &PathBuf::from("/rip/audio/Album Image.flac"));
}

/// Every reference in a multi-FILE CUE resolves independently.
#[test]
fn resolve_cue_audio_paths_resolves_multiple_files() {
    let sheet = cue_sheet_with(vec![
        audio_cue_track(1, "Track 01.flac"),
        audio_cue_track(2, "Track 02.flac"),
    ]);
    let audio_files = vec![
        PathBuf::from("/rip/Track 01.flac"),
        PathBuf::from("/rip/Track 02.flac"),
    ];

    let matched = resolve_cue_audio_paths(Path::new("/rip/Album.cue"), &sheet, &audio_files)
        .expect("both referenced audio files should resolve");
    assert_eq!(
        matched
            .into_iter()
            .map(|(_, path)| path.as_path())
            .collect::<Vec<_>>(),
        vec![
            Path::new("/rip/Track 01.flac"),
            Path::new("/rip/Track 02.flac"),
        ]
    );
}
