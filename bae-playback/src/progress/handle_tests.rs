use super::*;
use bae_util::test_channels::HANG_GUARD;
use tokio::time::timeout;

/// The fan-out over a fresh library's `playback_now` row, with the
/// sender that feeds it. The `TempDir` holds the library.
async fn started() -> (
    PlaybackProgressHandle,
    tokio_mpsc::UnboundedSender<PlaybackProgress>,
    bae_store::Store,
    tempfile::TempDir,
) {
    let tmp = tempfile::TempDir::new().unwrap();
    let db =
        bae_store::Store::new_test(crate::TEST_SCHEMA, tmp.path(), Arc::new(coven::SystemClock))
            .await
            .unwrap();
    let (progress_tx, progress_rx) = tokio_mpsc::unbounded_channel();
    let handle = PlaybackProgressHandle::new(
        progress_rx,
        tokio::runtime::Handle::current(),
        Arc::new(crate::ManualPlaybackClock::new(chrono::Utc::now())),
        crate::tables::PlaybackNowRow::new(db.clone()),
        crate::tables::PlaybackTimelineRow::new(db.clone()),
        bae_diagnostics::Diagnostics::noop(),
    );
    (handle, progress_tx, db, tmp)
}

#[tokio::test]
async fn forwards_to_live_subscribers_and_prunes_dropped_ones() {
    let (handle, progress_tx, _db, _tmp) = started().await;
    let mut live = handle.subscribe_all();
    let dropped = handle.subscribe_all();
    drop(dropped);

    progress_tx.send(PlaybackProgress::VolumeChanged { volume: 0.5 }).unwrap();

    match timeout(HANG_GUARD, live.recv())
        .await
        .expect("live subscriber receives event")
        .expect("live subscriber channel stays open")
    {
        PlaybackProgress::VolumeChanged { volume } => assert_eq!(volume, 0.5),
        other => panic!("expected volume event, got {other:?}"),
    }

    // The fan-out sends to every subscriber and prunes the closed ones under
    // one lock, so taking the lock after the live subscriber heard the event
    // waits for the prune.
    assert_eq!(handle.subscriptions.lock().unwrap().len(), 1, "dropped subscriber is pruned");
}

/// The row holds an event's change by the time the event is forwarded.
#[tokio::test]
async fn a_change_is_in_the_row_before_its_event_is_forwarded() {
    let (handle, progress_tx, db, _tmp) = started().await;
    let mut events = handle.subscribe_all();

    progress_tx.send(PlaybackProgress::VolumeChanged { volume: 0.25 }).unwrap();
    progress_tx.send(PlaybackProgress::MuteChanged { is_muted: true }).unwrap();
    next_event(&mut events).await;
    next_event(&mut events).await;

    assert_eq!(
        crate::tables::playback_level(&db).await.unwrap(),
        (crate::Volume::new(0.25).unwrap(), true)
    );
}

#[tokio::test]
async fn seek_revision_is_retained_across_later_position_ticks() {
    let (handle, progress_tx, _db, _tmp) = started().await;
    let mut events = handle.subscribe_all();

    progress_tx
        .send(PlaybackProgress::Seeked {
            track_id: "track-1".into(),
            position_ms: 70,
            duration_ms: Some(100),
            progress: 0.7,
        })
        .unwrap();
    progress_tx
        .send(PlaybackProgress::PositionUpdate {
            track_id: "track-1".into(),
            position_ms: 71,
            duration_ms: Some(100),
            progress: 0.71,
        })
        .unwrap();
    next_event(&mut events).await;
    next_event(&mut events).await;

    let positions = handle.subscribe_positions().positions();
    assert_eq!(positions.library.as_ref().map(|position| position.position_ms), Some(71));
    assert_eq!(positions.seek_revision, 1);
}

/// A new line is in the timeline row by the time its event is
/// forwarded; a tick along the line it holds writes nothing.
#[tokio::test]
async fn a_new_line_is_in_the_timeline_row_before_its_event_is_forwarded() {
    let (handle, progress_tx, db, _tmp) = started().await;
    let mut events = handle.subscribe_all();
    let tick = |position_ms| PlaybackProgress::PositionUpdate {
        track_id: "track-1".into(),
        position_ms,
        duration_ms: Some(100_000),
        progress: position_ms as f64 / 100_000.0,
    };
    progress_tx
        .send(PlaybackProgress::StateChanged {
            state: crate::PlaybackState::Playing {
                track: crate::PlayingTrack {
                    track_id: "track-1".into(),
                    duration_ms: Some(100_000),
                },
            },
        })
        .unwrap();
    progress_tx.send(tick(10_000)).unwrap();
    progress_tx.send(tick(10_200)).unwrap();
    for _ in 0..3 {
        next_event(&mut events).await;
    }

    let timeline = crate::tables::playback_timeline(&db).await.unwrap();
    assert_eq!(
        timeline.library,
        Some(crate::TimelinePoint {
            track_id: "track-1".into(),
            position_ms: 10_000,
            duration_ms: Some(100_000),
            moving: true,
        }),
        "the tick along the line left the line as told"
    );
}

/// The next event the fan-out forwards, which it writes and folds into
/// the positions first.
async fn next_event(events: &mut PlaybackProgressFeed) -> PlaybackProgress {
    timeout(HANG_GUARD, events.recv())
        .await
        .expect("the fan-out forwards the event")
        .expect("the fan-out stays open")
}

/// A state change reaches the row and leaves the positions; a position
/// tick moves the positions and leaves the row.
#[tokio::test]
async fn a_position_tick_moves_only_the_positions_and_a_state_change_only_the_row() {
    let (handle, progress_tx, db, _tmp) = started().await;
    let mut events = handle.subscribe_all();
    let mut positions = handle.subscribe_positions();
    positions.positions();
    assert_eq!(
        crate::tables::now_playing(&db).await.unwrap().map(|now| now.playback.state),
        Some(crate::PlaybackState::Stopped)
    );

    progress_tx
        .send(PlaybackProgress::StateChanged {
            state: crate::PlaybackState::Playing {
                track: crate::PlayingTrack { track_id: "track-1".into(), duration_ms: Some(100) },
            },
        })
        .unwrap();
    next_event(&mut events).await;
    assert_eq!(
        crate::tables::now_playing(&db).await.unwrap(),
        None,
        "the row names track-1, which the library does not hold"
    );
    assert!(!positions.has_changed().unwrap(), "a state change moves no position");

    progress_tx
        .send(PlaybackProgress::PositionUpdate {
            track_id: "track-1".into(),
            position_ms: 10,
            duration_ms: Some(100),
            progress: 0.1,
        })
        .unwrap();
    next_event(&mut events).await;
    assert!(positions.has_changed().unwrap(), "a position tick sends a position");
    assert_eq!(
        crate::tables::playback_level(&db).await.unwrap(),
        (crate::Volume::new(1.0).unwrap(), false)
    );
}
