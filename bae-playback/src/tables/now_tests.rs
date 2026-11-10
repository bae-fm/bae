//! The `playback_now` row: what each change writes, what the screens' read
//! makes of it, and how failures count.

use super::*;
use crate::renderer::RendererKind;
use crate::tables::tests::*;
use crate::{AudioOutputProblem, PlaybackErrorReason};
use crate::{
    LoadingTrack, PlaybackChange, PlaybackPauseBoundary, PlaybackPauseReason,
    PlaybackSideCountdown, PlaybackSidePausePrompt, PlaybackState, PlayingTrack, PreviewState,
    RemotePlayback,
};
use bae_track_audio::PreviewTarget;

async fn now(db: &Store) -> crate::NowPlayingValues {
    now_playing(db).await.unwrap().expect("the library holds the track the row names")
}

/// Each change reads back as it was written, the playing track joined with
/// its display.
#[tokio::test]
async fn each_change_reads_back_as_written() {
    let (db, _tmp, tracks) = release_db(1).await;
    let row = PlaybackNowRow::new(db.clone());
    let track = PlayingTrack { track_id: tracks[0].clone(), duration_ms: Some(1_000) };
    let unknown_length = PlayingTrack { duration_ms: None, ..track.clone() };
    let resumes_at = chrono::DateTime::from_timestamp_millis(1_800_000_000_000).unwrap();
    for state in [
        PlaybackState::Loading { track: LoadingTrack::Unprepared { track_id: tracks[0].clone() } },
        PlaybackState::Loading { track: LoadingTrack::Prepared(track.clone()) },
        PlaybackState::Playing { track: unknown_length },
        PlaybackState::Paused { track: track.clone(), reason: PlaybackPauseReason::Manual },
        PlaybackState::Paused {
            track: track.clone(),
            reason: PlaybackPauseReason::SideEnded(PlaybackSidePausePrompt {
                id: "side-a".to_string(),
                boundary: PlaybackPauseBoundary::Disc,
                side_label: "Disc 1".to_string(),
                countdown: Some(PlaybackSideCountdown { resumes_at }),
            }),
        },
        PlaybackState::Stopped,
    ] {
        row.apply(&PlaybackChange::State(state.clone())).await.unwrap();
        let read = now(&db).await.playback.state;
        let shown = read.clone().try_map_track(|shown| {
            assert_eq!(shown.display.title, "Track Title 0", "{state:?}");
            Ok::<_, ()>(shown.track)
        });
        assert_eq!(shown, Ok(state), "read back as {read:?}");
    }

    // Each kind reads back as the one written.
    let device =
        |kind| CastReceiver { id: "speaker-1".to_string(), name: "Kitchen".to_string(), kind };
    for remote in [
        RemotePlayback::On(device(RendererKind::Cast)),
        RemotePlayback::On(device(RendererKind::Dlna)),
        RemotePlayback::On(device(RendererKind::AirPlay)),
        RemotePlayback::Ended {
            device: device(RendererKind::AirPlay),
            detail: "the receiver stopped".to_string(),
        },
        RemotePlayback::Local,
    ] {
        row.apply(&PlaybackChange::Remote(remote.clone())).await.unwrap();
        assert_eq!(now(&db).await.playback.remote, remote);
    }

    let target = PreviewTarget::sample_range("/music/clip.flac".into(), 44_100, Some(88_200));
    for preview in [
        PreviewState::Playing { target: target.clone(), duration_ms: 1_000 },
        PreviewState::Paused {
            target: PreviewTarget::whole_file("/music/whole.flac".into()),
            duration_ms: 2_000,
        },
        PreviewState::Idle,
    ] {
        row.apply(&PlaybackChange::Preview(preview.clone())).await.unwrap();
        assert_eq!(now(&db).await.playback.preview, preview);
    }

    row.apply(&PlaybackChange::Volume(0.25)).await.unwrap();
    row.apply(&PlaybackChange::Muted(true)).await.unwrap();
    let read = now(&db).await;
    assert_eq!((read.playback.volume, read.playback.is_muted), (0.25, true));

    for reason in [
        PlaybackErrorReason::SyncDisconnected,
        PlaybackErrorReason::OnOtherDevice(bae_records::OtherDevice::Active {
            name: "Device A".to_string(),
        }),
        PlaybackErrorReason::OnOtherDevice(bae_records::OtherDevice::Former {
            name: "Device B".to_string(),
            state: bae_store::FormerDeviceState::Removed,
        }),
        PlaybackErrorReason::OnOtherDevice(bae_records::OtherDevice::Former {
            name: "Device C".to_string(),
            state: bae_store::FormerDeviceState::Replaced,
        }),
        PlaybackErrorReason::OriginalMissing,
        PlaybackErrorReason::AudioOutput { problem: AudioOutputProblem::DeviceUnavailable },
        PlaybackErrorReason::AudioOutput { problem: AudioOutputProblem::UnsupportedFormat },
        PlaybackErrorReason::AudioOutput { problem: AudioOutputProblem::AirPlayRefused },
        PlaybackErrorReason::internal("decode failed"),
        PlaybackErrorReason::renderer("LOAD refused"),
    ] {
        row.apply(&PlaybackChange::Failed(reason.clone())).await.unwrap();
        assert_eq!(now(&db).await.failure.map(|failure| failure.reason), Some(reason));
    }
}

/// Each failure counts up, so two alike are two values; playing again
/// clears the failure and keeps the count.
#[tokio::test]
async fn failures_count_up_and_playing_clears_the_last_one() {
    let (db, _tmp, tracks) = release_db(1).await;
    let row = PlaybackNowRow::new(db.clone());
    let failed = PlaybackChange::Failed(PlaybackErrorReason::OriginalMissing);

    row.apply(&failed).await.unwrap();
    let first = now(&db).await.failure.expect("a failure");
    row.apply(&failed).await.unwrap();
    let second = now(&db).await.failure.expect("a failure");
    assert_eq!((first.number, second.number), (1, 2));
    assert_eq!(first.reason, second.reason);

    row.apply(&PlaybackChange::State(PlaybackState::Paused {
        track: PlayingTrack { track_id: tracks[0].clone(), duration_ms: None },
        reason: PlaybackPauseReason::Manual,
    }))
    .await
    .unwrap();
    assert!(now(&db).await.failure.is_some(), "a pause leaves the failure");

    row.apply(&PlaybackChange::State(PlaybackState::Playing {
        track: PlayingTrack { track_id: tracks[0].clone(), duration_ms: None },
    }))
    .await
    .unwrap();
    assert_eq!(now(&db).await.failure, None, "playing clears it");

    row.apply(&failed).await.unwrap();
    assert_eq!(now(&db).await.failure.map(|failure| failure.number), Some(3));
}

/// A row naming a track the library no longer holds reads as nothing to
/// show, until playback moves off the track.
#[tokio::test]
async fn a_deleted_playing_track_reads_as_nothing_to_show() {
    let (db, _tmp, tracks) = release_db(1).await;
    let row = PlaybackNowRow::new(db.clone());
    row.apply(&PlaybackChange::State(PlaybackState::Playing {
        track: PlayingTrack { track_id: tracks[0].clone(), duration_ms: None },
    }))
    .await
    .unwrap();

    delete_release_of(&db, &tracks[0]).await;
    assert_eq!(now_playing(&db).await.unwrap(), None);

    row.apply(&PlaybackChange::State(PlaybackState::Stopped)).await.unwrap();
    assert_eq!(now(&db).await.playback.state, PlaybackState::Stopped);
}
