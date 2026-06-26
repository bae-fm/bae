//! EBU R128 loudness + true-peak measurement at import.
//!
//! Each track's PCM is measured once, producing two intrinsic facts: integrated
//! loudness (LUFS) and true peak (a linear ratio, 1.0 = 0 dBTP). Those raw
//! measurements are stored per track and combined per album; the playback gain
//! is derived from them against a constant target at play time, never stored.
//!
//! `ebur128::EbuR128::true_peak` already returns a linear ratio, so it is stored
//! verbatim — no dBTP→linear conversion. The per-track meters are kept so the
//! album loudness can be combined across them
//! (`EbuR128::loudness_global_multiple` is order-independent).

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
/// `finish` to read its loudness + true peak. `ebur128` carries its loudness and
/// true-peak filter state across `add_frames` calls, so chunked feeding measures
/// exactly the same value as one call — the import streams each decode chunk
/// straight in (filling a determinate bar continuously through the decode)
/// instead of buffering the whole track's PCM first.
pub struct LoudnessMeter {
    meter: EbuR128,
    shift: u32,
    channels: u32,
}

impl LoudnessMeter {
    /// `sample_bits` is the source's value range (16 for 16-bit FLAC, 24 for
    /// 24-bit, 32 for 32-bit/float) — the chunks fed in are interleaved i32 in
    /// that range, NOT pre-scaled to full i32. The meter left-shifts them so full
    /// scale maps to `i32::MAX`, which `ebur128`'s `add_frames_i32` and
    /// `true_peak` assume. The effective width is floored at 16 bits because
    /// `read_sample` scales an 8-bit source up into the 16-bit range while still
    /// probing it as 8-bit; trusting the declared 8 would over-shift by 8 bits and
    /// saturate every loud sample. Skipping the shift entirely makes a 16-bit
    /// track read ~96 dB too quiet (every track measures as silent) — the loudness
    /// equivalent of a unit error.
    pub fn new(channels: u32, sample_rate: u32, sample_bits: u32) -> Result<Self, String> {
        let effective_bits = sample_bits.max(16);
        let shift = 32u32.saturating_sub(effective_bits);
        let meter = EbuR128::new(channels, sample_rate, Mode::I | Mode::TRUE_PEAK)
            .map_err(|e| format!("ebur128 init failed: {e:?}"))?;
        Ok(Self {
            meter,
            shift,
            channels,
        })
    }

    /// Feed one interleaved-i32 chunk (frame-aligned: `len` a multiple of
    /// `channels`). Scaling to full i32 (which `add_frames_i32` assumes) happens
    /// here; a full-width source (shift 0) passes through untouched.
    pub fn add_chunk(&mut self, samples: &[i32]) -> Result<(), String> {
        if self.shift == 0 {
            self.meter.add_frames_i32(samples)
        } else {
            // Saturating shift: a decoded sample never fills its nominal width to
            // the sign-bit edge, so this won't overflow in practice, but
            // `wrapping_shl` would silently corrupt a max-magnitude sample.
            let scaled: Vec<i32> = samples
                .iter()
                .map(|&s| s.saturating_mul(1 << self.shift))
                .collect();
            self.meter.add_frames_i32(&scaled)
        }
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

/// Measure one track's PCM in a single shot. `samples` is interleaved i32 as
/// `decode_audio` returns it — see [`LoudnessMeter::new`] for the `sample_bits`
/// scaling. Returns the measurement alongside the meter the album combine reuses;
/// the import path streams via [`LoudnessMeter`] directly instead.
pub fn measure_track(
    samples: &[i32],
    channels: u32,
    sample_rate: u32,
    sample_bits: u32,
) -> Result<(EbuR128, Option<TrackLoudness>), String> {
    let mut meter = LoudnessMeter::new(channels, sample_rate, sample_bits)?;
    meter.add_chunk(samples)?;
    meter.finish()
}

/// The track's true peak: the max linear true-peak across its channels. The
/// crate already returns a linear ratio (1.0 = 0 dBTP), so it is stored as-is.
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

    #[test]
    fn louder_signal_measures_higher_lufs() {
        let sr = 48_000;
        let (_, quiet) = measure_track(&sine(0.1, 1_000.0, sr, 3.0), 2, sr, 32).unwrap();
        let (_, loud) = measure_track(&sine(0.5, 1_000.0, sr, 3.0), 2, sr, 32).unwrap();
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
        // Feeding the meter in pieces must measure byte-identically to one shot —
        // `ebur128` carries its filter state across `add_frames`. This is the
        // guarantee the import relies on to stream each decode chunk straight in.
        let sr = 48_000;
        let pcm = sine(0.4, 1_000.0, sr, 5.0);

        let (_, one_shot) = measure_track(&pcm, 2, sr, 16).unwrap();
        let one_shot = one_shot.expect("tone has usable loudness");

        let mut meter = LoudnessMeter::new(2, sr, 16).unwrap();
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
        let (_, m) = measure_track(&sine(0.99, 1_000.0, sr, 2.0), 2, sr, 32).unwrap();
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
        let (_, m) = measure_track(&silent, 2, sr, 32).unwrap();
        assert!(
            m.is_none(),
            "silence must measure as no usable loudness (unity gain at playback)"
        );
    }

    #[test]
    fn album_loudness_combines_meters_order_independently() {
        let sr = 48_000;
        let (m1, _) = measure_track(&sine(0.1, 1_000.0, sr, 3.0), 2, sr, 32).unwrap();
        let (m2, _) = measure_track(&sine(0.5, 1_000.0, sr, 3.0), 2, sr, 32).unwrap();

        let forward = album_loudness(&[m1, m2]).expect("usable album loudness");

        let (m1b, _) = measure_track(&sine(0.1, 1_000.0, sr, 3.0), 2, sr, 32).unwrap();
        let (m2b, _) = measure_track(&sine(0.5, 1_000.0, sr, 3.0), 2, sr, 32).unwrap();
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

    #[test]
    fn declared_8_bit_is_floored_to_16_not_over_shifted() {
        // `read_sample` upscales an 8-bit source into the 16-bit value range, so
        // the decoded i32 samples are 16-bit-range even when the probe declares
        // 8 bits. Measuring at the declared 8 must floor the shift to 16, not
        // over-shift to 24 and saturate every loud sample — the result must
        // match measuring the same samples as 16-bit.
        let sr = 48_000;
        let n = sr as usize * 3;
        let sixteen_bit_range: Vec<i32> = (0..n)
            .flat_map(|i| {
                let t = i as f64 / sr as f64;
                let v = ((2.0 * PI * 1_000.0 * t).sin() * 0.5 * i16::MAX as f64) as i32;
                [v, v]
            })
            .collect();
        let (_, as_eight) = measure_track(&sixteen_bit_range, 2, sr, 8).unwrap();
        let (_, as_sixteen) = measure_track(&sixteen_bit_range, 2, sr, 16).unwrap();
        let as_eight = as_eight.expect("declared-8 source measures a usable loudness");
        let as_sixteen = as_sixteen.expect("16-bit source measures a usable loudness");
        assert!(
            (as_eight.loudness_lufs - as_sixteen.loudness_lufs).abs() < 1e-6,
            "declared-8 loudness {} must equal 16-bit {} (floored, not over-shifted)",
            as_eight.loudness_lufs,
            as_sixteen.loudness_lufs
        );
    }

    /// Measuring a full-length track must stay fast. ebur128's pure-Rust
    /// true-peak interpolator runs ~80x slower unoptimized, so an unoptimized
    /// dev build made a whole-album loudness pass take minutes. The
    /// `[profile.dev.package.ebur128] opt-level = 3` override in the workspace
    /// Cargo.toml keeps the dependency optimized even in dev/test builds; this
    /// guards it — without the override this is ~20s, with it well under a
    /// second.
    #[test]
    fn measure_track_stays_fast_on_a_full_length_track() {
        let sr = 48_000u32;
        let frames = sr as usize * 240; // ~4 minutes, stereo
        let samples: Vec<i32> = (0..frames)
            .flat_map(|i| {
                let t = i as f64 / sr as f64;
                let v = ((2.0 * PI * 1_000.0 * t).sin() * 0.5 * i16::MAX as f64) as i32;
                [v, v]
            })
            .collect();

        let start = std::time::Instant::now();
        let (_, measured) = measure_track(&samples, 2, sr, 16).unwrap();
        let elapsed = start.elapsed();

        assert!(measured.is_some(), "the 1kHz tone has a usable loudness");
        assert!(
            elapsed.as_secs_f64() < 8.0,
            "measuring a 4-min track took {elapsed:?}; ebur128 must be optimized \
             in dev builds (see [profile.dev.package.ebur128] in Cargo.toml)"
        );
    }
}
