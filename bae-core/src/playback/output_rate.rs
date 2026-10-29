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
            [*range.start(), *range.end()].into_iter().chain(
                STANDARD_RATES
                    .into_iter()
                    .filter(move |rate| range.contains(rate)),
            )
        })
        .collect();
    let same_family: Vec<u32> = match family(track_rate) {
        Some(track_family) => candidates
            .iter()
            .copied()
            .filter(|rate| family(*rate) == Some(track_family))
            .collect(),
        None => Vec::new(),
    };
    let pool = if same_family.is_empty() {
        candidates
    } else {
        same_family
    };
    let at_or_below = pool
        .iter()
        .copied()
        .filter(|rate| *rate <= track_rate)
        .max();
    at_or_below.or_else(|| pool.iter().copied().filter(|rate| *rate > track_rate).min())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rates of a typical Mac output: built-in speakers, a display.
    fn discrete(rates: &[u32]) -> Vec<RangeInclusive<u32>> {
        rates.iter().map(|rate| *rate..=*rate).collect()
    }

    #[test]
    fn a_rate_the_device_runs_at_is_kept() {
        let device = discrete(&[44_100, 48_000, 88_200, 96_000]);
        assert_eq!(output_rate(44_100, &device), Some(44_100));
        assert_eq!(output_rate(96_000, &device), Some(96_000));
    }

    #[test]
    fn a_higher_rate_converts_down_within_its_family() {
        let device = discrete(&[44_100, 48_000, 88_200, 96_000]);
        // 176.4 kHz goes to 88.2 kHz (2:1), not to the higher 96 kHz.
        assert_eq!(output_rate(176_400, &device), Some(88_200));
        // 192 kHz goes to 96 kHz (2:1), not to 88.2 kHz.
        assert_eq!(output_rate(192_000, &device), Some(96_000));
        // 352.8 kHz goes to the family's highest rate the device has.
        assert_eq!(output_rate(352_800, &device), Some(88_200));
    }

    #[test]
    fn a_lower_rate_converts_up_to_the_lowest_of_its_family() {
        let device = discrete(&[44_100, 48_000, 88_200, 96_000]);
        assert_eq!(output_rate(22_050, &device), Some(44_100));
        assert_eq!(output_rate(32_000, &device), Some(48_000));
        assert_eq!(output_rate(16_000, &device), Some(48_000));
    }

    /// The family comes before the direction: a 44.1 kHz track on a device
    /// with 32 kHz and 88.2 kHz goes up 1:2 to 88.2 kHz rather than down to
    /// 32 kHz in the other family.
    #[test]
    fn the_family_comes_before_converting_down() {
        let device = discrete(&[32_000, 88_200]);
        assert_eq!(output_rate(44_100, &device), Some(88_200));
    }

    #[test]
    fn a_device_without_the_tracks_family_converts_across() {
        // A 48 kHz-only device, as some USB and HDMI outputs are.
        let device = discrete(&[48_000]);
        assert_eq!(output_rate(44_100, &device), Some(48_000));
        assert_eq!(output_rate(176_400, &device), Some(48_000));
        // Of two rates in the other family, the highest at or below wins.
        let device = discrete(&[48_000, 96_000]);
        assert_eq!(output_rate(88_200, &device), Some(48_000));
    }

    #[test]
    fn a_rate_of_no_family_uses_every_rate() {
        let device = discrete(&[44_100, 48_000]);
        assert_eq!(output_rate(47_250, &device), Some(44_100));
        assert_eq!(output_rate(50_400, &device), Some(48_000));
        assert_eq!(output_rate(37_800, &device), Some(44_100));
    }

    #[test]
    fn a_continuous_range_offers_its_standard_rates() {
        // The track's rate inside the range is kept.
        let device = vec![8_000..=96_000];
        assert_eq!(output_rate(37_800, &device), Some(37_800));
        // Above the range: the family's highest standard rate inside it.
        assert_eq!(output_rate(176_400, &device), Some(88_200));
        assert_eq!(output_rate(192_000, &device), Some(96_000));
    }

    #[test]
    fn a_device_with_no_rates_has_no_output_rate() {
        assert_eq!(output_rate(44_100, &[]), None);
    }
}
