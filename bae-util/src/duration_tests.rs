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
        DurationClock { negative: false, hours: None, minutes: 3, seconds: 7 },
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
        DurationClock { negative: false, hours: Some(1), minutes: 0, seconds: 0 },
    );
    // The 72-minute track every platform used to disagree about.
    assert_eq!(
        clock(4_354_000),
        DurationClock { negative: false, hours: Some(1), minutes: 12, seconds: 34 },
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
        DurationClock { negative: true, hours: None, minutes: 2, seconds: 30 },
    );
    assert_eq!(
        DurationClock::remaining(0, 4_354_000),
        DurationClock { negative: true, hours: Some(1), minutes: 12, seconds: 34 },
    );
}

/// At the end of a track, and past it, the countdown stops at "-0:00"
/// rather than counting back up.
#[test]
fn remaining_clamps_at_the_end_of_the_track() {
    let at_end = DurationClock::remaining(200_000, 200_000);
    let past_end = DurationClock::remaining(200_001, 200_000);
    let expected = DurationClock { negative: true, hours: None, minutes: 0, seconds: 0 };
    assert_eq!(at_end, expected);
    assert_eq!(past_end, expected);
}

/// The leading label follows the preference; the trailing one is always the
/// total, so a bar can never show the countdown on both sides.
#[test]
fn the_leading_clock_follows_the_preference() {
    let elapsed = SeekBarClocks::new(25_000, Some(100_000), false);
    assert_eq!(elapsed.leading, clock(25_000));
    assert_eq!(elapsed.trailing, Some(clock(100_000)));

    let remaining = SeekBarClocks::new(25_000, Some(100_000), true);
    assert_eq!(remaining.leading, DurationClock::remaining(25_000, 100_000));
    assert!(remaining.leading.negative);
    assert_eq!(remaining.trailing, Some(clock(100_000)));
}

#[test]
fn pregap_elapsed_clock_counts_down_with_ceiling() {
    let expected_two = DurationClock { negative: true, hours: None, minutes: 0, seconds: 2 };
    assert_eq!(SeekBarClocks::new(-2_000, Some(100_000), false).leading, expected_two);
    assert_eq!(SeekBarClocks::new(-1_001, Some(100_000), false).leading, expected_two);

    let expected_one = DurationClock { negative: true, hours: None, minutes: 0, seconds: 1 };
    assert_eq!(SeekBarClocks::new(-1_000, Some(100_000), false).leading, expected_one);
    assert_eq!(SeekBarClocks::new(-1, Some(100_000), false).leading, expected_one);
    assert_eq!(SeekBarClocks::new(0, Some(100_000), false).leading, clock(0));
}

#[test]
fn pregap_countdown_overrides_the_remaining_time_preference() {
    let pregap = SeekBarClocks::new(-1_000, Some(100_000), false).leading;
    assert_eq!(SeekBarClocks::new(-1_000, Some(100_000), true).leading, pregap,);
}

/// With no known length there is no total, and nothing to count down to —
/// so the leading label shows the position even when the user asked for
/// remaining, and keeps counting up past any length a track might have.
#[test]
fn an_unknown_duration_has_no_total_and_no_countdown() {
    for show_remaining in [false, true] {
        for position_ms in [25_000, 400_000] {
            let bar = SeekBarClocks::new(position_ms, None, show_remaining);
            assert_eq!(bar.leading, clock(position_ms));
            assert_eq!(bar.trailing, None);
        }
    }
}

/// The pregap counts down to the track's start whether or not its length
/// is known.
#[test]
fn an_unknown_duration_still_counts_down_the_pregap() {
    assert_eq!(
        SeekBarClocks::new(-1_500, None, true).leading,
        SeekBarClocks::new(-1_500, Some(100_000), false).leading,
    );
}

fn units(ms: u64) -> DurationUnits {
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
    assert_eq!(units(13_320_000), DurationUnits::HoursAndMinutes { hours: 3, minutes: 42 },);
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
    assert_eq!(units(14_369_000), DurationUnits::HoursAndMinutes { hours: 3, minutes: 59 },);
}

/// A release whose tracks report no length has no total to name. Under half
/// a minute of audio still does — "0 min" says it is shorter than the label
/// can express, which is true.
#[test]
fn nothing_to_name_has_no_units() {
    assert_eq!(DurationUnits::from_millis(0), None);
    assert_eq!(DurationUnits::from_millis(20_000), Some(DurationUnits::MinutesOnly { minutes: 0 }),);
}
