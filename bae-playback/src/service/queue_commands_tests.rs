use super::*;
use crate::service::tests::*;

/// A queue command whose write fails answers the failure to its caller: the
/// queue is as it was, and the caller shows why. Each command's own reply is
/// read here with the queue's writes failing.
async fn answer_with_failing_writes(
    command: impl FnOnce(QueueEditReply<()>) -> PlaybackCommand,
) -> Result<(), QueueEditError> {
    let (store, mut service, _progress_rx) =
        seeded_playback_service(&[(LOADING_RELEASE, &[FINISHED_TRACK, NEXT_TRACK])]).await;
    fill_test_release(&mut service, LOADING_RELEASE).await;
    crate::tables::fail_queue_writes_for_test(store.directory());
    let (reply, answer) = oneshot::channel();
    let _served = service.handle_command(command(reply)).await;
    answer.await.expect("the command answers")
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
