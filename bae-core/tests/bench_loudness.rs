//! Scratch benchmark: where does the import loudness pass spend its time?
//! Run against a real file (path via env, never committed):
//!   BENCH_FLAC="/path/to/file.flac" cargo test -p bae-core --test bench_loudness -- --nocapture --ignored

use std::time::Instant;

#[test]
#[ignore]
fn bench_loudness() {
    let Ok(path) = std::env::var("BENCH_FLAC") else {
        eprintln!("set BENCH_FLAC=/path/to/file.flac");
        return;
    };
    bae_core::audio_codec::init();

    let t = Instant::now();
    let bytes = std::fs::read(&path).unwrap();
    eprintln!(
        "read {:.1} MB in {:?}",
        bytes.len() as f64 / 1e6,
        t.elapsed()
    );

    // Decode the whole file (these Downloads tracks are standalone, 2-4 min).
    let t = Instant::now();
    let decoded = bae_core::audio_codec::decode_audio(&bytes, None, None).unwrap();
    let decode = t.elapsed();
    let frames = decoded.samples.len() / decoded.channels.max(1) as usize;
    eprintln!(
        "decode_audio(whole file): {decode:?}  -> {} samples ({} frames), {}Hz {}ch {}bit  | {:.1}x realtime",
        decoded.samples.len(),
        frames,
        decoded.sample_rate,
        decoded.channels,
        decoded.bits_per_sample,
        frames as f64 / decoded.sample_rate as f64 / decode.as_secs_f64(),
    );

    let t = Instant::now();
    let m = bae_core::loudness::measure_track(
        &decoded.samples,
        decoded.channels,
        decoded.sample_rate,
        decoded.bits_per_sample,
    )
    .unwrap();
    let measure = t.elapsed();
    eprintln!(
        "measure_track: {measure:?}  -> {:?} LUFS  | {:.1}x realtime",
        m.1.as_ref().map(|x| x.loudness_lufs),
        frames as f64 / decoded.sample_rate as f64 / measure.as_secs_f64(),
    );

    eprintln!(
        "TOTAL per 7-min track: {:?}  ({:.1}x realtime); a 10-track album ~= {:?}",
        decode + measure,
        frames as f64 / decoded.sample_rate as f64 / (decode + measure).as_secs_f64(),
        (decode + measure) * 10,
    );

    // The import path: decode straight into the meter (no buffered PCM Vec), so
    // measurement runs interleaved with the decode. The LUFS must equal the
    // separate decode-then-measure above exactly.
    struct StreamingSink {
        meter: Option<bae_core::loudness::LoudnessMeter>,
        source_bits: u32,
    }
    impl bae_core::audio_codec::DecodedSink for StreamingSink {
        fn on_format(&mut self, sample_rate: u32, channels: u32, bits_per_sample: u32) {
            let bits = if self.source_bits > 0 {
                self.source_bits
            } else {
                bits_per_sample
            };
            self.meter =
                Some(bae_core::loudness::LoudnessMeter::new(channels, sample_rate, bits).unwrap());
        }
        fn on_samples(&mut self, samples: &[i32]) {
            self.meter.as_mut().unwrap().add_chunk(samples).unwrap();
        }
    }

    let t = Instant::now();
    let mut sink = StreamingSink {
        meter: None,
        source_bits: decoded.bits_per_sample,
    };
    bae_core::audio_codec::decode_audio_to_sink(&bytes, None, None, &mut sink).unwrap();
    let streamed = sink.meter.unwrap().finish().unwrap();
    let interleaved = t.elapsed();
    eprintln!(
        "STREAMED decode+measure (interleaved): {interleaved:?}  -> {:?} LUFS  | {:.1}x realtime",
        streamed.1.as_ref().map(|x| x.loudness_lufs),
        frames as f64 / decoded.sample_rate as f64 / interleaved.as_secs_f64(),
    );
    assert_eq!(
        m.1.as_ref().map(|x| x.loudness_lufs),
        streamed.1.as_ref().map(|x| x.loudness_lufs),
        "streamed loudness must equal one-shot exactly"
    );
    assert_eq!(
        m.1.as_ref().map(|x| x.peak_linear),
        streamed.1.as_ref().map(|x| x.peak_linear),
        "streamed true peak must equal one-shot exactly"
    );
}
