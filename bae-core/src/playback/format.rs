use crate::util::format::format_minutes_seconds;

/// Format a duration as "M:SS".
pub fn format_time(ms: u64) -> String {
    format_minutes_seconds(ms)
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
