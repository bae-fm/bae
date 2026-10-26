//! A scrub inside a FLAC with no seektable jumps by byte to the stored scrub
//! point at or before its target, reads within one fill window from there,
//! and plays the samples a decode from the track's start trimmed to the target
//! plays.

use super::*;
use crate::audio_codec::{encode_i32, EncodeFormat, AVIO_BUFFER_SIZE};
use crate::db::{ScrubPoint, SegmentSpan};
use crate::playback::create_track_stream_pair_with_capacity;
use crate::playback::sparse_buffer::FILL_WINDOW_SIZE;
use crate::playback::stream_pipeline::{SegmentDecodeParams, StreamDecodeParams};
use crate::playback::ScrubPoints;
use std::sync::atomic::AtomicBool;

const SAMPLE_RATE: u32 = 96_000;
const CHANNELS: u32 = 2;

/// Seconds of 24-bit stereo noise at 96 kHz, encoded as a FLAC with no
/// seektable (FFmpeg's FLAC muxer writes none). Noise barely compresses, so
/// ten seconds outgrow one fill window and the byte rule spaces the points.
fn noise_flac(seconds: u32) -> Vec<u8> {
    let mut state = 0x2545_f491_u32;
    let samples: Vec<i32> = (0..SAMPLE_RATE * CHANNELS * seconds)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (state as i32 >> 8) << 8
        })
        .collect();
    encode_i32(
        EncodeFormat::Flac {
            bits_per_sample: 24,
        },
        &samples,
        SAMPLE_RATE,
        CHANNELS,
    )
    .expect("encode the noise FLAC")
}

fn filled_buffer(bytes: &[u8]) -> SharedSparseBuffer {
    let buffer = create_sparse_buffer(bytes.len() as u64);
    buffer.append_at(0, bytes);
    buffer
}

/// Takes a verifying decode's scrub points as import's sink does, and
/// discards its samples.
struct TakesScrubPoints(Option<crate::import::scrub_points::ScrubPointTaker>);

impl crate::audio_codec::DecodedSink for TakesScrubPoints {
    fn on_format(&mut self, _sample_rate: u32, _channels: u32) {}
    fn on_samples(&mut self, _samples: &[i32]) {}
    fn on_stream(&mut self, stream: &crate::audio_codec::DecodedStream) {
        self.0 = crate::import::scrub_points::ScrubPointTaker::for_stream(stream);
    }
    fn on_packet(&mut self, packet: crate::audio_codec::DecodedPacket) {
        if let Some(taker) = self.0.as_mut() {
            taker.observe(packet);
        }
    }
}

/// The scrub points the import's verifying decode takes over `span` of `flac`.
fn import_scrub_points(flac: &[u8], span: SegmentSpan) -> Vec<ScrubPoint> {
    let decode = StreamDecodeParams::new(
        vec![SegmentDecodeParams::new(filled_buffer(flac), span, 0)],
        true,
        0,
        0,
    );
    let mut sink = TakesScrubPoints(None);
    decode
        .verify_to_sink(&mut sink, Arc::new(AtomicBool::new(false)))
        .expect("the verifying decode reads the file");
    sink.0
        .expect("a FLAC with no seektable gets scrub points")
        .finish()
}

/// Run `decode` to its end and return the samples it played.
fn play(decode: StreamDecodeParams) -> Vec<f32> {
    let (mut sink, mut stream, _ready) =
        create_track_stream_pair_with_capacity(SAMPLE_RATE, CHANNELS, 4 * SAMPLE_RATE as usize);
    let decoder =
        std::thread::spawn(move || decode.run_decoder(&mut sink, Arc::new(AtomicBool::new(false))));
    let mut played = Vec::new();
    let mut chunk = [0.0f32; 4_096];
    loop {
        let pulled = stream.pull_samples(&mut chunk);
        played.extend_from_slice(&chunk[..pulled]);
        if pulled == 0 && stream.is_finished() {
            break;
        }
    }
    decoder
        .join()
        .expect("the decoder thread")
        .expect("the decode runs to the window's end");
    played
}

/// Scrub `segment` of `flac`, with `points` stored for it, to `target`
/// through the playback service's seek mapping, the track ending at `end` so
/// the decode stops there. Returns the offsets the demuxer read and what
/// played.
fn scrub(
    flac: &[u8],
    segment: SegmentSpan,
    points: &[ScrubPoint],
    target: u64,
    end: u64,
) -> (Vec<u64>, Vec<f32>) {
    let buffer = filled_buffer(flac);
    let mut prepared = test_prepared_track("track", buffer.clone());
    prepared.sample_rate = SAMPLE_RATE;
    prepared.segments[0].span = SegmentSpan {
        end_sample: Some(end),
        ..segment
    };
    prepared.segments[0].scrub_points = ScrubPoints::new(points.to_vec());
    let played = play(prepared.decode_params(target - segment.start_sample, false));
    (buffer.read_log(), played)
}

/// What a decode that jumps to the track's first frame and trims to `target`
/// plays up to `end`.
fn from_track_start(flac: &[u8], first_frame: u64, target: u64, end: u64) -> Vec<f32> {
    let window = SegmentSpan {
        start_sample: target,
        end_sample: Some(end),
        start_byte: Some(first_frame),
        end_byte: None,
    };
    play(StreamDecodeParams::new(
        vec![SegmentDecodeParams::new(filled_buffer(flac), window, 0)],
        true,
        0,
        0,
    ))
}

/// Scrub to the last sample before `points[next]`, the farthest any sample
/// sits from the point that serves it, and check the scrub's reads and
/// samples.
fn check_worst_scrub(flac: &[u8], segment: SegmentSpan, points: &[ScrubPoint], next: usize) {
    let point = points[next - 1];
    let target = points[next].sample - 1;

    // Handing over the target's frame, the scrub reads two runs: the file's
    // head (the probe of its header and first frames) and, from the point,
    // what one fill window holds. Nothing else before the point, and never the
    // file's end.
    let (reads, _) = scrub(flac, segment, points, target, target + 1);
    let in_head = |at: u64| at + AVIO_BUFFER_SIZE as u64 <= FILL_WINDOW_SIZE;
    let in_jump =
        |at: u64| at >= point.byte && at + AVIO_BUFFER_SIZE as u64 <= point.byte + FILL_WINDOW_SIZE;
    assert!(
        reads.iter().all(|&at| in_head(at) || in_jump(at)),
        "reads {reads:?} leave the head window and the window from the point {point:?}"
    );
    assert!(
        reads.iter().any(|&at| at >= point.byte),
        "the scrub never read from the point: {reads:?}"
    );

    let end = target + u64::from(SAMPLE_RATE);
    let (_, played) = scrub(flac, segment, points, target, end);
    let reference = from_track_start(flac, points[0].byte, target, end);
    assert_eq!(played.len(), (end - target) as usize * CHANNELS as usize);
    assert!(
        played == reference,
        "the scrub's samples differ from a decode from the track's start"
    );
}

#[test]
fn a_scrub_in_a_file_without_a_seektable_jumps_to_its_scrub_point() {
    crate::audio_codec::init();
    let flac = noise_flac(30);
    let whole = SegmentSpan::whole_file();
    let points = import_scrub_points(&flac, whole);
    assert!(
        points.len() >= 5,
        "30 s of noise spans several windows: {points:?}"
    );
    // Spaced by bytes, not by the 10 s rule: ten seconds of this noise
    // outgrow one window.
    for pair in points.windows(2) {
        assert!(
            pair[1].byte - pair[0].byte <= FILL_WINDOW_SIZE
                && pair[1].sample - pair[0].sample < 10 * u64::from(SAMPLE_RATE),
            "points {pair:?}"
        );
    }
    check_worst_scrub(&flac, whole, &points, 3);
    check_worst_scrub(&flac, whole, &points, points.len() - 1);
}

/// A track that starts inside the file, as a CUE album's do: its decode starts
/// at the byte import recorded, and its first point is the frame its first
/// sample lies in.
#[test]
fn a_scrub_inside_a_track_of_a_single_file_album_jumps_to_its_scrub_point() {
    crate::audio_codec::init();
    let flac = noise_flac(30);
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("album.flac");
    std::fs::write(&path, &flac).unwrap();
    let start_sample = 12 * u64::from(SAMPLE_RATE) + 1_234;
    let start_byte =
        crate::audio_codec::seek_landing_bytes(path.to_str().unwrap(), &[start_sample])
            .expect("the track's landing byte")[0];
    let track = SegmentSpan {
        start_sample,
        end_sample: None,
        start_byte: Some(start_byte),
        end_byte: None,
    };
    let points = import_scrub_points(&flac, track);
    assert_eq!(
        points[0].byte, start_byte,
        "the first point is the track's own frame"
    );
    assert!(points[0].sample <= start_sample && points[1].sample > start_sample);
    check_worst_scrub(&flac, track, &points, 2);
}
