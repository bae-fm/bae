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
}
