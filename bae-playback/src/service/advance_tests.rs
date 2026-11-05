use super::*;
use crate::service::tests::*;

#[tokio::test]
async fn gapless_crossing_evicts_finished_track_file_buffer() {
    let (_home, mut service, _progress_rx) =
        seeded_playback_service(&[(LOADING_RELEASE, &[FINISHED_TRACK, INCOMING_TRACK])]).await;
    fill_test_release(&mut service, LOADING_RELEASE).await;

    let finished_buffer = create_sparse_buffer(1_024);
    let incoming_buffer = create_sparse_buffer(1_024);
    service.file_buffers.cache_for_test("finished-file", finished_buffer.clone());
    service.file_buffers.cache_for_test("incoming-file", incoming_buffer.clone());
    service.slot = active_slot(
        test_prepared_track_with_file(FINISHED_TRACK, "finished-file", finished_buffer.clone()),
        TrackPhase::Playing,
    );
    service.preloaded_next = Some(test_preloaded_next(
        test_prepared_track_with_file(INCOMING_TRACK, "incoming-file", incoming_buffer),
        PreloadedNextSource::Staged,
    ));

    service
        .handle_track_crossed(TrackCrossing {
            finished_fmt: Arc::new(TrackFmt::for_test(FINISHED_TRACK)),
            decode_error_count: 0,
            samples_decoded: 44_100,
            incoming_fmt: Arc::new(TrackFmt::for_test(INCOMING_TRACK)),
        })
        .await;

    assert!(!service.file_buffers.holds("finished-file"));
    assert!(service.file_buffers.holds("incoming-file"));
    assert!(finished_buffer.is_cancelled());
}

#[tokio::test]
async fn gapless_crossing_keeps_file_buffer_used_by_incoming_track() {
    let (_home, mut service, _progress_rx) =
        seeded_playback_service(&[(LOADING_RELEASE, &[FINISHED_TRACK, INCOMING_TRACK])]).await;
    fill_test_release(&mut service, LOADING_RELEASE).await;

    let shared_buffer = create_sparse_buffer(1_024);
    service.file_buffers.cache_for_test("shared-file", shared_buffer.clone());
    service.slot = active_slot(
        test_prepared_track_with_file(FINISHED_TRACK, "shared-file", shared_buffer.clone()),
        TrackPhase::Playing,
    );
    service.preloaded_next = Some(test_preloaded_next(
        test_prepared_track_with_file(INCOMING_TRACK, "shared-file", shared_buffer.clone()),
        PreloadedNextSource::Staged,
    ));

    service
        .handle_track_crossed(TrackCrossing {
            finished_fmt: Arc::new(TrackFmt::for_test(FINISHED_TRACK)),
            decode_error_count: 0,
            samples_decoded: 44_100,
            incoming_fmt: Arc::new(TrackFmt::for_test(INCOMING_TRACK)),
        })
        .await;

    assert!(service.file_buffers.holds("shared-file"));
    assert!(!shared_buffer.is_cancelled());
}

/// A track whose audio crossed into the next one before its `TrackReady` was
/// handled (a load under load: the callback pulls samples as soon as the load
/// starts) was heard, so it reports Playing before the crossing reports the
/// track it crossed into, and the `TrackReady` that follows changes nothing.
#[tokio::test]
async fn a_crossing_before_its_load_resolves_reports_the_finished_track_playing() {
    let (_home, mut service, mut progress_rx) =
        seeded_playback_service(&[(LOADING_RELEASE, &[FINISHED_TRACK, INCOMING_TRACK])]).await;
    fill_test_release(&mut service, LOADING_RELEASE).await;

    let generation = service.next_load_generation();
    let finished_buffer = create_sparse_buffer(1_024);
    let incoming_buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(
        test_prepared_track(FINISHED_TRACK, finished_buffer),
        TrackPhase::Loading { generation, target: PlayTarget::Playing },
    );
    service.preloaded_next = Some(test_preloaded_next(
        test_prepared_track(INCOMING_TRACK, incoming_buffer),
        PreloadedNextSource::Staged,
    ));

    service
        .handle_track_crossed(TrackCrossing {
            finished_fmt: Arc::new(TrackFmt::for_test(FINISHED_TRACK)),
            decode_error_count: 0,
            samples_decoded: 44_100,
            incoming_fmt: Arc::new(TrackFmt::for_test(INCOMING_TRACK)),
        })
        .await;

    let states = drained_states(&mut progress_rx);
    assert!(
        matches!(
            states.as_slice(),
            [
                PlaybackState::Playing { track: finished },
                PlaybackState::Playing { track: incoming },
            ] if finished.track_id == FINISHED_TRACK && incoming.track_id == INCOMING_TRACK
        ),
        "the finished track plays, then the one it crossed into, got {states:?}"
    );

    service.resolve_track_ready(FINISHED_TRACK.to_string(), generation).await;
    assert!(
        drained_states(&mut progress_rx).is_empty(),
        "the load's TrackReady, handled after the crossing, reports nothing"
    );
}

/// A side-pause phase resuming into `next`, with `resumes_at` as its countdown.
fn side_paused_slot(resumes_at: Option<chrono::DateTime<chrono::Utc>>) -> PlaybackSlot {
    side_paused_between("t", "next", resumes_at)
}

#[tokio::test]
async fn cancelling_a_running_countdown_keeps_the_pause_and_announces_it() {
    let (_home, mut service, mut progress_rx) = test_playback_service().await;
    service.slot = side_paused_slot(Some(countdown_deadline()));

    service.cancel_side_pause_countdown();

    assert_eq!(service.side_pause_countdown_deadline(), None);
    assert!(service.is_side_paused(), "the pause itself stays");
    let states = drained_states(&mut progress_rx);
    assert!(
        matches!(
            states.as_slice(),
            [PlaybackState::Paused {
                reason: PlaybackPauseReason::SideEnded(prompt),
                ..
            }] if prompt.countdown.is_none()
        ),
        "one side-pause state without a countdown, got {states:?}"
    );
}

#[tokio::test]
async fn cancelling_with_no_countdown_running_changes_nothing() {
    let (_home, mut service, mut progress_rx) = test_playback_service().await;
    service.slot = side_paused_slot(None);

    service.cancel_side_pause_countdown();

    assert!(service.is_side_paused());
    assert!(drained_states(&mut progress_rx).is_empty());
}

#[tokio::test]
async fn a_countdown_that_is_no_longer_running_does_not_resume() {
    let (_home, mut service, mut progress_rx) = test_playback_service().await;
    service.slot = side_paused_slot(None);

    service.side_pause_countdown_elapsed(countdown_deadline()).await;

    assert!(service.is_side_paused(), "a cancelled countdown leaves the pause");
    assert!(drained_states(&mut progress_rx).is_empty());
}
