//! EBU R128 loudness + true-peak measurement at import.
//!
//! Each track's PCM is measured once, yielding two intrinsic facts: integrated
//! loudness (LUFS) and true peak (a linear ratio, 1.0 = 0 dBTP —
//! `EbuR128::true_peak` already returns it linear, so it is stored verbatim).
//! Those measurements are stored per track and combined per album; the playback
//! gain is derived from them against a constant target at play time, never
//! stored. The per-track meters are kept because the album loudness is combined
//! across them (`EbuR128::loudness_global_multiple`, order-independent).

use ebur128::{EbuR128, Mode};
use tracing::warn;

/// A track too quiet to derive a gain from. EBU R128 integrated loudness for
/// silence is `-inf`; without a floor, the derived boost (`target - loudness`)
/// runs away. A track at or below this measures as "no usable loudness" and
/// plays at unity gain. -70 LUFS is the EBU R128 absolute gating threshold —
/// content below it contributes nothing to integrated loudness anyway.
const SILENCE_FLOOR_LUFS: f64 = -70.0;

/// One track's loudness measurement: integrated loudness in LUFS and true peak
/// as a linear ratio (1.0 = 0 dBTP), already the max across channels.
pub struct TrackLoudness {
    pub loudness_lufs: f64,
    pub peak_linear: f64,
}

/// A streaming EBU R128 meter: feed a track's PCM in any number of chunks, then
/// `finish` to read its loudness + true peak. `ebur128` carries its filter state
/// across `add_frames` calls, so chunked feeding measures exactly the same value
/// as one call — which is what lets the import stream each decode chunk straight
/// in instead of buffering the whole track's PCM first.
pub struct LoudnessMeter {
    meter: EbuR128,
    channels: u32,
}

impl LoudnessMeter {
    /// Chunks are interleaved full-range i32 PCM, matching what
    /// `ebur128::EbuR128::add_frames_i32` and `true_peak` expect.
    pub fn new(channels: u32, sample_rate: u32) -> Result<Self, String> {
        let meter = EbuR128::new(channels, sample_rate, Mode::I | Mode::TRUE_PEAK)
            .map_err(|e| format!("ebur128 init failed: {e:?}"))?;
        Ok(Self { meter, channels })
    }

    /// Feed one interleaved-i32 chunk (frame-aligned: `len` a multiple of
    /// `channels`).
    pub fn add_chunk(&mut self, samples: &[i32]) -> Result<(), String> {
        self.meter
            .add_frames_i32(samples)
            .map_err(|e| format!("ebur128 add_frames failed: {e:?}"))
    }

    /// Read the track's integrated loudness + true peak, returning the measurement
    /// alongside the meter the album combine reuses. `Ok(None)` for a track with
    /// no usable loudness (silent / near-silent or a non-finite reading) — the
    /// caller stores NULL and playback applies unity gain. `Err` is a measurement
    /// failure (the caller logs it and stores NULL).
    pub fn finish(self) -> Result<(EbuR128, Option<TrackLoudness>), String> {
        let loudness_lufs = self
            .meter
            .loudness_global()
            .map_err(|e| format!("ebur128 loudness_global failed: {e:?}"))?;

        if !loudness_lufs.is_finite() || loudness_lufs < SILENCE_FLOOR_LUFS {
            return Ok((self.meter, None));
        }

        let peak_linear = max_true_peak(&self.meter, self.channels)?;
        Ok((
            self.meter,
            Some(TrackLoudness {
                loudness_lufs,
                peak_linear,
            }),
        ))
    }
}

/// The track's true peak: the max across its channels.
fn max_true_peak(meter: &EbuR128, channels: u32) -> Result<f64, String> {
    let mut peak = 0.0f64;
    for ch in 0..channels {
        let p = meter
            .true_peak(ch)
            .map_err(|e| format!("ebur128 true_peak({ch}) failed: {e:?}"))?;
        if p > peak {
            peak = p;
        }
    }
    Ok(peak)
}

/// Album integrated loudness combined across the tracks' meters, in LUFS.
/// Order-independent (`loudness_global_multiple`). `None` when there are no
/// meters or the combined loudness is non-finite / below the silence floor (an
/// all-silent album), so the album falls back to unity gain.
pub fn album_loudness(meters: &[EbuR128]) -> Option<f64> {
    if meters.is_empty() {
        return None;
    }
    match EbuR128::loudness_global_multiple(meters.iter()) {
        Ok(lufs) if lufs.is_finite() && lufs >= SILENCE_FLOOR_LUFS => Some(lufs),
        Ok(_) => None,
        Err(e) => {
            warn!("ebur128 loudness_global_multiple failed: {e:?}; album loudness unmeasured");
            None
        }
    }
}

/// Album true peak: the max of the per-track linear peaks. `None` when no track
/// had a usable measurement.
pub fn album_peak(track_peaks: &[f64]) -> Option<f64> {
    track_peaks
        .iter()
        .copied()
        .fold(None, |acc, p| Some(acc.map_or(p, |a: f64| a.max(p))))
}

#[cfg(test)]
mod tests {
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
        assert!(
            m.is_none(),
            "silence must measure as no usable loudness (unity gain at playback)"
        );
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
}
