use crate::test_playback_behavior::fixtures::*;
use crate::test_playback_behavior::support::*;

/// `SetShuffle` permutes and unpermutes the context lane the queue already
/// holds — nothing is re-fetched — and the track that is playing keeps playing
/// through both toggles.
#[tokio::test]
async fn set_shuffle_permutes_and_unpermutes_the_context_lane_in_place() {
    with_fixture(PlaybackTestFixture::new(), async |fixture| {
        let first = fixture.track_ids[0].clone();
        let second = fixture.track_ids[1].clone();
        let third = fixture.track_ids[2].clone();
        fixture.playback_handle.play(first.clone());
        fixture
            .wait_for_state(
                |s| matches!(s, PlaybackState::Playing { track, .. } if track.track_id == first),
            )
            .await
            .expect("the release starts playing");

        // Shuffle on re-materializes the playing context into a shuffled order —
        // not merely a persisted seed.
        fixture.playback_handle.set_shuffle(true).await.expect("set_shuffle succeeds");
        let shuffled = fixture.playback_handle.queue_value().await.expect("queue value");
        assert!(
            shuffled.shuffled && shuffled.context.is_some(),
            "SetShuffle(true) puts the lane in shuffled order"
        );

        // Shuffle off unpermutes the lane: the not-yet-played tail is the two tracks
        // after the current one, in release order.
        fixture.playback_handle.set_shuffle(false).await.expect("set_shuffle succeeds");
        let sequential = fixture.playback_handle.queue_value().await.expect("queue value");
        sequential.context.as_ref().expect("still a playing context");
        assert!(!sequential.shuffled, "SetShuffle(false) puts the lane back in sequential order");
        let upcoming: Vec<String> = fixture
            .playback_handle
            .queue_upcoming()
            .await
            .expect("queue upcoming")
            .iter()
            .map(|e| e.track_id.clone())
            .collect();
        assert_eq!(
            upcoming,
            vec![second, third],
            "sequential upcoming is source order after the current track"
        );

        // The lane projects correctly, but that alone doesn't prove the
        // currently-playing track's audio survived the churn.
        assert_position_advances(
            &fixture.playback_handle,
            &fixture.audio_clock,
            &mut fixture.progress_rx,
        )
        .await;
    })
    .await;
}

/// Shuffling changes what plays next, so the preloaded next track has to follow
/// the reshuffled queue front. The preload is not an optimization the queue can
/// disagree with: `Next` and the natural advance both play the preloaded track
/// in preference to asking the queue, while advancing the cursor past the row
/// the queue calls the front. A shuffle that emits its new order without
/// reconciling the preload therefore plays the pre-shuffle track over a lane
/// whose cursor has moved somewhere else — the speakers and the queue the UI
/// renders disagree about what is playing.
#[tokio::test]
async fn shuffling_makes_the_next_track_follow_the_reshuffled_queue_front() {
    with_fixture(PlaybackTestFixture::new(), async |fixture| {
    let first = fixture.track_ids[0].clone();
    let sequential_next = fixture.track_ids[1].clone();
    fixture.playback_handle.play(first.clone());
    fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { track, .. } if track.track_id == first),
        )
        .await
        .expect("the release starts playing");
    // Playing the release preloaded its sequential next track; that is the stale
    // preload a shuffle has to displace.

    // Each SetShuffle mints its own seed, so a permutation of the two-track tail
    // leaves the front alone about half the time. Reshuffle until it moves —
    // only then is a stale preload distinguishable from a reconciled one — and
    // read the new front off the projection, the same way a UI learns it.
    let mut shuffled_next = None;
    for _ in 0..32 {
        fixture.playback_handle.set_shuffle(true).await.expect("set_shuffle succeeds");
        let front = fixture
            .playback_handle
            .queue_upcoming()
            .await
            .expect("queue upcoming")
            .first()
            .expect("the shuffled lane still has an upcoming track")
            .track_id
            .clone();
        if front != sequential_next {
            shuffled_next = Some(front);
            break;
        }
        fixture.playback_handle.set_shuffle(false).await.expect("set_shuffle succeeds");
    }
    let shuffled_next =
        shuffled_next.expect("a shuffle that moves a different track to the lane's front");

    fixture.playback_handle.next();

    fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { track, .. } if track.track_id == shuffled_next),
        )
        .await
        .expect(
            "the track after a shuffle is the lane's new front, not the one preloaded before it",
        );
    })
    .await;
}

#[tokio::test]
async fn play_library_shuffled_plays_a_shuffled_library_context() {
    with_fixture(PlaybackTestFixture::new(), async |fixture| {
        let outcome =
            fixture.playback_handle.play_library_shuffled().await.expect("the play is answered");
        assert_eq!(outcome, bae_playback::PlayOutcome::Playing);
        fixture
            .wait_for_state(|s| matches!(s, PlaybackState::Playing { .. }))
            .await
            .expect("a library track starts playing");
        let proj = fixture.playback_handle.queue_value().await.expect("queue value");
        let ctx = proj.context.as_ref().expect("a library context");
        assert!(
            matches!(ctx.source, bae_playback::ContextSource::Library),
            "PlayLibraryShuffled plays from the whole library"
        );
        assert!(proj.shuffled, "the library context is shuffled");
    })
    .await;
}

/// A shuffled list plays the tracks the library holds, tells its caller which
/// it lacks, and leaves shuffle on.
#[tokio::test]
async fn play_tracks_shuffled_leaves_shuffle_on() {
    with_fixture(PlaybackTestFixture::new(), async |fixture| {
        let missing = "6f9a1c2e-0b3d-4e5f-8a7b-9c0d1e2f3a4b".to_string();
        let mut list = fixture.track_ids[..3].to_vec();
        list.push(missing.clone());

        let played = fixture
            .playback_handle
            .play_tracks(list, bae_playback::tables::TracksStart::Shuffled)
            .await
            .expect("the tracks play");

        assert_eq!(played, bae_playback::PlayOutcome::PlayingWithoutTracks(vec![missing]));
        let value = fixture.playback_handle.queue_value().await.expect("queue value");
        assert!(value.shuffled);
        assert_eq!(
            value.context.expect("a list context").source,
            bae_playback::ContextSource::Tracks
        );
    })
    .await;
}

/// An empty library plays nothing, and the caller is told there is nothing to
/// play.
#[tokio::test]
async fn play_library_shuffled_is_a_no_op_on_an_empty_library() {
    let (library, _temp) = empty_test_library().await;
    let (handle, _capture_rx, _audio_clock) = start_capture_service(&library).await;
    let mut progress = handle.subscribe_progress();
    let outcome = handle.play_library_shuffled().await.expect("the play is answered");
    assert_eq!(outcome, bae_playback::PlayOutcome::NothingToPlay);
    let events = settled_events_on(&handle, &mut progress).await;
    assert!(
        !states_in(&events).iter().any(|s| matches!(s, PlaybackState::Playing { .. })),
        "an empty library has nothing to shuffle-play: {events:?}"
    );
    handle.stop();
}
