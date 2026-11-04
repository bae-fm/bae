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
        self.meter.add_frames_i32(samples).map_err(|e| format!("ebur128 add_frames failed: {e:?}"))
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
        Ok((self.meter, Some(TrackLoudness { loudness_lufs, peak_linear })))
    }
}

/// The track's true peak: the max across its channels.
fn max_true_peak(meter: &EbuR128, channels: u32) -> Result<f64, String> {
    let mut peak = 0.0f64;
    for ch in 0..channels {
        let p =
            meter.true_peak(ch).map_err(|e| format!("ebur128 true_peak({ch}) failed: {e:?}"))?;
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
    track_peaks.iter().copied().fold(None, |acc, p| Some(acc.map_or(p, |a: f64| a.max(p))))
}

#[cfg(test)]
#[path = "meter_tests.rs"]
mod tests;
