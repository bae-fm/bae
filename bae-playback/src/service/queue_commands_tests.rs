use super::*;
use crate::service::tests::*;

/// A queue command whose write fails answers the failure to its caller: the
/// queue is as it was, and the caller shows why. Each command's own reply is
/// read here with the queue's writes failing.
async fn answer_with_failing_writes<T: Send + 'static>(
    command: impl FnOnce(QueueEditReply<T>) -> PlaybackCommand,
) -> Result<T, QueueEditError> {
    let (store, mut service, _progress_rx) =
        seeded_playback_service(&[(LOADING_RELEASE, &[FINISHED_TRACK, NEXT_TRACK])]).await;
    fill_test_release(&mut service, LOADING_RELEASE).await;
    crate::tables::fail_queue_writes_for_test(store.directory());
    let (reply, answer) = oneshot::channel();
    let _served = service.handle_command(command(reply)).await;
    answer.await.expect("the command answers")
}

/// Each add answers how many tracks it put in, so the screen that asked
/// confirms exactly those: a track the library lacks is left out.
#[tokio::test]
async fn each_add_answers_how_many_tracks_went_in() {
    let (_store, mut service, _progress_rx) =
        seeded_playback_service(&[(LOADING_RELEASE, &[FINISHED_TRACK, NEXT_TRACK])]).await;
    fill_test_release(&mut service, LOADING_RELEASE).await;
    let missing = bae_util::test_ids::test_uuid("missing-track");
    let tracks = vec![FINISHED_TRACK.to_string(), missing, NEXT_TRACK.to_string()];

    let (reply, answer) = oneshot::channel();
    let _served = service.handle_command(PlaybackCommand::AddToQueue(tracks.clone(), reply)).await;
    assert_eq!(answer.await.expect("the command answers").unwrap(), 2);

    let (reply, answer) = oneshot::channel();
    let _served = service.handle_command(PlaybackCommand::AddNext(tracks.clone(), reply)).await;
    assert_eq!(answer.await.expect("the command answers").unwrap(), 2);

    let (reply, answer) = oneshot::channel();
    let _served = service.handle_command(PlaybackCommand::InsertInQueue(tracks, 1, reply)).await;
    assert_eq!(answer.await.expect("the command answers").unwrap(), 2);

    let (reply, answer) = oneshot::channel();
    let _served = service
        .handle_command(PlaybackCommand::AddReleaseToQueue(LOADING_RELEASE.to_string(), reply))
        .await;
    assert_eq!(answer.await.expect("the command answers").unwrap(), 2);

    let (reply, answer) = oneshot::channel();
    let _served = service
        .handle_command(PlaybackCommand::AddToQueue(vec!["not-a-track".to_string()], reply))
        .await;
    assert_eq!(answer.await.expect("the command answers").unwrap(), 0);
}

#[tokio::test]
async fn an_add_to_the_queue_that_fails_answers_the_failure() {
    let answer = answer_with_failing_writes(|reply| {
        PlaybackCommand::AddToQueue(vec![NEXT_TRACK.to_string()], reply)
    })
    .await;
    assert!(matches!(answer, Err(QueueEditError::Queue(_))), "{answer:?}");
}

#[tokio::test]
async fn an_add_next_that_fails_answers_the_failure() {
    let answer = answer_with_failing_writes(|reply| {
        PlaybackCommand::AddNext(vec![NEXT_TRACK.to_string()], reply)
    })
    .await;
    assert!(matches!(answer, Err(QueueEditError::Queue(_))), "{answer:?}");
}

#[tokio::test]
async fn an_add_of_a_release_that_fails_answers_the_failure() {
    let answer = answer_with_failing_writes(|reply| {
        PlaybackCommand::AddReleaseToQueue(LOADING_RELEASE.to_string(), reply)
    })
    .await;
    assert!(matches!(answer, Err(QueueEditError::Queue(_))), "{answer:?}");
    let answer = answer_with_failing_writes(|reply| {
        PlaybackCommand::AddReleaseNext(LOADING_RELEASE.to_string(), reply)
    })
    .await;
    assert!(matches!(answer, Err(QueueEditError::Queue(_))), "{answer:?}");
}

#[tokio::test]
async fn an_insert_that_fails_answers_the_failure() {
    let answer = answer_with_failing_writes(|reply| {
        PlaybackCommand::InsertInQueue(vec![NEXT_TRACK.to_string()], 0, reply)
    })
    .await;
    assert!(matches!(answer, Err(QueueEditError::Queue(_))), "{answer:?}");
}

#[tokio::test]
async fn a_repeat_mode_that_fails_to_set_answers_the_failure() {
    let answer = answer_with_failing_writes(|reply| {
        PlaybackCommand::SetRepeatMode(RepeatMode::Track, reply)
    })
    .await;
    assert!(matches!(answer, Err(QueueEditError::Queue(_))), "{answer:?}");
}

#[tokio::test]
async fn a_shuffle_that_fails_answers_the_failure() {
    let answer = answer_with_failing_writes(|reply| PlaybackCommand::SetShuffle(true, reply)).await;
    assert!(matches!(answer, Err(QueueEditError::Queue(_))), "{answer:?}");
}

/// A queue that can't say what plays next stops playback with an error the
/// person sees, not silently.
#[tokio::test]
async fn a_failed_read_of_the_next_entry_sends_a_playback_error() {
    let (store, mut service, mut progress_rx) =
        seeded_playback_service(&[(LOADING_RELEASE, &[FINISHED_TRACK, NEXT_TRACK])]).await;
    fill_test_release(&mut service, LOADING_RELEASE).await;
    crate::tables::fail_queue_writes_for_test(store.directory());

    let next = service.next_entry().await;

    assert!(matches!(next, NextEntry::Stop));
    assert!(drained_a_playback_error(&mut progress_rx), "the failure reaches the person");
}

/// A queue that can't step back leaves the current track as it was, and tells
/// the person rather than doing nothing.
#[tokio::test]
async fn a_failed_step_back_sends_a_playback_error() {
    let (store, mut service, mut progress_rx) =
        seeded_playback_service(&[(LOADING_RELEASE, &[FINISHED_TRACK, NEXT_TRACK])]).await;
    // Playing the second track, so Previous has a track to step back to.
    service
        .playback_queue
        .play(ContextFill::Release {
            release_id: LOADING_RELEASE.to_string(),
            start: ReleaseStart::Track(NEXT_TRACK.to_string()),
        })
        .await
        .expect("the queue tables take the release");
    service.slot = active_slot(
        test_prepared_track(NEXT_TRACK, create_sparse_buffer(1_024)),
        TrackPhase::Playing,
    );
    crate::tables::fail_queue_writes_for_test(store.directory());

    service.handle_previous().await;

    assert!(drained_a_playback_error(&mut progress_rx), "the failure reaches the person");
}

/// The playing track was deleted and the queue can't move past it: nothing
/// is known to follow, so the person is told.
#[tokio::test]
async fn a_failed_move_past_a_deleted_track_sends_a_playback_error() {
    let (store, mut service, mut progress_rx) =
        seeded_playback_service(&[(LOADING_RELEASE, &[FINISHED_TRACK, NEXT_TRACK])]).await;
    fill_test_release(&mut service, LOADING_RELEASE).await;
    service.slot = active_slot(
        test_prepared_track(FINISHED_TRACK, create_sparse_buffer(1_024)),
        TrackPhase::Playing,
    );
    crate::tables::fail_queue_writes_for_test(store.directory());

    service.handle_held_tracks(vec![FINISHED_TRACK.to_string()], Vec::new()).await;

    assert!(drained_a_playback_error(&mut progress_rx), "the failure reaches the person");
}

/// A move onto the preloaded track that the queue can't record stops
/// playback with an error: the queue no longer says what is playing.
#[tokio::test]
async fn a_failed_move_onto_the_preloaded_track_sends_a_playback_error() {
    let (store, mut service, mut progress_rx) =
        seeded_playback_service(&[(LOADING_RELEASE, &[FINISHED_TRACK, NEXT_TRACK])]).await;
    fill_test_release(&mut service, LOADING_RELEASE).await;
    crate::tables::fail_queue_writes_for_test(store.directory());

    assert!(!service.advance_to_preloaded().await);

    assert!(drained_a_playback_error(&mut progress_rx), "the failure reaches the person");
}

/// A side pause that can't read what follows is dropped, with an error the
/// person sees.
#[tokio::test]
async fn a_failed_read_of_what_follows_a_side_pause_sends_a_playback_error() {
    let (store, mut service, mut progress_rx) =
        seeded_playback_service(&[(LOADING_RELEASE, &[FINISHED_TRACK, NEXT_TRACK])]).await;
    fill_test_release(&mut service, LOADING_RELEASE).await;
    service.slot = side_paused_between(FINISHED_TRACK, NEXT_TRACK, None);
    crate::tables::lose_up_next_for_test(store.directory());

    service.resume_from_side_pause().await;

    assert!(drained_a_playback_error(&mut progress_rx), "the failure reaches the person");
}
