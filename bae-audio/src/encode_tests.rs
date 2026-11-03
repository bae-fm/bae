use super::*;
use crate::test_samples::filled_sparse_buffer;
use crate::{decode_audio, init};

#[test]
fn test_decode_encode_roundtrip() {
    init();

    // 1 second of silence at 44100Hz stereo.
    let original_samples: Vec<i32> = vec![0i32; 44100 * 2];

    let flac_data =
        encode_i32(EncodeFormat::Flac { bits_per_sample: 16 }, &original_samples, 44100, 2)
            .unwrap();

    assert!(flac_data.len() > 42);
    assert_eq!(&flac_data[0..4], b"fLaC");

    let decoded = decode_audio(filled_sparse_buffer(&flac_data), None, None).unwrap();

    assert_eq!(decoded.sample_rate, 44100);
    assert_eq!(decoded.channels, 2);
    // The encoder pads the last frame, so the counts differ slightly.
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

    // A 440Hz sine wave, 1 second, stereo.
    let sample_rate = 44100u32;
    let duration_samples = sample_rate as usize;
    let amplitude = 0.9 * i32::MAX as f64;

    let samples: Vec<i32> = (0..duration_samples * 2)
        .map(|i| {
            let t = (i / 2) as f64 / sample_rate as f64;
            (amplitude * (2.0 * std::f64::consts::PI * 440.0 * t).sin()) as i32
        })
        .collect();

    let mp3_data =
        encode_i32(EncodeFormat::Mp3 { bitrate_kbps: 320 }, &samples, sample_rate, 2).unwrap();

    // An MP3 opens with either an ID3 tag or a frame sync word.
    assert!(mp3_data.len() > 100, "MP3 data too small: {}", mp3_data.len());
    assert!(
        (mp3_data[0] == 0xFF && (mp3_data[1] & 0xE0) == 0xE0) || &mp3_data[0..3] == b"ID3",
        "Invalid MP3 header: {:02X} {:02X} {:02X}",
        mp3_data[0],
        mp3_data[1],
        mp3_data[2],
    );

    let decoded = decode_audio(filled_sparse_buffer(&mp3_data), None, None).unwrap();
    assert_eq!(decoded.sample_rate, 44100);
    assert_eq!(decoded.channels, 2);
    assert!(decoded.samples.len() > 40000, "Too few decoded samples");
}

/// AAC export needs the native `aac` encoder (planar float input) and the
/// `ipod`/.m4a muxer from the bundled FFmpeg. The muxer writes its sample-table
/// index on finalize by seeking back, so this exercises the seekable path. AAC
/// is lossy, so the round-trip checks the sample count, not exactness.
#[test]
fn test_encode_aac() {
    init();

    let sample_rate = 44100u32;
    let duration_samples = sample_rate as usize;
    let amplitude = 0.5 * i32::MAX as f64;
    let samples: Vec<i32> = (0..duration_samples * 2)
        .map(|i| {
            let t = (i / 2) as f64 / sample_rate as f64;
            (amplitude * (2.0 * std::f64::consts::PI * 440.0 * t).sin()) as i32
        })
        .collect();

    let aac_data =
        encode_i32(EncodeFormat::Aac { bitrate_kbps: 256 }, &samples, sample_rate, 2).unwrap();

    // An MP4 file (the .m4a flavor the ipod muxer writes) opens with an `ftyp`
    // box: a 4-byte size, then the type "ftyp".
    assert!(aac_data.len() > 100, "AAC data too small: {}", aac_data.len());
    assert_eq!(&aac_data[4..8], b"ftyp", "AAC output missing ftyp box");

    let decoded = decode_audio(filled_sparse_buffer(&aac_data), None, None).unwrap();
    assert_eq!(decoded.sample_rate, 44100);
    assert_eq!(decoded.channels, 2);
    assert!(decoded.samples.len() > 40000, "Too few decoded samples");
}

#[test]
fn test_encode_opus_ogg() {
    init();

    let sample_rate = 44100u32;
    let duration_samples = sample_rate as usize;
    let amplitude = 0.5 * i32::MAX as f64;
    let samples: Vec<i32> = (0..duration_samples * 2)
        .map(|i| {
            let t = (i / 2) as f64 / sample_rate as f64;
            (amplitude * (2.0 * std::f64::consts::PI * 440.0 * t).sin()) as i32
        })
        .collect();

    let opus_data =
        encode_i32(EncodeFormat::OpusOgg { bitrate_kbps: 192 }, &samples, sample_rate, 2).unwrap();

    assert!(opus_data.len() > 100, "Opus/Ogg data too small: {}", opus_data.len());
    assert_eq!(&opus_data[0..4], b"OggS");

    let decoded = decode_audio(filled_sparse_buffer(&opus_data), None, None).unwrap();
    assert_eq!(decoded.sample_rate, 48_000);
    assert_eq!(decoded.channels, 2);
    assert!(!decoded.samples.is_empty());
}

/// AIFF export needs the big-endian PCM encoders and the aiff muxer from the
/// bundled FFmpeg — every offered bit depth encodes and decodes back.
#[test]
fn test_encode_aiff() {
    init();

    let sample_rate = 44100u32;
    let duration_samples = sample_rate as usize;
    let amplitude = 0.5 * i32::MAX as f64;
    let samples: Vec<i32> = (0..duration_samples * 2)
        .map(|i| {
            let t = (i / 2) as f64 / sample_rate as f64;
            (amplitude * (2.0 * std::f64::consts::PI * 440.0 * t).sin()) as i32
        })
        .collect();

    for bits_per_sample in [16u32, 24, 32] {
        let aiff_data =
            encode_i32(EncodeFormat::PcmAiff { bits_per_sample }, &samples, sample_rate, 2)
                .unwrap();

        assert_eq!(&aiff_data[0..4], b"FORM", "AIFF ({bits_per_sample}-bit) missing FORM chunk");
        assert_eq!(
            &aiff_data[8..12],
            b"AIFF",
            "AIFF ({bits_per_sample}-bit) missing AIFF form type"
        );

        let decoded = decode_audio(filled_sparse_buffer(&aiff_data), None, None).unwrap();
        assert_eq!(decoded.sample_rate, 44100);
        assert_eq!(decoded.channels, 2);
        assert!(decoded.samples.len() > 40000, "Too few decoded samples");
    }
}

/// 32-bit WAV export needs the pcm_s32le encoder from the bundled FFmpeg — the
/// bit-depth picker offers 32-bit for every lossless format.
#[test]
fn test_encode_wav_32() {
    init();

    let sample_rate = 44100u32;
    let duration_samples = sample_rate as usize;
    let amplitude = 0.5 * i32::MAX as f64;
    let samples: Vec<i32> = (0..duration_samples * 2)
        .map(|i| {
            let t = (i / 2) as f64 / sample_rate as f64;
            (amplitude * (2.0 * std::f64::consts::PI * 440.0 * t).sin()) as i32
        })
        .collect();

    let wav_data =
        encode_i32(EncodeFormat::PcmWav { bits_per_sample: 32 }, &samples, sample_rate, 2).unwrap();

    assert_eq!(&wav_data[0..4], b"RIFF");
    let decoded = decode_audio(filled_sparse_buffer(&wav_data), None, None).unwrap();
    assert_eq!(decoded.sample_rate, 44100);
    assert_eq!(decoded.channels, 2);
    assert!(decoded.samples.len() > 40000, "Too few decoded samples");
}

/// A FLAC round-trip is lossless: every sample matches exactly. Any sample-
/// conversion bug — wrong byte order, wrong scaling, wrong format detection —
/// shows up here as a mismatch.
#[test]
fn test_flac_roundtrip_is_lossless() {
    init();

    // A 440Hz sine wave aligned to 16-bit steps across the full i32 range.
    let sample_rate = 44100u32;
    let duration_samples = sample_rate as usize; // 1 second

    let original: Vec<i32> = (0..duration_samples)
        .map(|i| {
            let t = i as f64 / sample_rate as f64;
            let sample = (i16::MAX as f64 * (2.0 * std::f64::consts::PI * 440.0 * t).sin()) as i32;
            sample << 16
        })
        .collect();

    let flac_data =
        encode_i32(EncodeFormat::Flac { bits_per_sample: 16 }, &original, sample_rate, 1).unwrap();
    let decoded = decode_audio(filled_sparse_buffer(&flac_data), None, None).unwrap();

    let compare_len = original.len().min(decoded.samples.len());
    assert!(compare_len > 0, "No samples to compare");

    let mut mismatches = 0;
    let mut max_diff = 0i32;
    for (orig, dec) in original.iter().zip(decoded.samples.iter()).take(compare_len) {
        let diff = (orig - dec).abs();
        if diff > 0 {
            mismatches += 1;
            max_diff = max_diff.max(diff);
        }
    }

    assert!(
        max_diff < 2, // A tiny rounding error is tolerated.
        "FLAC roundtrip should be lossless. {} samples differ, max diff: {}. \
         This indicates a bug in sample conversion (wrong byte order, scaling, or format).",
        mismatches,
        max_diff
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
    assert!(bytes.len() > 100, "Opus/Ogg stream too small: {}", bytes.len());
    assert_eq!(&bytes[0..4], b"OggS");

    let decoded =
        decode_audio(filled_sparse_buffer(&bytes), None, None).expect("decode streamed Opus");
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

    let decoded =
        decode_audio(filled_sparse_buffer(&bytes), None, None).expect("decode streamed MP3");
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
    let flac = encode_i32(EncodeFormat::Flac { bits_per_sample: 16 }, &samples, 44_100, 1)
        .expect("encode FLAC");
    assert_eq!(&flac[0..4], b"fLaC");

    // STREAMINFO: byte 21 low nibble = total-samples bits 32..36, bytes 22..26
    // = bits 0..32.
    let total = ((u64::from(flac[21] & 0x0F)) << 32)
        | (u64::from(flac[22]) << 24)
        | (u64::from(flac[23]) << 16)
        | (u64::from(flac[24]) << 8)
        | u64::from(flac[25]);
    assert_eq!(total, frames, "STREAMINFO total_samples must be patched to the real frame count");
}

/// A PCM-shape change mid-encode (two decodes with different formats feeding
/// one encoder) is recorded and surfaces at finish — the guarantee the
/// CUE-image save relies on instead of its old pre-encode shape check.
#[test]
fn encoder_rejects_a_pcm_shape_change_mid_stream() {
    init();

    let mut encoder = StreamingEncoder::seekable(
        EncodeFormat::Flac { bits_per_sample: 16 },
        Box::new(std::io::Cursor::new(Vec::new())),
        Arc::new(std::sync::atomic::AtomicBool::new(false)),
    );
    encoder.on_format(44_100, 2);
    encoder.on_samples(&[0i32; 8820]);
    encoder.on_format(48_000, 2); // a second track probes to a different rate
    assert!(encoder.error().is_some(), "the shape change must be recorded when it happens");
    let err = encoder.finish().expect_err("finish must surface the shape change");
    assert!(err.contains("PCM shape changed"), "unexpected error: {err}");
}

/// A streaming (non-seekable) WAV encode is a valid WAV of unknown length: the
/// RIFF and `data` sizes say "unknown" (`0xFFFFFFFF`) rather than a count the
/// muxer would have patched in, the `fmt ` chunk describes the PCM, and the
/// `data` payload is exactly the input at the requested depth — at 16 bits the
/// top half of each sample, at 24 bits its top three bytes, little-endian. No
/// chunk sits between `fmt ` and `data`, so at 16 bits the header is the
/// canonical 44 bytes a renderer's simplest WAV reader expects.
#[test]
fn streaming_wav_encode_into_a_plain_write_sink_is_unsized_pcm() {
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

    for bits in [16u32, 24] {
        let out = SharedVec(Arc::new(Mutex::new(Vec::new())));
        let mut encoder = StreamingEncoder::streaming(
            StreamEncodeFormat::PcmWav { bits_per_sample: bits },
            Box::new(out.clone()),
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        );
        encoder.on_format(sample_rate, 2);
        encoder.on_samples(&samples);
        encoder.finish().expect("streaming WAV encode");
        let bytes = out.0.lock().unwrap().clone();

        let wav = crate::parse_streamed_wav(&bytes).expect("a WAV stream");
        if bits == 16 {
            assert_eq!(&bytes[36..40], b"data", "data follows fmt directly");
            assert_eq!(bytes.len() - wav.data.len(), 44, "a canonical header");
        }
        assert_eq!(wav.riff_size, u32::MAX, "{bits}-bit RIFF size is unknown");
        assert_eq!(wav.data_size, u32::MAX, "{bits}-bit data size is unknown");
        assert_eq!(wav.channels, 2);
        assert_eq!(wav.sample_rate, sample_rate);
        assert_eq!(wav.bits_per_sample, bits);
        let bytes_per_sample = bits as usize / 8;
        assert_eq!(
            wav.data.len(),
            samples.len() * bytes_per_sample,
            "{bits}-bit data holds every input frame"
        );
        let expected: Vec<u8> = samples
            .iter()
            .flat_map(|&s| s.to_le_bytes()[4 - bytes_per_sample..].to_vec())
            .collect();
        assert!(wav.data == expected, "{bits}-bit data is the input's top {bits} bits");
    }
}
