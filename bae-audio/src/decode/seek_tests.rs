#![expect(
    clippy::disallowed_methods,
    reason = "test code: what it spawns ends with the test, which has no crash reporter to report to"
)]

use crate::test_samples::filled_sparse_buffer;
use crate::{
    decode_audio, decode_audio_streaming, encode_i32, init, seek_landing_bytes, DecodeStart,
    EncodeFormat,
};
use std::sync::Arc;

/// A `seek_to_sample` streaming decode (FFmpeg's `avformat_seek_file`, over a
/// fully-buffered sparse buffer) yields the same samples as decoding the whole
/// file and slicing at the seek point.
///
/// `avformat_seek_file` lands on the nearest keyframe AT or BEFORE the target, so
/// the decoded output may start slightly before the requested position — the
/// actual alignment is recovered by correlating against the ground truth.
fn check_seek_to_produces_correct_samples(sample_rate: u32, channels: u32, bits_per_sample: u32) {
    use crate::sparse_buffer::create_sparse_buffer;
    use crate::track_stream::create_track_stream_pair_with_capacity;
    use std::thread;

    let duration_secs = 2;
    let frame_samples = sample_rate as usize * duration_secs;
    let mut original: Vec<i32> = Vec::with_capacity(frame_samples * channels as usize);
    for i in 0..frame_samples {
        let t = i as f64 / sample_rate as f64;
        let freq = 200.0 + (t / duration_secs as f64) * 1800.0;
        let sample = (0.8 * i32::MAX as f64 * (2.0 * std::f64::consts::PI * freq * t).sin()) as i32;
        original.push(sample);
        if channels == 2 {
            original.push(-sample);
        }
    }

    let flac_data =
        encode_i32(EncodeFormat::Flac { bits_per_sample }, &original, sample_rate, channels)
            .unwrap();
    let ground_truth = decode_audio(filled_sparse_buffer(&flac_data), None, None).unwrap();

    let seek_sample = sample_rate as u64; // 1 second

    // The whole file is buffered, so the decoder's seek never waits on a fill.
    let buffer = create_sparse_buffer(flac_data.len() as u64);
    buffer.append_at(0, &flac_data);

    let (mut sink, mut source, _ready) =
        create_track_stream_pair_with_capacity(sample_rate, channels, 500000);
    let decoder_handle = thread::spawn(move || {
        let token = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let result = decode_audio_streaming(
            buffer,
            &mut sink,
            DecodeStart::Sample(seek_sample),
            None,
            None,
            None,
            token,
        );
        // One segment is the whole track here: finish the sink as run_decoder would.
        sink.mark_finished();
        result
    });

    let result = decoder_handle.join().unwrap();
    assert!(result.is_ok(), "seek_to decode failed: {:?}", result.err());

    let mut seeked_samples = Vec::new();
    let mut buf = [0.0f32; 1024];
    loop {
        let n = source.pull_samples(&mut buf);
        if n == 0 && source.is_finished() {
            break;
        }
        seeked_samples.extend_from_slice(&buf[..n]);
    }

    assert!(seeked_samples.len() > 100, "Not enough seeked samples: {}", seeked_samples.len());

    let scale = i32::MAX as f32;

    // avformat_seek_file lands on a keyframe at or before the target.
    // Find where the seeked output actually starts in the ground truth
    // by correlating a window of samples. Search up to 16384 samples
    // before the target (FLAC blocksize varies by sample rate).
    let target_interleaved = (seek_sample as usize) * (channels as usize);
    let max_search = 16384 * channels as usize;
    let search_start = target_interleaved.saturating_sub(max_search);
    // Ensure channel-aligned
    let search_start = search_start - (search_start % channels as usize);

    // Use a large correlation window for reliable matching
    let window = 512.min(seeked_samples.len());
    let mut best_offset = search_start;
    let mut best_error = f64::MAX;

    for offset in (search_start..=target_interleaved).step_by(channels as usize) {
        if offset + window > ground_truth.samples.len() {
            break;
        }
        let mut err = 0.0f64;
        for (&seeked, &truth) in
            seeked_samples[..window].iter().zip(&ground_truth.samples[offset..offset + window])
        {
            let diff = seeked as f64 - truth as f64 / scale as f64;
            err += diff * diff;
        }
        if err < best_error {
            best_error = err;
            best_offset = offset;
        }
    }

    // Verify the seek landed within a reasonable distance of the target
    let offset_samples = (target_interleaved - best_offset) / channels as usize;
    assert!(
        offset_samples <= 16384,
        "Seek landed too far from target: {} samples before 1s mark",
        offset_samples,
    );

    // Now compare samples from the aligned position
    let expected_samples = &ground_truth.samples[best_offset..];
    let compare_len = 1000.min(seeked_samples.len()).min(expected_samples.len());
    assert!(compare_len > 100, "Not enough samples: {}", compare_len);

    let mut mismatches = 0;
    let mut max_diff = 0.0f32;
    let mut first_mismatch_idx = None;

    for i in 0..compare_len {
        let seeked = seeked_samples[i];
        let expected = expected_samples[i] as f32 / scale;
        let diff = (seeked - expected).abs();

        if diff > 0.01 {
            mismatches += 1;
            if first_mismatch_idx.is_none() {
                first_mismatch_idx = Some(i);
            }
            max_diff = max_diff.max(diff);
        }
    }

    if mismatches > 0 {
        let first_idx = first_mismatch_idx.unwrap();
        panic!(
            "seek_to mismatch! {}Hz {}ch {}bit (aligned at {} samples before target)\n\
             First mismatch at {}: seeked={:.4}, expected={:.4}\n\
             Mismatches: {}/{}, max_diff: {:.4}",
            sample_rate,
            channels,
            bits_per_sample,
            offset_samples,
            first_idx,
            seeked_samples[first_idx],
            expected_samples[first_idx] as f32 / scale,
            mismatches,
            compare_len,
            max_diff,
        );
    }
}

#[test]
fn test_seek_to_mono_44100_16bit() {
    init();
    check_seek_to_produces_correct_samples(44100, 1, 16);
}

#[test]
fn test_seek_to_stereo_44100_24bit() {
    init();
    check_seek_to_produces_correct_samples(44100, 2, 24);
}

#[test]
fn test_seek_to_stereo_96000_16bit() {
    init();
    check_seek_to_produces_correct_samples(96000, 2, 16);
}

/// A FLAC seek must use the file's seektable, not FFmpeg's generic binary search.
///
/// Without `AVFMT_FLAG_FAST_SEEK`, FFmpeg ignores the FLAC seektable and seeks by
/// binary search: it reads the file's *end* to find the last timestamp, then
/// bisects frame by frame. Over storage every one of those reads is a
/// separate ranged fetch (~15 of them), so a single CUE/FLAC track start or
/// in-track seek stalls for seconds. With the flag set, a seektable-bearing FLAC
/// jumps straight to the seekpoint at or before the target.
///
/// We prove the seektable path is taken by recording every byte offset the
/// demuxer reads while seeking to the middle of the file, and asserting none lands
/// in the file's tail. Reading the tail on a mid-file seek is the binary search's
/// signature; the seektable path never does it. Remove the flag and this fails.
#[test]
fn flac_seek_uses_seektable_not_binary_search() {
    use crate::sparse_buffer::create_sparse_buffer;
    use crate::track_stream::create_track_stream_pair_with_capacity;
    use std::thread;

    init();

    // 30s synthetic stereo CUE/FLAC fixture (silence / white noise / brown noise)
    // carrying a 6-point seektable.
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/cue_flac/Test Album.flac");
    let flac = std::fs::read(path).expect("read fixture");
    let file_len = flac.len() as u64;

    let buffer = create_sparse_buffer(file_len);
    buffer.append_at(0, &flac);

    // Seek to the middle of the file (~15s of 30s), far from both ends, and stop
    // ~1s later. Bounding the decode keeps the output inside the sink's capacity
    // so the decoder finishes instead of blocking on a full sink (which would
    // deadlock the join below), and keeps the forward decode clear of the tail.
    let seek_sample: u64 = 15 * 44100;
    let stop_sample: u64 = 16 * 44100;
    let (mut sink, mut source, _ready) = create_track_stream_pair_with_capacity(44100, 2, 500_000);
    let decode_buffer = buffer.clone();
    let handle = thread::spawn(move || {
        let token = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let result = decode_audio_streaming(
            decode_buffer,
            &mut sink,
            DecodeStart::Sample(seek_sample),
            None,
            Some(stop_sample),
            None,
            token,
        );
        // One segment is the whole track here: finish the sink as run_decoder would.
        sink.mark_finished();
        result
    });
    assert!(handle.join().unwrap().is_ok(), "seek decode failed");

    // Drain the decoded audio so we can confirm the seek produced output.
    let mut pulled = 0usize;
    let mut buf = [0.0f32; 1024];
    loop {
        let n = source.pull_samples(&mut buf);
        if n == 0 && source.is_finished() {
            break;
        }
        pulled += n;
    }
    assert!(pulled > 0, "no samples decoded after seek");

    // The binary search reads the file's end; the seektable path never does.
    let reads = buffer.read_log();
    assert!(!reads.is_empty(), "no reads recorded");
    let tail_start = file_len * 9 / 10;
    let tail_reads: Vec<u64> = reads.iter().copied().filter(|&o| o >= tail_start).collect();
    assert!(
        tail_reads.is_empty(),
        "seek read the file tail (offsets {tail_reads:?} >= {tail_start} of {file_len}) -- \
         FFmpeg binary-searched instead of using the seektable (AVFMT_FLAG_FAST_SEEK missing?)"
    );
    // Sanity: the seek moved past the header into the file, not decode-from-start.
    assert!(
        reads.iter().any(|&o| o >= file_len / 8),
        "no read past the header; the seek did not advance into the file: {reads:?}"
    );
}

/// The load-bearing accuracy guarantee: a by-byte seek to the recorded landing,
/// trimmed to the track's start sample, produces output whose *first* sample is
/// exactly the start sample. The by-byte jump lands on the frame boundary at or
/// before the target; `start_at_sample` trims the sub-frame remainder.
///
/// Two assertions prove the byte drives the landing, not the trim alone (over a
/// full in-memory buffer the trim would reach the target from a decode-from-zero
/// too, so accuracy on its own proves nothing):
/// - The correct landing byte yields the exact target sample, and a window one
///   frame earlier correlates worse.
/// - A *wrong* landing byte (a later track's, past the target) yields output that
///   is NOT the target sample -- so the byte-seek target, not the trim, decides
///   where output begins.
#[test]
fn byte_seek_to_landing_is_sample_exact() {
    use crate::sparse_buffer::create_sparse_buffer;
    use crate::track_stream::create_track_stream_pair_with_capacity;
    use std::thread;

    init();
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/cue_flac/Test Album.flac");
    let flac = std::fs::read(path).expect("read fixture");
    let sample_rate = 44100u64;
    let channels = 2u32;
    // Track 2 start (white noise) -- random content correlates precisely, unlike
    // the silent track 1. Track 3's landing (20s) is the deliberately wrong byte,
    // well past track 2's target.
    let start = 10 * sample_rate;
    let landings = seek_landing_bytes(std::path::Path::new(path), &[start, 20 * sample_rate])
        .expect("landings");
    let landing = landings[0];
    let wrong_landing = landings[1];

    let ground =
        decode_audio(filled_sparse_buffer(&flac), None, None).expect("ground-truth decode");
    let scale = i32::MAX as f32;

    // Decode track 2 with a given by-byte seek target, trimming to `start` and
    // stopping ~0.1s later; return the interleaved output samples.
    let decode_from = |landing: u64| -> Vec<f32> {
        let buffer = create_sparse_buffer(flac.len() as u64);
        buffer.append_at(0, &flac);
        let (mut sink, mut source, _ready) =
            create_track_stream_pair_with_capacity(sample_rate as u32, channels, 500_000);
        let decode_buffer = buffer.clone();
        let handle = thread::spawn(move || {
            let token = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let result = decode_audio_streaming(
                decode_buffer,
                &mut sink,
                DecodeStart::Byte(landing),
                Some(start),                    // trim lead-in to the start sample
                Some(start + sample_rate / 10), // stop ~0.1s later
                None,
                token,
            );
            // One segment is the whole track here: finish the sink as run_decoder would.
            sink.mark_finished();
            result
        });
        assert!(handle.join().unwrap().is_ok(), "byte-seek decode failed");
        let mut out = Vec::new();
        let mut buf = [0.0f32; 1024];
        loop {
            let n = source.pull_samples(&mut buf);
            if n == 0 && source.is_finished() {
                break;
            }
            out.extend_from_slice(&buf[..n]);
        }
        out
    };

    let target = start as usize * channels as usize;
    // Mean squared error of the first `window` samples of `out` against the ground
    // truth starting at interleaved offset `off`.
    let mse = |out: &[f32], off: usize| -> f64 {
        let window = 256usize.min(out.len());
        let e: f64 = out[..window]
            .iter()
            .zip(&ground.samples[off..off + window])
            .map(|(&s, &t)| {
                let d = s as f64 - t as f64 / scale as f64;
                d * d
            })
            .sum();
        e / window as f64
    };
    // The output's first sample is the target sample: enough samples, and the
    // first window matches the ground truth at exactly `target`.
    let is_sample_exact = |out: &[f32]| out.len() > 200 && mse(out, target) < 1e-4;

    // Correct landing -> exact target sample.
    let seeked = decode_from(landing);
    assert!(
        is_sample_exact(&seeked),
        "the correct landing byte must land on start_sample (len {}, mse {:.6})",
        seeked.len(),
        mse(&seeked, target),
    );
    // One frame earlier correlates worse -- the trim landed on the target, not a
    // frame short of it.
    let earlier = target.saturating_sub(4096 * channels as usize);
    assert!(
        mse(&seeked, earlier) > mse(&seeked, target),
        "output aligns at least as well one frame before the target -- the trim \
         did not land exactly on start_sample",
    );

    // Wrong landing (track 3's, past the target) -> NOT the target sample. This
    // isolates the byte-seek as the mechanism: with the trim alone the output
    // would be identical regardless of the byte. The seek is a valid in-file
    // offset (so it doesn't fail and fall back to decode-from-zero); it just lands
    // past the stop window, yielding empty/misaligned output.
    let sabotaged = decode_from(wrong_landing);
    assert!(
        !is_sample_exact(&sabotaged),
        "a wrong landing byte still produced the target sample (len {}, mse {:.6}) -- \
         the byte-seek target is not driving where output begins",
        sabotaged.len(),
        if sabotaged.len() >= 256 { mse(&sabotaged, target) } else { f64::NAN },
    );
}
/// Twenty seconds of a stereo two-tone sweep, encoded as `format`. Long enough
/// that a seek to its later targets lands inside the file rather than at its
/// first packet.
fn lossy_sweep(format: EncodeFormat) -> Vec<u8> {
    let frames = 44_100 * 20;
    let samples: Vec<i32> = (0..frames * 2)
        .map(|i| {
            let t = (i / 2) as f64 / 44_100.0;
            let v = 0.3 * (2.0 * std::f64::consts::PI * 440.0 * t).sin()
                + 0.2 * (2.0 * std::f64::consts::PI * 1_234.5 * t * (1.0 + t / 10.0)).sin();
            (v * i32::MAX as f64) as i32
        })
        .collect();
    encode_i32(format, &samples, 44_100, 2).expect("encode the sweep")
}

fn lossy_formats() -> [(&'static str, EncodeFormat); 3] {
    [
        ("MP3", EncodeFormat::Mp3 { bitrate_kbps: 192 }),
        ("AAC", EncodeFormat::Aac { bitrate_kbps: 192 }),
        ("Opus", EncodeFormat::OpusOgg { bitrate_kbps: 128 }),
    ]
}

/// Targets at the first frame's second sample, inside the first frames, and
/// well into the file, in the stream's own sample rate.
fn lossy_targets(sample_rate: u32) -> [u64; 3] {
    let second = u64::from(sample_rate);
    [1, 4_000, 12 * second + 123]
}

/// A sink decode that starts at sample N yields exactly what a decode from the
/// stream's start yields from N on, for codecs that skip an encoder delay
/// (MP3, Opus) and ones that don't (AAC here).
#[test]
fn a_lossy_sink_decode_from_a_sample_is_the_whole_decode_from_it() {
    init();
    for (name, format) in lossy_formats() {
        let bytes = lossy_sweep(format);
        let whole =
            decode_audio(filled_sparse_buffer(&bytes), None, None).expect("decode from the start");
        let channels = whole.channels as usize;
        for target in lossy_targets(whole.sample_rate) {
            let from = decode_audio(filled_sparse_buffer(&bytes), Some(target), None)
                .expect("decode from the target");
            let expected = &whole.samples[target as usize * channels..];
            assert_eq!(from.samples.len(), expected.len(), "{name} from {target}: sample count");
            assert!(
                from.samples == expected,
                "{name} from {target}: the samples differ from the whole decode's"
            );
        }
    }
}

/// Run playback's streaming decode over `bytes` and collect what it played.
fn stream_lossy(bytes: &[u8], sample_rate: u32, start: Option<u64>) -> Vec<f32> {
    use crate::track_stream::create_track_stream_pair_with_capacity;

    let buffer = filled_sparse_buffer(bytes);
    let (mut sink, mut source, _ready) =
        create_track_stream_pair_with_capacity(sample_rate, 2, 4 * sample_rate as usize);
    let decoder = std::thread::spawn(move || {
        let result = decode_audio_streaming(
            buffer,
            &mut sink,
            start.map_or(DecodeStart::StreamStart, DecodeStart::Sample),
            start,
            None,
            None,
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        );
        sink.mark_finished();
        result
    });
    let mut played = Vec::new();
    let mut chunk = [0.0f32; 4_096];
    loop {
        let pulled = source.pull_samples(&mut chunk);
        played.extend_from_slice(&chunk[..pulled]);
        if pulled == 0 && source.is_finished() {
            break;
        }
    }
    decoder.join().expect("the decoder thread").expect("the streaming decode");
    played
}

/// Playback's streaming decode, started at sample N, plays from sample N: as
/// many samples as the stream holds from N on, placed where a decode from the
/// start places them. Unlike a sink decode it seeks, so a lossy decoder starts
/// on a pre-roll before N rather than at the stream's start, and its samples
/// converge on the whole decode's within the pre-roll (AAC's, Opus's).
///
/// An MP3 has no index to seek a sample by, so its decode reads from the
/// stream's start and plays exactly the whole decode's samples from N.
#[test]
fn a_lossy_streaming_decode_from_a_sample_plays_from_it() {
    init();
    for (name, format) in lossy_formats() {
        let bytes = lossy_sweep(format);
        let sample_rate = decode_audio(filled_sparse_buffer(&bytes), Some(0), Some(1))
            .expect("probe the rate")
            .sample_rate;
        let whole = stream_lossy(&bytes, sample_rate, None);
        for target in lossy_targets(sample_rate) {
            let from = stream_lossy(&bytes, sample_rate, Some(target));
            let expected = &whole[target as usize * 2..];
            assert_eq!(from.len(), expected.len(), "{name} from {target}: sample count");
            if name == "MP3" {
                assert!(
                    from == expected,
                    "{name} from {target}: the samples differ from the whole decode's"
                );
            }
        }
        // A start whose pre-roll reaches the stream's start decodes as the
        // whole decode does.
        let from = stream_lossy(&bytes, sample_rate, Some(1));
        assert!(from == whole[2..], "{name} from 1: the samples differ from the whole decode's");
    }
}

/// A FLAC of `total` mono samples at 44.1 kHz, encoded in 4096-sample frames
/// with no seektable (FFmpeg's FLAC muxer writes none), with the samples it
/// decodes to.
fn seektable_less_flac(total: usize) -> (Vec<u8>, Vec<f32>) {
    init();
    let original: Vec<i32> =
        (0..total).map(|i| ((i as i64 * 7919 % 2_000_000) as i32 - 1_000_000) * 1000).collect();
    let flac = encode_i32(EncodeFormat::Flac { bits_per_sample: 24 }, &original, 44100, 1).unwrap();
    let ground = decode_audio(filled_sparse_buffer(&flac), None, None).unwrap();
    let samples = ground.samples.iter().map(|&s| s as f32 / i32::MAX as f32).collect();
    (flac, samples)
}

/// What a streaming decode of `flac` plays when it seeks to `target` and trims
/// to it.
fn play_from_sample(flac: &[u8], target: u64) -> Result<Vec<f32>, crate::DecodeError> {
    use crate::sparse_buffer::create_sparse_buffer;
    use crate::track_stream::create_track_stream_pair_with_capacity;

    let buffer = create_sparse_buffer(flac.len() as u64);
    buffer.append_at(0, flac);
    let (mut sink, mut source, _ready) = create_track_stream_pair_with_capacity(44100, 1, 500_000);
    let result = decode_audio_streaming(
        buffer,
        &mut sink,
        DecodeStart::Sample(target),
        Some(target),
        None,
        None,
        Arc::new(std::sync::atomic::AtomicBool::new(false)),
    );
    sink.mark_finished();
    let mut played = Vec::new();
    let mut chunk = [0.0f32; 1024];
    loop {
        let n = source.pull_samples(&mut chunk);
        if n == 0 && source.is_finished() {
            break;
        }
        played.extend_from_slice(&chunk[..n]);
    }
    result.map(|_| played)
}

/// FFmpeg's binary search, a seektable-less FLAC's only way to seek by sample,
/// can't place a sample in the stream's last frames: it fails and leaves the
/// reader at the file's end, where the decode found no packets and ended with
/// no samples, as a success. A seek to any sample plays from that sample.
#[test]
fn a_sample_seek_near_a_seektable_less_flacs_end_plays_from_the_sample() {
    let total = 4096 * 10 + 1000;
    let (flac, ground) = seektable_less_flac(total);
    for target in (0..total).step_by(1021).chain([total - 1]) {
        let played = play_from_sample(&flac, target as u64)
            .unwrap_or_else(|e| panic!("seek to {target} failed: {e}"));
        assert!(
            played.len() == total - target
                && played.iter().zip(&ground[target..]).all(|(a, b)| (a - b).abs() < 1e-6),
            "seek to {target} of {total} played {} samples, not the {} from it",
            played.len(),
            total - target,
        );
    }
}

/// A FLAC of one frame gets no scrub points (its last frame is never a point),
/// so every seek into it is a sample seek.
#[test]
fn a_sample_seek_into_a_one_frame_flac_plays_from_the_sample() {
    let total = 3000;
    let (flac, ground) = seektable_less_flac(total);
    for target in [1, 1500, total - 1] {
        let played = play_from_sample(&flac, target as u64)
            .unwrap_or_else(|e| panic!("seek to {target} failed: {e}"));
        assert!(
            played.len() == total - target
                && played.iter().zip(&ground[target..]).all(|(a, b)| (a - b).abs() < 1e-6),
            "seek to {target} of {total} played {} samples, not the {} from it",
            played.len(),
            total - target,
        );
    }
}

/// What a streaming decode of `bytes` (stereo, 44.1 kHz) plays from `start`,
/// trimmed to `trim`. The decode runs on its own thread under a time limit, so
/// a seek that never returns fails the test instead of hanging it.
fn play_from(
    bytes: &[u8],
    start: DecodeStart,
    trim: Option<u64>,
) -> Result<Vec<f32>, crate::DecodeError> {
    play_from_buffer(filled_sparse_buffer(bytes), start, trim)
}

/// [`play_from`] over a buffer in whatever state of fill the caller made it.
fn play_from_buffer(
    buffer: crate::sparse_buffer::SharedSparseBuffer,
    start: DecodeStart,
    trim: Option<u64>,
) -> Result<Vec<f32>, crate::DecodeError> {
    use crate::track_stream::create_track_stream_pair_with_capacity;

    // Room for the whole 20 s track, which nothing drains until the decode ends.
    let (mut sink, mut source, _ready) =
        create_track_stream_pair_with_capacity(44100, 2, 4_410_000);
    let (done, result) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = decode_audio_streaming(
            buffer,
            &mut sink,
            start,
            trim,
            None,
            None,
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        );
        sink.mark_finished();
        done.send(result).ok();
    });
    let result = result
        .recv_timeout(std::time::Duration::from_secs(60))
        .expect("the decode did not finish in 60 s");
    let mut played = Vec::new();
    let mut chunk = [0.0f32; 4096];
    loop {
        let n = source.pull_samples(&mut chunk);
        if n == 0 && source.is_finished() {
            break;
        }
        played.extend_from_slice(&chunk[..n]);
    }
    result.map(|_| played)
}

/// A seek to a recorded MP3 frame that FFmpeg can't make (here a byte the
/// index can't hold) left the decode reading on from wherever the search last
/// moved the reader: it played 765,454 samples, not the 882,000 from sample
/// 441,000, and reported success. The decode fails instead.
#[test]
fn a_failed_indexed_frame_seek_fails_the_decode() {
    init();
    let bytes = lossy_sweep(EncodeFormat::Mp3 { bitrate_kbps: 192 });
    let start = DecodeStart::Frame { byte: u64::MAX, sample: 441_000 };
    let played = play_from(&bytes, start, Some(441_000));
    assert!(
        played.is_err(),
        "the seek failed, yet the decode played {} samples",
        played.as_ref().map_or(0, Vec::len),
    );
}

/// An MP4 can't be jumped into by byte: FFmpeg declines before it moves the
/// reader, which stays where opening left it. A decode started at a byte
/// without a trim to a sample then played the whole file from its start,
/// as a success. The decode fails instead.
#[test]
fn a_failed_byte_seek_fails_the_decode() {
    init();
    let bytes = lossy_sweep(EncodeFormat::Aac { bitrate_kbps: 192 });
    let played = play_from(&bytes, DecodeStart::Byte(5000), None);
    assert!(
        played.is_err(),
        "the byte seek failed, yet the decode played {} samples",
        played.as_ref().map_or(0, Vec::len),
    );
}

/// A recorded byte at or past the file's end: FFmpeg's seek there succeeds, and
/// the decode then read nothing and ended as a success with 0 samples. The
/// decode fails instead, naming the record.
#[test]
fn a_recorded_byte_past_the_files_end_fails_the_decode() {
    init();
    let mp3 = lossy_sweep(EncodeFormat::Mp3 { bitrate_kbps: 192 });
    let opus = lossy_sweep(EncodeFormat::OpusOgg { bitrate_kbps: 128 });
    let cases = [
        (
            "MP3 frame",
            mp3.clone(),
            DecodeStart::Frame { byte: mp3.len() as u64 + 10_000, sample: 441_000 },
        ),
        ("Opus byte", opus.clone(), DecodeStart::Byte(opus.len() as u64 + 10_000)),
        (
            "Opus frame",
            opus.clone(),
            DecodeStart::Frame { byte: opus.len() as u64, sample: 441_000 },
        ),
    ];
    for (label, bytes, start) in cases {
        let played = play_from(&bytes, start, Some(441_000));
        assert!(
            played.is_err(),
            "{label}: the recorded byte is past the file's end, yet the decode played {} samples",
            played.as_ref().map_or(0, Vec::len),
        );
    }
}

/// A recorded byte above `i64::MAX` wrapped to a negative offset, which FFmpeg
/// clamped to the file's start: the decode played from there as a success.
/// The decode fails instead.
#[test]
fn a_recorded_byte_above_i64_max_fails_the_decode() {
    init();
    let opus = lossy_sweep(EncodeFormat::OpusOgg { bitrate_kbps: 128 });
    let played = play_from(&opus, DecodeStart::Byte(1 << 63), None);
    assert!(
        played.is_err(),
        "the byte does not fit an offset, yet the decode played {} samples",
        played.as_ref().map_or(0, Vec::len),
    );
    let mp3 = lossy_sweep(EncodeFormat::Mp3 { bitrate_kbps: 192 });
    let start = DecodeStart::Frame { byte: 1 << 63, sample: 441_000 };
    let played = play_from(&mp3, start, Some(441_000));
    assert!(
        played.is_err(),
        "the frame's byte does not fit an offset, yet the decode played {} samples",
        played.as_ref().map_or(0, Vec::len),
    );
}

/// Import records frames only for FLAC and MP3. A recorded frame given to any
/// other format logged an error and decoded from the stream's start, as a
/// success. The decode fails instead.
#[test]
fn a_recorded_frame_for_a_format_with_none_fails_the_decode() {
    init();
    let bytes = lossy_sweep(EncodeFormat::Aac { bitrate_kbps: 192 });
    let start = DecodeStart::Frame { byte: 5000, sample: 441_000 };
    let played = play_from(&bytes, start, Some(441_000));
    assert!(
        played.is_err(),
        "AAC has no recorded frames, yet the decode played {} samples",
        played.as_ref().map_or(0, Vec::len),
    );
}

/// A file still downloading reports its full length, not the bytes fetched so
/// far, so a recorded byte inside the file but beyond the fetched part is not
/// past its end: the decode waits for the fill and plays from it.
#[test]
fn a_recorded_byte_beyond_the_fetched_part_is_not_past_the_files_end() {
    init();
    let bytes = lossy_sweep(EncodeFormat::OpusOgg { bitrate_kbps: 128 });
    let fetched = bytes.len() / 4;
    let byte = (bytes.len() / 2) as u64;
    let buffer = crate::sparse_buffer::create_sparse_buffer(bytes.len() as u64);
    buffer.append_at(0, &bytes[..fetched]);
    let rest = buffer.clone();
    let tail = bytes[fetched..].to_vec();
    std::thread::spawn(move || {
        // The decode parks reading the bytes the first quarter lacks; only
        // then does the fill deliver them.
        rest.wait_until_parked(1);
        rest.append_at(fetched as u64, &tail);
    });
    let played = play_from_buffer(buffer, DecodeStart::Byte(byte), None)
        .expect("a byte inside the file decodes once the fill reaches it");
    assert!(!played.is_empty(), "the decode from byte {byte} played nothing");
}
