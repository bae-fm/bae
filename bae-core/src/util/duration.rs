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
    pub fn from_millis(ms: i64) -> Option<Self> {
        let ms = u64::try_from(ms).ok()?;
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

    fn units(ms: i64) -> DurationUnits {
        DurationUnits::from_millis(ms).expect("a positive duration has units")
    }

    /// Under an hour the label is minutes alone — never "0 hr, 39 min".
    #[test]
    fn under_an_hour_names_only_minutes() {
        assert_eq!(units(2_340_000), DurationUnits::MinutesOnly { minutes: 39 });
        assert_eq!(units(60_000), DurationUnits::MinutesOnly { minutes: 1 });
    }

    /// A whole number of hours names only hours — never "1 hr, 0 min".
    #[test]
    fn a_whole_hour_names_only_hours() {
        assert_eq!(units(3_600_000), DurationUnits::HoursOnly { hours: 1 });
        assert_eq!(units(10_800_000), DurationUnits::HoursOnly { hours: 3 });
    }

    #[test]
    fn hours_and_minutes_name_both() {
        assert_eq!(
            units(13_320_000),
            DurationUnits::HoursAndMinutes {
                hours: 3,
                minutes: 42,
            },
        );
    }

    /// The label has no seconds field, so it names the nearest minute: 29 s of a
    /// minute round away, 30 s round up.
    #[test]
    fn minutes_round_to_nearest() {
        assert_eq!(units(89_000), DurationUnits::MinutesOnly { minutes: 1 });
        assert_eq!(units(90_000), DurationUnits::MinutesOnly { minutes: 2 });
        assert_eq!(units(29_000), DurationUnits::MinutesOnly { minutes: 0 });
        assert_eq!(units(30_000), DurationUnits::MinutesOnly { minutes: 1 });
    }

    /// Rounding the total, not the minutes field, is what keeps "3 hr, 60 min"
    /// from ever existing: 3 h 59 m 45 s carries into the hour.
    #[test]
    fn rounding_carries_into_the_hours() {
        assert_eq!(units(14_385_000), DurationUnits::HoursOnly { hours: 4 });
        assert_eq!(
            units(14_369_000),
            DurationUnits::HoursAndMinutes {
                hours: 3,
                minutes: 59,
            },
        );
    }

    /// A release whose tracks report no length has no total to name. Under half
    /// a minute of audio still does — "0 min" says it is shorter than the label
    /// can express, which is true.
    #[test]
    fn nothing_to_name_has_no_units() {
        assert_eq!(DurationUnits::from_millis(0), None);
        assert_eq!(DurationUnits::from_millis(-1), None);
        assert_eq!(
            DurationUnits::from_millis(20_000),
            Some(DurationUnits::MinutesOnly { minutes: 0 }),
        );
    }
}
