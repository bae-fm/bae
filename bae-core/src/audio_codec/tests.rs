use super::probe::content_type_from_codec_id;
use super::*;
use crate::util::content_type::ContentType;
use std::sync::Arc;

/// `seek_landing_bytes` actually opens the file, seeks (no decode), and
/// reports ascending byte positions within the file that follow the sample
/// positions -- what the import-time byte boundary computation relies on.
#[test]
fn seek_landing_bytes_track_sample_positions() {
    init();
    let sample_rate = 44100u32;
    let seconds = 4usize;
    let total = sample_rate as usize * seconds;
    let samples: Vec<i32> = (0..total)
        .map(|i| ((i as f64 * 0.02).sin() * 12000.0) as i32)
        .collect();
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let flac = encode_to_flac(&samples, sample_rate, 1, 16, &cancel).unwrap();

    let mut file = tempfile::NamedTempFile::new().unwrap();
    std::io::Write::write_all(&mut file, &flac).unwrap();
    std::io::Write::flush(&mut file).unwrap();
    let path = file.path().to_str().unwrap();
    let file_size = flac.len() as u64;

    // Probe at second boundaries (0, 1s, 2s, 3s).
    let probes: Vec<u64> = (0..seconds as u64)
        .map(|s| s * sample_rate as u64)
        .collect();
    let offsets = seek_landing_bytes(path, &probes).expect("seek_landing_bytes");
    assert_eq!(offsets.len(), probes.len());

    // Non-decreasing and within the file.
    for w in offsets.windows(2) {
        assert!(
            w[1] >= w[0],
            "byte offsets must ascend with samples: {offsets:?}"
        );
    }
    assert!(
        *offsets.last().unwrap() < file_size,
        "offsets stay within the file ({offsets:?} vs {file_size})"
    );
    // A later sample is strictly later in the file, and the 3/4 sample is
    // well into the file -- the offsets follow the content, not a constant.
    assert!(offsets[seconds - 1] > offsets[0]);
    assert!(
        offsets[seconds - 1] as f64 > file_size as f64 * 0.4,
        "the 3/4 sample should be well into the file: {} of {file_size}",
        offsets[seconds - 1]
    );
}

/// `seek_landing_bytes` against the real single-file CUE/FLAC fixture:
/// silence -> white noise -> brown noise, so the bitrate swings widely from
/// track to track. The seek offsets follow the *content*, not time -- the
/// silence track compresses to almost nothing, so the 10s track-2 boundary
/// lands at a small slice of the file, far below where a time-proportional
/// estimate puts it. That gap is why the offsets are computed at import by
/// seeking rather than estimated from time at playback.
#[test]
fn seek_landing_bytes_follow_content_not_time_when_bitrate_swings() {
    init();
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/cue_flac/Test Album.flac"
    );
    let probe = probe_audio_from_path(path).expect("probe fixture");
    let sr = probe.sample_rate as u64;
    let total_samples = (probe.duration.as_secs_f64() * sr as f64) as u64;
    let file_size = std::fs::metadata(path).unwrap().len();

    // Track starts from the CUE: 0s (silence), 10s (white noise), 20s (brown).
    let starts = [0u64, 10 * sr, 20 * sr];
    let exact = seek_landing_bytes(path, &starts).expect("offsets");

    // Time-proportional estimate -- what we'd get without seeking.
    let proportional: Vec<u64> = starts
        .iter()
        .map(|&s| (s as f64 / total_samples as f64 * file_size as f64) as u64)
        .collect();

    // The 10s boundary (end of the silence track) is where they diverge most:
    // silence is nearly free to store, so the exact offset is a small slice of
    // the file while the estimate puts it ~10/total of the way in.
    assert!(
        exact[1] < file_size / 5,
        "silence track should occupy a small byte slice; exact[1]={} of {file_size}",
        exact[1]
    );
    assert!(
        proportional[1] > exact[1] * 2,
        "the time estimate over-places the boundary: prop={} exact={}",
        proportional[1],
        exact[1]
    );
    // Ascending and within the file.
    assert!(exact[0] <= exact[1] && exact[1] <= exact[2] && exact[2] < file_size);
    // The brown-noise track (20s start) lands deep in the file -- it compresses
    // far worse than the silence, so most of the file is its bytes. Confirms the
    // landings track content in both directions, not just the silent-track dip.
    assert!(
        exact[2] as f64 > file_size as f64 * 0.4,
        "brown-noise track start lands deep in the file: {} of {file_size}",
        exact[2]
    );
}

#[test]
fn content_type_from_codec_id_maps_every_named_codec() {
    use ffmpeg_sys_next::AVCodecID;
    assert_eq!(
        content_type_from_codec_id(AVCodecID::AV_CODEC_ID_FLAC),
        ContentType::Flac
    );
    assert_eq!(
        content_type_from_codec_id(AVCodecID::AV_CODEC_ID_MP3),
        ContentType::Mp3
    );
    assert_eq!(
        content_type_from_codec_id(AVCodecID::AV_CODEC_ID_APE),
        ContentType::Ape
    );
    assert_eq!(
        content_type_from_codec_id(AVCodecID::AV_CODEC_ID_ALAC),
        ContentType::Alac
    );
    assert_eq!(
        content_type_from_codec_id(AVCodecID::AV_CODEC_ID_AAC),
        ContentType::Aac
    );
}

#[test]
fn content_type_from_codec_id_unknown_falls_into_other() {
    use ffmpeg_sys_next::AVCodecID;
    // Any codec outside our whitelist must round-trip as `Other(...)` so
    // the DB preserves the ID and nothing downstream silently treats it
    // as a known format.
    let ct = content_type_from_codec_id(AVCodecID::AV_CODEC_ID_OPUS);
    match ct {
        ContentType::Other(s) => assert!(s.starts_with("codec:")),
        other => panic!("expected Other(codec:...), got {:?}", other),
    }
}

#[test]
fn test_decode_encode_roundtrip() {
    init();

    // Create test samples (1 second of silence at 44100Hz stereo)
    let original_samples: Vec<i32> = vec![0i32; 44100 * 2];

    // Encode to FLAC
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let flac_data = encode_to_flac(&original_samples, 44100, 2, 16, &cancel).unwrap();

    // Verify FLAC signature
    assert!(flac_data.len() > 42);
    assert_eq!(&flac_data[0..4], b"fLaC");

    // Decode back
    let decoded = decode_audio(&flac_data, None, None).unwrap();

    assert_eq!(decoded.sample_rate, 44100);
    assert_eq!(decoded.channels, 2);
    // Sample counts should be approximately equal (may differ slightly due to padding)
    assert!(
        (decoded.samples.len() as i64 - original_samples.len() as i64).abs() < 1000,
        "Sample count mismatch: {} vs {}",
        decoded.samples.len(),
        original_samples.len()
    );
}

#[test]
fn test_encode_mp3() {
    init();

    // Create a 440Hz sine wave - 1 second stereo
    let sample_rate = 44100u32;
    let duration_samples = sample_rate as usize;
    let amplitude = 30000i32;

    let samples: Vec<i32> = (0..duration_samples * 2)
        .map(|i| {
            let t = (i / 2) as f64 / sample_rate as f64;
            (amplitude as f64 * (2.0 * std::f64::consts::PI * 440.0 * t).sin()) as i32
        })
        .collect();

    let cancel = std::sync::atomic::AtomicBool::new(false);
    let mp3_data = encode_to_mp3(&samples, sample_rate, 2, 16, 320_000, &cancel).unwrap();

    // MP3 files start with either ID3 tag (0x49 0x44 0x33) or sync word (0xFF 0xFB)
    assert!(
        mp3_data.len() > 100,
        "MP3 data too small: {}",
        mp3_data.len()
    );
    assert!(
        (mp3_data[0] == 0xFF && (mp3_data[1] & 0xE0) == 0xE0) || &mp3_data[0..3] == b"ID3",
        "Invalid MP3 header: {:02X} {:02X} {:02X}",
        mp3_data[0],
        mp3_data[1],
        mp3_data[2],
    );

    // Decode it back and verify we get audio
    let decoded = decode_audio(&mp3_data, None, None).unwrap();
    assert_eq!(decoded.sample_rate, 44100);
    assert_eq!(decoded.channels, 2);
    assert!(decoded.samples.len() > 40000, "Too few decoded samples");
}

/// Test that FLAC encode/decode is lossless - samples should match exactly.
///
/// This catches any sample conversion bugs: wrong byte order, wrong scaling,
/// wrong format detection, etc. If anything is wrong, values won't match.
#[test]
fn test_flac_roundtrip_is_lossless() {
    init();

    // Create a 440Hz sine wave - uses the full 16-bit range
    let sample_rate = 44100u32;
    let duration_samples = sample_rate as usize; // 1 second
    let amplitude = 30000i32; // Near max 16-bit

    let original: Vec<i32> = (0..duration_samples)
        .map(|i| {
            let t = i as f64 / sample_rate as f64;
            (amplitude as f64 * (2.0 * std::f64::consts::PI * 440.0 * t).sin()) as i32
        })
        .collect();

    // Encode to FLAC and decode back
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let flac_data = encode_to_flac(&original, sample_rate, 1, 16, &cancel).unwrap();
    let decoded = decode_audio(&flac_data, None, None).unwrap();

    // FLAC is lossless - samples should match exactly
    let compare_len = original.len().min(decoded.samples.len());
    assert!(compare_len > 0, "No samples to compare");

    let mut mismatches = 0;
    let mut max_diff = 0i32;
    for (orig, dec) in original
        .iter()
        .zip(decoded.samples.iter())
        .take(compare_len)
    {
        let diff = (orig - dec).abs();
        if diff > 0 {
            mismatches += 1;
            max_diff = max_diff.max(diff);
        }
    }

    assert!(
        max_diff < 2, // Allow tiny rounding errors
        "FLAC roundtrip should be lossless. {} samples differ, max diff: {}. \
         This indicates a bug in sample conversion (wrong byte order, scaling, or format).",
        mismatches,
        max_diff
    );
}

#[test]
fn test_streaming_decode() {
    use crate::playback::create_track_stream_pair_with_capacity;
    use crate::playback::sparse_buffer::create_sparse_buffer;
    use std::thread;

    init();

    // Create test FLAC data
    let samples: Vec<i32> = (0..44100)
        .map(|i| ((i as f64 * 0.01).sin() * 10000.0) as i32)
        .collect();
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let flac_data = encode_to_flac(&samples, 44100, 1, 16, &cancel).unwrap();

    // Create streaming infrastructure with sparse buffer
    let buffer = create_sparse_buffer(flac_data.len() as u64);
    let (mut sink, mut source, _ready) = create_track_stream_pair_with_capacity(44100, 1, 100000);

    // Spawn decoder thread using new AVIO-based streaming decode
    let decoder_buffer = buffer.clone();
    let decoder_handle = thread::spawn(move || {
        let token = Arc::new(std::sync::atomic::AtomicBool::new(false));
        decode_audio_streaming(
            decoder_buffer,
            &mut sink,
            None,
            None,
            None,
            None,
            None,
            token,
        )
    });

    // Feed data to buffer (simulating download)
    buffer.append_at(0, &flac_data);

    // Wait for decoder
    let result = decoder_handle.join().unwrap();
    assert!(result.is_ok(), "Decode failed: {:?}", result.err());

    // Pull samples from source
    let mut decoded_samples = Vec::new();
    let mut buf = [0.0f32; 1024];
    loop {
        let n = source.pull_samples(&mut buf);
        if n == 0 && source.is_finished() {
            break;
        }
        decoded_samples.extend_from_slice(&buf[..n]);
    }

    // Should have approximately the same number of samples
    assert!(
        (decoded_samples.len() as i64 - samples.len() as i64).abs() < 1000,
        "Sample count mismatch: {} vs {}",
        decoded_samples.len(),
        samples.len()
    );
}

#[test]
fn test_streaming_decode_treats_cancelled_input_as_normal_stop() {
    use crate::playback::create_track_stream_pair_with_capacity;
    use crate::playback::sparse_buffer::create_sparse_buffer;

    init();

    // Size is irrelevant: the buffer is cancelled before any read.
    let buffer = create_sparse_buffer(4096);
    buffer.cancel();
    let (mut sink, source, _ready) = create_track_stream_pair_with_capacity(44100, 1, 4096);
    let token = Arc::new(std::sync::atomic::AtomicBool::new(false));

    let result = decode_audio_streaming(buffer, &mut sink, None, None, None, None, None, token);

    assert_eq!(result, Err(StreamingDecodeError::InputCancelled));
    assert!(!source.producer_finished());
    assert_eq!(source.samples_decoded(), 0);
}

/// Helper: test that seek_to via avformat_seek_file produces correct samples.
///
/// Same ground truth as check_seek_produces_correct_samples, but uses the
/// seek_to parameter (ffmpeg-level seek within a full SparseBuffer) instead
/// of the seektable-based restart approach.
///
/// avformat_seek_file lands on the nearest keyframe AT or BEFORE the target,
/// so decoded output may start slightly before the requested position.
/// We find the actual alignment by correlating against ground truth.
fn check_seek_to_produces_correct_samples(sample_rate: u32, channels: u32, bits_per_sample: u32) {
    use crate::playback::create_track_stream_pair_with_capacity;
    use crate::playback::sparse_buffer::create_sparse_buffer;
    use std::thread;

    let duration_secs = 2;
    let frame_samples = sample_rate as usize * duration_secs;
    let max_val = (1i32 << (bits_per_sample - 1)) - 1;

    let mut original: Vec<i32> = Vec::with_capacity(frame_samples * channels as usize);
    for i in 0..frame_samples {
        let t = i as f64 / sample_rate as f64;
        let freq = 200.0 + (t / duration_secs as f64) * 1800.0;
        let sample =
            ((max_val as f64 * 0.8) * (2.0 * std::f64::consts::PI * freq * t).sin()) as i32;
        original.push(sample);
        if channels == 2 {
            original.push(-sample);
        }
    }

    let cancel = std::sync::atomic::AtomicBool::new(false);
    let flac_data =
        encode_to_flac(&original, sample_rate, channels, bits_per_sample, &cancel).unwrap();
    let ground_truth = decode_audio(&flac_data, None, None).unwrap();

    let seek_sample = sample_rate as u64; // 1 second

    // Fill a SparseBuffer with the complete file, then seek via seek_to
    let buffer = create_sparse_buffer(flac_data.len() as u64);
    buffer.append_at(0, &flac_data);

    let (mut sink, mut source, _ready) =
        create_track_stream_pair_with_capacity(sample_rate, channels, 500000);
    let decoder_handle = thread::spawn(move || {
        let token = Arc::new(std::sync::atomic::AtomicBool::new(false));
        decode_audio_streaming(
            buffer,
            &mut sink,
            None,
            Some(seek_sample),
            None,
            None,
            None,
            token,
        )
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

    assert!(
        seeked_samples.len() > 100,
        "Not enough seeked samples: {}",
        seeked_samples.len()
    );

    let scale = if bits_per_sample <= 16 {
        i16::MAX as f32
    } else {
        i32::MAX as f32
    };

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
        for (&seeked, &truth) in seeked_samples[..window]
            .iter()
            .zip(&ground_truth.samples[offset..offset + window])
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
/// bisects frame by frame. Over a cloud home every one of those reads is a
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
    use crate::playback::create_track_stream_pair_with_capacity;
    use crate::playback::sparse_buffer::create_sparse_buffer;
    use std::thread;

    init();

    // 30s synthetic stereo CUE/FLAC fixture (silence / white noise / brown noise)
    // carrying a 6-point seektable.
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/cue_flac/Test Album.flac"
    );
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
        decode_audio_streaming(
            decode_buffer,
            &mut sink,
            None,
            Some(seek_sample),
            None,
            Some(stop_sample),
            None,
            token,
        )
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
    use crate::playback::create_track_stream_pair_with_capacity;
    use crate::playback::sparse_buffer::create_sparse_buffer;
    use std::thread;

    init();
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/cue_flac/Test Album.flac"
    );
    let flac = std::fs::read(path).expect("read fixture");
    let sample_rate = 44100u64;
    let channels = 2u32;
    // Track 2 start (white noise) -- random content correlates precisely, unlike
    // the silent track 1. Track 3's landing (20s) is the deliberately wrong byte,
    // well past track 2's target.
    let start = 10 * sample_rate;
    let landings = seek_landing_bytes(path, &[start, 20 * sample_rate]).expect("landings");
    let landing = landings[0];
    let wrong_landing = landings[1];

    let ground = decode_audio(&flac, None, None).expect("ground-truth decode");
    let scale = i16::MAX as f32; // 16-bit fixture

    // Decode track 2 with a given by-byte seek target, trimming to `start` and
    // stopping ~0.1s later; return the interleaved output samples.
    let decode_from = |seek_to_byte: Option<u64>| -> Vec<f32> {
        let buffer = create_sparse_buffer(flac.len() as u64);
        buffer.append_at(0, &flac);
        let (mut sink, mut source, _ready) =
            create_track_stream_pair_with_capacity(sample_rate as u32, channels, 500_000);
        let decode_buffer = buffer.clone();
        let handle = thread::spawn(move || {
            let token = Arc::new(std::sync::atomic::AtomicBool::new(false));
            decode_audio_streaming(
                decode_buffer,
                &mut sink,
                seek_to_byte,
                None,                           // no sample seek
                Some(start),                    // trim lead-in to the start sample
                Some(start + sample_rate / 10), // stop ~0.1s later
                None,
                token,
            )
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
    let seeked = decode_from(Some(landing));
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
    let sabotaged = decode_from(Some(wrong_landing));
    assert!(
        !is_sample_exact(&sabotaged),
        "a wrong landing byte still produced the target sample (len {}, mse {:.6}) -- \
         the byte-seek target is not driving where output begins",
        sabotaged.len(),
        if sabotaged.len() >= 256 {
            mse(&sabotaged, target)
        } else {
            f64::NAN
        },
    );
}
