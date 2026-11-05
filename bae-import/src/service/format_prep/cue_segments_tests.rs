//! Each CUE track's segments in the image it is cut from: a pregap in the
//! file before, index boundaries the sheet rejects, a data track, and the
//! byte each track's first sample lands on.

use super::*;
use crate::service::format_prep::tests::*;
use crate::service::format_prep::*;
use crate::types::{CueAnalyzedAudioFile, CueFlacAnalysis, TrackFile};
use bae_audio::ProbeResult;
use bae_db::DbTrack;
use bae_util::content_type::ContentType;
use coven::SequentialIds;
use std::sync::Arc;

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
    let cue_sheet = crate::cue_sheet::parse_cue_sheet(&cue_path).expect("parse cue");
    let make_probe = |seconds| ProbeResult {
        content_type: ContentType::Flac,
        duration: std::time::Duration::from_secs(seconds),
        sample_rate: std::num::NonZeroU32::new(44_100).expect("a test rate"),
        bits_per_sample: Some(16),
        bitrate_kbps: None,
        channels: std::num::NonZeroU32::new(2).expect("a test count"),
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

    let cue_sheet = crate::cue_sheet::parse_cue_sheet(&cue_path).expect("parse cue");
    let probe = bae_audio::probe_audio_from_path(&audio_path).expect("analyze ape");
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
            db_track: bae_db::DbTrack::new_test(
                "release-id",
                &format!("track-{index}"),
                track.title.as_deref().unwrap_or("Track Title"),
                track.number,
            ),
            audio: crate::TrackAudio::CueBacked {
                cue_pair: Arc::clone(&cue_pair),
                cue_index: index,
            },
        })
        .collect();
    let file_ids = HashMap::from([(audio_path, "file-id".to_string())]);
    let ids = coven::SequentialIds::new();

    let built = ImportService::build_audio_formats(
        &tracks_to_files,
        &file_ids,
        &test_clock(),
        &ids,
        &bae_diagnostics::Diagnostics::noop(),
    )
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
    assert_eq!(main_segments[0].start_byte, None, "track 1 starts at byte 0");
    assert_eq!(main_segments[2].end_byte, None, "track 3 runs to EOF");
}

/// INDEX 00 lines that fall before the previous track's INDEX 01 are dropped
/// by the parse, so each track's slice runs to the next track's INDEX 01. The
/// image is a real one at 75 Hz, a sample per CUE frame, so its track starts
/// read like any image's and no bug is reported.
#[test]
fn cue_backed_segments_ignore_rejected_index00_boundaries() {
    bae_audio::init();
    let temp = tempfile::tempdir().expect("tempdir");
    let cue_path = temp.path().join("Album.cue");
    let audio_path = temp.path().join("test.wav");
    let samples: Vec<i32> = (0..12 * 60 * 75 * 2)
        .map(|i| (((i / 2) as f64 * 0.3).sin() * 0.3 * i32::MAX as f64) as i32)
        .collect();
    std::fs::write(
        &audio_path,
        bae_audio::encode_i32(
            bae_audio::EncodeFormat::PcmWav { bits_per_sample: 16 },
            &samples,
            75,
            2,
        )
        .expect("encode the image"),
    )
    .expect("write the image");
    std::fs::write(
        &cue_path,
        r#"PERFORMER "Artist Name"
TITLE "Album Title"
FILE "test.wav" WAVE
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

    let cue_sheet = crate::cue_sheet::parse_cue_sheet(&cue_path).expect("parse cue");
    let cue_pair = Arc::new(CueFlacAnalysis {
        cue_sheet,
        audio_files: vec![CueAnalyzedAudioFile {
            file_reference: "test.wav".to_string(),
            path: audio_path.clone(),
            probe: bae_audio::probe_audio_from_path(&audio_path).expect("probe the image"),
        }],
    });

    let tracks_to_files: Vec<_> = cue_pair
        .cue_sheet
        .playable_tracks()
        .enumerate()
        .map(|(index, track)| TrackFile {
            db_track: bae_db::DbTrack::new_test(
                "release-id",
                &format!("track-{index}"),
                track.title.as_deref().unwrap_or("Track Title"),
                track.number,
            ),
            audio: crate::TrackAudio::CueBacked {
                cue_pair: Arc::clone(&cue_pair),
                cue_index: index,
            },
        })
        .collect();
    let file_ids = HashMap::from([(audio_path, "file-id".to_string())]);
    let clock = coven::FixedClock::new(
        (chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc))
        .into(),
    );
    let ids = coven::SequentialIds::new();
    let bugs = Arc::new(bae_diagnostics::RecordingBugReporter::default());

    let built = ImportService::build_audio_formats(
        &tracks_to_files,
        &file_ids,
        &clock,
        &ids,
        &bae_diagnostics::Diagnostics::noop().reporting_bugs_to(bugs.clone()),
    )
    .expect("build audio formats");
    let reports = bugs.reports();
    assert!(reports.is_empty(), "the image's track starts read: {reports:?}");
    let main_segments: Vec<_> = built
        .audio_segments
        .iter()
        .filter(|segment| segment.role == DbAudioSegmentRole::Main)
        .collect();

    assert_eq!(main_segments.len(), 4);
    for segment in &main_segments {
        assert!(
            segment.end_sample.is_none_or(|end_sample| end_sample > segment.start_sample),
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
    let cue_sheet = crate::cue_sheet::parse_cue_sheet(&cue_path).expect("parse cue");
    let path = temp.path().join("image.flac");
    let analysis = CueFlacAnalysis {
        cue_sheet,
        audio_files: vec![CueAnalyzedAudioFile {
            file_reference: "image.flac".to_string(),
            path: path.clone(),
            probe: ProbeResult {
                content_type: ContentType::Flac,
                duration: std::time::Duration::from_secs(20),
                sample_rate: std::num::NonZeroU32::new(44_100).expect("a test rate"),
                bits_per_sample: Some(16),
                bitrate_kbps: None,
                channels: std::num::NonZeroU32::new(2).expect("a test count"),
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
        crate::probe::sheet_track_duration_ms(&analysis, 0, "album.cue").unwrap(),
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
            bae_audio::EncodeFormat::Flac { bits_per_sample: 16 },
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

    let probe = |path: &std::path::Path| bae_audio::probe_audio_from_path(path).expect("probe");
    let cue_pair = Arc::new(CueFlacAnalysis {
        cue_sheet: crate::cue_sheet::parse_cue_sheet(&cue_path).expect("parse cue"),
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
            db_track: bae_db::DbTrack::new_test(
                "release-id",
                &format!("track-{index}"),
                "Track Title",
                index + 1,
            ),
            audio: crate::TrackAudio::CueBacked {
                cue_pair: Arc::clone(&cue_pair),
                cue_index: index as usize,
            },
        })
        .collect();
    let file_ids =
        HashMap::from([(first_path, "first".to_string()), (second_path, "second".to_string())]);

    let built = ImportService::build_audio_formats(
        &tracks_to_files,
        &file_ids,
        &test_clock(),
        &coven::SequentialIds::new(),
        &bae_diagnostics::Diagnostics::noop(),
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
    let decode = bae_track_audio::StreamDecodeParams::new(
        vec![bae_track_audio::SegmentDecodeParams::new(buffer(), pregap.span(), 0)],
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
    let (bytes, segments) = cue_image(temp.path(), name, format);
    let track2 =
        segments.iter().find(|segment| segment.start_sample > 0).expect("track 2's segment");

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
        .find(|packet| {
            packet.sample.is_some_and(|sample| {
                sample <= i64::try_from(track2.start_sample).expect("a track start fits i64")
            })
        })
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
    assert_track_start_is_its_frame(
        "image.mp3",
        bae_audio::EncodeFormat::Mp3 { bitrate_kbps: 192 },
    );
}

/// An AAC (MP4) image's track start byte is the frame its first sample is
/// in, though MP4's demuxer seeks by moving its place in the sample table and
/// reads nothing until the next packet.
#[test]
fn an_aac_image_tracks_start_byte_is_the_frame_holding_its_first_sample() {
    bae_audio::init();
    assert_track_start_is_its_frame(
        "image.m4a",
        bae_audio::EncodeFormat::Aac { bitrate_kbps: 192 },
    );
}

/// The scan probed every image a sheet names, so where its tracks start can
/// always be read; an image where it can't is a bug. It reaches the crash
/// reporter naming the image and why, and the import goes on with the
/// image's tracks holding no byte window.
#[test]
fn an_image_whose_track_starts_cannot_be_read_is_reported_as_a_bug() {
    bae_audio::init();
    let temp = tempfile::tempdir().expect("tempdir");
    let cue_path = temp.path().join("album.cue");
    std::fs::write(
        &cue_path,
        "FILE \"image.flac\" WAVE\n  TRACK 01 AUDIO\n    INDEX 01 00:00:00\n  \
         TRACK 02 AUDIO\n    INDEX 01 00:05:00\n",
    )
    .expect("write cue");
    let path = temp.path().join("image.flac");
    std::fs::write(&path, b"bytes FFmpeg reads as no audio").expect("write image");
    let cue_pair = Arc::new(CueFlacAnalysis {
        cue_sheet: crate::cue_sheet::parse_cue_sheet(&cue_path).expect("parse cue"),
        audio_files: vec![CueAnalyzedAudioFile {
            file_reference: "image.flac".to_string(),
            path: path.clone(),
            probe: probe_result(44_100, 2),
        }],
    });
    let tracks_to_files: Vec<_> = (0..2u32)
        .map(|index| TrackFile {
            db_track: bae_db::DbTrack::new_test(
                "release-id",
                &format!("track-{index}"),
                "Track Title",
                index + 1,
            ),
            audio: crate::TrackAudio::CueBacked {
                cue_pair: Arc::clone(&cue_pair),
                cue_index: index as usize,
            },
        })
        .collect();
    let bugs = Arc::new(bae_diagnostics::RecordingBugReporter::default());

    let built = ImportService::build_audio_formats(
        &tracks_to_files,
        &HashMap::from([(path.clone(), "image".to_string())]),
        &test_clock(),
        &coven::SequentialIds::new(),
        &bae_diagnostics::Diagnostics::noop().reporting_bugs_to(bugs.clone()),
    )
    .expect("the import goes on");

    let reports = bugs.reports();
    let [(operation, detail)] = reports.as_slice() else {
        panic!("one bug reported, got {reports:?}");
    };
    assert_eq!(operation, "read where each track of a CUE image starts");
    assert!(detail.contains(&path.display().to_string()), "the report names the image: {detail}");
    assert!(
        built
            .audio_segments
            .iter()
            .all(|segment| segment.start_byte.is_none() && segment.end_byte.is_none()),
        "the image's tracks hold no byte window: {:?}",
        built.audio_segments
    );
}

/// Build the CUE-backed track list for a single-file CUE album, reusing
/// the same codec probe the scanner stores on the candidate.
fn cue_backed_tracks(dir: &str) -> Vec<TrackFile> {
    let audio_path = PathBuf::from(format!("{dir}/Test Album.ape"));
    let cue_path = PathBuf::from(format!("{dir}/Test Album.cue"));
    let cue_sheet = crate::cue_sheet::parse_cue_sheet(&cue_path).expect("parse cue");
    let probe = bae_audio::probe_audio_from_path(&audio_path).expect("analyze ape");
    let cue_pair = Arc::new(crate::types::CueFlacAnalysis {
        cue_sheet,
        audio_files: vec![crate::types::CueAnalyzedAudioFile {
            file_reference: "Test Album.ape".to_string(),
            path: audio_path.clone(),
            probe,
        }],
    });
    (0..cue_pair.cue_sheet.tracks.len())
        .map(|index| TrackFile {
            db_track: DbTrack {
                id: format!("track-{index}"),
                release_id: "rel".to_string(),
                title: format!("Track {index}"),
                medium_id: bae_db::identity::release_medium_id("rel", 1),
                side: std::num::NonZeroU8::MIN,
                track_number: std::num::NonZeroU32::new(
                    u32::try_from(index + 1).expect("a test track number fits u32"),
                )
                .expect("track numbers count from one"),
                duration_ms: 180_000,
                created_at: chrono::DateTime::<chrono::Utc>::from(coven::Clock::now(&test_clock())),
            },
            audio: crate::TrackAudio::CueBacked {
                cue_pair: Arc::clone(&cue_pair),
                cue_index: index,
            },
        })
        .collect()
}

/// A CUE track's read-ahead ceiling is its `end_byte`; playback fills up to it
/// and stops, so every non-last track must carry a real end byte or the fill
/// streams the whole rest of the shared file. Ends derive from the next track's
/// start byte (`start[N+1]`), computed via `seek_landing_bytes` -- the AVIO
/// landing, defined for every format, including APE whose packets carry no byte
/// position. This drives `build_audio_formats` over the APE CUE fixture and
/// asserts the user-visible outcome: the two non-last tracks get `Some`,
/// ascending, in-file end bytes and the last track runs to EOF (`None`).
#[test]
fn build_audio_formats_gives_ape_cue_tracks_real_end_bytes() {
    bae_audio::init();
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/cue_ape");
    let tracks = cue_backed_tracks(dir);
    assert_eq!(tracks.len(), 3, "fixture is a 3-track single-file CUE album");

    let file_size = std::fs::metadata(format!("{dir}/Test Album.ape")).unwrap().len();
    let mut file_ids = HashMap::new();
    file_ids.insert(PathBuf::from(format!("{dir}/Test Album.ape")), "file-1".to_string());

    let built = ImportService::build_audio_formats(
        &tracks,
        &file_ids,
        &test_clock(),
        &SequentialIds::new(),
        &bae_diagnostics::Diagnostics::noop(),
    )
    .expect("build_audio_formats");

    let main_segments: Vec<_> = built
        .audio_segments
        .iter()
        .filter(|segment| segment.role == bae_db::DbAudioSegmentRole::Main)
        .collect();
    let ends: Vec<Option<u64>> = main_segments.iter().map(|segment| segment.end_byte).collect();
    // Non-last tracks carry a real, ascending, in-file end byte.
    let e0 = ends[0].expect("track 1 (non-last) must have an end byte");
    let e1 = ends[1].expect("track 2 (non-last) must have an end byte");
    assert!(e0 > 0 && e0 < file_size, "track 1 end within file: {e0} of {file_size}");
    assert!(e1 > 0 && e1 < file_size, "track 2 end within file: {e1} of {file_size}");
    assert!(e1 > e0, "end bytes ascend track to track: {ends:?}");
    // The last track runs to EOF.
    assert_eq!(ends[2], None, "the last track runs to EOF");

    // Each track's end is the next track's start byte -- one boundary, not two.
    assert_eq!(main_segments[1].start_byte, Some(e0), "track 2 starts where track 1 ends");
    assert_eq!(main_segments[2].start_byte, Some(e1), "track 3 starts where track 2 ends");
}

/// Ten seconds of a stereo tone encoded as `format`, written into `dir` as
/// `name` with a CUE sheet that starts track 2 three seconds in. Returns the
/// image's bytes and the segments import builds for its two tracks: track 1
/// first, then track 2, each in the file `"image"`.
fn cue_image(
    dir: &std::path::Path,
    name: &str,
    format: bae_audio::EncodeFormat,
) -> (Vec<u8>, Vec<bae_db::DbAudioSegment>) {
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
    let probe = bae_audio::probe_audio_from_path(&path).expect("probe");
    let cue_pair = Arc::new(CueFlacAnalysis {
        cue_sheet: crate::cue_sheet::parse_cue_sheet(&cue_path).expect("parse cue"),
        audio_files: vec![CueAnalyzedAudioFile {
            file_reference: name.to_string(),
            path: path.clone(),
            probe,
        }],
    });
    let tracks: Vec<_> = (0..2u32)
        .map(|index| TrackFile {
            db_track: bae_db::DbTrack::new_test(
                "release-id",
                &format!("track-{index}"),
                "Track Title",
                index + 1,
            ),
            audio: crate::TrackAudio::CueBacked {
                cue_pair: Arc::clone(&cue_pair),
                cue_index: index as usize,
            },
        })
        .collect();
    let built = crate::ImportService::build_audio_formats(
        &tracks,
        &HashMap::from([(path, "image".to_string())]),
        &coven::FixedClock::new(std::time::SystemTime::UNIX_EPOCH),
        &coven::SequentialIds::new(),
        &bae_diagnostics::Diagnostics::noop(),
    )
    .expect("build audio formats");
    (bytes, built.audio_segments)
}
