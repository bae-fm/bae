use super::*;
use bae_audio::ProbeResult;
use crate::import::types::{CueAnalyzedAudioFile, CueFlacAnalysis, TrackFile};
use std::sync::Arc;

fn scanned_non_audio(
    path: &str,
    role: crate::import::folder_scanner::FileRole,
) -> crate::import::folder_scanner::CandidateFile {
    crate::import::folder_scanner::CandidateFile {
        file: crate::import::folder_scanner::ScannedFile::new(
            PathBuf::from(path),
            Path::new(path)
                .file_name()
                .expect("test path has a file name")
                .to_string_lossy()
                .into_owned(),
            1,
            0,
        ),
        role,
    }
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
    use crate::import::folder_scanner::FileRole;
    assert_eq!(
        resolve_file_content_type(&scanned_non_audio("/x/cover.jpg", FileRole::Artwork)).unwrap(),
        ContentType::Jpeg
    );
    assert_eq!(
        resolve_file_content_type(&scanned_non_audio("/x/back.png", FileRole::Artwork)).unwrap(),
        ContentType::Png
    );
    assert_eq!(
        resolve_file_content_type(&scanned_non_audio("/x/notes.txt", FileRole::Document))
            .unwrap(),
        ContentType::PlainText
    );
    assert_eq!(
        resolve_file_content_type(&scanned_non_audio("/x/booklet.pdf", FileRole::Document))
            .unwrap(),
        ContentType::Pdf
    );
    // An extension the hint can't classify becomes opaque binary.
    assert_eq!(
        resolve_file_content_type(&scanned_non_audio("/x/data.bin", FileRole::Other)).unwrap(),
        ContentType::OctetStream
    );
}

/// A damaged image keeps its bytes but not its extension's claim: the library
/// reads a file stored as an image as a picture, and this one is not.
#[test]
fn a_damaged_image_is_stored_as_plain_bytes() {
    assert_eq!(
        resolve_file_content_type(&scanned_non_audio(
            "/x/front.png",
            crate::import::folder_scanner::FileRole::DamagedImage
        ))
        .unwrap(),
        ContentType::OctetStream
    );
}

/// A rip folder can hold an extensionless file — a `README`, a bare
/// checksum — and the release carries it like any other. It is opaque
/// binary, not a reason to fail the import.
#[test]
fn resolve_file_content_type_without_an_extension_is_opaque_binary() {
    assert_eq!(
        resolve_file_content_type(&scanned_non_audio(
            "/x/README",
            crate::import::folder_scanner::FileRole::Other
        ))
        .unwrap(),
        ContentType::OctetStream
    );
}

fn test_clock() -> coven::FixedClock {
    coven::FixedClock::new((chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc)).into())
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
        chrono::DateTime::<chrono::Utc>::from(coven::Clock::now(&test_clock())),
    )
    .expect("build audio format");

    assert_eq!(format.pregap_samples, Some(88_200));
}

/// A CUE track whose INDEX 00 sits after the previous track's INDEX 01 is a
/// real audio pregap (a hidden intro / count-in), so the pregap bytes come
/// from the container itself. `cue_segments` must emit an `AudioPregap`
/// segment spanning `[INDEX 00, INDEX 01)` ahead of the `Main` segment —
/// the branch `cue_backed_segments_ignore_rejected_index00_boundaries` never
/// reaches, because its INDEX 00 values are all bogus (before the prior
/// track) and the parse drops them.
///
/// Backed by the real APE CUE fixture so `seek_landing_bytes` produces real
/// byte offsets: asserts the sample windows *and* the byte windows,
/// including the contiguity invariant — every segment's `start_byte` equals
/// its predecessor's `end_byte`, so the album's read-ahead spans chain end
/// to end with no gaps or overlaps.
#[test]
fn cue_backed_segments_emit_audio_pregap_byte_window() {
    bae_audio::init();
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
    let probe = bae_audio::probe_audio_from_path(
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
        .map(|(index, track)| TrackFile {
            db_track: crate::db::DbTrack::new_test(
                "release-id",
                &format!("track-{index}"),
                track.title.as_deref().unwrap_or("Track Title"),
                track.number,
            ),
            audio: crate::import::TrackAudio::CueBacked {
                cue_pair: Arc::clone(&cue_pair),
                cue_index: index,
            },
        })
        .collect();
    let file_ids = HashMap::from([(audio_path, "file-id".to_string())]);
    let ids = coven::SequentialIds::new();

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
    let sample_rate = u64::from(built.audio_formats[1].sample_rate.get());
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
    let track = TrackFile {
        db_track: crate::db::DbTrack::new_test("release-id", "track-0", "Track Title", 1),
        audio: crate::import::TrackAudio::Standalone {
            file_path: path.clone(),
            source_audio: scanned_flac(),
        },
    };
    let file_ids = HashMap::from([(path, "file-0".to_string())]);
    let ids = coven::SequentialIds::new();

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

/// A track's stored depth is 1 to 32 bits; audio stating a deeper one is
/// refused by name at import rather than stored.
#[test]
fn cue_backed_audio_format_refuses_a_depth_over_32_bits() {
    let temp = tempfile::tempdir().expect("tempdir");
    let cue_path = temp.path().join("Album.cue");
    let audio_path = temp.path().join("test.wav");
    std::fs::write(
        &cue_path,
        r#"PERFORMER "Artist Name"
TITLE "Album Title"
FILE "test.wav" WAVE
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
            file_reference: "test.wav".to_string(),
            path: audio_path,
            probe: ProbeResult {
                content_type: ContentType::Pcm,
                bits_per_sample: Some(64),
                ..probe_result(44_100, 2)
            },
        }],
    };
    let now = chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);

    let error =
        cue_backed_audio_format("track-id", &cue_pair, 0, "audio-format-id".to_string(), now)
            .expect_err("a 64-bit depth is not stored");

    assert!(matches!(
        &error,
        ImportError::UnusableFile { detail }
            if detail.contains("test.wav") && detail.contains("64-bit")
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
        .map(|(index, track)| TrackFile {
            db_track: crate::db::DbTrack::new_test(
                "release-id",
                &format!("track-{index}"),
                track.title.as_deref().unwrap_or("Track Title"),
                track.number,
            ),
            audio: crate::import::TrackAudio::CueBacked {
                cue_pair: Arc::clone(&cue_pair),
                cue_index: index,
            },
        })
        .collect();
    let file_ids = HashMap::from([(audio_path, "file-id".to_string())]);
    let clock = coven::FixedClock::new((chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc)).into());
    let ids = coven::SequentialIds::new();

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

/// An enhanced CD ripped to one file: the audio track's slice ends where the
/// data track after it starts, and the segment the import writes ends there
/// too — the length the pane shows and the audio the import keeps are one
/// window.
#[test]
fn a_slice_before_a_data_track_ends_where_the_data_starts() {
    let temp = tempfile::tempdir().expect("tempdir");
    let cue_path = temp.path().join("album.cue");
    std::fs::write(
        &cue_path,
        "FILE \"image.flac\" WAVE\n  TRACK 01 AUDIO\n    INDEX 01 00:00:00\n  \
         TRACK 02 MODE1/2352\n    INDEX 01 00:05:00\n",
    )
    .expect("write cue");
    let cue_sheet = crate::cue_flac::parse_cue_sheet(&cue_path).expect("parse cue");
    let path = temp.path().join("image.flac");
    let analysis = CueFlacAnalysis {
        cue_sheet,
        audio_files: vec![CueAnalyzedAudioFile {
            file_reference: "image.flac".to_string(),
            path: path.clone(),
            probe: ProbeResult {
                content_type: ContentType::Flac,
                duration: std::time::Duration::from_secs(20),
                sample_rate: 44_100,
                bits_per_sample: Some(16),
                bitrate_kbps: None,
                channels: 2,
            },
        }],
    };

    let segments = ImportService::cue_segments(
        "format-1",
        &analysis,
        0,
        &HashMap::from([(path, "file-1".to_string())]),
        &HashMap::new(),
        &coven::SequentialIds::new(),
        chrono::DateTime::<chrono::Utc>::from(coven::Clock::now(&test_clock())),
    )
    .expect("build segments");

    let [main] = segments.as_slice() else {
        panic!("one main segment, got {segments:?}");
    };
    assert_eq!(main.end_sample, Some(5 * 44_100));
    assert_eq!(
        crate::import::probe::sheet_track_duration_ms(&analysis, 0, "album.cue").unwrap(),
        5_000,
        "the slice's own length reads the same boundary"
    );
}

/// Collects a decode's interleaved samples.
#[derive(Default)]
struct CollectedSamples(Vec<i32>);

impl bae_audio::DecodedSink for CollectedSamples {
    fn on_format(&mut self, _sample_rate: u32, _channels: u32) {}

    fn on_samples(&mut self, samples: &[i32]) {
        self.0.extend_from_slice(samples);
    }
}

/// Per-track files where track 2's INDEX 00 sits in the final frames of the
/// file before (its pregap is that file's tail). The pregap's start sample
/// lies in the third-to-last FLAC frame: a frame FFmpeg's FLAC binary search
/// can't place, since it can't read the timestamps of a stream's final
/// frames. Both the pregap segment and track 1's main segment, which ends
/// where the pregap begins, still get that frame's byte, and the pregap
/// decodes whole from it.
#[test]
fn a_pregap_in_the_final_frames_of_the_file_before_gets_its_byte() {
    bae_audio::init();
    let temp = tempfile::tempdir().expect("tempdir");
    let sample_rate = 44_100u64;
    // Two seconds: 150 CD frames of 588 samples each.
    let first_len = 2 * sample_rate;
    let pregap_start = first_len - 7_644; // 13 CD frames before the file's end
    let flac_of = |frames: u64| {
        let samples: Vec<i32> = (0..frames * 2)
            .map(|i| ((i as f64 * 0.013).sin() * 0.5 * i32::MAX as f64) as i32)
            .collect();
        bae_audio::encode_i32(
            bae_audio::EncodeFormat::Flac {
                bits_per_sample: 16,
            },
            &samples,
            sample_rate as u32,
            2,
        )
        .expect("encode FLAC")
    };
    let first_path = temp.path().join("first.flac");
    let second_path = temp.path().join("second.flac");
    let first = flac_of(first_len);
    std::fs::write(&first_path, &first).expect("write first.flac");
    std::fs::write(&second_path, flac_of(sample_rate)).expect("write second.flac");
    let cue_path = temp.path().join("album.cue");
    std::fs::write(
        &cue_path,
        r#"FILE "first.flac" WAVE
  TRACK 01 AUDIO
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    INDEX 00 00:01:62
FILE "second.flac" WAVE
    INDEX 01 00:00:00
"#,
    )
    .expect("write cue");

    let probe = |path: &std::path::Path| {
        bae_audio::probe_audio_from_path(path.to_str().expect("UTF-8 path"))
            .expect("probe")
    };
    let cue_pair = Arc::new(CueFlacAnalysis {
        cue_sheet: crate::cue_flac::parse_cue_sheet(&cue_path).expect("parse cue"),
        audio_files: vec![
            CueAnalyzedAudioFile {
                file_reference: "first.flac".to_string(),
                path: first_path.clone(),
                probe: probe(&first_path),
            },
            CueAnalyzedAudioFile {
                file_reference: "second.flac".to_string(),
                path: second_path.clone(),
                probe: probe(&second_path),
            },
        ],
    });
    let tracks_to_files: Vec<_> = (0..2u32)
        .map(|index| TrackFile {
            db_track: crate::db::DbTrack::new_test(
                "release-id",
                &format!("track-{index}"),
                "Track Title",
                index + 1,
            ),
            audio: crate::import::TrackAudio::CueBacked {
                cue_pair: Arc::clone(&cue_pair),
                cue_index: index as usize,
            },
        })
        .collect();
    let file_ids = HashMap::from([
        (first_path, "first".to_string()),
        (second_path, "second".to_string()),
    ]);

    let built = ImportService::build_audio_formats(
        &tracks_to_files,
        &file_ids,
        &test_clock(),
        &coven::SequentialIds::new(),
    )
    .expect("build audio formats");

    let [track1_main, pregap, _track2_main] = built.audio_segments.as_slice() else {
        panic!("three segments, got {:?}", built.audio_segments);
    };
    assert_eq!(pregap.role, DbAudioSegmentRole::AudioPregap);
    assert_eq!(pregap.file_id, "first");
    assert_eq!(pregap.start_sample, pregap_start);
    assert_eq!(pregap.end_sample, Some(first_len));
    let pregap_byte = pregap.start_byte.expect("the pregap's start byte");
    assert_eq!(track1_main.end_sample, Some(pregap_start));
    assert_eq!(
        track1_main.end_byte,
        Some(pregap_byte),
        "track 1 ends where track 2's pregap begins"
    );

    let buffer = || {
        let buffer = bae_audio::sparse_buffer::create_sparse_buffer(first.len() as u64);
        buffer.append_at(0, &first);
        buffer
    };
    let ground = bae_audio::decode_audio(buffer(), None, None).expect("decode first");
    let decode = crate::playback::stream_pipeline::StreamDecodeParams::new(
        vec![crate::playback::stream_pipeline::SegmentDecodeParams::new(
            buffer(),
            pregap.span(),
            0,
        )],
        true,
        0,
        0,
    );
    let mut decoded = CollectedSamples::default();
    decode
        .run_to_sink(
            sample_rate as u32,
            2,
            &mut decoded,
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
        .expect("decode the pregap");
    assert!(
        decoded.0 == ground.samples[pregap_start as usize * 2..],
        "the pregap decodes exactly the file's last {} samples from its byte, got {}",
        first_len - pregap_start,
        decoded.0.len() / 2,
    );
}

/// Ten seconds of a stereo tone encoded as `format`, written as `name` with
/// a CUE sheet that starts track 2 three seconds in, and the audio formats
/// import builds for its two tracks.
fn lossy_image(
    dir: &std::path::Path,
    name: &str,
    format: bae_audio::EncodeFormat,
) -> (Vec<u8>, BuiltAudioFormats) {
    let samples: Vec<i32> = (0..44_100 * 10 * 2)
        .map(|i| (((i / 2) as f64 * 0.031).sin() * 0.3 * i32::MAX as f64) as i32)
        .collect();
    let bytes = bae_audio::encode_i32(format, &samples, 44_100, 2).expect("encode the image");
    let path = dir.join(name);
    std::fs::write(&path, &bytes).expect("write the image");
    let cue_path = dir.join(format!("{name}.cue"));
    std::fs::write(
        &cue_path,
        format!(
            "FILE \"{name}\" WAVE\n  TRACK 01 AUDIO\n    INDEX 01 00:00:00\n  \
             TRACK 02 AUDIO\n    INDEX 01 00:03:00\n"
        ),
    )
    .expect("write the cue");
    let probe =
        bae_audio::probe_audio_from_path(path.to_str().expect("UTF-8 path")).expect("probe");
    let cue_pair = Arc::new(CueFlacAnalysis {
        cue_sheet: crate::cue_flac::parse_cue_sheet(&cue_path).expect("parse cue"),
        audio_files: vec![CueAnalyzedAudioFile {
            file_reference: name.to_string(),
            path: path.clone(),
            probe,
        }],
    });
    let tracks: Vec<_> = (0..2u32)
        .map(|index| TrackFile {
            db_track: crate::db::DbTrack::new_test(
                "release-id",
                &format!("track-{index}"),
                "Track Title",
                index + 1,
            ),
            audio: crate::import::TrackAudio::CueBacked {
                cue_pair: Arc::clone(&cue_pair),
                cue_index: index as usize,
            },
        })
        .collect();
    let built = ImportService::build_audio_formats(
        &tracks,
        &HashMap::from([(path, "image".to_string())]),
        &test_clock(),
        &coven::SequentialIds::new(),
    )
    .expect("build audio formats");
    (bytes, built)
}

/// Records the byte and first sample of each packet a decode reads.
#[derive(Default)]
struct PacketPlaces(Vec<bae_audio::DecodedPacket>);

impl bae_audio::DecodedSink for PacketPlaces {
    fn on_format(&mut self, _sample_rate: u32, _channels: u32) {}
    fn on_samples(&mut self, _samples: &[i32]) {}
    fn on_packet(&mut self, packet: bae_audio::DecodedPacket) {
        self.0.push(packet);
    }
}

/// Assert that the start byte import records for track 2 of a `format` image
/// is the frame its first sample is in, as a decode from the stream's start
/// places its frames.
fn assert_track_start_is_its_frame(name: &str, format: bae_audio::EncodeFormat) {
    let temp = tempfile::tempdir().expect("tempdir");
    let (bytes, built) = lossy_image(temp.path(), name, format);
    let track2 = built
        .audio_segments
        .iter()
        .find(|segment| segment.start_sample > 0)
        .expect("track 2's segment");

    let mut places = PacketPlaces::default();
    let buffer = bae_audio::sparse_buffer::create_sparse_buffer(bytes.len() as u64);
    buffer.append_at(0, &bytes);
    bae_audio::decode_audio_to_sink(
        buffer,
        None,
        None,
        &mut places,
        Arc::new(std::sync::atomic::AtomicBool::new(false)),
    )
    .expect("decode the image");
    let holding = places
        .0
        .iter()
        .rev()
        .find(|packet| packet.sample.is_some_and(|sample| sample <= track2.start_sample))
        .expect("a frame at or before the track's start");
    assert_eq!(
        track2.start_byte, holding.byte,
        "{name}: track 2 starts in the frame at {:?}",
        holding.byte
    );
}

/// An MP3 image's track start byte is the frame its first sample is in:
/// past the MP3's encoder delay, which its timestamps count and its samples
/// don't.
#[test]
fn an_mp3_image_tracks_start_byte_is_the_frame_holding_its_first_sample() {
    bae_audio::init();
    assert_track_start_is_its_frame("image.mp3", bae_audio::EncodeFormat::Mp3 { bitrate_kbps: 192 });
}
