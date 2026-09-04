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
        let elapsed_ms = if position_ms < 0 {
            0
        } else {
            position_ms as u64
        };
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
/// One decision, not four: the leading label is the elapsed position or the
/// countdown to the end, whichever the user asked for (`show_remaining_time` in
/// the config), and the trailing label is always the track's total length. The
/// UI renders the two and owns neither choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeekBarClocks {
    pub leading: DurationClock,
    /// `None` when the track's length is not known — there is no total to show.
    pub trailing: Option<DurationClock>,
}

impl SeekBarClocks {
    /// A `duration_ms` of zero is how playback reports "length unknown", not a
    /// zero-length track: there is nothing to count down to and no total to
    /// name, so the leading label shows the elapsed position whatever the
    /// preference says.
    pub fn new(position_ms: i64, duration_ms: u64, show_remaining: bool) -> Self {
        if duration_ms == 0 {
            return Self {
                leading: DurationClock::playback_position(position_ms, None),
                trailing: None,
            };
        }
        let leading = if show_remaining {
            DurationClock::remaining(position_ms, duration_ms)
        } else {
            DurationClock::playback_position(position_ms, Some(duration_ms))
        };
        Self {
            leading,
            trailing: Some(DurationClock::from_seconds(duration_ms / 1000, false)),
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

    /// The leading label follows the preference; the trailing one is always the
    /// total, so a bar can never show the countdown on both sides.
    #[test]
    fn the_leading_clock_follows_the_preference() {
        let elapsed = SeekBarClocks::new(25_000, 100_000, false);
        assert_eq!(elapsed.leading, clock(25_000));
        assert_eq!(elapsed.trailing, Some(clock(100_000)));

        let remaining = SeekBarClocks::new(25_000, 100_000, true);
        assert_eq!(remaining.leading, DurationClock::remaining(25_000, 100_000));
        assert!(remaining.leading.negative);
        assert_eq!(remaining.trailing, Some(clock(100_000)));
    }

    #[test]
    fn pregap_elapsed_clock_counts_down_with_ceiling() {
        let expected_two = DurationClock {
            negative: true,
            hours: None,
            minutes: 0,
            seconds: 2,
        };
        assert_eq!(
            SeekBarClocks::new(-2_000, 100_000, false).leading,
            expected_two
        );
        assert_eq!(
            SeekBarClocks::new(-1_001, 100_000, false).leading,
            expected_two
        );

        let expected_one = DurationClock {
            negative: true,
            hours: None,
            minutes: 0,
            seconds: 1,
        };
        assert_eq!(
            SeekBarClocks::new(-1_000, 100_000, false).leading,
            expected_one
        );
        assert_eq!(SeekBarClocks::new(-1, 100_000, false).leading, expected_one);
        assert_eq!(SeekBarClocks::new(0, 100_000, false).leading, clock(0));
    }

    #[test]
    fn remaining_clock_holds_the_full_track_duration_during_pregap() {
        assert_eq!(
            SeekBarClocks::new(-1_000, 100_000, true).leading,
            DurationClock::remaining(0, 100_000),
        );
    }

    /// A zero duration is playback saying it does not know the length. There is
    /// no total, and nothing to count down to — so the leading label shows the
    /// position even when the user asked for remaining.
    #[test]
    fn an_unknown_duration_has_no_total_and_no_countdown() {
        for show_remaining in [false, true] {
            let bar = SeekBarClocks::new(25_000, 0, show_remaining);
            assert_eq!(bar.leading, clock(25_000));
            assert_eq!(bar.trailing, None);
        }
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
