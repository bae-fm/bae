use super::*;
#[test]
fn test_parse_time() {
    // "MM:SS:FF" -> total CUE frames (75 frames per second).
    let cases = [
        ("03:45:12", (3 * 60 + 45) * 75 + 12),
        ("00:00:00", 0),
        ("60:35:00", (60 * 60 + 35) * 75),
    ];
    for (input, expected) in cases {
        let (_, cue_frames) = CueFlacProcessor::parse_time(input).unwrap();
        assert_eq!(cue_frames, expected, "input: {input}");
    }
}
#[test]
fn parse_multi_file_cue_stamps_per_track_file_reference() {
    // One FILE per TRACK, the spec shape used by lossy-format rips
    // (rippers don't concatenate per-track files to avoid re-encoding).
    let cue_content = r#"PERFORMER "Artist Name"
TITLE "Album Title"
FILE "01 - Track One.m4a" WAVE
  TRACK 01 AUDIO
    TITLE "Track One"
    PERFORMER "Artist Name"
    INDEX 01 00:00:00
FILE "02 - Track Two.m4a" WAVE
  TRACK 02 AUDIO
    TITLE "Track Two"
    INDEX 01 00:00:00
FILE "03 - Track Three.m4a" WAVE
  TRACK 03 AUDIO
    TITLE "Track Three"
    INDEX 01 00:00:00
"#;
    let (_, sheet) = CueFlacProcessor::parse_cue_content(cue_content).unwrap();
    assert_eq!(sheet.tracks.len(), 3);
    assert_eq!(sheet.tracks[0].file_reference, "01 - Track One.m4a");
    assert_eq!(sheet.tracks[1].file_reference, "02 - Track Two.m4a");
    assert_eq!(sheet.tracks[2].file_reference, "03 - Track Three.m4a");
    assert!(sheet.single_file().is_none());
}

#[test]
fn parse_single_file_cue_stamps_same_reference_on_every_track() {
    let cue_content = r#"PERFORMER "Artist Name"
TITLE "Album Title"
FILE "Album.flac" WAVE
  TRACK 01 AUDIO
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    INDEX 01 03:45:00
  TRACK 03 AUDIO
    INDEX 01 07:30:00
"#;
    let (_, sheet) = CueFlacProcessor::parse_cue_content(cue_content).unwrap();
    assert_eq!(sheet.tracks.len(), 3);
    for track in &sheet.tracks {
        assert_eq!(track.file_reference, "Album.flac");
    }
    assert_eq!(sheet.single_file(), Some("Album.flac"));
}

#[test]
fn parse_multi_file_cue_splits_at_file_inside_track_body() {
    // PERFORMER/TITLE/INDEX sit inside the track body; the FILE that
    // starts the next group terminates the current track.
    let cue_content = r#"PERFORMER "Artist Name"
TITLE "Album Title"
FILE "01.m4a" WAVE
  TRACK 01 AUDIO
    TITLE "Track One"
    PERFORMER "Artist Name"
    INDEX 01 00:00:00
FILE "02.m4a" WAVE
  TRACK 02 AUDIO
    TITLE "Track Two"
    PERFORMER "Artist Name"
    INDEX 01 00:00:00
"#;
    let (_, sheet) = CueFlacProcessor::parse_cue_content(cue_content).unwrap();
    assert_eq!(sheet.tracks.len(), 2);
    assert_eq!(sheet.tracks[0].title.as_deref(), Some("Track One"));
    assert_eq!(sheet.tracks[0].file_reference, "01.m4a");
    assert_eq!(sheet.tracks[1].title.as_deref(), Some("Track Two"));
    assert_eq!(sheet.tracks[1].file_reference, "02.m4a");
}

#[test]
fn parse_per_track_rip_with_cross_file_pregap_preserves_both_files() {
    // Track 2's INDEX 00 is in file 1 and INDEX 01 is in file 2. The parsed
    // layout keeps both indexes with their FILE scope so resolution can build
    // an audio-pregap segment followed by the main segment.
    let cue_content = r#"TITLE "Album Title"
PERFORMER "Artist Name"
FILE "01.wav" WAVE
  TRACK 01 AUDIO
    TITLE "Track One"
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE "Track Two"
    INDEX 00 02:55:33
FILE "02.wav" WAVE
    INDEX 01 00:00:00
  TRACK 03 AUDIO
    TITLE "Track Three"
    INDEX 00 02:17:46
FILE "03.wav" WAVE
    INDEX 01 00:00:00
"#;
    let (_, sheet) = CueFlacProcessor::parse_cue_content(cue_content).unwrap();
    assert_eq!(sheet.tracks.len(), 3);
    let track2 = &sheet.tracks[1];
    assert_eq!(track2.file_reference, "02.wav");
    assert_eq!(track2.index(0).unwrap().file_reference, "01.wav");
    assert_eq!(track2.index(1).unwrap().file_reference, "02.wav");
    assert_eq!(track2.pregap_cue_frames, Some((2 * 60 + 55) * 75 + 33));
}

#[test]
fn parse_second_track_without_index_01_propagates_failure() {
    // When a track body lacks INDEX 01, the body-loop's error arm must
    // propagate `Err::Failure` instead of treating it as a loop
    // terminator — otherwise the parse returns `Ok` with whatever
    // tracks accumulated so far and the remainder is silently dropped.
    let cue_content = r#"PERFORMER "Artist Name"
TITLE "Album Title"
FILE "Album.flac" WAVE
  TRACK 01 AUDIO
    TITLE "Track One"
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE "Track Two"
"#;
    let err = CueFlacProcessor::parse_cue_content(cue_content).unwrap_err();
    assert!(
        matches!(err, nom::Err::Failure(_)),
        "expected Failure when a track body lacks INDEX 01, got {err:?}",
    );
}

#[test]
fn parse_cue_content_rejects_track_before_any_file() {
    // A TRACK with no FILE above it has no audio file to bind to. The
    // empty file_reference would silently mislead downstream pair
    // detection; reject at parse time.
    let cue_content = r#"PERFORMER "Artist Name"
TITLE "Album Title"
  TRACK 01 AUDIO
    TITLE "Track One"
    INDEX 01 00:00:00
"#;
    let err = CueFlacProcessor::parse_cue_content(cue_content).unwrap_err();
    assert!(
        matches!(err, nom::Err::Failure(_)),
        "expected Failure for TRACK before FILE, got {err:?}",
    );
}

#[test]
fn test_parse_simple_cue_sheet() {
    let cue_content = r#"PERFORMER "Test Artist"
TITLE "Test Album"
FILE "test.flac" WAVE
  TRACK 01 AUDIO
    TITLE "Track 1"
    PERFORMER "Test Artist"
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE "Track 2"
    PERFORMER "Test Artist"
    INDEX 01 03:45:00
"#;
    let result = CueFlacProcessor::parse_cue_content(cue_content);
    assert!(result.is_ok());
    let (_, cue_sheet) = result.unwrap();
    assert_eq!(cue_sheet.title.as_deref(), Some("Test Album"));
    assert_eq!(cue_sheet.performer.as_deref(), Some("Test Artist"));
    assert_eq!(cue_sheet.tracks.len(), 2);
    assert_eq!(cue_sheet.tracks[0].title.as_deref(), Some("Track 1"));
    assert_eq!(cue_sheet.tracks[0].start_time_ms(), 0);
    assert_eq!(cue_sheet.tracks[1].title.as_deref(), Some("Track 2"));
    assert_eq!(
        cue_sheet.tracks[1].start_time_ms(),
        3 * 60 * 1000 + 45 * 1000
    );
}
#[test]
fn test_parse_cue_sheet_with_catalog_line() {
    let cue_content = r#"REM GENRE Rock
REM DATE 1989
REM DISCID 8F09030C
REM COMMENT "ExactAudioCopy v1.0b3"
CATALOG 0000000000000
PERFORMER "Artist Name"
TITLE "Album Title"
FILE "Artist Name - Album Title.flac" WAVE
  TRACK 01 AUDIO
    TITLE "Track One"
    PERFORMER "Artist Name"
    INDEX 00 00:00:00
    INDEX 01 00:00:37
  TRACK 02 AUDIO
    TITLE "Track Two"
    PERFORMER "Artist Name"
    INDEX 00 03:39:16
    INDEX 01 03:40:62
  TRACK 03 AUDIO
    TITLE "Track Three"
    PERFORMER "Artist Name"
    INDEX 00 05:44:73
    INDEX 01 05:46:50
"#;
    let result = CueFlacProcessor::parse_cue_content(cue_content);
    assert!(
        result.is_ok(),
        "Failed to parse CUE with CATALOG line: {:?}",
        result.err()
    );
    let (_, cue_sheet) = result.unwrap();
    assert_eq!(cue_sheet.title.as_deref(), Some("Album Title"));
    assert_eq!(cue_sheet.performer.as_deref(), Some("Artist Name"));
    assert_eq!(cue_sheet.tracks.len(), 3);
    assert_eq!(cue_sheet.tracks[0].title.as_deref(), Some("Track One"));
    // Track 1 has pregap at 00:00:00, INDEX 01 at 00:00:37
    assert_eq!(cue_sheet.tracks[0].pregap_cue_frames, Some(0));
    assert_eq!(cue_sheet.tracks[0].start_cue_frames, 37);
    // Track 2 has pregap
    assert!(cue_sheet.tracks[1].pregap_cue_frames.is_some());
}

#[test]
fn test_parse_cue_sheet_with_comments() {
    let cue_content = r#"REM GENRE "Genre Name"
REM DATE 2000 / 2004
REM COMMENT "Vinyl Rip by User Name"
PERFORMER "Artist Name"
TITLE "Album Title"
FILE "Artist Name - Album Title.flac" WAVE
  TRACK 01 AUDIO
    TITLE "Track One"
    PERFORMER "Artist Name"
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE "Track Two"
    PERFORMER "Artist Name"
    INDEX 01 03:04:00
"#;
    let result = CueFlacProcessor::parse_cue_content(cue_content);
    assert!(result.is_ok());
    let (_, cue_sheet) = result.unwrap();
    assert_eq!(cue_sheet.title.as_deref(), Some("Album Title"));
    assert_eq!(cue_sheet.performer.as_deref(), Some("Artist Name"));
    assert_eq!(cue_sheet.tracks.len(), 2);
    assert_eq!(cue_sheet.tracks[0].title.as_deref(), Some("Track One"));
    assert_eq!(cue_sheet.tracks[1].title.as_deref(), Some("Track Two"));
}
#[test]
fn test_parse_cue_sheet_with_windows_line_endings() {
    let cue_content = "REM GENRE \"Genre Name\"\r\nPERFORMER \"Test Artist\"\r\nTITLE \"Test Album\"\r\nFILE \"test.flac\" WAVE\r\n  TRACK 01 AUDIO\r\n    TITLE \"Track 1\"\r\n    PERFORMER \"Test Artist\"\r\n    INDEX 01 00:00:00\r\n";
    let result = CueFlacProcessor::parse_cue_content(cue_content);
    assert!(result.is_ok());
    let (_, cue_sheet) = result.unwrap();
    assert_eq!(cue_sheet.title.as_deref(), Some("Test Album"));
    assert_eq!(cue_sheet.performer.as_deref(), Some("Test Artist"));
    assert_eq!(cue_sheet.tracks.len(), 1);
}
#[test]
fn test_parse_cue_sheet_calculates_end_times() {
    let cue_content = r#"PERFORMER "Test Artist"
TITLE "Test Album"
FILE "test.flac" WAVE
  TRACK 01 AUDIO
    TITLE "Track 1"
    PERFORMER "Test Artist"
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE "Track 2"
    PERFORMER "Test Artist"
    INDEX 01 03:00:00
  TRACK 03 AUDIO
    TITLE "Track 3"
    PERFORMER "Test Artist"
    INDEX 01 06:00:00
"#;
    let result = CueFlacProcessor::parse_cue_content(cue_content);
    assert!(result.is_ok());
    let (_, cue_sheet) = result.unwrap();
    assert_eq!(cue_sheet.tracks[0].end_time_ms(), Some(3 * 60 * 1000));
    assert_eq!(cue_sheet.tracks[1].end_time_ms(), Some(6 * 60 * 1000));
    assert_eq!(cue_sheet.tracks[2].end_time_ms(), None);
}
#[test]
fn test_parse_cue_sheet_without_per_track_performer() {
    let cue_content = r#"PERFORMER "Test Artist"
TITLE "Test Album"
FILE "test.flac" WAVE
  TRACK 01 AUDIO
    TITLE "Track 1"
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE "Track 2"
    INDEX 01 03:00:00
"#;
    let result = CueFlacProcessor::parse_cue_content(cue_content);
    assert!(result.is_ok());
    let (_, cue_sheet) = result.unwrap();
    assert_eq!(cue_sheet.tracks.len(), 2);
    assert_eq!(cue_sheet.tracks[0].performer, None);
    assert_eq!(cue_sheet.tracks[1].performer, None);
}
#[test]
fn parse_cue_content_captures_release_identification_signals() {
    let cue_content = r#"REM GENRE Rock
REM DATE 2001
REM DISCID 7F0A4C0B
REM COMMENT "Some Ripper v1.0"
CATALOG 0123456789012
PERFORMER "Artist Name"
TITLE "Album Title"
FILE "Artist Name - Album Title.flac" WAVE
  TRACK 01 AUDIO
    INDEX 01 00:00:00
"#;
    let (_, sheet) = CueFlacProcessor::parse_cue_content(cue_content).unwrap();
    assert_eq!(sheet.catalog.as_deref(), Some("0123456789012"));
    assert_eq!(sheet.date.as_deref(), Some("2001"));
}

#[test]
fn parse_rem_strips_quoted_values_and_ignores_unknown_keywords() {
    let cue_content = r#"REM DATE "2000 / 2004"
REM GENRE "Indie Rock"
REM DISCID 7F0A4C0B
REM CUSTOM_RIPPER_FIELD some-value
PERFORMER "Artist Name"
TITLE "Album Title"
FILE "x.flac" WAVE
  TRACK 01 AUDIO
    INDEX 01 00:00:00
"#;
    let (_, sheet) = CueFlacProcessor::parse_cue_content(cue_content).unwrap();
    assert_eq!(sheet.date.as_deref(), Some("2000 / 2004"));
    // REM GENRE, REM DISCID, and unknown REM keywords are silently consumed;
    // only the identification fields the parser captures are populated.
    assert_eq!(sheet.catalog, None);
}

#[test]
fn parse_track_with_isrc_populates_field() {
    let cue_content = r#"PERFORMER "Artist Name"
TITLE "Album Title"
FILE "Artist Name - Album Title.flac" WAVE
  TRACK 01 AUDIO
    TITLE "Track One"
    PERFORMER "Artist Name"
    ISRC GB000000000001
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE "Track Two"
    PERFORMER "Artist Name"
    ISRC GB000000000002
    INDEX 01 03:45:00
"#;
    let (_, sheet) = CueFlacProcessor::parse_cue_content(cue_content).unwrap();
    assert_eq!(sheet.tracks.len(), 2);
    assert_eq!(sheet.tracks[0].isrc.as_deref(), Some("GB000000000001"));
    assert_eq!(sheet.tracks[1].isrc.as_deref(), Some("GB000000000002"));
}

#[test]
fn parse_track_silently_consumes_spec_known_commands() {
    let cue_content = r#"PERFORMER "Artist Name"
TITLE "Album Title"
FILE "Artist Name - Album Title.flac" WAVE
  TRACK 01 AUDIO
    TITLE "Track One"
    PERFORMER "Artist Name"
    SONGWRITER "Songwriter Name"
    FLAGS DCP
    PREGAP 00:02:00
    INDEX 01 00:00:00
    POSTGAP 00:01:00
  TRACK 02 AUDIO
    TITLE "Track Two"
    FLAGS PRE 4CH
    INDEX 01 03:45:00
"#;
    let (_, sheet) = CueFlacProcessor::parse_cue_content(cue_content).unwrap();
    assert_eq!(sheet.tracks.len(), 2);
    assert_eq!(sheet.tracks[0].title.as_deref(), Some("Track One"));
    assert_eq!(sheet.tracks[1].title.as_deref(), Some("Track Two"));
}

#[test]
fn parse_track_skips_unknown_command_without_failing() {
    let cue_content = r#"PERFORMER "Artist Name"
TITLE "Album Title"
FILE "Artist Name - Album Title.flac" WAVE
  TRACK 01 AUDIO
    TITLE "Track One"
    SOMETHING_UNKNOWN value
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE "Track Two"
    VENDOR_EXTENSION whatever
    INDEX 01 03:45:00
"#;
    let (_, sheet) = CueFlacProcessor::parse_cue_content(cue_content).unwrap();
    assert_eq!(sheet.tracks.len(), 2);
    assert_eq!(sheet.tracks[0].title.as_deref(), Some("Track One"));
    assert_eq!(sheet.tracks[1].title.as_deref(), Some("Track Two"));
}

#[test]
fn parse_track_rejects_audio_and_generated_pregap_together() {
    let cue_content = r#"PERFORMER "Artist Name"
TITLE "Album Title"
FILE "Artist Name - Album Title.flac" WAVE
  TRACK 01 AUDIO
    TITLE "Track One"
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE "Track Two"
    INDEX 00 03:43:00
    PREGAP 00:02:00
    INDEX 01 03:45:00
"#;
    assert!(CueFlacProcessor::parse_cue_content(cue_content).is_err());
}

#[test]
fn parse_track_without_index_01_returns_err() {
    let cue_content = r#"PERFORMER "Artist Name"
TITLE "Album Title"
FILE "Artist Name - Album Title.flac" WAVE
  TRACK 01 AUDIO
    TITLE "Track One"
    PERFORMER "Artist Name"
"#;
    assert!(CueFlacProcessor::parse_cue_content(cue_content).is_err());
}

#[test]
fn parse_zero_track_cue_returns_err() {
    let cue_content = r#"PERFORMER "Artist Name"
TITLE "Album Title"
FILE "Artist Name - Album Title.flac" WAVE
"#;
    assert!(CueFlacProcessor::parse_cue_content(cue_content).is_err());
}

#[test]
fn parse_track_consumes_subindexes_above_one() {
    let cue_content = r#"PERFORMER "Artist Name"
TITLE "Album Title"
FILE "Artist Name - Album Title.flac" WAVE
  TRACK 01 AUDIO
    TITLE "Track One"
    INDEX 01 00:00:00
    INDEX 02 01:30:00
    INDEX 03 02:15:00
  TRACK 02 AUDIO
    TITLE "Track Two"
    INDEX 01 03:45:00
"#;
    let (_, sheet) = CueFlacProcessor::parse_cue_content(cue_content).unwrap();
    assert_eq!(sheet.tracks.len(), 2);
    assert_eq!(sheet.tracks[0].start_cue_frames, 0);
    assert_eq!(sheet.tracks[1].start_cue_frames, 3 * 60 * 75 + 45 * 75);
    assert_eq!(sheet.tracks[0].index(2).unwrap().frames, 90 * 75);
    assert_eq!(sheet.tracks[0].index(3).unwrap().frames, 135 * 75);
}

#[test]
fn parse_track_captures_flags_postgap_composer_and_songwriter() {
    let cue_content = r#"PERFORMER "Artist Name"
TITLE "Album Title"
COMPOSER "Release Composer"
SONGWRITER "Release Songwriter"
FILE "Album.flac" WAVE
  TRACK 01 AUDIO
    TITLE "Track One"
    COMPOSER "Track Composer"
    SONGWRITER "Track Songwriter"
    FLAGS DCP PRE
    INDEX 01 00:00:00
    POSTGAP 00:01:00
"#;
    let (_, sheet) = CueFlacProcessor::parse_cue_content(cue_content).unwrap();
    assert_eq!(sheet.composer.as_deref(), Some("Release Composer"));
    assert_eq!(sheet.songwriter.as_deref(), Some("Release Songwriter"));
    let track = &sheet.tracks[0];
    assert_eq!(track.composer.as_deref(), Some("Track Composer"));
    assert_eq!(track.songwriter.as_deref(), Some("Track Songwriter"));
    assert_eq!(track.flags, vec!["DCP".to_string(), "PRE".to_string()]);
    assert_eq!(track.postgap_frames, Some(75));
}

#[test]
fn parse_non_audio_track_preserves_layout_but_not_playable_count() {
    let cue_content = r#"PERFORMER "Artist Name"
TITLE "Album Title"
FILE "Album.bin" BINARY
  TRACK 01 MODE1/2352
    INDEX 01 00:00:00
FILE "Album.flac" WAVE
  TRACK 02 AUDIO
    INDEX 01 00:00:00
"#;
    let (_, sheet) = CueFlacProcessor::parse_cue_content(cue_content).unwrap();
    assert_eq!(sheet.tracks.len(), 2);
    assert!(!sheet.tracks[0].is_playable_audio());
    assert_eq!(sheet.playable_track_count(), 1);
    assert_eq!(sheet.audio_file_references(), vec!["Album.flac"]);
}

#[test]
fn test_parse_cue_with_index_00_minimal_repro() {
    let cue_content = r#"PERFORMER "Test Artist"
TITLE "Test Album"
FILE "test.flac" WAVE
  TRACK 01 AUDIO
    TITLE "Track 1"
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE "Track 2"
    INDEX 00 03:00:00
    INDEX 01 03:01:00
"#;
    let result = CueFlacProcessor::parse_cue_content(cue_content);
    assert!(result.is_ok());
    let (_, cue_sheet) = result.unwrap();
    assert_eq!(cue_sheet.tracks.len(), 2, "Should parse 2 tracks");
}

#[test]
fn test_pregap_sets_correct_track_boundary() {
    // Track 2 has a 3-second pregap (INDEX 00 at 2:46, INDEX 01 at 2:49)
    // Track 1 should end at INDEX 00 (2:46), not INDEX 01 (2:49)
    let cue_content = r#"PERFORMER "Test Artist"
TITLE "Test Album"
FILE "Test Artist - Test Album.flac" WAVE
  TRACK 01 AUDIO
    TITLE "Track One"
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE "Track Two"
    INDEX 00 02:46:00
    INDEX 01 02:49:00
  TRACK 03 AUDIO
    TITLE "Track Three"
    INDEX 01 09:31:00
"#;
    let result = CueFlacProcessor::parse_cue_content(cue_content);
    assert!(result.is_ok());
    let (_, cue_sheet) = result.unwrap();

    // Track 1 should end at track 2's pregap (INDEX 00), not INDEX 01
    let track1_end_ms = cue_sheet.tracks[0].end_time_ms().unwrap();
    let track2_pregap_frames = cue_sheet.tracks[1].pregap_cue_frames.unwrap();
    let track2_start_frames = cue_sheet.tracks[1].start_cue_frames;

    // Verify pregap was parsed correctly (INDEX 00 = 2:46:00 = 12450 CUE frames)
    assert_eq!(track2_pregap_frames, (2 * 60 + 46) * 75);
    // Verify start was parsed correctly (INDEX 01 = 2:49:00 = 12675 CUE frames)
    assert_eq!(track2_start_frames, (2 * 60 + 49) * 75);

    // THE KEY ASSERTION: Track 1 ends at pregap, not at start
    assert_eq!(
        track1_end_ms,
        track2_pregap_frames * 1000 / 75,
        "Track 1 should end at track 2's INDEX 00 (pregap), not INDEX 01"
    );
}

#[test]
fn test_pregap_duration_calculation() {
    // Pregap duration = INDEX 01 - INDEX 00
    let cue_content = r#"PERFORMER "Test Artist"
TITLE "Test Album"
FILE "test.flac" WAVE
  TRACK 01 AUDIO
    TITLE "Track 1"
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE "Track 2"
    INDEX 00 03:00:00
    INDEX 01 03:03:00
"#;
    let result = CueFlacProcessor::parse_cue_content(cue_content);
    assert!(result.is_ok());
    let (_, cue_sheet) = result.unwrap();

    let track2 = &cue_sheet.tracks[1];
    let pregap_frames = track2.pregap_cue_frames.unwrap();
    let start_frames = track2.start_cue_frames;

    // Pregap duration should be 3 seconds (3:03 - 3:00 = 225 CUE frames = 3s)
    let pregap_duration_frames = start_frames - pregap_frames;
    assert_eq!(
        pregap_duration_frames,
        3 * 75,
        "Pregap duration should be 3 seconds"
    );
}

#[test]
fn test_track_without_pregap_uses_start_for_boundary() {
    // Track 3 has no pregap, so track 2 should end at track 3's INDEX 01
    let cue_content = r#"PERFORMER "Test Artist"
TITLE "Test Album"
FILE "test.flac" WAVE
  TRACK 01 AUDIO
    TITLE "Track 1"
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE "Track 2"
    INDEX 00 03:00:00
    INDEX 01 03:02:00
  TRACK 03 AUDIO
    TITLE "Track 3"
    INDEX 01 06:00:00
"#;
    let result = CueFlacProcessor::parse_cue_content(cue_content);
    assert!(result.is_ok());
    let (_, cue_sheet) = result.unwrap();

    // Track 2 should end at track 3's start (no pregap on track 3)
    let track2_end_frames = cue_sheet.tracks[1].end_cue_frames.unwrap();
    let track3_start_frames = cue_sheet.tracks[2].start_cue_frames;

    assert_eq!(
        track2_end_frames, track3_start_frames,
        "Track 2 should end at track 3's INDEX 01 (no pregap)"
    );
    assert_eq!(track2_end_frames, 6 * 60 * 75);
}

#[test]
fn test_cue_track_audio_methods() {
    // Track 2 has pregap at 2:46 (INDEX 00) and start at 2:49 (INDEX 01)
    // Track 3 starts at 9:31, so track 2 ends at 9:31
    let cue_content = r#"PERFORMER "Test Artist"
TITLE "Test Album"
FILE "test.flac" WAVE
  TRACK 01 AUDIO
    TITLE "Track 1"
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE "Track 2"
    INDEX 00 02:46:00
    INDEX 01 02:49:00
  TRACK 03 AUDIO
    TITLE "Track 3"
    INDEX 01 09:31:00
"#;
    let (_, cue_sheet) = CueFlacProcessor::parse_cue_content(cue_content).unwrap();

    // Track 1: no pregap
    let track1 = &cue_sheet.tracks[0];
    assert_eq!(track1.audio_start_ms(), 0);
    assert_eq!(track1.pregap_duration_ms(), None);

    // Track 2: has pregap
    let track2 = &cue_sheet.tracks[1];
    assert_eq!(track2.audio_start_ms(), 166000); // 2:46 (INDEX 00)
    assert_eq!(track2.pregap_duration_ms(), Some(3000)); // 3 seconds
                                                         // Track duration excludes pregap: 9:31 - 2:49 = 402 seconds
    assert_eq!(track2.track_duration_ms(), Some(402000));

    // Track 3: last track, no end time
    let track3 = &cue_sheet.tracks[2];
    assert_eq!(track3.audio_start_ms(), 571000); // 9:31
    assert_eq!(track3.pregap_duration_ms(), None);
    assert_eq!(track3.track_duration_ms(), None);
}

#[test]
fn test_parse_cue_with_rem_between_title_and_file() {
    let cue_content = r#"REM DATE 1970
REM DISCID A1B2C3D4
REM COMMENT "ExactAudioCopy v1.3"
PERFORMER "Test Artist"
TITLE "Test Album"
REM COMPOSER ""
FILE "Test Artist - Test Album.flac" WAVE
  TRACK 01 AUDIO
    TITLE "Track 1"
    PERFORMER "Test Artist"
    REM COMPOSER ""
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE "Track 2"
    PERFORMER "Test Artist"
    REM COMPOSER ""
    INDEX 01 06:17:53
  TRACK 03 AUDIO
    TITLE "Track 3 With Multiple Sections"
    PERFORMER "Test Artist"
    REM COMPOSER ""
    INDEX 00 10:39:50
    INDEX 01 10:41:28
"#;
    let result = CueFlacProcessor::parse_cue_content(cue_content);
    assert!(
        result.is_ok(),
        "Should parse CUE with REM between TITLE and FILE"
    );
    let (_, cue_sheet) = result.unwrap();
    assert_eq!(cue_sheet.title.as_deref(), Some("Test Album"));
    assert_eq!(cue_sheet.performer.as_deref(), Some("Test Artist"));
    assert_eq!(cue_sheet.tracks.len(), 3, "Should parse 3 tracks");
    assert_eq!(cue_sheet.tracks[0].title.as_deref(), Some("Track 1"));
    assert_eq!(cue_sheet.tracks[1].title.as_deref(), Some("Track 2"));
    assert_eq!(
        cue_sheet.tracks[2].title.as_deref(),
        Some("Track 3 With Multiple Sections")
    );
    assert_eq!(cue_sheet.tracks[0].start_time_ms(), 0);
    assert_eq!(
        cue_sheet.tracks[1].start_time_ms(),
        6 * 60 * 1000 + 17 * 1000 + 53 * 1000 / 75,
    );
}

#[test]
fn test_bogus_pregap_before_previous_track_uses_index01() {
    // Real-world CUE: EAC wrote INDEX 00 05:05:00 for all tracks 3-19,
    // which is before track 2's INDEX 01. This caused:
    //   - Track 2 duration = 0 (boundary == start)
    //   - Track 3+ duration underflow (boundary < start on u64)
    let cue_content = r#"PERFORMER "Test Artist"
TITLE "Test Album"
FILE "test.ape" WAVE
  TRACK 01 AUDIO
    TITLE "Track One"
    INDEX 00 00:00:00
    INDEX 01 00:00:32
  TRACK 02 AUDIO
    TITLE "Track Two"
    INDEX 01 05:05:00
  TRACK 03 AUDIO
    TITLE "Track Three"
    INDEX 00 05:05:00
    INDEX 01 08:31:20
  TRACK 04 AUDIO
    TITLE "Track Four"
    INDEX 00 05:05:00
    INDEX 01 11:01:30
"#;
    let (_, cue_sheet) = CueFlacProcessor::parse_cue_content(cue_content).unwrap();

    // Track 1: end should be track 2's start (no pregap on track 2)
    let track1 = &cue_sheet.tracks[0];
    assert_eq!(track1.start_time_ms(), 426); // INDEX 01 00:00:32 = 32 frames = 426.67ms
    let track1_dur = track1.track_duration_ms().unwrap();
    assert!(
        track1_dur > 300_000,
        "Track 1 should be ~5min, got {}ms",
        track1_dur
    );

    // Track 2: end should NOT use track 3's bogus pregap (05:05:00 = same as track 2 start)
    // It should use track 3's INDEX 01 instead
    let track2 = &cue_sheet.tracks[1];
    assert_eq!(track2.start_time_ms(), 305_000); // 5:05:00
    let track2_dur = track2.track_duration_ms().unwrap();
    assert!(
        track2_dur > 200_000,
        "Track 2 should be ~3.5min, got {}ms",
        track2_dur
    );

    // Track 3: end should use track 4's INDEX 01 (not bogus pregap)
    let track3 = &cue_sheet.tracks[2];
    assert_eq!(track3.start_time_ms(), 511_266); // 8:31:20
    let track3_dur = track3.track_duration_ms().unwrap();
    assert!(
        track3_dur > 140_000,
        "Track 3 should be ~2.5min, got {}ms",
        track3_dur
    );

    // audio_start_ms must ignore bogus pregap and use INDEX 01
    assert_eq!(
        track3.audio_start_ms(),
        track3.start_time_ms(),
        "Track 3 audio_start_ms should use INDEX 01, not bogus INDEX 00"
    );
    assert_eq!(
        track3.pregap_duration_ms(),
        None,
        "Bogus pregap should be cleared, leaving no duration"
    );
    assert!(
        track3.index(0).is_none(),
        "Bogus INDEX 00 should be removed from the raw index list"
    );
}

#[test]
fn test_detect_cue_flac_from_paths_lowercase() {
    use std::path::PathBuf;

    let paths = vec![
        PathBuf::from("/music/album.flac"),
        PathBuf::from("/music/album.cue"),
        PathBuf::from("/music/cover.jpg"),
    ];

    let pairs = CueFlacProcessor::detect_cue_flac_from_paths(&paths).unwrap();

    assert_eq!(pairs.len(), 1);
    assert_eq!(pairs[0].audio_path, PathBuf::from("/music/album.flac"));
    assert_eq!(pairs[0].cue_path, PathBuf::from("/music/album.cue"));
}

#[test]
fn test_detect_cue_flac_from_paths_uppercase() {
    use std::path::PathBuf;

    let paths = vec![
        PathBuf::from("/music/album.FLAC"),
        PathBuf::from("/music/album.CUE"),
        PathBuf::from("/music/cover.jpg"),
    ];

    let pairs = CueFlacProcessor::detect_cue_flac_from_paths(&paths).unwrap();

    assert_eq!(
        pairs.len(),
        1,
        "Should detect CUE/FLAC pair with uppercase extensions"
    );
    assert_eq!(pairs[0].audio_path, PathBuf::from("/music/album.FLAC"));
    assert_eq!(pairs[0].cue_path, PathBuf::from("/music/album.CUE"));
}

#[test]
fn test_detect_cue_flac_from_paths_mixed_case() {
    use std::path::PathBuf;

    let paths = vec![
        PathBuf::from("/music/album.Flac"),
        PathBuf::from("/music/album.Cue"),
    ];

    let pairs = CueFlacProcessor::detect_cue_flac_from_paths(&paths).unwrap();

    assert_eq!(
        pairs.len(),
        1,
        "Should detect CUE/FLAC pair with mixed case extensions"
    );
}

#[test]
fn test_detect_cue_flac_from_paths_no_match() {
    use std::path::PathBuf;

    let paths = vec![
        PathBuf::from("/music/album.flac"),
        PathBuf::from("/music/different.cue"),
    ];

    let pairs = CueFlacProcessor::detect_cue_flac_from_paths(&paths).unwrap();

    assert_eq!(
        pairs.len(),
        0,
        "Should not match CUE/FLAC with different stems"
    );
}

#[test]
fn test_detect_cue_flac_from_paths_multiple_pairs() {
    use std::path::PathBuf;

    let paths = vec![
        PathBuf::from("/music/disc1.flac"),
        PathBuf::from("/music/disc1.cue"),
        PathBuf::from("/music/disc2.flac"),
        PathBuf::from("/music/disc2.cue"),
    ];

    let pairs = CueFlacProcessor::detect_cue_flac_from_paths(&paths).unwrap();

    assert_eq!(pairs.len(), 2, "Should detect multiple CUE/FLAC pairs");
}

#[test]
fn test_detect_cue_flac_from_paths_with_spaces_and_dashes() {
    use std::path::PathBuf;

    let paths = vec![
        PathBuf::from("/music/Some Artist - Some Album.cue"),
        PathBuf::from("/music/Some Artist - Some Album.flac"),
        PathBuf::from("/music/front.jpg"),
    ];

    let pairs = CueFlacProcessor::detect_cue_flac_from_paths(&paths).unwrap();

    assert_eq!(
        pairs.len(),
        1,
        "Should detect CUE/FLAC pair with spaces and dashes in filename"
    );
    assert!(pairs[0]
        .audio_path
        .to_string_lossy()
        .contains("Some Artist"));
    assert!(pairs[0].cue_path.to_string_lossy().contains("Some Artist"));
}

#[test]
fn test_detect_cue_ape_pair() {
    use std::path::PathBuf;

    let paths = vec![
        PathBuf::from("/music/album.ape"),
        PathBuf::from("/music/album.cue"),
        PathBuf::from("/music/cover.jpg"),
    ];

    let pairs = CueFlacProcessor::detect_cue_flac_from_paths(&paths).unwrap();

    assert_eq!(pairs.len(), 1);
    assert_eq!(pairs[0].audio_path, PathBuf::from("/music/album.ape"));
    assert_eq!(pairs[0].cue_path, PathBuf::from("/music/album.cue"));
}

#[test]
fn test_detect_cue_ape_uppercase() {
    use std::path::PathBuf;

    let paths = vec![
        PathBuf::from("/music/album.APE"),
        PathBuf::from("/music/album.CUE"),
    ];

    let pairs = CueFlacProcessor::detect_cue_flac_from_paths(&paths).unwrap();

    assert_eq!(pairs.len(), 1);
    assert_eq!(pairs[0].audio_path, PathBuf::from("/music/album.APE"));
}

#[test]
fn test_detect_cue_ape_no_match_different_stems() {
    use std::path::PathBuf;

    let paths = vec![
        PathBuf::from("/music/album.ape"),
        PathBuf::from("/music/different.cue"),
    ];

    let pairs = CueFlacProcessor::detect_cue_flac_from_paths(&paths).unwrap();
    assert_eq!(pairs.len(), 0);
}

#[test]
fn test_detect_cue_alac_pair() {
    use std::path::PathBuf;

    // `.m4a` is the MP4 container extension for CUE+ALAC rips. Detection
    // is extension-only at this layer; the codec is confirmed later by
    // the analyzer.
    let paths = vec![
        PathBuf::from("/music/album.m4a"),
        PathBuf::from("/music/album.cue"),
        PathBuf::from("/music/cover.jpg"),
    ];

    let pairs = CueFlacProcessor::detect_cue_flac_from_paths(&paths).unwrap();

    assert_eq!(pairs.len(), 1);
    assert_eq!(pairs[0].audio_path, PathBuf::from("/music/album.m4a"));
    assert_eq!(pairs[0].cue_path, PathBuf::from("/music/album.cue"));
}

#[test]
fn test_detect_cue_alac_uppercase() {
    use std::path::PathBuf;

    let paths = vec![
        PathBuf::from("/music/album.M4A"),
        PathBuf::from("/music/album.CUE"),
    ];

    let pairs = CueFlacProcessor::detect_cue_flac_from_paths(&paths).unwrap();

    assert_eq!(pairs.len(), 1);
    assert_eq!(pairs[0].audio_path, PathBuf::from("/music/album.M4A"));
}

#[test]
fn test_detect_mixed_flac_and_ape_pairs() {
    use std::path::PathBuf;

    let paths = vec![
        PathBuf::from("/music/disc1.flac"),
        PathBuf::from("/music/disc1.cue"),
        PathBuf::from("/music/disc2.ape"),
        PathBuf::from("/music/disc2.cue"),
    ];

    let pairs = CueFlacProcessor::detect_cue_flac_from_paths(&paths).unwrap();

    assert_eq!(pairs.len(), 2);
    assert_eq!(pairs[0].audio_path, PathBuf::from("/music/disc1.flac"));
    assert_eq!(pairs[1].audio_path, PathBuf::from("/music/disc2.ape"));
}

#[test]
fn test_detect_multi_disc_same_stem_different_dirs() {
    use std::path::PathBuf;

    let paths = vec![
        PathBuf::from("/music/CD1/CDImage.ape"),
        PathBuf::from("/music/CD1/CDImage.cue"),
        PathBuf::from("/music/CD2/CDImage.ape"),
        PathBuf::from("/music/CD2/CDImage.cue"),
    ];

    let pairs = CueFlacProcessor::detect_cue_flac_from_paths(&paths).unwrap();

    assert_eq!(pairs.len(), 2);
    assert_eq!(pairs[0].cue_path, PathBuf::from("/music/CD1/CDImage.cue"));
    assert_eq!(pairs[0].audio_path, PathBuf::from("/music/CD1/CDImage.ape"));
    assert_eq!(pairs[1].cue_path, PathBuf::from("/music/CD2/CDImage.cue"));
    assert_eq!(pairs[1].audio_path, PathBuf::from("/music/CD2/CDImage.ape"));
}

#[test]
fn test_detect_three_discs_same_stem() {
    use std::path::PathBuf;

    let paths = vec![
        PathBuf::from("/music/CD1/CDImage.flac"),
        PathBuf::from("/music/CD1/CDImage.cue"),
        PathBuf::from("/music/CD2/CDImage.flac"),
        PathBuf::from("/music/CD2/CDImage.cue"),
        PathBuf::from("/music/CD3/CDImage.flac"),
        PathBuf::from("/music/CD3/CDImage.cue"),
    ];

    let pairs = CueFlacProcessor::detect_cue_flac_from_paths(&paths).unwrap();

    assert_eq!(pairs.len(), 3);
    assert_eq!(
        pairs[0].audio_path,
        PathBuf::from("/music/CD1/CDImage.flac")
    );
    assert_eq!(
        pairs[1].audio_path,
        PathBuf::from("/music/CD2/CDImage.flac")
    );
    assert_eq!(
        pairs[2].audio_path,
        PathBuf::from("/music/CD3/CDImage.flac")
    );
}

#[test]
fn test_detect_cue_and_audio_must_be_same_directory() {
    use std::path::PathBuf;

    // CUE in one dir, audio in another — not a valid pair
    let paths = vec![
        PathBuf::from("/music/CDImage.cue"),
        PathBuf::from("/music/CD1/CDImage.ape"),
    ];

    let pairs = CueFlacProcessor::detect_cue_flac_from_paths(&paths).unwrap();

    assert_eq!(pairs.len(), 0);
}
