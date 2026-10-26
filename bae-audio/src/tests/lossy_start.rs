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
        let whole = decode_audio(buffer_from(&bytes), None, None).expect("decode from the start");
        let channels = whole.channels as usize;
        for target in lossy_targets(whole.sample_rate) {
            let from = decode_audio(buffer_from(&bytes), Some(target), None)
                .expect("decode from the target");
            let expected = &whole.samples[target as usize * channels..];
            assert_eq!(
                from.samples.len(),
                expected.len(),
                "{name} from {target}: sample count"
            );
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

    let buffer = buffer_from(bytes);
    let (mut sink, mut source, _ready) =
        create_track_stream_pair_with_capacity(sample_rate, 2, 4 * sample_rate as usize);
    let decoder = std::thread::spawn(move || {
        let result = decode_audio_streaming(
            buffer,
            &mut sink,
            None,
            start,
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
    decoder
        .join()
        .expect("the decoder thread")
        .expect("the streaming decode");
    played
}

/// Playback's streaming decode, started at sample N, plays from sample N: as
/// many samples as the stream holds from N on, placed where a decode from the
/// start places them. Unlike a sink decode it seeks, so a lossy decoder starts
/// on a pre-roll before N rather than at the stream's start, and its samples
/// converge on the whole decode's within the pre-roll (AAC's, Opus's) or
/// within a frame or two of N (an MP3's bit reservoir).
///
/// An MP3 seek far into the file lands where FFmpeg's fast MP3 seek estimates
/// from the byte position, so it is checked only within the first seconds.
#[test]
fn a_lossy_streaming_decode_from_a_sample_plays_from_it() {
    init();
    for (name, format) in lossy_formats() {
        let bytes = lossy_sweep(format);
        let sample_rate = decode_audio(buffer_from(&bytes), Some(0), Some(1))
            .expect("probe the rate")
            .sample_rate;
        let whole = stream_lossy(&bytes, sample_rate, None);
        for target in lossy_targets(sample_rate) {
            if name == "MP3" && target > u64::from(sample_rate) {
                continue;
            }
            let from = stream_lossy(&bytes, sample_rate, Some(target));
            let expected = &whole[target as usize * 2..];
            assert_eq!(
                from.len(),
                expected.len(),
                "{name} from {target}: sample count"
            );
        }
        // A start whose pre-roll reaches the stream's start decodes as the
        // whole decode does.
        let from = stream_lossy(&bytes, sample_rate, Some(1));
        assert!(
            from == whole[2..],
            "{name} from 1: the samples differ from the whole decode's"
        );
    }
}
