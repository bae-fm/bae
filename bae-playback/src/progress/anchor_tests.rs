use super::*;
use crate::service::PlaybackState;
use crate::{progress::PlaybackProgress, PlayingTrack};

const MS: u64 = 1_000_000;
const DURATION_MS: u64 = 300_000;

fn state(playing: bool, track_id: &str) -> PlaybackProgress {
    let track = PlayingTrack { track_id: track_id.into(), duration_ms: Some(DURATION_MS) };
    PlaybackProgress::StateChanged {
        state: if playing {
            PlaybackState::Playing { track }
        } else {
            PlaybackState::Paused { track, reason: crate::PlaybackPauseReason::Manual }
        },
    }
}

fn tick(position_ms: i64, rate: f64) -> PlaybackProgress {
    PlaybackProgress::PositionUpdate {
        track_id: "track-1".into(),
        position_ms,
        duration_ms: Some(DURATION_MS),
        progress: 0.0,
        rate,
    }
}

fn seeked(position_ms: i64, rate: f64) -> PlaybackProgress {
    PlaybackProgress::Seeked {
        track_id: "track-1".into(),
        position_ms,
        duration_ms: Some(DURATION_MS),
        progress: 0.0,
        rate,
    }
}

/// The positions as playback events move them, folded at each event's time,
/// and the anchors the fold told.
struct Playback {
    positions: PlaybackPositions,
    fold: AnchorFold,
    told: Vec<TrackAnchor>,
}

impl Playback {
    fn new() -> Self {
        Self { positions: PlaybackPositions::initial(), fold: AnchorFold::new(), told: vec![] }
    }

    /// `event` at `at_ms` of the clock.
    fn at(&mut self, at_ms: u64, event: PlaybackProgress) -> &mut Self {
        if let Some(next) = self.positions.applying(&event) {
            self.positions = next;
        }
        if self.fold.fold(&self.positions, at_ms * MS) {
            self.told.extend(self.fold.anchor().cloned());
        }
        self
    }
}

fn line(playback: &Playback) -> Vec<(i64, f64, u64)> {
    playback
        .told
        .iter()
        .map(|anchor| (anchor.position_ms, anchor.rate, anchor.seek_revision))
        .collect()
}

#[test]
fn ticks_along_the_line_tell_nothing_after_the_first() {
    let mut playback = Playback::new();
    playback.at(0, state(true, "track-1")).at(0, tick(10_000, 1.0));
    for step in 1..=50 {
        playback.at(step * 200, tick(10_000 + step as i64 * 200, 1.0));
    }
    assert_eq!(line(&playback), vec![(10_000, 1.0, 0)]);
}

#[test]
fn a_seek_tells_a_new_anchor_even_where_the_line_already_was() {
    let mut playback = Playback::new();
    playback.at(0, state(true, "track-1")).at(0, tick(10_000, 1.0));
    playback.at(200, tick(10_200, 1.0)).at(400, seeked(10_400, 1.0));
    playback.at(600, tick(10_600, 1.0));
    playback.at(800, seeked(120_000, 1.0)).at(1_000, tick(120_200, 1.0));
    assert_eq!(line(&playback), vec![(10_000, 1.0, 0), (10_400, 1.0, 1), (120_000, 1.0, 2)]);
}

#[test]
fn a_pause_and_a_resume_each_tell_a_new_anchor() {
    let mut playback = Playback::new();
    playback.at(0, state(true, "track-1")).at(0, tick(10_000, 1.0));
    playback.at(200, tick(10_200, 1.0)).at(300, state(false, "track-1"));
    // A paused track's position stands, and nothing is told for it.
    playback.at(5_000, seeked(10_200, 1.0)).at(6_000, state(true, "track-1"));
    playback.at(6_200, tick(10_400, 1.0));
    assert_eq!(
        line(&playback),
        vec![(10_000, 1.0, 0), (10_200, 0.0, 0), (10_200, 0.0, 1), (10_200, 1.0, 1)]
    );
}

#[test]
fn a_change_of_rate_tells_a_new_anchor() {
    let mut playback = Playback::new();
    playback.at(0, state(true, "track-1")).at(0, tick(10_000, 1.0));
    playback.at(200, tick(10_200, 1.0)).at(400, tick(10_300, 0.5));
    playback.at(800, tick(10_500, 0.5));
    assert_eq!(line(&playback), vec![(10_000, 1.0, 0), (10_300, 0.5, 0)]);
}

#[test]
fn another_track_ends_the_anchor_and_its_first_position_starts_one() {
    let mut playback = Playback::new();
    playback.at(0, state(true, "track-1")).at(0, tick(10_000, 1.0));
    playback.at(200, state(true, "track-2"));
    assert_eq!(playback.fold.anchor(), None);
    playback.at(
        400,
        PlaybackProgress::PositionUpdate {
            track_id: "track-2".into(),
            position_ms: 0,
            duration_ms: Some(DURATION_MS),
            progress: 0.0,
            rate: 1.0,
        },
    );
    assert_eq!(playback.fold.anchor().map(|anchor| anchor.track_id.as_str()), Some("track-2"));
}

#[test]
fn a_position_that_lands_off_the_line_tells_a_new_anchor() {
    let mut playback = Playback::new();
    playback.at(0, state(true, "track-1")).at(0, tick(10_000, 1.0));
    // The line puts the position at 12 s; it lands at 14 s.
    playback.at(2_000, tick(14_000, 1.0));
    assert_eq!(line(&playback), vec![(10_000, 1.0, 0), (14_000, 1.0, 0)]);
}

#[test]
fn the_pregap_is_on_the_line_below_zero() {
    let mut playback = Playback::new();
    playback.at(0, state(true, "track-1")).at(0, tick(-1_250, 1.0));
    for step in 1..=5 {
        playback.at(step * 200, tick(-1_250 + step as i64 * 200, 1.0));
    }
    assert_eq!(line(&playback), vec![(-1_250, 1.0, 0)]);
}
