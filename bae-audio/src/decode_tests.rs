#![expect(
    clippy::disallowed_methods,
    reason = "test code: what it spawns ends with the test, which has no crash reporter to report to"
)]

use super::*;
use crate::test_samples::{filled_sparse_buffer, wav_with_fmt};
use crate::{decode_audio_to_verifying_sink, encode_i32, init, DecodeStart, EncodeFormat};
use std::sync::Arc;

#[derive(Default)]
struct TestDecodedSink {
    format: Option<(u32, u32)>,
}

impl DecodedSink for TestDecodedSink {
    fn on_format(&mut self, sample_rate: u32, channels: u32) {
        self.format = Some((sample_rate, channels));
    }

    fn on_samples(&mut self, _samples: &[i32]) {}
}

fn rational(num: c_int, den: c_int) -> ffmpeg_sys_next::AVRational {
    ffmpeg_sys_next::AVRational { num, den }
}

#[test]
fn a_sample_clock_counts_samples_from_the_streams_start() {
    // A time base of one sample: timestamps count samples.
    let samples = SampleClock::new(rational(1, 44_100), 44_100, 0);
    assert_eq!(samples.sample_at(1000).unwrap(), 1000);
    assert_eq!(samples.timestamp_of(1000), 1000);
    // A millisecond time base: 500 ms is 22,050 samples, and back.
    let millis = SampleClock::new(rational(1, 1000), 44_100, 0);
    assert_eq!(millis.sample_at(500).unwrap(), 22_050);
    assert_eq!(millis.timestamp_of(22_050), 500);
    // An MP3's: 1/14,112,000 s, its first sample after a 1,105-sample delay.
    let mp3 = SampleClock::new(rational(1, 14_112_000), 44_100, 1_105 * 320);
    assert_eq!(mp3.sample_at(1_105 * 320).unwrap(), 0);
    assert_eq!(mp3.sample_at(0).unwrap(), -1_105);
    assert_eq!(mp3.timestamp_of(4_000), (1_105 + 4_000) * 320);
    assert_eq!(mp3.timestamp_of(-1_105), 0);
}

#[test]
fn frame_sample_bounds_tracks_position_across_pts_gaps() {
    let clock = SampleClock::new(rational(1, 44_100), 44_100, 0);

    // A valid pts places the frame and advances the tracked position to its end.
    let mut tracked = None;
    assert_eq!(frame_sample_bounds(1000, 100, &clock, &mut tracked), Ok(Some((1000, 1100))));
    assert_eq!(tracked, Some(1100));

    // A frame with no pts continues from the tracked position.
    assert_eq!(
        frame_sample_bounds(ffmpeg_sys_next::AV_NOPTS_VALUE, 200, &clock, &mut tracked),
        Ok(Some((1100, 1300)))
    );
    assert_eq!(tracked, Some(1300));

    // No pts and no tracked position yet: the frame is unplaced, and so is
    // the next.
    let mut untracked = None;
    assert_eq!(
        frame_sample_bounds(ffmpeg_sys_next::AV_NOPTS_VALUE, 50, &clock, &mut untracked),
        Ok(None)
    );
    assert_eq!(untracked, None);

    // A frame before the stream's first sample (an encoder delay the decoder
    // kept) is placed before it.
    let mut before = None;
    assert_eq!(frame_sample_bounds(-312, 960, &clock, &mut before), Ok(Some((-312, 648))));
}

/// Timestamps near the ends of the `i64` range fail the decode instead of
/// wrapping to a wrong sample number.
#[test]
fn timestamps_that_overflow_the_sample_count_are_refused() {
    let clock = SampleClock::new(rational(1, 44_100), 44_100, i64::MAX - 10);
    assert!(clock.sample_at(i64::MIN + 1).is_err());
    let mut tracked = None;
    assert!(frame_sample_bounds(-1_000, 10, &clock, &mut tracked).is_err());

    let clock = SampleClock::new(rational(1, 44_100), 44_100, 0);
    let mut tracked = None;
    assert!(frame_sample_bounds(i64::MAX - 5, 10, &clock, &mut tracked).is_err());

    // A frame stretching from near `i64::MIN` has no countable lead-in to
    // trim, and a window start past the sample count is no sample.
    assert!(frame_output_window(Some((i64::MIN + 1, i64::MAX)), 10, 2, Some(5), None).is_err());
    assert!(frame_output_window(Some((0, 100)), 10, 2, Some(u64::MAX), None).is_err());
}

#[test]
fn frame_output_window_trims_at_boundaries() {
    use FrameOutputWindow::*;

    struct Row {
        name: &'static str,
        bounds: Option<(i64, i64)>,
        output_len: usize,
        channels: usize,
        start: Option<u64>,
        end: Option<u64>,
        expected: FrameOutputWindow,
    }
    let rows = [
        Row {
            name: "no window emits whole frame",
            bounds: Some((0, 100)),
            output_len: 200,
            channels: 2,
            start: None,
            end: None,
            expected: Emit { skip_start: 0, take_end: 200, reached_end: false },
        },
        Row {
            name: "empty output with no window skips",
            bounds: Some((0, 0)),
            output_len: 0,
            channels: 2,
            start: None,
            end: None,
            expected: Skip,
        },
        Row {
            name: "frame entirely before start skips",
            bounds: Some((0, 100)),
            output_len: 200,
            channels: 2,
            start: Some(200),
            end: None,
            expected: Skip,
        },
        Row {
            name: "start at frame_end is exclusive, skips",
            bounds: Some((0, 100)),
            output_len: 200,
            channels: 2,
            start: Some(100),
            end: None,
            expected: Skip,
        },
        Row {
            name: "frame straddling start trims the lead-in",
            bounds: Some((0, 100)),
            output_len: 200,
            channels: 2,
            start: Some(40),
            end: None,
            expected: Emit { skip_start: 80, take_end: 200, reached_end: false },
        },
        Row {
            name: "frame at or past end stops",
            bounds: Some((200, 300)),
            output_len: 200,
            channels: 2,
            start: None,
            end: Some(200),
            expected: Stop,
        },
        Row {
            name: "frame straddling end trims the tail and reaches end",
            bounds: Some((0, 100)),
            output_len: 200,
            channels: 2,
            start: None,
            end: Some(60),
            expected: Emit { skip_start: 0, take_end: 120, reached_end: true },
        },
        Row {
            name: "take_end clamps to a short output buffer",
            bounds: Some((0, 100)),
            output_len: 50,
            channels: 1,
            start: None,
            end: Some(80),
            expected: Emit { skip_start: 0, take_end: 50, reached_end: true },
        },
        Row {
            name: "start-skip past a reached end stops",
            bounds: Some((0, 100)),
            output_len: 100,
            channels: 1,
            start: Some(70),
            end: Some(60),
            expected: Stop,
        },
        Row {
            name: "start-skip clamped to output with no end skips",
            bounds: Some((0, 100)),
            output_len: 50,
            channels: 1,
            start: Some(80),
            end: None,
            expected: Skip,
        },
        Row {
            name: "an unplaced frame disables trimming",
            bounds: None,
            output_len: 200,
            channels: 2,
            start: Some(50),
            end: Some(60),
            expected: Emit { skip_start: 0, take_end: 200, reached_end: false },
        },
    ];

    for row in rows {
        let got = frame_output_window(row.bounds, row.output_len, row.channels, row.start, row.end);
        assert_eq!(got, Ok(row.expected), "{}", row.name);
    }
}

#[test]
fn packed_s64_decoder_rejects_zero_channels_before_format_signal() {
    crate::init();

    unsafe {
        let fmt_ctx = ffmpeg_sys_next::avformat_alloc_context();
        assert!(!fmt_ctx.is_null());
        let stream = ffmpeg_sys_next::avformat_new_stream(fmt_ctx, ptr::null());
        assert!(!stream.is_null());
        (*stream).time_base = ffmpeg_sys_next::AVRational { num: 1, den: 44_100 };
        let mut codecpar = ffmpeg_sys_next::avcodec_parameters_alloc();
        assert!(!codecpar.is_null());
        (*codecpar).ch_layout.nb_channels = 0;

        let mut sink = TestDecodedSink::default();
        let result = decode_packed_s64_packets_to_sink(
            fmt_ctx, 0, stream, codecpar, 44_100, 0, None, None, &mut sink,
        );

        ffmpeg_sys_next::avcodec_parameters_free(&mut codecpar);
        ffmpeg_sys_next::avformat_free_context(fmt_ctx);

        let err = result.expect_err("zero-channel PCM should fail");
        assert!(
            err.contains("channel count must be greater than zero"),
            "unexpected decode error: {err}"
        );
        assert_eq!(sink.format, None);
    }
}

fn wav_extensible_pcm(
    bits_per_sample: u16,
    channels: u16,
    sample_rate: u32,
    data: &[u8],
) -> Vec<u8> {
    let block_align = channels * bits_per_sample / 8;
    let byte_rate = sample_rate * block_align as u32;
    let fmt_size = 40u32;
    let riff_size = 4 + 8 + fmt_size + 8 + data.len() as u32;
    let mut wav = Vec::with_capacity((riff_size + 8) as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&riff_size.to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&fmt_size.to_le_bytes());
    wav.extend_from_slice(&0xFFFEu16.to_le_bytes());
    wav.extend_from_slice(&channels.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&bits_per_sample.to_le_bytes());
    wav.extend_from_slice(&22u16.to_le_bytes());
    wav.extend_from_slice(&bits_per_sample.to_le_bytes());
    wav.extend_from_slice(&0u32.to_le_bytes());
    wav.extend_from_slice(&[
        0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0xaa, 0x00, 0x38, 0x9b,
        0x71,
    ]);
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(data.len() as u32).to_le_bytes());
    wav.extend_from_slice(data);
    wav
}

fn truncated_flac_packet_stream() -> Vec<u8> {
    let original_samples: Vec<i32> =
        (0..44100).map(|i| ((i as f64 * 0.01).sin() * 0.5 * i32::MAX as f64) as i32).collect();
    let mut flac_data =
        encode_i32(EncodeFormat::Flac { bits_per_sample: 16 }, &original_samples, 44100, 1)
            .unwrap();
    flac_data.truncate(flac_data.len() / 2);
    flac_data
}

fn mp3_with_invalid_terminal_packet() -> (Vec<u8>, usize) {
    let sample_rate = 44_100u32;
    let samples: Vec<i32> = (0..sample_rate as usize * 2)
        .map(|i| {
            let t = (i / 2) as f64 / sample_rate as f64;
            (0.5 * i32::MAX as f64 * (2.0 * std::f64::consts::PI * 440.0 * t).sin()) as i32
        })
        .collect();
    let clean =
        encode_i32(EncodeFormat::Mp3 { bitrate_kbps: 128 }, &samples, sample_rate, 2).unwrap();
    let expected_samples =
        decode_audio(filled_sparse_buffer(&clean), None, None).expect("clean decode").samples.len();
    let frame_start = clean
        .windows(2)
        .position(|bytes| bytes[0] == 0xff && bytes[1] & 0xe0 == 0xe0)
        .expect("encoded MP3 frame");
    let mut damaged = clean.clone();
    damaged.extend_from_slice(&clean[frame_start..frame_start + 16]);
    damaged.extend_from_slice(b"LYRICSBEGININD0000000LYRICS200TAG");
    (damaged, expected_samples)
}
/// Each PCM sample format decodes to full-range i32: the low input sample
/// lands near `i32::MIN`, the mid sample near zero, the high sample near
/// `i32::MAX`. Covers the unsigned-8-bit, 64-bit-float, and signed-64-bit
/// (`WAVE_FORMAT_EXTENSIBLE`) conversion paths.
#[test]
fn decode_audio_scales_pcm_sample_formats_to_full_range_i32() {
    init();

    struct Case {
        name: &'static str,
        wav: Vec<u8>,
        min_below: i32,
        max_above: i32,
    }

    let float64 = {
        let mut d = Vec::new();
        for sample in [-0.5f64, 0.0, 0.5] {
            d.extend_from_slice(&sample.to_le_bytes());
        }
        d
    };
    let signed64 = {
        let mut d = Vec::new();
        for sample in [i64::MIN / 2, 0, i64::MAX / 2] {
            d.extend_from_slice(&sample.to_le_bytes());
        }
        d
    };

    let cases = [
        Case {
            name: "unsigned 8-bit",
            wav: wav_with_fmt(1, 8, 1, 44_100, &[0, 128, 255]),
            min_below: -2_000_000_000,
            max_above: 2_000_000_000,
        },
        Case {
            name: "64-bit float",
            wav: wav_with_fmt(3, 64, 1, 44_100, &float64),
            min_below: -1_000_000_000,
            max_above: 1_000_000_000,
        },
        Case {
            name: "signed 64-bit extensible",
            wav: wav_extensible_pcm(64, 1, 44_100, &signed64),
            min_below: -1_000_000_000,
            max_above: 1_000_000_000,
        },
    ];

    for case in cases {
        let decoded = decode_audio(filled_sparse_buffer(&case.wav), None, None).unwrap();
        assert_eq!(decoded.samples.len(), 3, "{}", case.name);
        assert!(decoded.samples[0] < case.min_below, "{}: {:?}", case.name, decoded.samples);
        assert!(decoded.samples[1].abs() < 20_000_000, "{}: {:?}", case.name, decoded.samples);
        assert!(decoded.samples[2] > case.max_above, "{}: {:?}", case.name, decoded.samples);
    }
}

#[test]
fn decode_audio_decodes_audio_format_fixtures() {
    init();
    for name in [
        "placeholder-pcm.wav",
        "placeholder-pcm.aiff",
        "placeholder-opus.opus",
        "placeholder-vorbis.ogg",
        "placeholder-wavpack.wv",
        "placeholder-dsd.dsf",
        "placeholder-dsd.dff",
    ] {
        let bytes = std::fs::read(format!(
            "{}/test-fixtures/audio-format/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        let decoded = decode_audio(filled_sparse_buffer(&bytes), None, None)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(decoded.sample_rate > 0, "{name}");
        assert!(decoded.channels > 0, "{name}");
        assert!(!decoded.samples.is_empty(), "{name}");
    }
}

/// A buffer cancelled mid-decode (a failed fill, or the caller aborting) must
/// fail the i32 decode rather than return a truncated `Ok` — a consumer
/// treating a partial decode as the whole window (a save writing a short file
/// as success) would be silent corruption.
#[test]
fn i32_decode_fails_loud_when_the_buffer_is_cancelled_mid_stream() {
    use crate::sparse_buffer::create_sparse_buffer;

    init();

    struct NullSink;
    impl DecodedSink for NullSink {
        fn on_format(&mut self, _sample_rate: u32, _channels: u32) {}
        fn on_samples(&mut self, _samples: &[i32]) {}
    }

    let samples = vec![0i32; 44_100 * 2]; // 1s stereo silence
    let wav =
        encode_i32(EncodeFormat::PcmWav { bits_per_sample: 16 }, &samples, 44_100, 2).unwrap();

    // Only the front half is ever delivered; the decode blocks on the rest
    // until the cancel below unblocks it.
    let buffer = create_sparse_buffer(wav.len() as u64);
    buffer.append_at(0, &wav[..wav.len() / 2]);

    let decode_buffer = buffer.clone();
    let handle = std::thread::spawn(move || {
        let mut sink = NullSink;
        decode_audio_to_sink(
            decode_buffer,
            None,
            None,
            &mut sink,
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
    });

    // Cancel only once the decode is blocked on the missing back half.
    buffer.wait_until_parked(1);
    buffer.cancel();

    let result = handle.join().expect("decode thread");
    assert!(result.is_err(), "a cancelled input must fail the decode, not return a truncated Ok");
}

#[test]
fn decode_audio_rejects_truncated_flac_packet_stream() {
    init();

    let flac_data = truncated_flac_packet_stream();

    let err = decode_audio(filled_sparse_buffer(&flac_data), None, None)
        .expect_err("truncated FLAC must fail");
    assert!(
        matches!(&err, DecodeError::Decode(message) if message.contains("Failed to send packet")),
        "unexpected decode error: {err}"
    );
}

#[test]
#[cfg(desktop)]
fn sink_decode_discards_an_invalid_terminal_mp3_packet_after_complete_audio() {
    init();

    #[derive(Default)]
    struct CountingSink {
        samples: usize,
        discarded_packets: u32,
    }

    impl DecodedSink for CountingSink {
        fn on_format(&mut self, _sample_rate: u32, _channels: u32) {}

        fn on_samples(&mut self, samples: &[i32]) {
            self.samples += samples.len();
        }

        fn add_discarded_packet_count(&mut self, count: u32) {
            self.discarded_packets = self.discarded_packets.saturating_add(count);
        }
    }

    let (damaged, expected_samples) = mp3_with_invalid_terminal_packet();
    assert!(
        decode_audio(filled_sparse_buffer(&damaged), None, None).is_err(),
        "a strict whole-file decode must still report the invalid packet"
    );

    let mut sink = CountingSink::default();
    let result = decode_audio_to_verifying_sink(
        filled_sparse_buffer(&damaged),
        DecodeStart::StreamStart,
        None,
        None,
        &mut sink,
        Arc::new(std::sync::atomic::AtomicBool::new(false)),
    );
    assert!(result.is_ok(), "verifying sink decode failed: {result:?}");
    assert_eq!(sink.samples, expected_samples);
    assert_eq!(sink.discarded_packets, 1);
}

#[test]
fn streaming_decode_continues_after_an_invalid_terminal_mp3_packet() {
    use crate::track_stream::create_track_stream_pair_with_capacity;

    init();
    let (damaged, expected_samples) = mp3_with_invalid_terminal_packet();
    let (mut sink, source, _ready) =
        create_track_stream_pair_with_capacity(44_100, 2, expected_samples + 1024);
    let error_count = decode_audio_streaming(
        filled_sparse_buffer(&damaged),
        &mut sink,
        DecodeStart::StreamStart,
        None,
        None,
        None,
        Arc::new(std::sync::atomic::AtomicBool::new(false)),
    )
    .expect("playback decode");

    assert_eq!(error_count, 1);
    assert_eq!(source.samples_decoded() as usize, expected_samples);
}

#[test]
fn test_streaming_decode() {
    use crate::sparse_buffer::create_sparse_buffer;
    use crate::track_stream::create_track_stream_pair_with_capacity;
    use std::thread;

    init();

    let samples: Vec<i32> =
        (0..44100).map(|i| ((i as f64 * 0.01).sin() * 0.5 * i32::MAX as f64) as i32).collect();
    let flac_data =
        encode_i32(EncodeFormat::Flac { bits_per_sample: 16 }, &samples, 44100, 1).unwrap();

    let buffer = create_sparse_buffer(flac_data.len() as u64);
    let (mut sink, mut source, _ready) = create_track_stream_pair_with_capacity(44100, 1, 100000);

    let decoder_buffer = buffer.clone();
    let decoder_handle = thread::spawn(move || {
        let token = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let result = decode_audio_streaming(
            decoder_buffer,
            &mut sink,
            DecodeStart::StreamStart,
            None,
            None,
            None,
            token,
        );
        // One segment is the whole track here: finish the sink as run_decoder would.
        sink.mark_finished();
        result
    });

    // The decoder is already blocked reading; this is the download landing.
    buffer.append_at(0, &flac_data);

    let result = decoder_handle.join().unwrap();
    assert!(result.is_ok(), "Decode failed: {:?}", result.err());

    let mut decoded_samples = Vec::new();
    let mut buf = [0.0f32; 1024];
    loop {
        let n = source.pull_samples(&mut buf);
        if n == 0 && source.is_finished() {
            break;
        }
        decoded_samples.extend_from_slice(&buf[..n]);
    }

    // The encoder pads the last frame, so the counts differ slightly.
    assert!(
        (decoded_samples.len() as i64 - samples.len() as i64).abs() < 1000,
        "Sample count mismatch: {} vs {}",
        decoded_samples.len(),
        samples.len()
    );
}

#[test]
fn test_streaming_decode_treats_cancelled_input_as_normal_stop() {
    use crate::sparse_buffer::create_sparse_buffer;
    use crate::track_stream::create_track_stream_pair_with_capacity;

    init();

    // Size is irrelevant: the buffer is cancelled before any read.
    let buffer = create_sparse_buffer(4096);
    buffer.cancel();
    let (mut sink, source, _ready) = create_track_stream_pair_with_capacity(44100, 1, 4096);
    let token = Arc::new(std::sync::atomic::AtomicBool::new(false));

    let result = decode_audio_streaming(
        buffer,
        &mut sink,
        DecodeStart::StreamStart,
        None,
        None,
        None,
        token,
    );

    assert!(matches!(result, Err(DecodeError::InputCancelled)), "{result:?}");
    assert!(!source.producer_finished());
    assert_eq!(source.samples_decoded(), 0);
}

#[test]
fn streaming_decode_reports_truncated_flac_packet_stream() {
    use crate::sparse_buffer::create_sparse_buffer;
    use crate::track_stream::create_track_stream_pair_with_capacity;

    init();

    let flac_data = truncated_flac_packet_stream();

    let buffer = create_sparse_buffer(flac_data.len() as u64);
    buffer.append_at(0, &flac_data);
    let (mut sink, source, _ready) = create_track_stream_pair_with_capacity(44100, 1, 100000);
    let token = Arc::new(std::sync::atomic::AtomicBool::new(false));

    let error_count = decode_audio_streaming(
        buffer,
        &mut sink,
        DecodeStart::StreamStart,
        None,
        None,
        None,
        token,
    )
    .expect("playback keeps the decodable prefix");

    assert!(error_count > 0, "truncation must remain observable");
    assert!(source.samples_decoded() > 0, "the decodable prefix remains playable");
    assert!(!source.producer_finished());
}
/// The OS error kind and text of a decode that stopped on a source read, or
/// `None` for any other outcome.
fn source_read_failure(result: Result<(), DecodeError>) -> Option<(std::io::ErrorKind, String)> {
    match result {
        Err(DecodeError::SourceRead(error)) => {
            crate::test_source::io_kind(&error).map(|kind| (kind, error.to_string()))
        }
        _ => None,
    }
}

/// A minute of stereo noise as 16-bit WAV (~10 MiB): several fill windows, so
/// a decode reads well past the first fetch.
fn multi_window_wav() -> Vec<u8> {
    let sample_rate = 44_100u32;
    let samples: Vec<i32> = (0..sample_rate as usize * 60 * 2)
        .map(|i| (i as u32).wrapping_mul(2_654_435_761) as i32)
        .collect();
    let wav =
        encode_i32(EncodeFormat::PcmWav { bits_per_sample: 16 }, &samples, sample_rate, 2).unwrap();
    assert!(
        wav.len() as u64 > 2 * crate::test_source::TEST_WINDOW,
        "the source must span several fill windows"
    );
    wav
}

/// Fill `buffer` from `bytes` through the production fill loop, failing every
/// fetch that reaches byte `fail_from` or past it with a full-disk error. (The
/// fill's fetches follow the reader's demand, so they aren't window-aligned;
/// keying on the range's end makes the fetch covering `fail_from` the one that
/// fails.)
fn spawn_fill_failing_from(
    buffer: &crate::sparse_buffer::SharedSparseBuffer,
    bytes: Vec<u8>,
    fail_from: u64,
) {
    tokio::spawn(buffer.clone().fill_on_demand(
        crate::test_source::TEST_WINDOW,
        move |start, len| {
            let result = if start + len <= fail_from {
                Ok(bytes[start as usize..(start + len) as usize].to_vec())
            } else {
                Err(crate::test_source::TestReadError::io(
                    format!("Failed to read {len} bytes at {start} from /volume/track.wav"),
                    std::io::ErrorKind::StorageFull,
                ))
            };
            async move { result }
        },
    ));
}

struct NullSink;
impl DecodedSink for NullSink {
    fn on_format(&mut self, _sample_rate: u32, _channels: u32) {}
    fn on_samples(&mut self, _samples: &[i32]) {}
}

/// A read that fails after the decode is under way (the demuxer opened on the
/// first window) is the decode's outcome too: FFmpeg's packet loop ends on it
/// the way it ends on EOF, and the decode must not pass that off as a clean
/// (truncated) window.
#[tokio::test]
async fn i32_decode_names_the_read_error_when_a_later_read_fails() {
    use crate::sparse_buffer::create_sparse_buffer;
    use crate::test_source::TEST_WINDOW;

    init();

    let wav = multi_window_wav();
    let buffer = create_sparse_buffer(wav.len() as u64);
    // The third window fails: its fetch goes out once the decode reads past
    // the first, well after the demuxer opened.
    spawn_fill_failing_from(&buffer, wav, 2 * TEST_WINDOW);

    let result = bae_util::test_blocking::on_blocking_pool(move || {
        decode_audio_to_sink(
            buffer,
            None,
            None,
            &mut NullSink,
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
    })
    .await;

    let (kind, text) = source_read_failure(result.clone())
        .unwrap_or_else(|| panic!("a mid-stream read failure is a read failure: {result:?}"));
    assert_eq!(kind, std::io::ErrorKind::StorageFull, "{text}");
    assert!(text.contains("/volume/track.wav"), "{text}");
}

/// The import verifier's decode distinguishes the two failures: a source
/// whose first read fails is a read failure, the same as a later one.
#[tokio::test]
async fn verifying_decode_reports_a_first_read_failure_as_a_read_failure() {
    use crate::sparse_buffer::create_sparse_buffer;

    init();

    let wav = multi_window_wav();
    let buffer = create_sparse_buffer(wav.len() as u64);
    spawn_fill_failing_from(&buffer, wav, 0);

    let result = bae_util::test_blocking::on_blocking_pool(move || {
        decode_audio_to_verifying_sink(
            buffer,
            DecodeStart::Sample(0),
            Some(0),
            None,
            &mut NullSink,
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
    })
    .await;

    let (kind, text) = source_read_failure(result.clone())
        .unwrap_or_else(|| panic!("an unreadable source is a read failure: {result:?}"));
    assert_eq!(kind, std::io::ErrorKind::StorageFull, "{text}");
    assert!(!text.contains("Invalid data found"), "{text}");
}

/// Playback's streaming decode stops on the read failure as its own outcome
/// (the fill reports it to the command loop), not as a cancel or a decode
/// error.
#[tokio::test]
async fn streaming_decode_stops_on_a_read_failure_as_a_read_failure() {
    use crate::sparse_buffer::create_sparse_buffer;
    use crate::test_source::TEST_WINDOW;
    use crate::track_stream::create_track_stream_pair_with_capacity;

    init();

    let wav = multi_window_wav();
    let buffer = create_sparse_buffer(wav.len() as u64);
    // The third window fails: its fetch goes out once the decode reads past
    // the first, well after the demuxer opened.
    spawn_fill_failing_from(&buffer, wav, 2 * TEST_WINDOW);

    let result = bae_util::test_blocking::on_blocking_pool(move || {
        let (mut sink, _source, _ready) =
            create_track_stream_pair_with_capacity(44_100, 2, 1 << 22);
        let token = Arc::new(std::sync::atomic::AtomicBool::new(false));
        decode_audio_streaming(buffer, &mut sink, DecodeStart::StreamStart, None, None, None, token)
            .map(|_| ())
    })
    .await;

    let (kind, _) = source_read_failure(result.clone())
        .unwrap_or_else(|| panic!("a mid-stream read failure is a read failure: {result:?}"));
    assert_eq!(kind, std::io::ErrorKind::StorageFull);
}
