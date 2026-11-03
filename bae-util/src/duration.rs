//! How a duration reads: the one place that decides it, for every surface.
//!
//! Two shapes, and they are not interchangeable. A [`DurationClock`] *counts*
//! time — "3:07", "1:12:34", "-0:42" — and is what a track's length, the elapsed
//! position, and the remaining countdown are. [`DurationUnits`] *names an
//! amount* of it — "39 min", "3 hr, 42 min" — and is what a release's total
//! playing time is.
//!
//! Core owns every decision either shape embodies: which fields appear, what an
//! absent or negative duration means, whether seconds floor or minutes round,
//! how a countdown behaves at the end of a track. The UI owns only the rendering
//! — the digits of a clock, the catalog words of a units label — because the
//! locale never crosses the bridge: an Arabic-Indic locale renders "٣:٠٧" from
//! the same clock an "en" locale renders "3:07" from.

/// A duration split into the fields a clock label shows.
///
/// `minutes` and `seconds` are always in `0..=59`; `hours` carries whatever
/// remains, so the fields reconstruct the duration exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DurationClock {
    /// The label reads as a countdown and carries a leading minus.
    pub negative: bool,
    /// Set from one hour up, and only then: below an hour the label is "M:SS",
    /// not "0:03:07".
    pub hours: Option<u64>,
    pub minutes: u32,
    pub seconds: u32,
}

impl DurationClock {
    /// The clock for a duration, or `None` when there is no label to show.
    ///
    /// A negative duration is not a short duration — it is a gap in the data,
    /// and reads the same as an absent one: no label at all, rather than a
    /// "0:00" that claims a length nothing knows.
    pub fn from_millis(ms: Option<i64>) -> Option<Self> {
        let ms = u64::try_from(ms?).ok()?;
        Some(Self::of_length(ms))
    }

    /// The clock for a length every track has ("3:07"): a stored or measured
    /// length always labels.
    pub fn of_length(ms: u64) -> Self {
        Self::from_seconds(ms / 1000, false)
    }

    /// A track-relative playback position. Negative positions count down to
    /// INDEX 01 and use ceiling so `-0:01` remains visible until the boundary;
    /// non-negative positions use the ordinary elapsed-time floor.
    fn playback_position(position_ms: i64, duration_ms: Option<u64>) -> Self {
        if position_ms < 0 {
            return Self::from_seconds(position_ms.unsigned_abs().div_ceil(1_000), true);
        }
        let position_ms = position_ms as u64;
        let position_ms = duration_ms.map_or(position_ms, |duration| position_ms.min(duration));
        Self::from_seconds(position_ms / 1_000, false)
    }

    /// The countdown from a position within a duration ("-1:23").
    ///
    /// A position past the duration is a real state — a track whose stored
    /// length undershoots the audio the decoder actually produces — so the
    /// countdown stops at "-0:00" instead of counting back up.
    pub fn remaining(position_ms: i64, duration_ms: u64) -> Self {
        let elapsed_ms = if position_ms < 0 { 0 } else { position_ms as u64 };
        Self::from_seconds(duration_ms.saturating_sub(elapsed_ms) / 1000, true)
    }

    /// Whole seconds floor: a track is "3:07" for the whole of its 3:07.x.
    fn from_seconds(seconds: u64, negative: bool) -> Self {
        let hours = seconds / 3600;
        Self {
            negative,
            hours: (hours > 0).then_some(hours),
            minutes: ((seconds % 3600) / 60) as u32,
            seconds: (seconds % 60) as u32,
        }
    }
}

/// The two clocks a seek bar shows.
///
/// The leading label is the pregap countdown while position is negative. From
/// track start onward it is the elapsed position or the countdown to the end,
/// whichever the user asked for (`show_remaining_time` in the config) — the
/// elapsed position alone when the track's length is unknown. The trailing
/// label is the track's total length, absent when it is unknown. The UI renders
/// the two and owns none of these choices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeekBarClocks {
    pub leading: DurationClock,
    /// `None` when the track's length is not known — there is no total to show.
    pub trailing: Option<DurationClock>,
}

impl SeekBarClocks {
    /// A `duration_ms` of `None` is a track whose length is not known: there is
    /// nothing to count down to and no total to name, so the leading label
    /// shows the elapsed position whatever the preference says.
    pub fn new(position_ms: i64, duration_ms: Option<u64>, show_remaining: bool) -> Self {
        let Some(duration_ms) = duration_ms else {
            return Self {
                leading: DurationClock::playback_position(position_ms, None),
                trailing: None,
            };
        };
        let leading = if position_ms < 0 {
            DurationClock::playback_position(position_ms, Some(duration_ms))
        } else if show_remaining {
            DurationClock::remaining(position_ms, duration_ms)
        } else {
            DurationClock::playback_position(position_ms, Some(duration_ms))
        };
        Self { leading, trailing: Some(DurationClock::from_seconds(duration_ms / 1000, false)) }
    }
}

/// A duration named in words — "39 min", "3 hr", "3 hr, 42 min" — the form an
/// album's total playing time takes.
///
/// A [`DurationClock`] *counts* time; this *names an amount* of it. They are
/// separate on purpose: a clock always has minutes and seconds, while a units
/// label never shows a component that is zero — an hour of music reads "1 hr",
/// not "1 hr, 0 min". The variants make the zero component unrepresentable
/// rather than leaving each UI to filter one out.
///
/// The UI renders each variant through the `core.duration.*` catalog messages;
/// the words and the join between them are the catalog's, not the UI's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DurationUnits {
    /// A whole number of hours: "3 hr", never "3 hr, 0 min".
    HoursOnly { hours: u64 },
    /// Under an hour: "39 min", never "0 hr, 39 min".
    MinutesOnly { minutes: u64 },
    /// Both, with `minutes` in `1..=59`.
    HoursAndMinutes { hours: u64, minutes: u64 },
}

impl DurationUnits {
    /// The units for a total duration, or `None` when there is nothing to name.
    ///
    /// The label has no seconds field, so it is an approximation by
    /// construction and names the *nearest* minute. The whole total rounds
    /// before it splits, so 3 h 59 m 45 s reads "4 hr" and can never produce the
    /// "3 hr, 60 min" that rounding the minutes alone would.
    pub fn from_millis(ms: u64) -> Option<Self> {
        if ms == 0 {
            return None;
        }
        // Round half up, in whole minutes.
        let total_minutes = (ms + 30_000) / 60_000;
        let hours = total_minutes / 60;
        let minutes = total_minutes % 60;
        Some(match (hours, minutes) {
            (0, minutes) => Self::MinutesOnly { minutes },
            (hours, 0) => Self::HoursOnly { hours },
            (hours, minutes) => Self::HoursAndMinutes { hours, minutes },
        })
    }
}

#[cfg(test)]
#[path = "duration_tests.rs"]
mod tests;
