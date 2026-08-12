#[tokio::test]
async fn test_previous_track_navigation() {
    let mut fixture = PlaybackTestFixture::new().await;
    let first_track_id = fixture.track_ids[0].clone();
    let second_track_id = fixture.track_ids[1].clone();
    fixture.playback_handle.play(first_track_id.clone());
    let first_track_state = fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { .. }),
            Duration::from_secs(5),
        )
        .await;
    assert!(
        first_track_state.is_some(),
        "Should be playing first track after play command",
    );
    fixture.playback_handle.next();
    let second_track_state = fixture
        .wait_for_state(
            |s| {
                if let PlaybackState::Playing { track_info, .. } = s {
                    track_info.track_id == second_track_id
                } else {
                    false
                }
            },
            Duration::from_secs(5),
        )
        .await;
    assert!(
        second_track_state.is_some(),
        "Should be playing second track after Next command",
    );
    fixture.playback_handle.seek(Duration::from_secs(1));
    let _position = fixture
        .wait_for_position_update(Duration::from_secs(2))
        .await;
    fixture.playback_handle.previous();
    let previous_track_state = fixture
        .wait_for_state(
            |s| {
                if let PlaybackState::Playing { track_info, .. } = s {
                    track_info.track_id == first_track_id
                } else {
                    false
                }
            },
            Duration::from_secs(5),
        )
        .await;
    assert!(
        previous_track_state.is_some(),
        "Should go to previous track when Previous is called early in track",
    );
    fixture.playback_handle.seek(Duration::from_secs(4));
    let _position = fixture
        .wait_for_position_update(Duration::from_secs(2))
        .await;
    fixture.playback_handle.previous();
    let restart_state = fixture
        .wait_for_state(
            |s| {
                if let PlaybackState::Playing { track_info, .. } = s {
                    track_info.track_id == first_track_id
                } else {
                    false
                }
            },
            Duration::from_secs(5),
        )
        .await;
    assert!(
        restart_state.is_some(),
        "Should restart current track when Previous is called late in track",
    );
    let restart_position = fixture
        .wait_for_position_update(Duration::from_secs(2))
        .await
        .expect("Position update after restart");
    assert!(
        Duration::from_millis(restart_position) < Duration::from_secs(1),
        "Restart should reset position near 0, got {restart_position}ms",
    );
}
#[tokio::test]
async fn test_same_position_seek_keeps_position_updates_flowing() {
    let mut fixture = PlaybackTestFixture::new().await;
    let track_id = &fixture.track_ids[0];
    fixture.playback_handle.play(track_id.clone());
    let playing_state = fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { .. }),
            Duration::from_secs(5),
        )
        .await;
    assert!(
        playing_state.is_some(),
        "Should be playing after play command"
    );
    let seek_position = Duration::from_secs(2);
    fixture.playback_handle.seek(seek_position);
    let _position = fixture
        .wait_for_position_update(Duration::from_secs(2))
        .await;
    let current_pos_ms = fixture
        .wait_for_position_update(Duration::from_secs(2))
        .await
        .expect("a position update should arrive while playing");
    let same_position = Duration::from_millis(current_pos_ms + 50);
    fixture.playback_handle.seek(same_position);
    let seeked_position = fixture.wait_for_seeked(Duration::from_secs(2)).await;
    assert!(
        seeked_position.is_some(),
        "Should receive Seeked event when position difference < 100ms",
    );
    if let Some(seeked_position) = seeked_position {
        let diff = Duration::from_millis(seeked_position).abs_diff(same_position);
        assert!(
            diff < Duration::from_millis(100),
            "Seeked display should stay near the requested position, got {:?}",
            diff,
        );
    }
    let position_update = fixture
        .wait_for_position_update(Duration::from_secs(2))
        .await;
    assert!(
        position_update.is_some(),
        "Position updates should continue after skipped seek",
    );
}
#[tokio::test]
async fn test_queue_maintained_after_previous_navigation() {
    let mut fixture = PlaybackTestFixture::new().await;
    let first_track_id = fixture.track_ids[0].clone();
    let second_track_id = fixture.track_ids[1].clone();
    fixture.playback_handle.play(first_track_id.clone());
    let _first_track_state = fixture
        .wait_for_state(
            |s| {
                if let PlaybackState::Playing { track_info, .. } = s {
                    track_info.track_id == first_track_id
                } else {
                    false
                }
            },
            Duration::from_secs(5),
        )
        .await;
    fixture.playback_handle.next();
    let second_track_state = fixture
        .wait_for_state(
            |s| {
                if let PlaybackState::Playing { track_info, .. } = s {
                    track_info.track_id == second_track_id
                } else {
                    false
                }
            },
            Duration::from_secs(5),
        )
        .await;
    assert!(
        second_track_state.is_some(),
        "Should be playing second track after Next command",
    );
    fixture.playback_handle.seek(Duration::from_secs(1));
    let _position = fixture
        .wait_for_position_update(Duration::from_secs(2))
        .await;
    fixture.playback_handle.previous();
    let back_to_first_state = fixture
        .wait_for_state(
            |s| {
                if let PlaybackState::Playing { track_info, .. } = s {
                    track_info.track_id == first_track_id
                } else {
                    false
                }
            },
            Duration::from_secs(5),
        )
        .await;
    assert!(
        back_to_first_state.is_some(),
        "Should go back to first track when Previous is called from second track",
    );
    fixture.playback_handle.seek(Duration::from_secs(1));
    let _position = fixture
        .wait_for_position_update(Duration::from_secs(2))
        .await;
    fixture.playback_handle.next();
    let should_be_second_state = fixture
        .wait_for_state(
            |s| {
                if let PlaybackState::Playing { track_info, .. } = s {
                    track_info.track_id == second_track_id
                } else {
                    false
                }
            },
            Duration::from_secs(5),
        )
        .await;
    assert!(
        should_be_second_state.is_some(),
        "Should go to track 2 when Next is called after navigating back to track 1",
    );
}
/// Verifies that after a queue mutation, pressing Next plays `expected` and not the
/// track that was preloaded before the mutation.
///
/// `initial_queue` is added before play so the first entry gets preloaded.
/// `mutate` is called after track0 is confirmed playing, then Next is pressed.
async fn assert_preload_refreshed_after_queue_mutation<F>(
    fixture: &mut PlaybackTestFixture,
    initial_queue: Vec<String>,
    track0: &str,
    expected: &str,
    mutate: F,
) where
    F: FnOnce(&bae_core::playback::PlaybackHandle, &[bae_core::playback::QueueEntry]),
{
    fixture.playback_handle.add_to_queue(initial_queue);
    fixture.playback_handle.play(track0.to_string());

    // `play` refills the context lane from the track's release, minting fresh
    // entry ids, and leaves Up Next alone — so capture the entries *after* play
    // settles: drain up to Playing, then read the current queue value.
    let (played, entries) = fixture
        .wait_for_playing_capturing_queue(Duration::from_secs(5))
        .await;
    assert!(played, "track0 should start playing");

    mutate(&fixture.playback_handle, &entries);
    fixture.playback_handle.next();

    let next_state = fixture
        .wait_for_state(
            |s| match s {
                PlaybackState::Playing { track_info, .. }
                | PlaybackState::Paused { track_info, .. } => track_info.track_id != track0,
                _ => false,
            },
            Duration::from_secs(5),
        )
        .await;

    let state = next_state.expect("Next should switch off track0 after the queue mutation");
    let playing_id = match &state {
        PlaybackState::Playing { track_info, .. } => track_info.track_id.clone(),
        PlaybackState::Paused { track_info, .. } => track_info.track_id.clone(),
        _ => unreachable!(),
    };
    assert_eq!(playing_id, expected);

    // The queue/context projection landing on `expected` isn't proof audio is
    // actually flowing on it — assert the position keeps climbing too.
    if matches!(state, PlaybackState::Playing { .. }) {
        assert_position_advances(&mut fixture.progress_rx).await;
    }
}

#[tokio::test]
async fn test_add_next_displaces_preloaded_track() {
    let mut fixture = PlaybackTestFixture::new().await;
    let (track0, track1, track2) = (
        fixture.track_ids[0].clone(),
        fixture.track_ids[1].clone(),
        fixture.track_ids[2].clone(),
    );
    let t2 = track2.clone();
    assert_preload_refreshed_after_queue_mutation(
        &mut fixture,
        vec![track1],
        &track0,
        &track2,
        move |h, _entries| h.add_next(vec![t2]),
    )
    .await;
}

#[tokio::test]
async fn test_reorder_entry_displaces_preloaded_track() {
    let mut fixture = PlaybackTestFixture::new().await;
    let (track0, track1, track2) = (
        fixture.track_ids[0].clone(),
        fixture.track_ids[1].clone(),
        fixture.track_ids[2].clone(),
    );
    assert_preload_refreshed_after_queue_mutation(
        &mut fixture,
        vec![track1, track2.clone()],
        &track0,
        &track2,
        |h, entries| h.reorder_entry(entries[1].id.clone(), Some(entries[0].id.clone())),
    )
    .await;
}

#[tokio::test]
async fn test_insert_in_queue_displaces_preloaded_track() {
    let mut fixture = PlaybackTestFixture::new().await;
    let (track0, track1, track2) = (
        fixture.track_ids[0].clone(),
        fixture.track_ids[1].clone(),
        fixture.track_ids[2].clone(),
    );
    let t2 = track2.clone();
    assert_preload_refreshed_after_queue_mutation(
        &mut fixture,
        vec![track1],
        &track0,
        &track2,
        move |h, _entries| h.insert_in_queue(vec![t2], 0),
    )
    .await;
}

#[tokio::test]
async fn test_remove_entry_refreshes_preloaded_track() {
    let mut fixture = PlaybackTestFixture::new().await;
    let (track0, track1, track2) = (
        fixture.track_ids[0].clone(),
        fixture.track_ids[1].clone(),
        fixture.track_ids[2].clone(),
    );
    assert_preload_refreshed_after_queue_mutation(
        &mut fixture,
        vec![track1, track2.clone()],
        &track0,
        &track2,
        |h, entries| h.remove_entry(entries[0].id.clone()),
    )
    .await;
}

// ============================================================================
// Service command coverage
// ============================================================================
// One focused test per PlaybackCommand / LibraryEvent branch that had no
// coverage, driven through the PlaybackHandle like the rest of the suite. Where
// a command's effect is a queue-shape change, the serial command loop lets a
// following queue_projection() read reflect it (the projection query is
// processed after the mutation it follows).

/// The release id of the currently-playing context, read off the projection.
async fn current_release_id(handle: &bae_core::playback::PlaybackHandle) -> String {
    let proj = handle.queue_projection().await.expect("queue projection");
    match proj.context.expect("a playing context").source {
        bae_core::playback::ContextSource::Release(id) => id,
        bae_core::playback::ContextSource::Releases(ids) => {
            panic!("expected a single-release context, got a multi-release one: {ids:?}")
        }
        bae_core::playback::ContextSource::Library => {
            panic!("expected a release context, got the whole-library context")
        }
    }
}

/// `SetShuffle` permutes and unpermutes the context lane the queue already
/// holds — nothing is re-fetched — and the track that is playing keeps playing
/// through both toggles.
#[tokio::test]
async fn set_shuffle_permutes_and_unpermutes_the_context_lane_in_place() {
    let mut fixture = PlaybackTestFixture::new().await;
    let first = fixture.track_ids[0].clone();
    let second = fixture.track_ids[1].clone();
    let third = fixture.track_ids[2].clone();
    fixture.playback_handle.play(first.clone());
    fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == first),
            Duration::from_secs(5),
        )
        .await
        .expect("the release starts playing");

    // Shuffle on re-materializes the playing context into a shuffled order —
    // not merely a persisted seed.
    fixture.playback_handle.set_shuffle(true);
    let shuffled = fixture
        .playback_handle
        .queue_projection()
        .await
        .expect("queue projection");
    assert!(
        shuffled.context.as_ref().is_some_and(|c| c.shuffled),
        "SetShuffle(true) puts the lane in shuffled order"
    );

    // Shuffle off unpermutes the lane: the not-yet-played tail is the two tracks
    // after the current one, in release order.
    fixture.playback_handle.set_shuffle(false);
    let sequential = fixture
        .playback_handle
        .queue_projection()
        .await
        .expect("queue projection");
    let ctx = sequential.context.expect("still a playing context");
    assert!(
        !ctx.shuffled,
        "SetShuffle(false) puts the lane back in sequential order"
    );
    let upcoming: Vec<String> = ctx.upcoming.iter().map(|e| e.track_id.clone()).collect();
    assert_eq!(
        upcoming,
        vec![second, third],
        "sequential upcoming is source order after the current track"
    );

    // The lane projects correctly, but that alone doesn't prove the
    // currently-playing track's audio survived the churn.
    assert_position_advances(&mut fixture.progress_rx).await;
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
    let mut fixture = PlaybackTestFixture::new().await;
    let first = fixture.track_ids[0].clone();
    let sequential_next = fixture.track_ids[1].clone();
    fixture.playback_handle.play(first.clone());
    fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == first),
            Duration::from_secs(5),
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
        fixture.playback_handle.set_shuffle(true);
        let front = fixture
            .playback_handle
            .queue_projection()
            .await
            .expect("queue projection")
            .context
            .expect("a playing context")
            .upcoming
            .first()
            .expect("the shuffled lane still has an upcoming track")
            .track_id
            .clone();
        if front != sequential_next {
            shuffled_next = Some(front);
            break;
        }
        fixture.playback_handle.set_shuffle(false);
    }
    let shuffled_next =
        shuffled_next.expect("a shuffle that moves a different track to the lane's front");

    fixture.playback_handle.next();

    fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == shuffled_next),
            Duration::from_secs(10),
        )
        .await
        .expect(
            "the track after a shuffle is the lane's new front, not the one preloaded before it",
        );
}

#[tokio::test]
async fn play_library_shuffled_plays_a_shuffled_library_context() {
    let mut fixture = PlaybackTestFixture::new().await;
    fixture.playback_handle.play_library_shuffled();
    fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { .. }),
            Duration::from_secs(5),
        )
        .await
        .expect("a library track starts playing");
    let proj = fixture
        .playback_handle
        .queue_projection()
        .await
        .expect("queue projection");
    let ctx = proj.context.expect("a library context");
    assert!(
        matches!(ctx.source, bae_core::playback::ContextSource::Library),
        "PlayLibraryShuffled plays from the whole library"
    );
    assert!(ctx.shuffled, "the library context is shuffled");
}

#[tokio::test]
async fn play_library_shuffled_is_a_no_op_on_an_empty_library() {
    let (library_manager, runtime_handle, _temp) = empty_test_library().await;
    let (handle, _capture_rx) = start_capture_service(library_manager, runtime_handle);
    let mut progress = handle.subscribe_progress();
    handle.play_library_shuffled();
    let played = wait_for_state_on(
        &mut progress,
        |s| matches!(s, PlaybackState::Playing { .. }),
        Duration::from_secs(1),
    )
    .await;
    assert!(
        played.is_none(),
        "an empty library has nothing to shuffle-play"
    );
    handle.stop();
}

#[tokio::test]
async fn set_repeat_mode_applies_and_reports_each_mode() {
    let mut fixture = PlaybackTestFixture::new().await;
    for expected in [RepeatMode::Context, RepeatMode::Track, RepeatMode::Off] {
        fixture.playback_handle.set_repeat_mode(expected);
        let mode = wait_for_repeat_mode(&mut fixture.progress_rx, Duration::from_secs(3)).await;
        assert_eq!(mode, expected, "SetRepeatMode applies the requested mode");
    }
}

#[tokio::test]
async fn set_muted_round_trips_and_set_volume_clears_mute() {
    let mut fixture = PlaybackTestFixture::new().await;
    fixture.playback_handle.set_volume(0.5);

    fixture.playback_handle.set_muted(true);
    assert!(
        wait_for_mute(&mut fixture.progress_rx, Duration::from_secs(3)).await,
        "SetMuted(true) mutes"
    );

    // A repeated set to the state already held emits nothing; the next
    // MuteChanged is the unmute that follows. A toggle-shaped implementation
    // would instead emit an unmute here and re-mute below, so the waited event
    // would be `true`.
    fixture.playback_handle.set_muted(true);
    fixture.playback_handle.set_muted(false);
    assert!(
        !wait_for_mute(&mut fixture.progress_rx, Duration::from_secs(3)).await,
        "the repeated SetMuted(true) was a no-op; only SetMuted(false) emitted"
    );

    // Mute again, then a non-zero SetVolume clears the mute.
    fixture.playback_handle.set_muted(true);
    assert!(
        wait_for_mute(&mut fixture.progress_rx, Duration::from_secs(3)).await,
        "muted again"
    );
    fixture.playback_handle.set_volume(0.7);
    assert!(
        !wait_for_mute(&mut fixture.progress_rx, Duration::from_secs(3)).await,
        "SetVolume above zero clears the mute"
    );
}

#[tokio::test]
async fn skip_to_entry_jumps_to_that_queue_entry() {
    let mut fixture = PlaybackTestFixture::new().await;
    let first = fixture.track_ids[0].clone();
    let third = fixture.track_ids[2].clone();
    fixture.playback_handle.play(first.clone());
    let (played, entries) = fixture
        .wait_for_playing_capturing_queue(Duration::from_secs(5))
        .await;
    assert!(played, "the release starts playing");
    let target = entries
        .iter()
        .find(|e| e.track_id == third)
        .expect("the third track is queued in the context");
    fixture.playback_handle.skip_to_entry(target.id.clone());
    fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == third),
            Duration::from_secs(5),
        )
        .await
        .expect("SkipTo jumps to the targeted entry");

    // Landing on the right track's state isn't proof its audio is flowing.
    assert_position_advances(&mut fixture.progress_rx).await;
}

#[tokio::test]
async fn clear_up_next_empties_the_manual_lane_keeping_the_context() {
    let mut fixture = PlaybackTestFixture::new().await;
    let first = fixture.track_ids[0].clone();
    let second = fixture.track_ids[1].clone();
    fixture.playback_handle.play(first.clone());
    fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == first),
            Duration::from_secs(5),
        )
        .await
        .expect("the release starts playing");
    fixture.playback_handle.add_to_queue(vec![second]);
    let before = fixture
        .playback_handle
        .queue_projection()
        .await
        .expect("queue projection");
    assert!(
        !before.manual.is_empty(),
        "the manual lane has the added track"
    );

    fixture.playback_handle.clear_up_next();
    let after = fixture
        .playback_handle
        .queue_projection()
        .await
        .expect("queue projection");
    assert!(
        after.manual.is_empty(),
        "ClearUpNext empties the manual lane"
    );
    assert!(
        after.context.is_some(),
        "ClearUpNext leaves the playing context intact"
    );
}

/// The counterpart: clearing "Playing From" drops the section the release
/// filled, but not the track coming out of the speakers — that keeps playing,
/// and Up Next keeps its rows.
#[tokio::test]
async fn clear_playing_from_drops_the_context_while_the_track_keeps_playing() {
    let mut fixture = PlaybackTestFixture::new().await;
    let first = fixture.track_ids[0].clone();
    let queued = fixture.track_ids[2].clone();
    fixture.playback_handle.play(first.clone());
    fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == first),
            Duration::from_secs(5),
        )
        .await
        .expect("the release starts playing");
    fixture.playback_handle.add_to_queue(vec![queued.clone()]);

    fixture.playback_handle.clear_playing_from();

    let after = fixture
        .playback_handle
        .queue_projection()
        .await
        .expect("queue projection");
    assert!(
        after.context.is_none(),
        "the context section leaves the snapshot entirely"
    );
    let manual: Vec<String> = after.manual.iter().map(|e| e.track_id.clone()).collect();
    assert_eq!(manual, vec![queued], "Up Next is untouched");
    assert!(
        !after.has_previous,
        "the context's history went with it, so there is nothing to step back to"
    );

    // The section is gone from the queue; the audio is not.
    assert_position_advances(&mut fixture.progress_rx).await;
}

#[tokio::test]
async fn add_release_to_queue_appends_its_tracks_to_the_manual_lane() {
    let mut fixture = PlaybackTestFixture::new().await;
    let first = fixture.track_ids[0].clone();
    fixture.playback_handle.play(first.clone());
    fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == first),
            Duration::from_secs(5),
        )
        .await
        .expect("the release starts playing");
    let release_id = current_release_id(&fixture.playback_handle).await;
    fixture.playback_handle.add_release_to_queue(release_id);
    let proj = fixture
        .playback_handle
        .queue_projection()
        .await
        .expect("queue projection");
    assert_eq!(
        proj.manual.len(),
        3,
        "the release's three tracks are appended to the manual lane"
    );
}

#[tokio::test]
async fn add_release_next_puts_its_tracks_at_the_front_of_the_manual_lane() {
    let mut fixture = PlaybackTestFixture::new().await;
    let first = fixture.track_ids[0].clone();
    let second = fixture.track_ids[1].clone();
    fixture.playback_handle.play(first.clone());
    fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == first),
            Duration::from_secs(5),
        )
        .await
        .expect("the release starts playing");
    // A marker already in the manual lane: AddReleaseNext must land before it.
    fixture.playback_handle.add_to_queue(vec![second.clone()]);
    let release_id = current_release_id(&fixture.playback_handle).await;
    fixture.playback_handle.add_release_next(release_id);
    let proj = fixture
        .playback_handle
        .queue_projection()
        .await
        .expect("queue projection");
    assert_eq!(proj.manual.len(), 4, "three release tracks plus the marker");
    assert_eq!(
        proj.manual.last().unwrap().track_id,
        second,
        "AddReleaseNext inserts the release before the existing marker"
    );
}

#[tokio::test]
async fn remove_currently_playing_entry_stops_playback() {
    let mut fixture = PlaybackTestFixture::new().await;
    let first = fixture.track_ids[0].clone();
    let second = fixture.track_ids[1].clone();
    fixture.playback_handle.play(first.clone());
    let (played, entries) = fixture
        .wait_for_playing_capturing_queue(Duration::from_secs(5))
        .await;
    assert!(played, "the release starts playing");

    // Skip onto the second track so it's the current context entry, then remove
    // that very entry: the handler compares the removed entry's track to the
    // playing track and stops.
    let entry = entries
        .iter()
        .find(|e| e.track_id == second)
        .expect("the second track is queued in the context")
        .clone();
    fixture.playback_handle.skip_to_entry(entry.id.clone());
    fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == second),
            Duration::from_secs(5),
        )
        .await
        .expect("SkipTo makes the second track current");

    fixture.playback_handle.remove_entry(entry.id.clone());
    fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Stopped),
            Duration::from_secs(5),
        )
        .await
        .expect("removing the currently-playing entry stops playback");
}

#[tokio::test]
async fn play_release_clamps_an_out_of_range_start_index() {
    let mut fixture = PlaybackTestFixture::new().await;
    let first = fixture.track_ids[0].clone();
    let third = fixture.track_ids[2].clone();
    // Start on the third track so a clamp-to-first is observable.
    fixture.playback_handle.play(third.clone());
    fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == third),
            Duration::from_secs(5),
        )
        .await
        .expect("the third track plays");
    let release_id = current_release_id(&fixture.playback_handle).await;
    fixture
        .playback_handle
        .play_release(release_id, Some(99), false);
    fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == first),
            Duration::from_secs(5),
        )
        .await
        .expect("an out-of-range start index clamps to the first track");
}

/// LibraryEvent::TracksDeleted, both branches. Two releases share one library so
/// the current track (release A) and a preloaded next (release B) can be deleted
/// independently: deleting B clears the preloaded next and purges its queued
/// track while A keeps playing; deleting A deletes the current track and stops.
/// Deleting B also races the event against B's own byte fill, which finds the
/// release's file rows gone: either order has to leave A playing.
#[tokio::test]
async fn tracks_deleted_clears_a_preloaded_next_then_stops_on_the_current() {
    let lib = restore_test_library().await;
    let first = lib.track_ids[0].clone();
    let a_release_id = lib
        .library_manager
        .get_releases_for_album(&lib.library_manager.get_albums(&[]).await.unwrap()[0].id)
        .await
        .unwrap()[0]
        .id
        .clone();
    let (b_release_id, b_tracks, _b_source) = import_second_release(&lib).await;
    let b_first = b_tracks[0].clone();

    let (handle, _capture_rx) =
        start_capture_service(lib.library_manager.clone(), lib.runtime_handle.clone());
    let mut progress = handle.subscribe_progress();

    handle.play(first.clone());
    wait_for_state_on(
        &mut progress,
        |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == first),
        Duration::from_secs(5),
    )
    .await
    .expect("release A starts playing");

    // Queue B's first track as the preloaded next.
    handle.add_next(vec![b_first.clone()]);
    let queued = handle.queue_projection().await.expect("queue projection");
    assert!(
        queued.manual.iter().any(|e| e.track_id == b_first),
        "B's track is queued as the next up"
    );

    // Delete B: current (A) survives, the preloaded next (B) is cleared and its
    // queued entry purged — no stop.
    lib.library_manager
        .delete_release(&b_release_id)
        .await
        .expect("delete release B");
    let still_playing =
        wait_for_track_position(&mut progress, &first, Duration::from_secs(5)).await;
    assert!(
        still_playing.is_some(),
        "deleting only the preloaded-next release leaves the current track playing"
    );
    let after = handle.queue_projection().await.expect("queue projection");
    assert!(
        !after.manual.iter().any(|e| e.track_id == b_first),
        "the deleted release's queued track is purged"
    );

    // Delete A: the current track is deleted → playback stops.
    lib.library_manager
        .delete_release(&a_release_id)
        .await
        .expect("delete release A");
    wait_for_state_on(
        &mut progress,
        |s| matches!(s, PlaybackState::Stopped),
        Duration::from_secs(5),
    )
    .await
    .expect("deleting the current track's release stops playback");

    handle.stop();
}
