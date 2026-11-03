//! The rate a Mac's output stream runs at for a track.
//!
//! cpal sets the macOS output device to the rate a stream is built at, so a
//! stream at the track's own rate plays the track's samples unconverted. A
//! device that can't run at the track's rate gets a stream at one it can, and
//! bae converts the track to it (`ResamplingDrain`).

use std::ops::RangeInclusive;

/// The rates a continuous range offers besides its two ends. Most devices list
/// discrete rates; an aggregate or virtual device can list a range instead.
const STANDARD_RATES: [u32; 14] = [
    8_000, 11_025, 16_000, 22_050, 24_000, 32_000, 44_100, 48_000, 88_200, 96_000, 176_400,
    192_000, 352_800, 384_000,
];

/// Which of the two families of standard rates a rate belongs to: whole
/// multiples of 11 025 Hz (44.1 kHz and its multiples and halves), or of
/// 8 000 Hz (48 kHz and its multiples and halves, 32 kHz).
#[derive(Clone, Copy, PartialEq, Eq)]
enum RateFamily {
    Cd,
    Video,
}

fn family(rate: u32) -> Option<RateFamily> {
    if rate.is_multiple_of(11_025) {
        Some(RateFamily::Cd)
    } else if rate.is_multiple_of(8_000) {
        Some(RateFamily::Video)
    } else {
        None
    }
}

/// The rate to build the output stream at for a track at `track_rate`, on a
/// device that runs at the rates in `device_rates`: the track's own rate when
/// the device runs at it, else the one bae converts the track to. `None` when
/// the device lists no rate at all.
///
/// The rate converted to is chosen in this order:
///
/// 1. **The track's own family.** A rate in the same family is a ratio of
///    small whole numbers away (176.4 kHz to 88.2 kHz is exactly 2:1), the
///    simplest and most exact conversion; between families the ratio is
///    147:160 or a multiple of it.
/// 2. **Within it, the highest rate at or below the track's.** Converting down
///    to the device's highest rate keeps as much of the recording's bandwidth
///    as the device can carry, and never runs the device faster than the
///    material it plays.
/// 3. **Else the lowest rate above the track's.** When every rate the device
///    runs at is higher, converting up loses nothing of the track, and the
///    lowest of them is the smallest change.
///
/// Only when the device runs at no rate of the track's family do rates of the
/// other family (or of none) count, by the same at-or-below, else-lowest-above
/// order.
pub(crate) fn output_rate(track_rate: u32, device_rates: &[RangeInclusive<u32>]) -> Option<u32> {
    if device_rates.iter().any(|range| range.contains(&track_rate)) {
        return Some(track_rate);
    }
    let candidates: Vec<u32> = device_rates
        .iter()
        .flat_map(|range| {
            [*range.start(), *range.end()]
                .into_iter()
                .chain(STANDARD_RATES.into_iter().filter(move |rate| range.contains(rate)))
        })
        .collect();
    let same_family: Vec<u32> = match family(track_rate) {
        Some(track_family) => {
            candidates.iter().copied().filter(|rate| family(*rate) == Some(track_family)).collect()
        }
        None => Vec::new(),
    };
    let pool = if same_family.is_empty() { candidates } else { same_family };
    let at_or_below = pool.iter().copied().filter(|rate| *rate <= track_rate).max();
    at_or_below.or_else(|| pool.iter().copied().filter(|rate| *rate > track_rate).min())
}

#[cfg(test)]
#[path = "output_rate_tests.rs"]
mod tests;
