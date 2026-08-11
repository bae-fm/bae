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
        let decoded = decode_audio(buffer_from(&case.wav), None, None).unwrap();
        assert_eq!(decoded.samples.len(), 3, "{}", case.name);
        assert!(
            decoded.samples[0] < case.min_below,
            "{}: {:?}",
            case.name,
            decoded.samples
        );
        assert!(
            decoded.samples[1].abs() < 20_000_000,
            "{}: {:?}",
            case.name,
            decoded.samples
        );
        assert!(
            decoded.samples[2] > case.max_above,
            "{}: {:?}",
            case.name,
            decoded.samples
        );
    }
}

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
        .map(|i| ((i as f64 * 0.02).sin() * 0.5 * i32::MAX as f64) as i32)
        .collect();
    let flac = encode_i32(
        EncodeFormat::Flac {
            bits_per_sample: 16,
        },
        &samples,
        sample_rate,
        1,
    )
    .unwrap();

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
    assert_eq!(
        content_type_from_codec_id(AVCodecID::AV_CODEC_ID_PCM_S16LE),
        ContentType::Pcm
    );
    assert_eq!(
        content_type_from_codec_id(AVCodecID::AV_CODEC_ID_PCM_F64BE),
        ContentType::Pcm
    );
    assert_eq!(
        content_type_from_codec_id(AVCodecID::AV_CODEC_ID_OPUS),
        ContentType::Opus
    );
    assert_eq!(
        content_type_from_codec_id(AVCodecID::AV_CODEC_ID_VORBIS),
        ContentType::Vorbis
    );
    assert_eq!(
        content_type_from_codec_id(AVCodecID::AV_CODEC_ID_WAVPACK),
        ContentType::WavPack
    );
    assert_eq!(
        content_type_from_codec_id(AVCodecID::AV_CODEC_ID_DSD_LSBF),
        ContentType::Dsd
    );
    assert_eq!(
        content_type_from_codec_id(AVCodecID::AV_CODEC_ID_DSD_MSBF),
        ContentType::Dsd
    );
    assert_eq!(
        content_type_from_codec_id(AVCodecID::AV_CODEC_ID_DSD_LSBF_PLANAR),
        ContentType::Dsd
    );
    assert_eq!(
        content_type_from_codec_id(AVCodecID::AV_CODEC_ID_DSD_MSBF_PLANAR),
        ContentType::Dsd
    );
}

#[test]
fn content_type_from_codec_id_unknown_falls_into_other() {
    use ffmpeg_sys_next::AVCodecID;
    // Any codec outside our whitelist must round-trip as `Other(...)` so
    // the DB preserves the ID and nothing downstream silently treats it
    // as a known format.
    let ct = content_type_from_codec_id(AVCodecID::AV_CODEC_ID_AC3);
    match ct {
        ContentType::Other(s) => assert!(s.starts_with("codec:")),
        other => panic!("expected Other(codec:...), got {:?}", other),
    }
}

/// A probe describes the bytes at the path, not the path. Writing different
/// audio over a file changes its size and modification time, and the next probe
/// reads the new content rather than repeating what the old one said — the
/// whole reason the remembered answer is stamped with the file's identity.
#[test]
fn probing_a_path_whose_file_changed_reads_the_new_file() {
    init();
    let fixture = |name: &str| {
        format!(
            "{}/test-fixtures/audio-format/{name}",
            env!("CARGO_MANIFEST_DIR")
        )
    };
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let target = tmp.path().join("audio");
    let target_str = target.to_str().expect("temp path is UTF-8");

    std::fs::copy(fixture("placeholder-opus.opus"), &target).expect("write first audio");
    let first = probe_audio_from_path(target_str).expect("probe first audio");
    assert_eq!(first.content_type, ContentType::Opus);
    assert_eq!(
        probe_audio_from_path(target_str).map(|probe| probe.content_type),
        Some(ContentType::Opus),
        "an unchanged file answers again without being read again",
    );
    assert_eq!(super::probe_opens_for(&target), 1);

    std::fs::copy(fixture("placeholder-wavpack.wv"), &target).expect("write second audio");
    let second = probe_audio_from_path(target_str).expect("probe second audio");
    assert_eq!(second.content_type, ContentType::WavPack);
    assert_eq!(super::probe_opens_for(&target), 2);
}

#[test]
fn probe_audio_from_path_maps_audio_format_fixtures() {
    init();
    let fixture = |name: &str| {
        format!(
            "{}/test-fixtures/audio-format/{name}",
            env!("CARGO_MANIFEST_DIR")
        )
    };
    for (name, expected) in [
        ("placeholder-pcm.wav", ContentType::Pcm),
        ("placeholder-pcm.aiff", ContentType::Pcm),
        ("placeholder-opus.opus", ContentType::Opus),
        ("placeholder-vorbis.ogg", ContentType::Vorbis),
        ("placeholder-wavpack.wv", ContentType::WavPack),
        ("placeholder-dsd.dsf", ContentType::Dsd),
        ("placeholder-dsd.dff", ContentType::Dsd),
    ] {
        let path = fixture(name);
        let probe = probe_audio_from_path(&path).unwrap_or_else(|| panic!("probe {name}"));
        assert_eq!(probe.content_type, expected, "{name}");
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
        let decoded =
            decode_audio(buffer_from(&bytes), None, None).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(decoded.sample_rate > 0, "{name}");
        assert!(decoded.channels > 0, "{name}");
        assert!(!decoded.samples.is_empty(), "{name}");
    }
}

/// The i32 decode path reads through a buffer being filled *concurrently* by a
/// `LocalReader`: the decode blocks on the fill and produces exactly the
/// pre-filled result — and, because the sink path sets no read-ahead ceiling,
/// the fill keeps only a window ahead and evicts behind the decode, so the
/// buffer never holds the whole file.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn i32_decode_streams_from_a_live_fill_and_stays_windowed() {
    use crate::playback::data_source::{AudioDataReader, LocalReader};
    use crate::playback::sparse_buffer::create_sparse_buffer;

    init();

    /// Order-sensitive digest of the decode, so two decodes can be compared
    /// without holding both PCM streams.
    #[derive(Default)]
    struct DigestSink {
        format: Option<(u32, u32)>,
        count: u64,
        digest: u64,
    }
    impl DecodedSink for DigestSink {
        fn on_format(&mut self, sample_rate: u32, channels: u32) {
            self.format = Some((sample_rate, channels));
        }
        fn on_samples(&mut self, samples: &[i32]) {
            for &sample in samples {
                self.count += 1;
                self.digest = self
                    .digest
                    .wrapping_mul(1099511628211)
                    .wrapping_add(sample as u32 as u64);
            }
        }
    }

    // ~100s of pseudo-noise as WAV (~17 MiB): comfortably larger than the
    // fill's read-ahead window plus its keep-behind margin, so an unwindowed
    // buffer would blow the residency bound below.
    let sample_rate = 44_100u32;
    let seconds = 100usize;
    let samples: Vec<i32> = (0..sample_rate as usize * seconds * 2)
        .map(|i| (i as u32).wrapping_mul(2_654_435_761) as i32)
        .collect();
    let wav = encode_i32(
        EncodeFormat::PcmWav {
            bits_per_sample: 16,
        },
        &samples,
        sample_rate,
        2,
    )
    .unwrap();
    drop(samples);

    let never = || Arc::new(std::sync::atomic::AtomicBool::new(false));

    // Ground truth: the same window over a fully pre-filled buffer.
    let prefilled = buffer_from(&wav);
    let expected = tokio::task::spawn_blocking({
        let cancel = never();
        move || {
            let mut sink = DigestSink::default();
            decode_audio_to_sink(prefilled, None, None, &mut sink, cancel).expect("prefilled");
            sink
        }
    })
    .await
    .expect("prefilled decode task");

    // Live fill: the decoder starts against an empty buffer and blocks on the
    // LocalReader's windows as they land.
    let mut temp = tempfile::NamedTempFile::new().unwrap();
    std::io::Write::write_all(&mut temp, &wav).unwrap();
    std::io::Write::flush(&mut temp).unwrap();
    let buffer = create_sparse_buffer(wav.len() as u64);
    Box::new(LocalReader::new(temp.path().to_str().expect("UTF-8 path")))
        .start_reading(buffer.clone(), Box::new(|_| {}));

    let streamed = tokio::task::spawn_blocking({
        let buffer = buffer.clone();
        let cancel = never();
        move || {
            let mut sink = DigestSink::default();
            decode_audio_to_sink(buffer, None, None, &mut sink, cancel).expect("streamed");
            sink
        }
    })
    .await
    .expect("streamed decode task");

    assert_eq!(streamed.format, expected.format);
    assert_eq!(
        streamed.count, expected.count,
        "live-fill decode must produce the same sample count"
    );
    assert_eq!(
        streamed.digest, expected.digest,
        "live-fill decode must be byte-identical to the pre-filled decode"
    );

    // The residency bound: one 4 MiB read-ahead window plus the 4 MiB
    // keep-behind margin, with slack for in-flight windows — far below the
    // whole file. Without eviction (or with a whole-file ceiling) the buffer
    // would hold all ~17 MiB.
    let bound = 13 * 1024 * 1024;
    let buffered = buffer.total_buffered();
    assert!(
        buffered <= bound,
        "buffer must stay windowed: held {buffered} bytes, bound {bound}, whole file {}",
        wav.len()
    );
}

/// A buffer cancelled mid-decode (a failed fill, or the caller aborting) must
/// fail the i32 decode rather than return a truncated `Ok` — a consumer
/// treating a partial decode as the whole window (a save writing a short file
/// as success) would be silent corruption.
#[test]
fn i32_decode_fails_loud_when_the_buffer_is_cancelled_mid_stream() {
    use crate::playback::sparse_buffer::create_sparse_buffer;

    init();

    struct NullSink;
    impl DecodedSink for NullSink {
        fn on_format(&mut self, _sample_rate: u32, _channels: u32) {}
        fn on_samples(&mut self, _samples: &[i32]) {}
    }

    let samples = vec![0i32; 44_100 * 2]; // 1s stereo silence
    let wav = encode_i32(
        EncodeFormat::PcmWav {
            bits_per_sample: 16,
        },
        &samples,
        44_100,
        2,
    )
    .unwrap();

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

    std::thread::sleep(std::time::Duration::from_millis(50));
    buffer.cancel();

    let result = handle.join().expect("decode thread");
    assert!(
        result.is_err(),
        "a cancelled input must fail the decode, not return a truncated Ok"
    );
}

#[test]
fn test_decode_encode_roundtrip() {
    init();

    // 1 second of silence at 44100Hz stereo.
    let original_samples: Vec<i32> = vec![0i32; 44100 * 2];

    let flac_data = encode_i32(
        EncodeFormat::Flac {
            bits_per_sample: 16,
        },
        &original_samples,
        44100,
        2,
    )
    .unwrap();

    assert!(flac_data.len() > 42);
    assert_eq!(&flac_data[0..4], b"fLaC");

    let decoded = decode_audio(buffer_from(&flac_data), None, None).unwrap();

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
fn decode_audio_rejects_truncated_flac_packet_stream() {
    init();

    let flac_data = truncated_flac_packet_stream();

    let err =
        decode_audio(buffer_from(&flac_data), None, None).expect_err("truncated FLAC must fail");
    assert!(
        err.contains("Failed to send packet"),
        "unexpected decode error: {err}"
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

    let mp3_data = encode_i32(
        EncodeFormat::Mp3 { bitrate_kbps: 320 },
        &samples,
        sample_rate,
        2,
    )
    .unwrap();

    // An MP3 opens with either an ID3 tag or a frame sync word.
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

    let decoded = decode_audio(buffer_from(&mp3_data), None, None).unwrap();
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

    let aac_data = encode_i32(
        EncodeFormat::Aac { bitrate_kbps: 256 },
        &samples,
        sample_rate,
        2,
    )
    .unwrap();

    // An MP4 file (the .m4a flavor the ipod muxer writes) opens with an `ftyp`
    // box: a 4-byte size, then the type "ftyp".
    assert!(
        aac_data.len() > 100,
        "AAC data too small: {}",
        aac_data.len()
    );
    assert_eq!(&aac_data[4..8], b"ftyp", "AAC output missing ftyp box");

    let decoded = decode_audio(buffer_from(&aac_data), None, None).unwrap();
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

    let opus_data = encode_i32(
        EncodeFormat::OpusOgg { bitrate_kbps: 192 },
        &samples,
        sample_rate,
        2,
    )
    .unwrap();

    assert!(
        opus_data.len() > 100,
        "Opus/Ogg data too small: {}",
        opus_data.len()
    );
    assert_eq!(&opus_data[0..4], b"OggS");

    let decoded = decode_audio(buffer_from(&opus_data), None, None).unwrap();
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
        let aiff_data = encode_i32(
            EncodeFormat::PcmAiff { bits_per_sample },
            &samples,
            sample_rate,
            2,
        )
        .unwrap();

        assert_eq!(
            &aiff_data[0..4],
            b"FORM",
            "AIFF ({bits_per_sample}-bit) missing FORM chunk"
        );
        assert_eq!(
            &aiff_data[8..12],
            b"AIFF",
            "AIFF ({bits_per_sample}-bit) missing AIFF form type"
        );

        let decoded = decode_audio(buffer_from(&aiff_data), None, None).unwrap();
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

    let wav_data = encode_i32(
        EncodeFormat::PcmWav {
            bits_per_sample: 32,
        },
        &samples,
        sample_rate,
        2,
    )
    .unwrap();

    assert_eq!(&wav_data[0..4], b"RIFF");
    let decoded = decode_audio(buffer_from(&wav_data), None, None).unwrap();
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

    let flac_data = encode_i32(
        EncodeFormat::Flac {
            bits_per_sample: 16,
        },
        &original,
        sample_rate,
        1,
    )
    .unwrap();
    let decoded = decode_audio(buffer_from(&flac_data), None, None).unwrap();

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
        max_diff < 2, // A tiny rounding error is tolerated.
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

    let samples: Vec<i32> = (0..44100)
        .map(|i| ((i as f64 * 0.01).sin() * 0.5 * i32::MAX as f64) as i32)
        .collect();
    let flac_data = encode_i32(
        EncodeFormat::Flac {
            bits_per_sample: 16,
        },
        &samples,
        44100,
        1,
    )
    .unwrap();

    let buffer = create_sparse_buffer(flac_data.len() as u64);
    let (mut sink, mut source, _ready) = create_track_stream_pair_with_capacity(44100, 1, 100000);

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

#[test]
fn streaming_decode_rejects_truncated_flac_packet_stream() {
    use crate::playback::create_track_stream_pair_with_capacity;
    use crate::playback::sparse_buffer::create_sparse_buffer;

    init();

    let flac_data = truncated_flac_packet_stream();

    let buffer = create_sparse_buffer(flac_data.len() as u64);
    buffer.append_at(0, &flac_data);
    let (mut sink, source, _ready) = create_track_stream_pair_with_capacity(44100, 1, 100000);
    let token = Arc::new(std::sync::atomic::AtomicBool::new(false));

    let err = decode_audio_streaming(buffer, &mut sink, None, None, None, None, None, token)
        .expect_err("truncated FLAC must fail");

    match err {
        StreamingDecodeError::Decode(message) => {
            assert!(
                message.contains("Failed to send packet"),
                "unexpected decode error: {message}"
            );
        }
        StreamingDecodeError::InputCancelled => panic!("truncation is not cancellation"),
    }
    assert!(!source.producer_finished());
}
