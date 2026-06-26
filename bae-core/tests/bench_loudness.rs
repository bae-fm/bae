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
        m.1.map(|x| x.loudness_lufs),
        frames as f64 / decoded.sample_rate as f64 / measure.as_secs_f64(),
    );

    eprintln!(
        "TOTAL per 7-min track: {:?}  ({:.1}x realtime); a 10-track album ~= {:?}",
        decode + measure,
        frames as f64 / decoded.sample_rate as f64 / (decode + measure).as_secs_f64(),
        (decode + measure) * 10,
    );
}
