use super::*;
use crate::audio_codec::ProbeResult;
use crate::import::types::{CueAnalyzedAudioFile, CueFlacAnalysis, TrackFile};
use std::sync::Arc;

fn scanned_non_audio(path: &str) -> crate::import::folder_scanner::ScannedFile {
    crate::import::folder_scanner::ScannedFile::new(
        PathBuf::from(path),
        Path::new(path)
            .file_name()
            .expect("test path has a file name")
            .to_string_lossy()
            .into_owned(),
        1,
        0,
    )
}

fn scanned_flac() -> crate::import::folder_scanner::ScannedAudio {
    crate::import::folder_scanner::ScannedAudio {
        content_type: ContentType::Flac,
        duration_ms: 1_000,
        format: crate::album_detail::AudioFormat {
            codec: "FLAC".to_string(),
            sample_rate_hz: 44_100,
            bits_per_sample: Some(16),
            bitrate_kbps: None,
            channels: 2,
        },
    }
}

#[test]
fn admitted_audio_content_type_gates_unlisted_codecs() {
    // Dispatch check: an admitted codec passes the gate.
    assert!(ContentType::Flac.is_supported_audio());
    // The gate's job — reject anything not on the import allowlist.
    assert!(!ContentType::Other("codec:AV_CODEC_ID_SPEEX".to_string()).is_supported_audio());
    assert!(!ContentType::Other("audio/x-ms-wma".to_string()).is_supported_audio());
}

#[test]
fn resolve_file_content_type_maps_non_audio_by_extension() {
    // Non-audio files map straight from the extension hint — no probe, so
    // the paths need not exist.
    assert_eq!(
        resolve_file_content_type(&scanned_non_audio("/x/cover.jpg")).unwrap(),
        ContentType::Jpeg
    );
    assert_eq!(
        resolve_file_content_type(&scanned_non_audio("/x/back.png")).unwrap(),
        ContentType::Png
    );
    assert_eq!(
        resolve_file_content_type(&scanned_non_audio("/x/notes.txt")).unwrap(),
        ContentType::PlainText
    );
    assert_eq!(
        resolve_file_content_type(&scanned_non_audio("/x/booklet.pdf")).unwrap(),
        ContentType::Pdf
    );
    // An extension the hint can't classify becomes opaque binary.
    assert_eq!(
        resolve_file_content_type(&scanned_non_audio("/x/data.bin")).unwrap(),
        ContentType::OctetStream
    );
}

/// A rip folder can hold an extensionless file — a `README`, a bare
/// checksum — and the release carries it like any other. It is opaque
/// binary, not a reason to fail the import.
#[test]
fn resolve_file_content_type_without_an_extension_is_opaque_binary() {
    assert_eq!(
        resolve_file_content_type(&scanned_non_audio("/x/README")).unwrap(),
        ContentType::OctetStream
    );
}

fn test_clock() -> coven::FixedClock {
    coven::FixedClock(
        chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc),
    )
}

#[test]
fn cross_file_index00_uses_the_previous_files_tail_as_pregap() {
    let temp = tempfile::tempdir().expect("tempdir");
    let cue_path = temp.path().join("album.cue");
    std::fs::write(
        &cue_path,
        r#"FILE "first.flac" WAVE
  TRACK 01 AUDIO
INDEX 01 00:00:00
  TRACK 02 AUDIO
INDEX 00 00:08:00
FILE "second.flac" WAVE
INDEX 01 00:00:00
"#,
    )
    .expect("write cue");
    let cue_sheet = crate::cue_flac::parse_cue_sheet(&cue_path).expect("parse cue");
    let make_probe = |seconds| ProbeResult {
        content_type: ContentType::Flac,
        duration: std::time::Duration::from_secs(seconds),
        sample_rate: 44_100,
        bits_per_sample: Some(16),
        bitrate_kbps: None,
        channels: 2,
    };
    let analysis = CueFlacAnalysis {
        cue_sheet,
        audio_files: vec![
            CueAnalyzedAudioFile {
                file_reference: "first.flac".to_string(),
                path: temp.path().join("first.flac"),
                probe: make_probe(10),
            },
            CueAnalyzedAudioFile {
                file_reference: "second.flac".to_string(),
                path: temp.path().join("second.flac"),
                probe: make_probe(20),
            },
        ],
    };

    let format = cue_backed_audio_format(
        "track-2",
        &analysis,
        1,
        "format-2".to_string(),
        test_clock().0,
    )
    .expect("build audio format");

    assert_eq!(format.pregap_ms, Some(2_000));
    assert_eq!(format.pregap_samples, Some(88_200));
}

/// A CUE track whose INDEX 00 sits after the previous track's INDEX 01 is a
/// real audio pregap (a hidden intro / count-in), so the pregap bytes come
/// from the container itself. `cue_segments` must emit an `AudioPregap`
/// segment spanning `[INDEX 00, INDEX 01)` ahead of the `Main` segment —
/// the branch `cue_backed_segments_ignore_rejected_index00_boundaries` never
/// reaches, because its INDEX 00 values are all bogus (before the prior
/// track) and get rejected to `CuePregap::None`.
///
/// Backed by the real APE CUE fixture so `seek_landing_bytes` produces real
/// byte offsets: asserts the sample windows *and* the byte windows,
/// including the contiguity invariant — every segment's `start_byte` equals
/// its predecessor's `end_byte`, so the album's read-ahead spans chain end
/// to end with no gaps or overlaps.
#[test]
fn cue_backed_segments_emit_audio_pregap_byte_window() {
    crate::audio_codec::init();
    let audio_path = PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/cue_ape/Test Album.ape"
    ));
    // A synthetic CUE over the real container: track 2 carries a genuine
    // audio pregap (INDEX 00 at 0:20, after track 1's INDEX 01 at 0:00). All
    // positions sit inside the ~1:30 fixture so every seek lands.
    let temp = tempfile::tempdir().expect("tempdir");
    let cue_path = temp.path().join("Test Album.cue");
    std::fs::write(
        &cue_path,
        r#"PERFORMER "Test Artist"
TITLE "Test Album"
FILE "Test Album.ape" WAVE
  TRACK 01 AUDIO
TITLE "Track One"
INDEX 01 00:00:00
  TRACK 02 AUDIO
TITLE "Track Two"
INDEX 00 00:20:00
INDEX 01 00:30:00
  TRACK 03 AUDIO
TITLE "Track Three"
INDEX 01 01:00:00
"#,
    )
    .expect("write cue");

    let cue_sheet = crate::cue_flac::parse_cue_sheet(&cue_path).expect("parse cue");
    let probe = crate::audio_codec::probe_audio_from_path(
        audio_path.to_str().expect("fixture path is UTF-8"),
    )
    .expect("analyze ape");
    let cue_pair = Arc::new(CueFlacAnalysis {
        cue_sheet,
        audio_files: vec![CueAnalyzedAudioFile {
            file_reference: "Test Album.ape".to_string(),
            path: audio_path.clone(),
            probe,
        }],
    });

    let tracks_to_files: Vec<_> = cue_pair
        .cue_sheet
        .playable_tracks()
        .enumerate()
        .map(|(index, track)| TrackFile::CueBacked {
            db_track: crate::db::DbTrack::new_test(
                "release-id",
                &format!("track-{index}"),
                track.title.as_deref().unwrap_or("Track Title"),
                Some(track.number as i32),
            ),
            file_path: audio_path.clone(),
            cue_pair: Arc::clone(&cue_pair),
            cue_index: index,
        })
        .collect();
    let file_ids = HashMap::from([(audio_path, "file-id".to_string())]);
    let ids = coven::SequentialIdProvider::new("audio");

    let built =
        ImportService::build_audio_formats(&tracks_to_files, &file_ids, &test_clock(), &ids)
            .expect("build audio formats");

    // Exactly one track (track 2) has a real audio pregap.
    let pregaps: Vec<_> = built
        .audio_segments
        .iter()
        .filter(|segment| segment.role == DbAudioSegmentRole::AudioPregap)
        .collect();
    assert_eq!(pregaps.len(), 1, "only track 2 has an audio pregap");
    let pregap = pregaps[0];
    assert_eq!(pregap.segment_index, 0, "pregap precedes the main segment");
    assert_eq!(pregap.file_id, "file-id");

    // Sample windows: INDEX 00 (0:20) .. INDEX 01 (0:30), frames -> samples.
    let sample_rate = built.audio_formats[1].sample_rate;
    assert_eq!(pregap.start_sample, (20 * 75) * sample_rate / 75);
    assert_eq!(pregap.end_sample, Some((30 * 75) * sample_rate / 75));

    let main_segments: Vec<_> = built
        .audio_segments
        .iter()
        .filter(|segment| segment.role == DbAudioSegmentRole::Main)
        .collect();
    assert_eq!(main_segments.len(), 3);
    let track2_main = main_segments[1];
    assert_eq!(
        track2_main.audio_format_id, pregap.audio_format_id,
        "main_segments are in track order",
    );
    assert_eq!(track2_main.start_sample, (30 * 75) * sample_rate / 75);
    assert_eq!(track2_main.end_sample, Some((60 * 75) * sample_rate / 75));

    // Byte windows are real (read from the container) and chain
    // contiguously: track1.main.end -> track2.pregap.start ->
    // track2.pregap.end -> track2.main.start -> track2.main.end ->
    // track3.main.start.
    let pregap_start = pregap.start_byte.expect("pregap start byte");
    let pregap_end = pregap.end_byte.expect("pregap end byte");
    let track2_main_start = track2_main.start_byte.expect("track2 main start byte");
    let track2_main_end = track2_main.end_byte.expect("track2 main end byte");
    assert_eq!(
        pregap_end, track2_main_start,
        "track 2's main segment starts where its pregap ends",
    );
    assert_eq!(
        main_segments[0].end_byte,
        Some(pregap_start),
        "track 1 ends where track 2's pregap begins",
    );
    assert_eq!(
        main_segments[2].start_byte,
        Some(track2_main_end),
        "track 3 starts where track 2's main segment ends",
    );
    assert!(
        pregap_start < pregap_end && pregap_end < track2_main_end,
        "byte offsets increase across the pregap and main windows: \
         {pregap_start} < {pregap_end} < {track2_main_end}",
    );
    // The first track starts at byte 0 (nothing to prefetch); the last runs
    // to EOF.
    assert_eq!(
        main_segments[0].start_byte, None,
        "track 1 starts at byte 0"
    );
    assert_eq!(main_segments[2].end_byte, None, "track 3 runs to EOF");
}

/// The standalone (non-CUE) arm reuses the scan's format and emits one
/// whole-file `Main` segment: sample window open (`start_sample` 0,
/// `end_sample` None) and no byte window, since a per-track file is its own
/// source.
#[test]
fn build_audio_formats_uses_stored_standalone_facts() {
    let path = PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/flac/01 Test Track 1.flac"
    ));
    let track = TrackFile::Standalone {
        db_track: crate::db::DbTrack::new_test("release-id", "track-0", "Track Title", Some(1)),
        file_path: path.clone(),
        source_audio: scanned_flac(),
    };
    let file_ids = HashMap::from([(path, "file-0".to_string())]);
    let ids = coven::SequentialIdProvider::new("af");

    let built = ImportService::build_audio_formats(&[track], &file_ids, &test_clock(), &ids)
        .expect("build audio formats");

    assert_eq!(built.audio_formats.len(), 1);
    assert_eq!(built.audio_formats[0].track_id, "track-0");
    assert_eq!(built.audio_formats[0].content_type, ContentType::Flac);

    assert_eq!(built.audio_segments.len(), 1);
    let segment = &built.audio_segments[0];
    assert_eq!(segment.role, DbAudioSegmentRole::Main);
    assert_eq!(segment.segment_index, 0);
    assert_eq!(segment.file_id, "file-0");
    assert_eq!(segment.start_sample, 0);
    assert_eq!(segment.end_sample, None, "whole-file source has no end");
    assert_eq!(segment.start_byte, None);
    assert_eq!(segment.end_byte, None);
}

fn probe_result(sample_rate: u32, channels: u32) -> ProbeResult {
    ProbeResult {
        content_type: ContentType::Flac,
        duration: std::time::Duration::from_secs(1),
        sample_rate,
        bits_per_sample: Some(16),
        bitrate_kbps: None,
        channels,
    }
}

#[test]
fn probe_audio_format_rejects_zero_channels() {
    let path = Path::new("track.flac");

    let error = ensure_probe_audio_format(path, &probe_result(44_100, 0))
        .expect_err("zero channels should be rejected");

    assert!(matches!(
        &error,
        ImportError::UnusableFile { detail } if detail.contains("unusable audio format")
    ));
}

#[test]
fn probe_audio_format_rejects_zero_sample_rate() {
    let path = Path::new("track.flac");

    let error = ensure_probe_audio_format(path, &probe_result(0, 2))
        .expect_err("zero sample rate should be rejected");

    assert!(matches!(
        &error,
        ImportError::UnusableFile { detail } if detail.contains("unusable audio format")
    ));
}

#[test]
fn cue_backed_audio_format_rejects_zero_channels() {
    let temp = tempfile::tempdir().expect("tempdir");
    let cue_path = temp.path().join("Album.cue");
    let audio_path = temp.path().join("test.flac");
    std::fs::write(
        &cue_path,
        r#"PERFORMER "Artist Name"
TITLE "Album Title"
FILE "test.flac" WAVE
  TRACK 01 AUDIO
TITLE "Track One"
INDEX 01 00:00:00
"#,
    )
    .expect("write cue");
    let cue_sheet = crate::cue_flac::parse_cue_sheet(&cue_path).expect("parse cue");
    let cue_pair = CueFlacAnalysis {
        cue_sheet,
        audio_files: vec![CueAnalyzedAudioFile {
            file_reference: "test.flac".to_string(),
            path: audio_path,
            probe: probe_result(44_100, 0),
        }],
    };
    let now = chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);

    let error =
        cue_backed_audio_format("track-id", &cue_pair, 0, "audio-format-id".to_string(), now)
            .expect_err("zero-channel CUE audio format should fail");

    assert!(matches!(
        &error,
        ImportError::UnusableFile { detail } if detail.contains("unusable audio format")
    ));
}

#[test]
fn cue_backed_segments_ignore_rejected_index00_boundaries() {
    let temp = tempfile::tempdir().expect("tempdir");
    let cue_path = temp.path().join("Album.cue");
    let audio_path = temp.path().join("test.ape");
    std::fs::write(
        &cue_path,
        r#"PERFORMER "Artist Name"
TITLE "Album Title"
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
"#,
    )
    .expect("write cue");

    let cue_sheet = crate::cue_flac::parse_cue_sheet(&cue_path).expect("parse cue");
    let cue_pair = Arc::new(CueFlacAnalysis {
        cue_sheet,
        audio_files: vec![CueAnalyzedAudioFile {
            file_reference: "test.ape".to_string(),
            path: audio_path.clone(),
            probe: ProbeResult {
                content_type: ContentType::Ape,
                duration: std::time::Duration::from_secs(12 * 60),
                sample_rate: 75,
                bits_per_sample: Some(16),
                bitrate_kbps: None,
                channels: 2,
            },
        }],
    });

    let tracks_to_files: Vec<_> = cue_pair
        .cue_sheet
        .playable_tracks()
        .enumerate()
        .map(|(index, track)| TrackFile::CueBacked {
            db_track: crate::db::DbTrack::new_test(
                "release-id",
                &format!("track-{index}"),
                track.title.as_deref().unwrap_or("Track Title"),
                Some(track.number as i32),
            ),
            file_path: audio_path.clone(),
            cue_pair: Arc::clone(&cue_pair),
            cue_index: index,
        })
        .collect();
    let file_ids = HashMap::from([(audio_path, "file-id".to_string())]);
    let clock = coven::FixedClock(
        chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc),
    );
    let ids = coven::SequentialIdProvider::new("audio");

    let built = ImportService::build_audio_formats(&tracks_to_files, &file_ids, &clock, &ids)
        .expect("build audio formats");
    let main_segments: Vec<_> = built
        .audio_segments
        .iter()
        .filter(|segment| segment.role == DbAudioSegmentRole::Main)
        .collect();

    assert_eq!(main_segments.len(), 4);
    for segment in &main_segments {
        assert!(
            segment
                .end_sample
                .is_none_or(|end_sample| end_sample > segment.start_sample),
            "main segment must have a positive sample window: {segment:?}",
        );
    }
    assert_eq!(main_segments[1].end_sample, Some((8 * 60 + 31) * 75 + 20),);
    assert_eq!(main_segments[2].end_sample, Some((11 * 60 + 1) * 75 + 30),);
}
