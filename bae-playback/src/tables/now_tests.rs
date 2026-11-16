//! The `playback_now` row: what each change writes, what the screens' read
//! makes of it, and what clears a failure.

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
    now_playing(db).await.unwrap()
}

/// The next value `values` answers.
async fn next_value(
    values: &mut coven::LiveQuery<crate::NowPlayingValues, DbError>,
) -> crate::NowPlayingValues {
    values.next().await.expect("the read runs").unwrap()
}

/// The title `state` shows for its track, `None` for a removed track, or a
/// panic when it names none.
fn shown_title(
    state: &PlaybackState<crate::NowPlayingTrack, crate::NowPlayingTrack<String>>,
) -> Option<String> {
    let display = match state {
        PlaybackState::Playing { track } | PlaybackState::Paused { track, .. } => &track.display,
        PlaybackState::Loading { track: LoadingTrack::Prepared(track), .. } => &track.display,
        PlaybackState::Loading { track: LoadingTrack::Unprepared(track), .. } => &track.display,
        PlaybackState::Stopped => panic!("a stopped state shows no track"),
    };
    match display {
        NowPlayingDisplay::Library(display) => Some(display.title.clone()),
        NowPlayingDisplay::Removed => None,
    }
}

/// One statement reads what playback is doing, the playing track's display
/// and the queue's repeat mode, and joins the track's display to the row.
#[tokio::test]
async fn now_playing_is_read_in_one_statement() {
    let (db, _tmp, tracks) = release_db(1).await;
    let row = PlaybackNowRow::new(db.clone());
    let track = PlayingTrack { track_id: tracks[0].clone(), duration_ms: Some(1_000) };
    row.apply(&PlaybackChange::State(PlaybackState::Playing { track })).await.unwrap();
    let log = db.record_statements_for_test().await.unwrap();
    assert!(matches!(now(&db).await.playback.state, PlaybackState::Playing { .. }));
    assert_eq!(log.take().len(), 1, "one statement reads the row and the track's display");
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
        PlaybackState::Loading {
            track: LoadingTrack::Unprepared(tracks[0].clone()),
            waits_on_storage: false,
        },
        PlaybackState::Loading {
            track: LoadingTrack::Prepared(track.clone()),
            waits_on_storage: true,
        },
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
        if read != PlaybackState::Stopped {
            assert_eq!(shown_title(&read).as_deref(), Some("Track Title 0"), "{state:?}");
        }
        let shown = read.clone().map_tracks(|shown| shown.track, |shown| shown.track);
        assert_eq!(shown, state, "read back as {read:?}");
    }

    // Each kind reads back as the one written.
    let device = |kind| CastReceiver {
        id: "speaker-1".to_string(),
        name: "Kitchen".to_string(),
        kind,
        model: Some("Audio Pro".to_string()),
        address: "192.168.1.9".to_string(),
        takes_volume: true,
    };
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
        assert_eq!(now(&db).await.failure, Some(reason));
    }
}

/// A failure shows until the person dismisses it, and a failure after the
/// dismissal shows again, even one with the same reason as the dismissed one.
#[tokio::test]
async fn a_dismissed_failure_clears_and_the_next_one_shows() {
    let (db, _tmp, _tracks) = release_db(1).await;
    let row = PlaybackNowRow::new(db.clone());
    let failed = PlaybackChange::Failed(PlaybackErrorReason::OriginalMissing);

    row.apply(&failed).await.unwrap();
    assert_eq!(now(&db).await.failure, Some(PlaybackErrorReason::OriginalMissing));

    dismiss_playback_failure(&db).await.unwrap();
    assert_eq!(now(&db).await.failure, None, "dismissing clears the failure");

    row.apply(&failed).await.unwrap();
    assert_eq!(
        now(&db).await.failure,
        Some(PlaybackErrorReason::OriginalMissing),
        "the next failure shows"
    );
}

/// A later failure replaces the one the row holds. A pause leaves the failure
/// in the row, and playing again clears it.
#[tokio::test]
async fn a_later_failure_replaces_the_last_and_playing_clears_it() {
    let (db, _tmp, tracks) = release_db(1).await;
    let row = PlaybackNowRow::new(db.clone());

    row.apply(&PlaybackChange::Failed(PlaybackErrorReason::OriginalMissing)).await.unwrap();
    row.apply(&PlaybackChange::Failed(PlaybackErrorReason::SyncDisconnected)).await.unwrap();
    assert_eq!(now(&db).await.failure, Some(PlaybackErrorReason::SyncDisconnected));

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
}

/// Deleting the playing track changes the value at once: the row names the
/// track until playback moves off it, and the value shows it removed.
#[tokio::test]
async fn a_deleted_playing_track_shows_as_removed_at_once() {
    let (db, _tmp, tracks) = release_db(1).await;
    let row = PlaybackNowRow::new(db.clone());
    let playing = PlaybackState::Playing {
        track: PlayingTrack { track_id: tracks[0].clone(), duration_ms: None },
    };
    row.apply(&PlaybackChange::State(playing.clone())).await.unwrap();
    let mut values = subscribe_now_playing(&db);
    let first = next_value(&mut values).await.playback.state;
    assert_eq!(shown_title(&first).as_deref(), Some("Track Title 0"));

    delete_release_of(&db, &tracks[0]).await;
    let removed = next_value(&mut values).await.playback.state;
    assert_eq!(shown_title(&removed), None, "the next value shows the track removed");
    assert_eq!(removed.map_tracks(|shown| shown.track, |shown| shown.track), playing);

    row.apply(&PlaybackChange::State(PlaybackState::Stopped)).await.unwrap();
    assert_eq!(next_value(&mut values).await.playback.state, PlaybackState::Stopped);
}

/// A load shows the track it loads from the start: the value that names the
/// next track, before playback has prepared it, carries its display, so a
/// track change has no value without one.
#[tokio::test]
async fn a_track_change_shows_the_next_track_before_it_is_prepared() {
    let (db, _tmp, tracks) = release_db(2).await;
    let row = PlaybackNowRow::new(db.clone());
    row.apply(&PlaybackChange::State(PlaybackState::Playing {
        track: PlayingTrack { track_id: tracks[0].clone(), duration_ms: Some(1_000) },
    }))
    .await
    .unwrap();
    let mut values = subscribe_now_playing(&db);
    let first = next_value(&mut values).await.playback.state;
    assert_eq!(shown_title(&first).as_deref(), Some("Track Title 0"));

    row.apply(&PlaybackChange::State(PlaybackState::Loading {
        track: LoadingTrack::Unprepared(tracks[1].clone()),
        waits_on_storage: false,
    }))
    .await
    .unwrap();
    let loading = next_value(&mut values).await.playback.state;
    assert!(
        matches!(&loading, PlaybackState::Loading { track: LoadingTrack::Unprepared(track), .. }
            if track.track == tracks[1]),
        "the next value loads the next track: {loading:?}"
    );
    assert_eq!(shown_title(&loading).as_deref(), Some("Track Title 1"));
}

/// A load's wait on storage is in the row while it lasts: the load that
/// starts waiting writes it, the load that stops waiting clears it, and the
/// track playing leaves no wait.
#[tokio::test]
async fn the_row_holds_a_loads_wait_on_storage_while_it_lasts() {
    let (db, _tmp, tracks) = release_db(1).await;
    let row = PlaybackNowRow::new(db.clone());
    let track = PlayingTrack { track_id: tracks[0].clone(), duration_ms: Some(1_000) };
    let loading = |waits_on_storage| {
        PlaybackChange::State(PlaybackState::Loading {
            track: LoadingTrack::Prepared(track.clone()),
            waits_on_storage,
        })
    };
    let waits = |state: PlaybackState<crate::NowPlayingTrack, crate::NowPlayingTrack<String>>| match state {
        PlaybackState::Loading { waits_on_storage, .. } => Some(waits_on_storage),
        _ => None,
    };

    row.apply(&loading(false)).await.unwrap();
    assert_eq!(waits(now(&db).await.playback.state), Some(false));
    row.apply(&loading(true)).await.unwrap();
    assert_eq!(waits(now(&db).await.playback.state), Some(true), "the wait started");
    row.apply(&loading(false)).await.unwrap();
    assert_eq!(waits(now(&db).await.playback.state), Some(false), "the wait ended");

    row.apply(&loading(true)).await.unwrap();
    row.apply(&PlaybackChange::State(PlaybackState::Playing { track: track.clone() }))
        .await
        .unwrap();
    assert_eq!(waits(now(&db).await.playback.state), None, "a playing track waits on nothing");
    let column: Option<bool> = db
        .read(|sql| {
            Ok(sql.query_row("SELECT waits_on_storage FROM playback_now", [], |row| row.get(0))?)
        })
        .await
        .unwrap();
    assert_eq!(column, None, "playing clears the wait");
}

/// The schema refuses a wait on storage unless playback is loading a track.
#[tokio::test]
async fn the_schema_holds_a_wait_on_storage_only_while_loading() {
    let (db, _tmp, tracks) = release_db(1).await;
    let track_id = tracks[0].clone();
    for sql in [
        format!(
            "UPDATE playback_now SET state = 'playing', track_id = '{track_id}', \
                 waits_on_storage = 0"
        ),
        format!(
            "UPDATE playback_now SET state = 'loading', track_id = '{track_id}', prepared = 1, \
                 waits_on_storage = NULL"
        ),
        "UPDATE playback_now SET waits_on_storage = 1".to_string(),
    ] {
        let statement = sql.clone();
        let refused = db
            .call(move |conn| conn.execute_batch(&statement).map_err(DbError::from))
            .await
            .is_err();
        assert!(refused, "{sql}");
    }
}
