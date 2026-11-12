//! How commands are classified: the telemetry kind a user's command ships,
//! and which commands stop the side-pause countdown.

use super::*;

/// A user-intent command maps to its telemetry kind; internal/system commands,
/// queries, and continuous inputs map to `None` and ship nothing.
#[test]
fn playback_command_kind_maps_user_intent_only() {
    assert!(matches!(
        playback_command_kind(&PlaybackCommand::Play(
            "08c80007-b56a-4fc9-8df6-af2967fa09b9".to_string()
        )),
        Some(PlaybackCommandKind::Play)
    ));
    assert!(matches!(
        playback_command_kind(&PlaybackCommand::SetRepeatMode(
            RepeatMode::Off,
            oneshot::channel().0
        )),
        Some(PlaybackCommandKind::SetRepeat)
    ));
    // A continuous input and an internal command ship nothing.
    assert!(playback_command_kind(&PlaybackCommand::SetVolume(crate::Volume::new(0.5).unwrap()))
        .is_none());
    assert!(playback_command_kind(&PlaybackCommand::AutoAdvance {
        track_id: "08c80007-b56a-4fc9-8df6-af2967fa09b9".to_string()
    })
    .is_none());
}

#[test]
fn commands_that_steer_playback_stop_the_countdown_and_the_rest_leave_it() {
    let stops = [
        PlaybackCommand::Pause,
        PlaybackCommand::Stop,
        PlaybackCommand::Next,
        PlaybackCommand::Previous,
        PlaybackCommand::Seek(TrackTime::START),
        PlaybackCommand::SeekByRatio(0.5),
        PlaybackCommand::Play("track".to_string()),
        PlaybackCommand::PlayLibraryShuffled(oneshot::channel().0),
        PlaybackCommand::AddToQueue(vec!["track".to_string()], oneshot::channel().0),
        PlaybackCommand::ClearUpNext(oneshot::channel().0),
        PlaybackCommand::SetShuffle(true, oneshot::channel().0),
        PlaybackCommand::StopRemote,
    ];
    for command in &stops {
        assert!(cancels_side_pause_countdown(command), "{command:?} stops the countdown");
    }
    let leaves = [
        PlaybackCommand::Resume,
        PlaybackCommand::SetVolume(crate::Volume::new(0.5).unwrap()),
        PlaybackCommand::SetMuted(true),
        PlaybackCommand::SetRepeatMode(RepeatMode::Context, oneshot::channel().0),
        PlaybackCommand::AutoAdvance { track_id: "track".to_string() },
    ];
    for command in &leaves {
        assert!(!cancels_side_pause_countdown(command), "{command:?} leaves the countdown running");
    }
}
