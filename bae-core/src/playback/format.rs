//! Conversions between decoder time (raw, includes pregap) and track time (what
//! the user sees and drags). Every one of them derives the pregap offset here,
//! so the slider and the seek that follows it cannot disagree about where a
//! track starts.

/// A negative pregap means no offset.
fn pregap_offset(pregap_ms: Option<i64>) -> u64 {
    pregap_ms.unwrap_or(0).max(0) as u64
}

/// Adjust raw position/duration for pregap: position clamped to 0 during pregap,
/// duration excludes pregap. All external consumers see "track time" not "decoder time".
pub fn adjust_for_pregap(position_ms: u64, duration_ms: u64, pregap_ms: Option<i64>) -> (u64, u64) {
    let offset = pregap_offset(pregap_ms);
    (
        position_ms.saturating_sub(offset),
        duration_ms.saturating_sub(offset),
    )
}

/// Compute slider progress (0.0-1.0), clamped to 0 during pregap.
pub fn compute_progress(position_ms: u64, duration_ms: u64, pregap_ms: Option<i64>) -> f64 {
    let (track_position, track_duration) = adjust_for_pregap(position_ms, duration_ms, pregap_ms);
    if track_duration == 0 {
        return 0.0;
    }
    (track_position as f64 / track_duration as f64).clamp(0.0, 1.0)
}

/// The raw position a slider ratio points at — the inverse of [`compute_progress`].
/// Callers must pass the same pregap they render the slider with, or a seek lands
/// somewhere other than where the user dropped it.
pub fn position_for_progress(ratio: f64, duration_ms: u64, pregap_ms: Option<i64>) -> u64 {
    let offset = pregap_offset(pregap_ms);
    let track_duration = duration_ms.saturating_sub(offset);
    offset + (ratio.clamp(0.0, 1.0) * track_duration as f64) as u64
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

    /// A negative pregap is clamped to no offset, so position and duration pass
    /// through unchanged and progress is measured against the full duration.
    #[test]
    fn negative_pregap_clamps_to_zero_offset() {
        assert_eq!(adjust_for_pregap(5000, 10_000, Some(-2000)), (5000, 10_000));
        assert_eq!(compute_progress(5000, 10_000, Some(-2000)), 0.5);
    }

    // -- position_for_progress --
    //
    // The inverse of compute_progress. A slider drawn from one and dragged back
    // through the other must land where the user dropped it, whatever the pregap.

    #[test]
    fn position_for_progress_inverts_compute_progress() {
        for pregap in [None, Some(2000), Some(-2000)] {
            for ratio in [0.0, 0.25, 0.5, 0.75, 1.0] {
                let position = position_for_progress(ratio, 12_000, pregap);
                let round_tripped = compute_progress(position, 12_000, pregap);
                assert!(
                    (round_tripped - ratio).abs() < 1e-9,
                    "pregap {pregap:?} ratio {ratio} -> {position}ms -> {round_tripped}"
                );
            }
        }
    }

    #[test]
    fn position_for_progress_offsets_past_the_pregap() {
        // 12s total, 2s pregap -> 10s track. Halfway is 5s in, i.e. 7s raw.
        assert_eq!(position_for_progress(0.0, 12_000, Some(2000)), 2000);
        assert_eq!(position_for_progress(0.5, 12_000, Some(2000)), 7000);
        assert_eq!(position_for_progress(1.0, 12_000, Some(2000)), 12_000);
    }

    /// A CUE track with a PREGAP directive has `generated_pregap_ms` but no
    /// `pregap_ms`. The slider is drawn against `total_pregap_ms()` (the `.or()`
    /// of the two), so a seek must use that same value -- reading the raw
    /// `pregap_ms` here would seek to 5s where the slider says 7s.
    #[test]
    fn generated_pregap_seeks_where_the_slider_points() {
        let pregap_ms: Option<i64> = None;
        let generated_pregap_ms: Option<i64> = Some(2000);
        let total = pregap_ms.or(generated_pregap_ms);

        assert_eq!(position_for_progress(0.5, 12_000, total), 7000);
        assert_eq!(compute_progress(7000, 12_000, total), 0.5);
        // What the raw field alone would have given:
        assert_eq!(position_for_progress(0.5, 12_000, pregap_ms), 6000);
    }
}
