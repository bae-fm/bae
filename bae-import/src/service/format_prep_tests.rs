use super::*;
use crate::types::{CueAnalyzedAudioFile, CueFlacAnalysis, TrackFile};
use bae_audio::ProbeResult;
use bae_util::content_type::ContentType;

pub(super) fn scanned_flac() -> crate::folder_scanner::ScannedAudio {
    crate::folder_scanner::ScannedAudio {
        content_type: ContentType::Flac,
        container: bae_util::audio_container::AudioContainer::Flac,
        duration_ms: 1_000,
        format: bae_types::audio_format::AudioFormat {
            codec: "FLAC".to_string(),
            sample_rate_hz: 44_100,
            bits_per_sample: Some(16),
            bitrate_kbps: None,
            channels: 2,
        },
    }
}

pub(super) fn test_clock() -> coven::FixedClock {
    coven::FixedClock::new(
        (chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc))
        .into(),
    )
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
        db_track: bae_records::DbTrack::new_test("release-id", "track-0", "Track Title", 1),
        audio: crate::TrackAudio::Standalone {
            file_path: path.clone(),
            source_audio: scanned_flac(),
        },
    };
    let file_ids = HashMap::from([(path, "file-0".to_string())]);
    let ids = coven::SequentialIds::new();

    let built =
        build_audio_formats(&[track], &file_ids, &test_clock(), &ids).expect("build audio formats");

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

pub(super) fn probe_result(sample_rate: u32, channels: u32) -> ProbeResult {
    let count = |count| std::num::NonZeroU32::new(count).expect("a test count");
    ProbeResult {
        content_type: ContentType::Flac,
        container: bae_util::audio_container::AudioContainer::Flac,
        duration: std::time::Duration::from_secs(1),
        sample_rate: count(sample_rate),
        bits_per_sample: Some(16),
        bitrate_kbps: None,
        channels: count(channels),
    }
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
    let cue_sheet = crate::cue_sheet::parse_cue_sheet(&cue_path).expect("parse cue");
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
