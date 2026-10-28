use super::*;
use crate::playback::progress::{PlaybackPosition, PlaybackProgress};

fn at(ms: i64) -> DateTime<Utc> {
    DateTime::<Utc>::from_timestamp_millis(1_700_000_000_000 + ms).unwrap()
}

fn position(track_id: &str, position_ms: i64) -> PlaybackPositions {
    PlaybackPositions {
        library: Some(PlaybackPosition {
            track_id: track_id.into(),
            position_ms,
            duration_ms: Some(200_000),
            progress: position_ms as f64 / 200_000.0,
        }),
        seek_revision: 0,
        preview: None,
    }
}

fn track(track_id: &str) -> crate::playback::PlayingTrack {
    crate::playback::PlayingTrack {
        track_id: track_id.into(),
        duration_ms: Some(200_000),
    }
}

fn playing(track_id: &str) -> PlaybackValues {
    paused(track_id)
        .applying(&PlaybackProgress::StateChanged {
            state: PlaybackState::Playing {
                track: track(track_id),
            },
        })
        .unwrap()
}

fn paused(track_id: &str) -> PlaybackValues {
    PlaybackValues::initial()
        .applying(&PlaybackProgress::StateChanged {
            state: PlaybackState::Paused {
                track: track(track_id),
                reason: crate::playback::PlaybackPauseReason::Manual,
            },
        })
        .unwrap()
}

fn line(timeline: &PlaybackTimeline) -> (u64, bool) {
    let point = timeline.library.as_ref().expect("a library line");
    (point.position_ms, point.moving)
}

/// Ticks along the line a playing track was told say nothing; the first
/// position and a pause or resume each tell a new line.
#[test]
fn ticks_on_the_line_tell_nothing() {
    let mut fold = TimelineFold::new();
    let values = playing("track-1");

    let first = fold.next(&position("track-1", 10_000), &values, at(0));
    assert_eq!(
        line(&first.expect("the first position is a line")),
        (10_000, true)
    );

    for tick in 1..=8 {
        let elapsed = tick * 250;
        assert_eq!(
            fold.next(&position("track-1", 10_000 + elapsed), &values, at(elapsed)),
            None,
            "a tick where the line puts it tells nothing"
        );
    }

    let paused = fold.next(&position("track-1", 12_000), &paused("track-1"), at(2_000));
    assert_eq!(
        line(&paused.expect("a pause is a new line")),
        (12_000, false)
    );
}

/// A position that lands off the line by more than the threshold tells the
/// line again, playing or paused.
#[test]
fn a_position_off_the_line_tells_it_again() {
    let mut fold = TimelineFold::new();
    let values = playing("track-1");
    fold.next(&position("track-1", 10_000), &values, at(0));

    let stalled = fold.next(&position("track-1", 10_500), &values, at(3_000));
    assert_eq!(
        line(&stalled.expect("a stall leaves the line")),
        (10_500, true)
    );

    let mut fold = TimelineFold::new();
    let values = paused("track-1");
    fold.next(&position("track-1", 10_000), &values, at(0));
    assert_eq!(
        fold.next(&position("track-1", 10_400), &values, at(5_000)),
        None
    );
    assert!(fold
        .next(&position("track-1", 12_000), &values, at(6_000))
        .is_some());
}

/// A seek tells a new line even where the line already was, with its
/// revision.
#[test]
fn a_seek_tells_a_new_line() {
    let mut fold = TimelineFold::new();
    let values = playing("track-1");
    fold.next(&position("track-1", 10_000), &values, at(0));

    let mut seeked = position("track-1", 11_000);
    seeked.seek_revision = 1;
    let timeline = fold
        .next(&seeked, &values, at(1_000))
        .expect("a seek is told");
    assert_eq!(timeline.seek_revision, 1);
    assert_eq!(line(&timeline), (11_000, true));
}

/// Another track, the pregap, and no track: each is told, the pregap at the
/// track's start.
#[test]
fn another_track_and_the_pregap_and_none_are_told() {
    let mut fold = TimelineFold::new();
    fold.next(&position("track-1", 10_000), &playing("track-1"), at(0));

    let next = fold
        .next(&position("track-2", -1_500), &playing("track-2"), at(500))
        .expect("another track is a new line");
    let point = next.library.expect("the next track's line");
    assert_eq!((point.track_id.as_str(), point.position_ms), ("track-2", 0));

    let stopped = fold
        .next(
            &PlaybackPositions::initial(),
            &PlaybackValues::initial(),
            at(800),
        )
        .expect("no track is told");
    assert_eq!(stopped.library, None);
}

/// A playing pregap is one line standing at the track's start, however long
/// it plays; the track's time reaching its start is the next line, moving.
#[test]
fn a_pregap_stands_at_the_start_then_moves_once() {
    let mut fold = TimelineFold::new();
    let values = playing("track-1");

    let pregap = fold
        .next(&position("track-1", -3_000), &values, at(0))
        .expect("the pregap's first position is a line");
    assert_eq!(line(&pregap), (0, false));
    for tick in 1..12 {
        let elapsed = tick * 250;
        assert_eq!(
            fold.next(&position("track-1", -3_000 + elapsed), &values, at(elapsed)),
            None,
            "the pregap playing on tells nothing"
        );
    }

    let started = fold
        .next(&position("track-1", 0), &values, at(3_000))
        .expect("the track's start is a new line");
    assert_eq!(line(&started), (0, true));
    assert_eq!(
        fold.next(&position("track-1", 1_000), &values, at(4_000)),
        None,
        "the track plays on its line"
    );
}

/// A position of a track the state does not play is not moving.
#[test]
fn a_position_of_a_track_not_playing_does_not_move() {
    let mut fold = TimelineFold::new();
    let timeline = fold
        .next(&position("track-2", 0), &playing("track-1"), at(0))
        .unwrap();
    assert_eq!(line(&timeline), (0, false));
}
