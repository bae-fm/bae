use super::*;

fn timeline(duration_ms: u64, pregap_ms: u64) -> TrackTimeline {
    TrackTimeline::new(Some(Duration::from_millis(duration_ms)), Duration::from_millis(pregap_ms))
}

/// The timeline of a track whose length nothing knows.
fn unknown_length(pregap_ms: u64) -> TrackTimeline {
    TrackTimeline::new(None, Duration::from_millis(pregap_ms))
}

fn at(ms: u64) -> StreamPosition {
    StreamPosition::from_millis(ms)
}

// -- progress --
//
// Progress is 0.0-1.0 representing position within the track (after pregap).
// During pregap, progress stays at 0 -- the slider doesn't move until the
// track starts. At the end of the track, progress reaches 1.0.

#[test]
fn progress_no_pregap() {
    let t = timeline(10_000, 0);
    assert_eq!(t.progress(at(0)), 0.0);
    assert_eq!(t.progress(at(5_000)), 0.5);
    assert_eq!(t.progress(at(10_000)), 1.0);
}

#[test]
fn progress_stays_zero_during_pregap() {
    let t = timeline(10_000, 2000);
    assert_eq!(t.progress(at(0)), 0.0);
    assert_eq!(t.progress(at(1000)), 0.0);
    assert_eq!(t.progress(at(1999)), 0.0);
}

#[test]
fn progress_starts_after_pregap() {
    // 10s track plus a 2s pregap. At 7s in the stream = 5s into track = 50%.
    let t = timeline(10_000, 2000);
    assert_eq!(t.progress(at(2000)), 0.0);
    assert_eq!(t.progress(at(7000)), 0.5);
    assert_eq!(t.progress(at(12_000)), 1.0);
}

#[test]
fn progress_zero_duration() {
    assert_eq!(timeline(0, 0).progress(at(0)), 0.0);
}

/// Without a length there is no end to fill toward, so the bar stays empty
/// however far the track plays.
#[test]
fn progress_stays_empty_without_a_length() {
    let t = unknown_length(2000);
    assert_eq!(t.progress(at(0)), 0.0);
    assert_eq!(t.progress(at(400_000)), 0.0);
}

/// An unknown length is no length: not the track's, and not its stream's.
#[test]
fn an_unknown_length_has_no_duration() {
    let t = unknown_length(2000);
    assert_eq!(t.duration(), None);
    assert_eq!(t.duration_ms(), None);
    assert_eq!(t.stream_duration(), None);
    assert_eq!(t.track_start(), at(2000));
}

// -- track_time --
//
// The stream includes the pregap; the stored duration does not. The player
// shows a negative countdown before INDEX 01, zero at the track's start, and
// the stored duration unchanged.

#[test]
fn track_time_no_pregap() {
    assert_eq!(timeline(60_000, 0).track_time(at(5000)), TrackTime::from_millis(5000));
}

#[test]
fn track_time_counts_down_during_pregap() {
    let t = timeline(10_000, 2000);
    assert_eq!(t.track_time(at(0)), TrackTime::from_millis(-2000));
    assert_eq!(t.track_time(at(1000)), TrackTime::from_millis(-1000));
    assert_eq!(t.track_time(at(1999)), TrackTime::from_millis(-1));
    assert_eq!(t.duration_ms(), Some(10_000));
}

#[test]
fn track_time_subtracts_the_pregap() {
    assert_eq!(timeline(10_000, 2000).track_time(at(5000)), TrackTime::from_millis(3000));
}

#[test]
fn track_time_holds_at_the_duration_past_the_end() {
    assert_eq!(timeline(10_000, 2000).track_time(at(13_000)), TrackTime::from_millis(10_000));
}

/// With no declared end there is nothing to hold the time at: it runs on
/// for as long as the audio does.
#[test]
fn track_time_runs_on_without_a_length() {
    let t = unknown_length(2000);
    assert_eq!(t.track_time(at(0)), TrackTime::from_millis(-2000));
    assert_eq!(t.track_time(at(402_000)), TrackTime::from_millis(400_000));
}

// -- stream_position --

#[test]
fn stream_position_inverts_track_time() {
    let t = timeline(10_000, 2000);
    for ms in [-2000, -500, 0, 3000, 10_000] {
        let time = TrackTime::from_millis(ms);
        assert_eq!(t.track_time(t.stream_position(time)), time, "{ms}ms");
    }
    assert_eq!(t.stream_position(TrackTime::START), t.track_start());
}

#[test]
fn stream_position_holds_at_the_stream_start() {
    assert_eq!(
        timeline(10_000, 2000).stream_position(TrackTime::from_millis(-5000)),
        StreamPosition::START
    );
}

// -- position_at_progress --
//
// The inverse of progress. A slider drawn from one and dragged back through
// the other must land where the user dropped it, whatever the pregap.

#[test]
fn position_at_progress_inverts_progress() {
    for pregap in [0, 2000] {
        let t = timeline(10_000, pregap);
        for ratio in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let position = t
                .position_at_progress(ratio)
                .expect("a timeline with a length has a position at every fill");
            let round_tripped = t.progress(position);
            assert!(
                (round_tripped - ratio).abs() < 1e-9,
                "pregap {pregap:?} ratio {ratio} -> {position:?} -> {round_tripped}"
            );
        }
    }
}

#[test]
fn position_at_progress_offsets_past_the_pregap() {
    // 10s track plus a 2s pregap. Halfway is 5s in, i.e. 7s into the stream.
    let t = timeline(10_000, 2000);
    assert_eq!(t.position_at_progress(0.0), Some(at(2000)));
    assert_eq!(t.position_at_progress(0.5), Some(at(7000)));
    assert_eq!(t.position_at_progress(1.0), Some(at(12_000)));
}

/// A fill is a fraction of the length; without one it points nowhere.
#[test]
fn position_at_progress_has_no_position_without_a_length() {
    let t = unknown_length(2000);
    for ratio in [0.0, 0.5, 1.0] {
        assert_eq!(t.position_at_progress(ratio), None, "ratio {ratio}");
    }
}
