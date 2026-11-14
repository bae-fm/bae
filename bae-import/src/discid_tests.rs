use super::*;
use crate::cue_sheet::tests::*;
use crate::tests::fixture;
use std::path::Path;

/// The disc ID one log file hashes to, decoded the way the extraction pass
/// decodes it.
fn discid_from_log_file(path: &Path) -> Result<String, std::io::Error> {
    discid_from_log_text(
        &bae_util::text_encoding::read_text_file(path, bae_util::test_files::place())?.text,
    )?
    .ok_or_else(|| std::io::Error::other("the log holds no table of contents"))
}

fn assert_invalid_data(result: Result<String, std::io::Error>) {
    let error = result.expect_err("out-of-range sector should return an error");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
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
    assert_invalid_data(discid_from_raw_offsets("LOG", &[i32::MAX], 100, "from test lead-out"));
}

#[test]
fn discid_rejects_leadout_sector_that_overflows_pregap_offset() {
    assert_invalid_data(discid_from_raw_offsets("LOG", &[0], i32::MAX, "from test lead-out"));
}

#[test]
fn discid_from_log_rejects_end_sector_that_overflows_leadout_derivation() {
    let log_file = write_log_with_toc(i32::MAX);

    assert_invalid_data(discid_from_log_file(log_file.path()));
}

#[test]
fn test_extract_leadout_from_log() {
    let log_content = bae_util::text_encoding::read_text_file(
        &fixture("logs/test_album.log"),
        bae_util::test_files::place(),
    )
    .expect("LOG fixture should be readable")
    .text;
    let toc_sectors = extract_log_toc_sectors(&log_content)
        .expect("LOG TOC should parse")
        .expect("LOG should hold a TOC");
    let last_end_sector = toc_sectors.last().expect("LOG TOC should include at least one track").1;
    let raw_sector = last_end_sector + 1;
    let final_offset = raw_sector + 150;
    assert_eq!(final_offset, 188965, "Expected lead-out to be 188965 (188814 + 1 + 150)",);
    assert_eq!(raw_sector, 188815, "Expected raw lead-out sector to be 188815 (188814 + 1)",);
}

#[test]
fn test_discid_from_log_text() {
    let discid = discid_from_log_file(&fixture("logs/test_album.log"))
        .expect("disc ID should compute from the LOG fixture");
    assert_eq!(discid.len(), 28, "DiscID should be 28 characters");
    assert!(
        discid.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
        "DiscID should contain only alphanumeric characters, dashes, and underscores",
    );
}

/// The shared duration-based disc ID path is container-agnostic: given a
/// CUE sheet and a container duration in seconds, the FLAC and probe
/// wrappers produce the same disc ID.
#[test]
fn test_cue_duration_discid_matches_across_codecs() {
    use crate::cue_sheet::{CueIndex, CueSheet, CueTrack, CueTrackMode};

    // Three tracks, 75 CUE frames/sec → minute 0, minute 3, minute 6.
    let sheet = CueSheet {
        title: Some("Album Title".to_string()),
        performer: Some("Artist Name".to_string()),
        catalog: None,
        date: None,
        ripper: None,
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
                generated_pregap_frames: None,
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
                generated_pregap_frames: None,
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
                generated_pregap_frames: None,
                end_cue_frames: None,
            },
        ],
    };

    let audio = [SheetAudioDuration { file_reference: "Album.flac", duration_ms: 9 * 60 * 1000 }];
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
    use crate::cue_sheet::{CueIndex, CueSheet, CueTrack, CueTrackMode};

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
            generated_pregap_frames: None,
            end_cue_frames: None,
        }
    }

    let audio_tracks =
        vec![track(1, CueTrackMode::Audio, 0), track(2, CueTrackMode::Audio, 3 * 60 * 75)];
    let audio_only = CueSheet {
        title: Some("Album Title".to_string()),
        performer: Some("Artist Name".to_string()),
        catalog: None,
        date: None,
        ripper: None,
        tracks: audio_tracks.clone(),
    };
    let with_data_track = CueSheet {
        title: Some("Album Title".to_string()),
        performer: Some("Artist Name".to_string()),
        catalog: None,
        date: None,
        ripper: None,
        tracks: audio_tracks
            .into_iter()
            .chain(std::iter::once(track(
                3,
                CueTrackMode::Other("MODE1/2352".to_string()),
                6 * 60 * 75,
            )))
            .collect(),
    };

    let audio =
        [SheetAudioDuration { file_reference: "disc-image.bin", duration_ms: 6 * 60 * 1000 }];
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
    let image = cue_sheet(
        "Album Title",
        vec![
            audio_cue_track_at(1, "Album.flac", 0),
            audio_cue_track_at(2, "Album.flac", 3 * 60 * 75),
            audio_cue_track_at(3, "Album.flac", 6 * 60 * 75),
        ],
    );
    let image_id = calculate_mb_discid_from_cue(
        &image,
        &[SheetAudioDuration { file_reference: "Album.flac", duration_ms: 9 * 60 * 1000 }],
        "image",
    )
    .expect("the image computes");

    let per_track = cue_sheet(
        "Album Title",
        vec![
            audio_cue_track(1, "01.flac"),
            audio_cue_track(2, "02.flac"),
            audio_cue_track(3, "03.flac"),
        ],
    );
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
    let with_gap_on_disc = cue_sheet(
        "Album Title",
        vec![
            audio_cue_track_at(1, "Album.flac", 0),
            audio_cue_track_at(2, "Album.flac", 3 * 60 * 75 + 150),
        ],
    );
    let expected = calculate_mb_discid_from_cue(
        &with_gap_on_disc,
        &[SheetAudioDuration { file_reference: "Album.flac", duration_ms: 6 * 60 * 1000 + 2_000 }],
        "image with gap",
    )
    .expect("the image computes");

    let mut second = audio_cue_track(2, "02.flac");
    second.generated_pregap_frames = Some(150);
    let gap_left_out = cue_sheet("Album Title", vec![audio_cue_track(1, "01.flac"), second]);
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
    second.indexes.insert(
        0,
        crate::cue_sheet::CueIndex {
            number: 0,
            frames: 3 * 60 * 75 - 150,
            file_reference: "01.flac".to_string(),
        },
    );
    let gaps_appended = cue_sheet("Album Title", vec![audio_cue_track(1, "01.flac"), second]);
    let durations = ["01.flac", "02.flac"]
        .map(|file_reference| SheetAudioDuration { file_reference, duration_ms: 3 * 60 * 1000 });
    let computed = calculate_mb_discid_from_cue(&gaps_appended, &durations, "gaps appended")
        .expect("the gaps-appended rip computes");

    let plain = cue_sheet(
        "Album Title",
        vec![audio_cue_track(1, "01.flac"), audio_cue_track(2, "02.flac")],
    );
    let expected =
        calculate_mb_discid_from_cue(&plain, &durations, "plain").expect("the plain rip computes");
    assert_eq!(computed, expected);
}

/// A sheet naming a file whose length nobody measured cannot be laid out.
#[test]
fn a_file_without_a_measured_length_is_an_error() {
    let sheet = cue_sheet(
        "Album Title",
        vec![audio_cue_track(1, "01.flac"), audio_cue_track(2, "02.flac")],
    );
    assert_invalid_data(calculate_mb_discid_from_cue(
        &sheet,
        &[SheetAudioDuration { file_reference: "01.flac", duration_ms: 3 * 60 * 1000 }],
        "half measured",
    ));
}

#[test]
fn test_cue_duration_discid_empty_tracks_is_error() {
    use crate::cue_sheet::CueSheet;

    let sheet = CueSheet {
        title: None,
        performer: None,
        catalog: None,
        date: None,
        ripper: None,
        tracks: vec![],
    };

    let result = calculate_mb_discid_from_cue(&sheet, &[], "CUE/probe");
    assert!(result.is_err(), "empty track list must return an error");
}

#[test]
fn parse_log_toc_row_accepts_valid_and_rejects_malformed() {
    // A well-formed EAC/XLD TOC row: track | start | length | start | end.
    assert_eq!(
        parse_log_toc_row("   1 | 0:00.00 | 1:00.00 |      0 |  188814"),
        Some((1, 0, 188814))
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
fn extract_log_toc_sectors_finds_no_disc_without_toc_rows() {
    let log = "Some header line\nRandom prose\nNo TOC table anywhere\n";
    assert_eq!(
        extract_log_toc_sectors(log).unwrap(),
        None,
        "a LOG with no TOC rows states no disc"
    );
    assert_eq!(discid_from_log_text(log).unwrap(), None);
}

#[test]
fn extract_log_toc_sectors_parses_headerless_table() {
    // A bare TOC table with no "TOC of the extracted CD" header still parses:
    // the first parseable row opens the section.
    let log = "   1 | 0:00.00 | 1:00.00 | 0 | 100\n   2 | 1:00.00 | 1:00.00 | 100 | 200\n";
    let rows = extract_log_toc_sectors(log)
        .expect("headerless TOC table should parse")
        .expect("the table holds rows");
    assert_eq!(rows, vec![(0, 100), (100, 200)]);
}

/// A row that does not read, or a gap in the track numbers, fails the log: a
/// skipped row hashes a disc one track short, to a DiscID that looks valid.
#[test]
fn extract_log_toc_sectors_fails_on_a_damaged_row_or_a_gap_in_track_numbers() {
    let toc = |rows: &str| {
        format!(
            "TOC of the extracted CD\n\
             Track | Start | Length | Start sector | End sector\n\
             ---------------------------------------------------\n\
             {rows}\n\
             \n\
             Range status and errors\n"
        )
    };
    let whole = toc("1 | 0:00.00 | 1:00.00 | 0 | 99\n2 | 1:00.00 | 1:00.00 | 100 | 199");
    assert_eq!(extract_log_toc_sectors(&whole).unwrap(), Some(vec![(0, 99), (100, 199)]));

    // A middle row with an unreadable sector, a last row with an unreadable
    // sector, and a first row with an unreadable track number.
    for damaged in [
        "1 | 0:00.00 | 1:00.00 | 0 | 99\n2 | 1:00.00 | 1:00.00 | 1O0 | 199\n3 | 2:00.00 | 1:00.00 | 200 | 299",
        "1 | 0:00.00 | 1:00.00 | 0 | 99\n2 | 1:00.00 | 1:00.00 | 100 | 1x9",
        "x | 0:00.00 | 1:00.00 | 0 | 99\n2 | 1:00.00 | 1:00.00 | 100 | 199",
    ] {
        let error = extract_log_toc_sectors(&toc(damaged)).expect_err(damaged);
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData, "{damaged}");
    }
    // Track numbers that skip, repeat or start late.
    for numbered in [
        "1 | 0:00.00 | 1:00.00 | 0 | 99\n3 | 1:00.00 | 1:00.00 | 100 | 199",
        "1 | 0:00.00 | 1:00.00 | 0 | 99\n1 | 1:00.00 | 1:00.00 | 100 | 199",
        "2 | 0:00.00 | 1:00.00 | 0 | 99",
    ] {
        let error = extract_log_toc_sectors(&toc(numbered)).expect_err(numbered);
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData, "{numbered}");
    }
}
