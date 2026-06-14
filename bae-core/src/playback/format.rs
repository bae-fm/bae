use crate::util::format::format_minutes_seconds;

/// Format a duration as "M:SS".
pub fn format_time(ms: u64) -> String {
    format_minutes_seconds(ms)
}

/// Format an elapsed time label, accounting for pregap.
/// During pregap (position < pregap_ms), shows "-0:02" style countdown.
///
/// `duration_ms` clamps position so elapsed never exceeds the duration label.
/// The decoder position can overshoot the declared track duration slightly
/// because a file's metadata duration can round differently than its actual
/// sample count. Clamping here keeps the displayed elapsed time from exceeding
/// the label.
pub fn format_elapsed(position_ms: u64, duration_ms: u64, pregap_ms: Option<i64>) -> String {
    let position_ms = position_ms.min(duration_ms);
    let offset = pregap_ms.unwrap_or(0).max(0) as u64;
    if position_ms < offset {
        // Ceil remaining seconds so -0:01 shows until the exact pregap boundary.
        let remaining = offset - position_ms;
        let total_seconds = remaining.div_ceil(1000);
        let minutes = total_seconds / 60;
        let seconds = total_seconds % 60;
        format!("-{}:{:02}", minutes, seconds)
    } else {
        format_time(position_ms - offset)
    }
}

/// Format a duration label, subtracting pregap.
pub fn format_duration(duration_ms: u64, pregap_ms: Option<i64>) -> String {
    let offset = pregap_ms.unwrap_or(0).max(0) as u64;
    format_time(duration_ms.saturating_sub(offset))
}

/// Format remaining time as "-M:SS".
/// During pregap, remaining includes the pregap countdown plus the full track.
pub fn format_remaining(position_ms: u64, duration_ms: u64, pregap_ms: Option<i64>) -> String {
    let offset = pregap_ms.unwrap_or(0).max(0) as u64;
    let position_ms = position_ms.min(duration_ms);
    let track_duration = duration_ms.saturating_sub(offset);
    let track_position = position_ms.saturating_sub(offset);
    let remaining = track_duration.saturating_sub(track_position);
    format!("-{}", format_minutes_seconds(remaining))
}

/// Compute slider progress (0.0-1.0), clamped to 0 during pregap.
pub fn compute_progress(position_ms: u64, duration_ms: u64, pregap_ms: Option<i64>) -> f64 {
    let offset = pregap_ms.unwrap_or(0).max(0) as u64;
    let track_duration = duration_ms.saturating_sub(offset);
    if track_duration == 0 {
        return 0.0;
    }
    let track_position = position_ms.saturating_sub(offset);
    (track_position as f64 / track_duration as f64).clamp(0.0, 1.0)
}

/// Format the time label for a given seek ratio and track duration.
/// Used by the UI to preview the seek target while dragging the slider.
pub fn format_time_at_ratio(ratio: f64, duration_ms: u64) -> String {
    let position_ms = (ratio.clamp(0.0, 1.0) * duration_ms as f64) as u64;
    format_time(position_ms)
}

/// Format the remaining time label for a given seek ratio and track duration.
/// Used by the UI to preview the seek target while dragging the slider when
/// the user has the elapsed label toggled to remaining mode.
///
/// `duration_ms` here is the user-facing track duration (already pregap-
/// adjusted via `adjust_for_pregap` before reaching the UI), so the slider
/// ratio maps directly to post-pregap position and `format_remaining` gets
/// `None` pregap — nothing left to subtract.
pub fn format_remaining_at_ratio(ratio: f64, duration_ms: u64) -> String {
    let position_ms = (ratio.clamp(0.0, 1.0) * duration_ms as f64) as u64;
    format_remaining(position_ms, duration_ms, None)
}

/// Adjust raw position/duration for pregap: position clamped to 0 during pregap,
/// duration excludes pregap. All external consumers see "track time" not "decoder time".
pub fn adjust_for_pregap(position_ms: u64, duration_ms: u64, pregap_ms: Option<i64>) -> (u64, u64) {
    let offset = pregap_ms.unwrap_or(0).max(0) as u64;
    (
        position_ms.saturating_sub(offset),
        duration_ms.saturating_sub(offset),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- format_time --

    #[test]
    fn format_time_cases() {
        // (milliseconds, "M:SS"): seconds pad to two digits, minutes don't,
        // and sub-second remainders truncate down rather than rounding.
        let cases = [
            (0, "0:00"),
            (5_000, "0:05"),
            (63_000, "1:03"),
            (600_000, "10:00"),
            (1_999, "0:01"),
        ];
        for (ms, expected) in cases {
            assert_eq!(format_time(ms), expected, "ms: {ms}");
        }
    }

    // -- format_elapsed --
    //
    // During a CUE/FLAC pregap the decoder is already playing audio (the gap
    // between INDEX 00 and INDEX 01) but the track hasn't "started" yet.
    // The elapsed display counts down with a minus sign, like a CD player:
    //   -0:02, -0:01, then 0:00 at the track boundary, 0:01, 0:02, ...
    //
    // The countdown uses ceiling so that -0:01 holds until the exact pregap
    // boundary. This gives every displayed value ~1 second of real time.

    #[test]
    fn format_elapsed_no_pregap() {
        assert_eq!(format_elapsed(0, 240_000, None), "0:00");
        assert_eq!(format_elapsed(63_000, 240_000, None), "1:03");
    }

    #[test]
    fn format_elapsed_pregap_countdown_uses_ceiling() {
        // 2-second pregap. Countdown ceils remaining time to whole seconds.
        let pregap = Some(2000);
        assert_eq!(format_elapsed(0, 12_000, pregap), "-0:02"); // 2000ms left -> 2s
        assert_eq!(format_elapsed(500, 12_000, pregap), "-0:02"); // 1500ms left -> ceil -> 2s
        assert_eq!(format_elapsed(999, 12_000, pregap), "-0:02"); // 1001ms left -> ceil -> 2s
        assert_eq!(format_elapsed(1000, 12_000, pregap), "-0:01"); // 1000ms left -> ceil -> 1s
        assert_eq!(format_elapsed(1001, 12_000, pregap), "-0:01"); // 999ms left -> ceil -> 1s
        assert_eq!(format_elapsed(1999, 12_000, pregap), "-0:01"); // 1ms left -> ceil -> 1s
    }

    #[test]
    fn format_elapsed_counts_up_after_pregap() {
        let pregap = Some(2000);
        assert_eq!(format_elapsed(2000, 12_000, pregap), "0:00"); // exactly at boundary
        assert_eq!(format_elapsed(3000, 12_000, pregap), "0:01");
        assert_eq!(format_elapsed(5000, 12_000, pregap), "0:03");
    }

    #[test]
    fn format_elapsed_clamps_to_duration() {
        // Decoder position can overshoot declared duration (see doc comment).
        // Elapsed must never exceed the duration label.
        assert_eq!(format_elapsed(241_000, 240_000, None), "4:00");
        assert_eq!(format_elapsed(12_100, 12_000, Some(2000)), "0:10");
    }

    #[test]
    fn format_elapsed_negative_pregap_treated_as_zero() {
        // Negative pregap_ms is invalid; treat as no pregap.
        assert_eq!(format_elapsed(500, 60_000, Some(-100)), "0:00");
    }

    // -- format_duration --
    //
    // Duration excludes pregap so the user sees actual track length.

    #[test]
    fn format_duration_no_pregap() {
        assert_eq!(format_duration(240_000, None), "4:00");
    }

    #[test]
    fn format_duration_subtracts_pregap() {
        // 4:02 total with 2-second pregap -> 4:00 track duration.
        assert_eq!(format_duration(242_000, Some(2000)), "4:00");
    }

    // -- compute_progress --
    //
    // Progress is 0.0-1.0 representing position within the track (after pregap).
    // During pregap, progress stays at 0 -- the slider doesn't move until the
    // track starts. At the end of the track, progress reaches 1.0.

    #[test]
    fn compute_progress_no_pregap() {
        assert_eq!(compute_progress(0, 10_000, None), 0.0);
        assert_eq!(compute_progress(5_000, 10_000, None), 0.5);
        assert_eq!(compute_progress(10_000, 10_000, None), 1.0);
    }

    #[test]
    fn compute_progress_stays_zero_during_pregap() {
        let pregap = Some(2000);
        assert_eq!(compute_progress(0, 12_000, pregap), 0.0);
        assert_eq!(compute_progress(1000, 12_000, pregap), 0.0);
        assert_eq!(compute_progress(1999, 12_000, pregap), 0.0);
    }

    #[test]
    fn compute_progress_starts_after_pregap() {
        // 12s total, 2s pregap -> 10s track. At 7s position = 5s into track = 50%.
        let pregap = Some(2000);
        assert_eq!(compute_progress(2000, 12_000, pregap), 0.0);
        assert_eq!(compute_progress(7000, 12_000, pregap), 0.5);
        assert_eq!(compute_progress(12_000, 12_000, pregap), 1.0);
    }

    #[test]
    fn compute_progress_zero_duration() {
        assert_eq!(compute_progress(0, 0, None), 0.0);
    }

    // -- adjust_for_pregap --
    //
    // Raw position/duration from the decoder include pregap time. External
    // consumers (media centers, Control Center) should see "track time":
    // position 0 at track start, duration excluding pregap.

    #[test]
    fn adjust_for_pregap_no_pregap() {
        assert_eq!(adjust_for_pregap(5000, 60_000, None), (5000, 60_000));
    }

    #[test]
    fn adjust_for_pregap_clamps_position_to_zero_during_pregap() {
        let pregap = Some(2000);
        assert_eq!(adjust_for_pregap(0, 12_000, pregap), (0, 10_000));
        assert_eq!(adjust_for_pregap(1000, 12_000, pregap), (0, 10_000));
    }

    #[test]
    fn adjust_for_pregap_subtracts_offset_after_pregap() {
        let pregap = Some(2000);
        assert_eq!(adjust_for_pregap(5000, 12_000, pregap), (3000, 10_000));
    }

    // -- format_time_at_ratio --

    #[test]
    fn format_time_at_ratio_boundaries() {
        assert_eq!(format_time_at_ratio(0.0, 240_000), "0:00");
        assert_eq!(format_time_at_ratio(1.0, 240_000), "4:00");
    }

    #[test]
    fn format_time_at_ratio_midpoint() {
        assert_eq!(format_time_at_ratio(0.5, 240_000), "2:00");
    }

    #[test]
    fn format_time_at_ratio_clamps() {
        assert_eq!(format_time_at_ratio(-0.5, 240_000), "0:00");
        assert_eq!(format_time_at_ratio(1.5, 240_000), "4:00");
    }

    // -- format_remaining_at_ratio --

    #[test]
    fn format_remaining_at_ratio_boundaries() {
        assert_eq!(format_remaining_at_ratio(0.0, 240_000), "-4:00");
        assert_eq!(format_remaining_at_ratio(1.0, 240_000), "-0:00");
    }

    #[test]
    fn format_remaining_at_ratio_midpoint() {
        assert_eq!(format_remaining_at_ratio(0.5, 240_000), "-2:00");
    }

    #[test]
    fn format_remaining_at_ratio_clamps() {
        assert_eq!(format_remaining_at_ratio(-0.5, 240_000), "-4:00");
        assert_eq!(format_remaining_at_ratio(1.5, 240_000), "-0:00");
    }
}
