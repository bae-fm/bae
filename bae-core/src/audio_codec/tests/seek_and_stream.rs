/// A `seek_to_sample` streaming decode (FFmpeg's `avformat_seek_file`, over a
/// fully-buffered sparse buffer) yields the same samples as decoding the whole
/// file and slicing at the seek point.
///
/// `avformat_seek_file` lands on the nearest keyframe AT or BEFORE the target, so
/// the decoded output may start slightly before the requested position — the
/// actual alignment is recovered by correlating against the ground truth.
fn check_seek_to_produces_correct_samples(sample_rate: u32, channels: u32, bits_per_sample: u32) {
    use crate::playback::create_track_stream_pair_with_capacity;
    use crate::playback::sparse_buffer::create_sparse_buffer;
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

    let flac_data = encode_i32(
        EncodeFormat::Flac { bits_per_sample },
        &original,
        sample_rate,
        channels,
    )
    .unwrap();
    let ground_truth = decode_audio(buffer_from(&flac_data), None, None).unwrap();

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
            None,
            Some(seek_sample),
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

    assert!(
        seeked_samples.len() > 100,
        "Not enough seeked samples: {}",
        seeked_samples.len()
    );

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
        let result = decode_audio_streaming(
            decode_buffer,
            &mut sink,
            None,
            Some(seek_sample),
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

    let ground = decode_audio(buffer_from(&flac), None, None).expect("ground-truth decode");
    let scale = i32::MAX as f32;

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
            let result = decode_audio_streaming(
                decode_buffer,
                &mut sink,
                seek_to_byte,
                None,                           // no sample seek
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

/// A streaming (non-seekable) Opus/Ogg encode produces a valid stream: the Ogg
/// muxer has a true streaming path, so a plain `Write` sink with no seek
/// callback still yields a decodable file. This is the sink type a socket
/// would use.
#[test]
fn streaming_opus_encode_into_a_plain_write_sink_is_decodable() {
    use std::sync::Mutex;

    init();

    /// A Write-only sink (no Seek): bytes land in a shared Vec.
    #[derive(Clone)]
    struct SharedVec(Arc<Mutex<Vec<u8>>>);
    impl std::io::Write for SharedVec {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let sample_rate = 44_100u32;
    let samples: Vec<i32> = (0..sample_rate as usize * 2)
        .map(|i| {
            let t = (i / 2) as f64 / sample_rate as f64;
            (0.5 * i32::MAX as f64 * (2.0 * std::f64::consts::PI * 440.0 * t).sin()) as i32
        })
        .collect();

    let out = SharedVec(Arc::new(Mutex::new(Vec::new())));
    let mut encoder = StreamingEncoder::streaming(
        StreamEncodeFormat::OpusOgg { bitrate_kbps: 192 },
        Box::new(out.clone()),
        Arc::new(std::sync::atomic::AtomicBool::new(false)),
    );
    encoder.on_format(sample_rate, 2);
    encoder.on_samples(&samples);
    encoder.finish().expect("streaming Opus encode");

    let bytes = out.0.lock().unwrap().clone();
    assert!(
        bytes.len() > 100,
        "Opus/Ogg stream too small: {}",
        bytes.len()
    );
    assert_eq!(&bytes[0..4], b"OggS");

    let decoded = decode_audio(buffer_from(&bytes), None, None).expect("decode streamed Opus");
    assert_eq!(decoded.sample_rate, 48_000);
    assert_eq!(decoded.channels, 2);
    assert!(!decoded.samples.is_empty());
}

/// A streaming (non-seekable) MP3 encode produces a decodable stream: the MP3
/// muxer's Xing/LAME VBR header is patched by seeking back, so the streaming
/// sink disables it (`write_xing=0`), leaving a plain CBR frame stream that
/// needs no seek-back. A socket serving a transcode uses this sink.
#[test]
fn streaming_mp3_encode_into_a_plain_write_sink_is_decodable() {
    use std::sync::Mutex;

    init();

    /// A Write-only sink (no Seek): bytes land in a shared Vec.
    #[derive(Clone)]
    struct SharedVec(Arc<Mutex<Vec<u8>>>);
    impl std::io::Write for SharedVec {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let sample_rate = 44_100u32;
    let seconds = 2;
    let samples: Vec<i32> = (0..sample_rate as usize * 2 * seconds)
        .map(|i| {
            let t = (i / 2) as f64 / sample_rate as f64;
            (0.5 * i32::MAX as f64 * (2.0 * std::f64::consts::PI * 440.0 * t).sin()) as i32
        })
        .collect();

    let out = SharedVec(Arc::new(Mutex::new(Vec::new())));
    let mut encoder = StreamingEncoder::streaming(
        StreamEncodeFormat::Mp3 { bitrate_kbps: 128 },
        Box::new(out.clone()),
        Arc::new(std::sync::atomic::AtomicBool::new(false)),
    );
    encoder.on_format(sample_rate, 2);
    encoder.on_samples(&samples);
    encoder.finish().expect("streaming MP3 encode");

    let bytes = out.0.lock().unwrap().clone();
    assert!(bytes.len() > 100, "MP3 stream too small: {}", bytes.len());

    let decoded = decode_audio(buffer_from(&bytes), None, None).expect("decode streamed MP3");
    assert_eq!(decoded.sample_rate, sample_rate);
    assert_eq!(decoded.channels, 2);
    // The decode recovers roughly the source duration (MP3's encoder/decoder
    // delay shifts the exact frame count, so allow a generous window).
    let decoded_frames = decoded.samples.len() / 2;
    let expected_frames = sample_rate as usize * seconds;
    assert!(
        decoded_frames > expected_frames * 3 / 4,
        "decoded {decoded_frames} frames, expected near {expected_frames}"
    );
}

/// A seekable FLAC encode's patched STREAMINFO carries the real total-sample
/// count — the header patch-back the seekable sink exists for. STREAMINFO's
/// 36-bit total-samples field sits at bytes 21..26 of the file (after "fLaC" +
/// the 4-byte block header + 10 bytes of rates/counts), high bits first.
#[test]
fn seekable_flac_encode_patches_streaminfo_total_samples() {
    init();

    let frames = 44_100u64; // 1s mono
    let samples: Vec<i32> = vec![0i32; frames as usize];
    let flac = encode_i32(
        EncodeFormat::Flac {
            bits_per_sample: 16,
        },
        &samples,
        44_100,
        1,
    )
    .expect("encode FLAC");
    assert_eq!(&flac[0..4], b"fLaC");

    // STREAMINFO: byte 21 low nibble = total-samples bits 32..36, bytes 22..26
    // = bits 0..32.
    let total = ((u64::from(flac[21] & 0x0F)) << 32)
        | (u64::from(flac[22]) << 24)
        | (u64::from(flac[23]) << 16)
        | (u64::from(flac[24]) << 8)
        | u64::from(flac[25]);
    assert_eq!(
        total, frames,
        "STREAMINFO total_samples must be patched to the real frame count"
    );
}

/// A PCM-shape change mid-encode (two decodes with different formats feeding
/// one encoder) is recorded and surfaces at finish — the guarantee the
/// CUE-image save relies on instead of its old pre-encode shape check.
#[test]
fn encoder_rejects_a_pcm_shape_change_mid_stream() {
    init();

    let mut encoder = StreamingEncoder::seekable(
        EncodeFormat::Flac {
            bits_per_sample: 16,
        },
        Box::new(std::io::Cursor::new(Vec::new())),
        Arc::new(std::sync::atomic::AtomicBool::new(false)),
    );
    encoder.on_format(44_100, 2);
    encoder.on_samples(&[0i32; 8820]);
    encoder.on_format(48_000, 2); // a second track probes to a different rate
    assert!(
        encoder.error().is_some(),
        "the shape change must be recorded when it happens"
    );
    let err = encoder
        .finish()
        .expect_err("finish must surface the shape change");
    assert!(err.contains("PCM shape changed"), "unexpected error: {err}");
}
