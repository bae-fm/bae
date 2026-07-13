//! The clock label's fields — the one place that decides how a millisecond
//! duration reads as "3:07" / "1:12:34" / "-0:42".
//!
//! Core owns which fields a clock shows, what an absent or negative duration
//! means, and how a remaining countdown behaves at the end of a track. The UI
//! owns nothing but the digits: `:` between the fields, every field after the
//! first zero-padded to two, a leading `-` when the clock is negative. Digits
//! stay in the UI because the locale never crosses the bridge — an Arabic-Indic
//! locale renders "٣:٠٧" from the same fields "en" renders "3:07" from.

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
        Some(Self::from_seconds(ms / 1000, false))
    }

    /// The countdown from a position within a duration ("-1:23").
    ///
    /// A position past the duration is a real state — a track whose stored
    /// length undershoots the audio the decoder actually produces — so the
    /// countdown stops at "-0:00" instead of counting back up.
    pub fn remaining(position_ms: u64, duration_ms: u64) -> Self {
        Self::from_seconds(duration_ms.saturating_sub(position_ms) / 1000, true)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn clock(ms: i64) -> DurationClock {
        DurationClock::from_millis(Some(ms)).expect("a non-negative duration has a clock")
    }

    /// Below an hour there is no hours field, so the label reads "3:07" and not
    /// "0:03:07"; sub-second remainders floor into the second they are in.
    #[test]
    fn under_an_hour_has_no_hours_field() {
        assert_eq!(
            clock(187_000),
            DurationClock {
                negative: false,
                hours: None,
                minutes: 3,
                seconds: 7,
            },
        );
        assert_eq!(clock(187_999).seconds, 7);
        assert_eq!(clock(400).seconds, 0);
        assert_eq!(clock(0).minutes, 0);
    }

    /// The hours field appears exactly at one hour — the second before it, the
    /// label is still "59:59".
    #[test]
    fn hours_appear_at_one_hour_and_not_before() {
        let last_minute_second = clock(3_599_999);
        assert_eq!(last_minute_second.hours, None);
        assert_eq!(last_minute_second.minutes, 59);
        assert_eq!(last_minute_second.seconds, 59);

        assert_eq!(
            clock(3_600_000),
            DurationClock {
                negative: false,
                hours: Some(1),
                minutes: 0,
                seconds: 0,
            },
        );
        // The 72-minute track every platform used to disagree about.
        assert_eq!(
            clock(4_354_000),
            DurationClock {
                negative: false,
                hours: Some(1),
                minutes: 12,
                seconds: 34,
            },
        );
    }

    /// An absent duration and a negative one are the same thing to a reader:
    /// nothing is known, so nothing is shown.
    #[test]
    fn absent_and_negative_durations_have_no_clock() {
        assert_eq!(DurationClock::from_millis(None), None);
        assert_eq!(DurationClock::from_millis(Some(-1)), None);
        assert_eq!(DurationClock::from_millis(Some(-5_000)), None);
    }

    /// Remaining counts down and carries the minus; it is a whole clock, so it
    /// grows an hours field on a long track just like an elapsed one.
    #[test]
    fn remaining_counts_down_from_the_position() {
        assert_eq!(
            DurationClock::remaining(50_000, 200_000),
            DurationClock {
                negative: true,
                hours: None,
                minutes: 2,
                seconds: 30,
            },
        );
        assert_eq!(
            DurationClock::remaining(0, 4_354_000),
            DurationClock {
                negative: true,
                hours: Some(1),
                minutes: 12,
                seconds: 34,
            },
        );
    }

    /// At the end of a track, and past it, the countdown stops at "-0:00"
    /// rather than counting back up.
    #[test]
    fn remaining_clamps_at_the_end_of_the_track() {
        let at_end = DurationClock::remaining(200_000, 200_000);
        let past_end = DurationClock::remaining(200_001, 200_000);
        let expected = DurationClock {
            negative: true,
            hours: None,
            minutes: 0,
            seconds: 0,
        };
        assert_eq!(at_end, expected);
        assert_eq!(past_end, expected);
    }
}
