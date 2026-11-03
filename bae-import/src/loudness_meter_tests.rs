use super::*;
use std::f64::consts::PI;

/// Interleaved i32 stereo sine at `amplitude` (0.0..=1.0 of full scale).
fn sine(amplitude: f64, freq_hz: f64, sample_rate: u32, secs: f64) -> Vec<i32> {
    let n = (sample_rate as f64 * secs) as usize;
    let scale = amplitude * i32::MAX as f64;
    let mut out = Vec::with_capacity(n * 2);
    for i in 0..n {
        let t = i as f64 / sample_rate as f64;
        let v = (2.0 * PI * freq_hz * t).sin() * scale;
        let s = v as i32;
        out.push(s);
        out.push(s);
    }
    out
}

/// One-shot measurement over [`LoudnessMeter`]: feed all samples in a single
/// chunk and finish. Production streams chunks in instead; this is what the
/// chunked-vs-one-shot test compares against.
fn measure_track(
    samples: &[i32],
    channels: u32,
    sample_rate: u32,
) -> Result<(EbuR128, Option<TrackLoudness>), String> {
    let mut meter = LoudnessMeter::new(channels, sample_rate)?;
    meter.add_chunk(samples)?;
    meter.finish()
}

#[test]
fn louder_signal_measures_higher_lufs() {
    let sr = 48_000;
    let (_, quiet) = measure_track(&sine(0.1, 1_000.0, sr, 3.0), 2, sr).unwrap();
    let (_, loud) = measure_track(&sine(0.5, 1_000.0, sr, 3.0), 2, sr).unwrap();
    let quiet = quiet.expect("quiet tone has usable loudness");
    let loud = loud.expect("loud tone has usable loudness");
    // A 5x amplitude increase is ~14 dB louder; assert a clear ordering with
    // margin rather than an exact figure (the algorithm is version-defined).
    assert!(
        loud.loudness_lufs > quiet.loudness_lufs + 10.0,
        "loud {} should exceed quiet {} by >10 LUFS",
        loud.loudness_lufs,
        quiet.loudness_lufs
    );
}

#[test]
fn chunked_measurement_equals_one_shot() {
    // The guarantee the streaming import rests on: feeding the meter in pieces
    // measures identically to one shot.
    let sr = 48_000;
    let pcm = sine(0.4, 1_000.0, sr, 5.0);

    let (_, one_shot) = measure_track(&pcm, 2, sr).unwrap();
    let one_shot = one_shot.expect("tone has usable loudness");

    let mut meter = LoudnessMeter::new(2, sr).unwrap();
    // Uneven, frame-aligned chunks (a multiple of the 2 channels), nothing
    // like the one-shot's single call.
    for chunk in pcm.chunks(7_001 * 2) {
        meter.add_chunk(chunk).unwrap();
    }
    let (_, chunked) = meter.finish().unwrap();
    let chunked = chunked.expect("tone has usable loudness");

    assert_eq!(
        one_shot.loudness_lufs, chunked.loudness_lufs,
        "chunked loudness must equal one-shot exactly"
    );
    assert_eq!(
        one_shot.peak_linear, chunked.peak_linear,
        "chunked true peak must equal one-shot exactly"
    );
}

#[test]
fn near_full_scale_peak_is_near_unity() {
    let sr = 48_000;
    let (_, m) = measure_track(&sine(0.99, 1_000.0, sr, 2.0), 2, sr).unwrap();
    let m = m.expect("usable loudness");
    // A 0.99-full-scale sine peaks just under 1.0 linear (true-peak
    // interpolation can nudge it slightly past the sample peak).
    assert!(
        m.peak_linear > 0.9 && m.peak_linear < 1.1,
        "peak {} should be near 1.0 for a near-full-scale sine",
        m.peak_linear
    );
}

#[test]
fn silence_has_no_usable_loudness() {
    let sr = 48_000;
    let silent = vec![0i32; sr as usize * 2 * 2];
    let (_, m) = measure_track(&silent, 2, sr).unwrap();
    assert!(m.is_none(), "silence must measure as no usable loudness (unity gain at playback)");
}

#[test]
fn album_loudness_combines_meters_order_independently() {
    let sr = 48_000;
    let (m1, _) = measure_track(&sine(0.1, 1_000.0, sr, 3.0), 2, sr).unwrap();
    let (m2, _) = measure_track(&sine(0.5, 1_000.0, sr, 3.0), 2, sr).unwrap();

    let forward = album_loudness(&[m1, m2]).expect("usable album loudness");

    let (m1b, _) = measure_track(&sine(0.1, 1_000.0, sr, 3.0), 2, sr).unwrap();
    let (m2b, _) = measure_track(&sine(0.5, 1_000.0, sr, 3.0), 2, sr).unwrap();
    let reversed = album_loudness(&[m2b, m1b]).expect("usable album loudness");

    assert!(
        (forward - reversed).abs() < 1e-6,
        "album loudness must be order-independent: {forward} vs {reversed}"
    );
}

#[test]
fn album_peak_is_the_max() {
    assert_eq!(album_peak(&[]), None);
    assert_eq!(album_peak(&[0.3, 0.9, 0.5]), Some(0.9));
}

/// Measuring a full-length track stays well under a second. ebur128's
/// pure-Rust true-peak interpolator is ~80x slower unoptimized, so the
/// `[profile.dev.package.ebur128] opt-level = 3` override in the workspace
/// Cargo.toml keeps it optimized even in dev/test builds; this test guards
/// that — an unoptimized build measures a 4-minute track in tens of seconds.
#[test]
fn measure_track_stays_fast_on_a_full_length_track() {
    let sr = 48_000u32;
    let frames = sr as usize * 240; // ~4 minutes, stereo
    let samples: Vec<i32> = (0..frames)
        .flat_map(|i| {
            let t = i as f64 / sr as f64;
            let v = ((2.0 * PI * 1_000.0 * t).sin() * 0.5 * i32::MAX as f64) as i32;
            [v, v]
        })
        .collect();

    let start = std::time::Instant::now();
    let (_, measured) = measure_track(&samples, 2, sr).unwrap();
    let elapsed = start.elapsed();

    assert!(measured.is_some(), "the 1kHz tone has a usable loudness");
    assert!(
        elapsed.as_secs_f64() < 8.0,
        "measuring a 4-min track took {elapsed:?}; ebur128 must be optimized \
         in dev builds (see [profile.dev.package.ebur128] in Cargo.toml)"
    );
}
