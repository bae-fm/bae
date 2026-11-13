use super::*;

/// `event`'s change, taken by the row, as the fan-out does on a good write.
fn applied(values: &mut PlaybackValues, event: &PlaybackProgress) -> Option<PlaybackChange> {
    let change = values.change_for(event)?;
    values.took(&change);
    Some(change)
}

fn preview_target(path: &str) -> bae_track_audio::PreviewTarget {
    bae_track_audio::PreviewTarget::sample_range(path.into(), 0, None)
}

#[test]
fn media_control_uses_preview_only_while_preview_is_active() {
    let mut values = PlaybackValues::initial();
    assert_eq!(values.media_control(), MediaControlPlayback::Library);

    let playing =
        PreviewState::Playing { target: preview_target("/tmp/preview.flac"), duration_ms: 120_000 };
    assert_eq!(
        applied(&mut values, &PlaybackProgress::PreviewStateChanged(playing.clone())),
        Some(PlaybackChange::Preview(playing))
    );
    assert!(matches!(
        values.media_control(),
        MediaControlPlayback::Preview { duration_ms: 120_000, is_playing: true, .. }
    ));

    assert_eq!(
        applied(&mut values, &PlaybackProgress::PreviewStateChanged(PreviewState::Idle)),
        Some(PlaybackChange::Preview(PreviewState::Idle))
    );
    assert_eq!(values.media_control(), MediaControlPlayback::Library);
}

/// A value is a change only when it moves; a failure always is one, so
/// two failures alike are two changes.
#[test]
fn a_value_changes_only_when_it_moves_and_every_failure_is_a_change() {
    let mut values = PlaybackValues::initial();
    assert_eq!(
        applied(&mut values, &PlaybackProgress::VolumeChanged { volume: 1.0 }),
        None,
        "the volume playback starts at"
    );
    assert_eq!(
        applied(&mut values, &PlaybackProgress::VolumeChanged { volume: 0.5 }),
        Some(PlaybackChange::Volume(0.5))
    );
    assert_eq!(values.volume, 0.5);
    assert_eq!(
        applied(&mut values, &PlaybackProgress::StateChanged { state: PlaybackState::Stopped }),
        None
    );
    let failure = crate::PlaybackErrorReason::SyncDisconnected;
    for _ in 0..2 {
        assert_eq!(
            applied(&mut values, &PlaybackProgress::PlaybackError { reason: failure.clone() }),
            Some(PlaybackChange::Failed(failure.clone()))
        );
    }
}

#[test]
fn the_in_app_position_keeps_the_pregap() {
    let positions = PlaybackPositions::initial()
        .applying(&PlaybackProgress::PositionUpdate {
            position_ms: -1_250,
            duration_ms: Some(100_000),
            track_id: "track-1".to_string(),
            progress: 0.0,
        })
        .expect("position update is retained");

    assert_eq!(positions.library.as_ref().expect("UI position").position_ms, -1_250);
}

#[test]
fn position_ticks_change_no_playback_value() {
    let mut values = PlaybackValues::initial();
    for tick in [
        PlaybackProgress::PositionUpdate {
            position_ms: 1_000,
            duration_ms: Some(100_000),
            track_id: "track-1".to_string(),
            progress: 0.01,
        },
        PlaybackProgress::Seeked {
            position_ms: 50_000,
            duration_ms: Some(100_000),
            track_id: "track-1".to_string(),
            progress: 0.5,
        },
        PlaybackProgress::PreviewPositionUpdate { position_ms: 1_000, progress: 0.01 },
        PlaybackProgress::PreviewSeeked { position_ms: 1_000, progress: 0.01 },
    ] {
        assert!(applied(&mut values, &tick).is_none(), "{tick:?} is a position");
    }
}

#[test]
fn a_state_on_another_track_ends_the_library_position() {
    let positions = PlaybackPositions::initial()
        .applying(&PlaybackProgress::PositionUpdate {
            position_ms: 1_000,
            duration_ms: Some(100_000),
            track_id: "track-1".to_string(),
            progress: 0.01,
        })
        .expect("position update is retained");
    let playing = |track_id: &str| PlaybackProgress::StateChanged {
        state: PlaybackState::Playing {
            track: crate::PlayingTrack {
                track_id: track_id.to_string(),
                duration_ms: Some(100_000),
            },
        },
    };

    let same_track = positions.applying(&playing("track-1")).expect("a state change is folded");
    assert_eq!(same_track.library, positions.library);
    assert!(same_track.playing);

    let next_track = positions.applying(&playing("track-2")).expect("a state change is folded");
    assert_eq!(next_track.library, None);

    let stopped = positions
        .applying(&PlaybackProgress::StateChanged { state: PlaybackState::Stopped })
        .expect("a state change is folded");
    assert_eq!(stopped.library, None);
    assert!(!stopped.playing);

    let paused = same_track
        .applying(&PlaybackProgress::StateChanged {
            state: PlaybackState::Paused {
                track: crate::PlayingTrack {
                    track_id: "track-1".to_string(),
                    duration_ms: Some(100_000),
                },
                reason: crate::PlaybackPauseReason::Manual,
            },
        })
        .expect("a state change is folded");
    assert_eq!(paused.library, positions.library);
    assert!(!paused.playing);
}

#[test]
fn a_preview_position_names_its_clip_and_keeps_its_place_only_on_that_clip() {
    let first = preview_target("/tmp/first.flac");
    let playing_first = PlaybackPositions::initial()
        .applying(&PlaybackProgress::PreviewStateChanged(PreviewState::Playing {
            target: first.clone(),
            duration_ms: 120_000,
        }))
        .and_then(|positions| {
            positions.applying(&PlaybackProgress::PreviewPositionUpdate {
                position_ms: 30_000,
                progress: 0.25,
            })
        })
        .expect("the first clip plays");
    assert_eq!(
        playing_first.preview,
        Some(PreviewPosition {
            target: first.clone(),
            duration_ms: 120_000,
            position_ms: 30_000,
            progress: 0.25,
        })
    );

    let paused_first = playing_first
        .applying(&PlaybackProgress::PreviewStateChanged(PreviewState::Paused {
            target: first,
            duration_ms: 120_000,
        }))
        .expect("the first clip pauses");
    assert_eq!(paused_first, playing_first);

    let second = preview_target("/tmp/second.flac");
    let playing_second = paused_first
        .applying(&PlaybackProgress::PreviewStateChanged(PreviewState::Playing {
            target: second.clone(),
            duration_ms: 60_000,
        }))
        .expect("the second clip plays");
    assert_eq!(
        playing_second.preview,
        Some(PreviewPosition {
            target: second,
            duration_ms: 60_000,
            position_ms: 0,
            progress: 0.0,
        })
    );

    let idle = playing_second
        .applying(&PlaybackProgress::PreviewStateChanged(PreviewState::Idle))
        .expect("the preview ends");
    assert_eq!(idle.preview, None);
    assert!(
        idle.applying(&PlaybackProgress::PreviewPositionUpdate {
            position_ms: 1_000,
            progress: 0.01,
        })
        .is_none(),
        "a position with no preview loaded is no one's"
    );
}

/// A preview seek moves the position and counts as applied, as a track's
/// does; ticks and another clip starting leave the count where it is, so a
/// bar holding a drop can tell its seek from an earlier one.
#[test]
fn an_applied_preview_seek_moves_the_position_and_is_counted() {
    let playing = PlaybackPositions::initial()
        .applying(&PlaybackProgress::PreviewStateChanged(PreviewState::Playing {
            target: preview_target("/tmp/first.flac"),
            duration_ms: 120_000,
        }))
        .expect("the clip plays");
    assert_eq!(playing.preview_seek_revision, 0);

    let seeked = playing
        .applying(&PlaybackProgress::PreviewSeeked { position_ms: 60_000, progress: 0.5 })
        .expect("a preview seek is folded");
    assert_eq!(seeked.preview_seek_revision, 1);
    let preview = seeked.preview.as_ref().expect("the clip is loaded");
    assert_eq!((preview.position_ms, preview.progress), (60_000, 0.5));
    assert_eq!(seeked.seek_revision, 0, "a preview seek is not a track's");

    let ticked = seeked
        .applying(&PlaybackProgress::PreviewPositionUpdate { position_ms: 61_000, progress: 0.51 })
        .expect("a tick is folded");
    assert_eq!(ticked.preview_seek_revision, 1);

    let next_clip = ticked
        .applying(&PlaybackProgress::PreviewStateChanged(PreviewState::Playing {
            target: preview_target("/tmp/second.flac"),
            duration_ms: 60_000,
        }))
        .expect("another clip plays");
    assert_eq!(next_clip.preview_seek_revision, 1);

    assert!(
        PlaybackPositions::initial()
            .applying(&PlaybackProgress::PreviewSeeked { position_ms: 1, progress: 0.0 })
            .is_none(),
        "a seek with no preview loaded is no one's"
    );
}
