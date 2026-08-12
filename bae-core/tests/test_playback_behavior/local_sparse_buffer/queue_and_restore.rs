/// Multi-window analog of `assert_skip_preserves_play_state`: Next/Previous
/// must land on the adjacent track in the same play/pause state — exercised
/// here against the sparse buffer's promotion/rebuild paths (the single-window
/// fixture's file is fully buffered before either test ever seeks).
async fn assert_skip_preserves_play_state_over_sparse_buffer(
    direction: SkipDirection,
    start_paused: bool,
) {
    let mut playback = MultiWindowPlayback::new("multi-window-skip-state").await;
    let first = playback.track_ids[0].clone();
    let second = playback.track_ids[1].clone();

    let (start_track_id, target_track_id) = match direction {
        SkipDirection::Next => (first, second),
        SkipDirection::Previous => (second, first),
    };

    playback.play_and_wait(&start_track_id).await;

    if start_paused {
        playback.playback_handle.pause();
        wait_for_state_on(
            &mut playback.progress_rx,
            |s| matches!(s, PlaybackState::Paused { .. }),
            Duration::from_secs(5),
        )
        .await
        .expect("playback should pause");
    }

    // Previous needs to land inside its 3s "step back" window; Next has no
    // such window. Pressed immediately after play_and_wait/pause either way.
    match direction {
        SkipDirection::Next => playback.playback_handle.next(),
        SkipDirection::Previous => playback.playback_handle.previous(),
    }

    let landed = wait_for_state_on(
        &mut playback.progress_rx,
        |s| {
            let (track_info, is_paused) = match s {
                PlaybackState::Playing { track_info, .. } => (track_info, false),
                PlaybackState::Paused { track_info, .. } => (track_info, true),
                _ => return false,
            };
            track_info.track_id == target_track_id && is_paused == start_paused
        },
        Duration::from_secs(25),
    )
    .await;
    assert!(
        landed.is_some(),
        "{} while {} should land on the adjacent track in the same play/pause state",
        direction.label(),
        if start_paused { "paused" } else { "playing" },
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn next_while_paused_stays_paused_over_sparse_buffer() {
    assert_skip_preserves_play_state_over_sparse_buffer(SkipDirection::Next, true).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn next_while_playing_stays_playing_over_sparse_buffer() {
    assert_skip_preserves_play_state_over_sparse_buffer(SkipDirection::Next, false).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn previous_while_paused_stays_paused_over_sparse_buffer() {
    assert_skip_preserves_play_state_over_sparse_buffer(SkipDirection::Previous, true).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn previous_while_playing_stays_playing_over_sparse_buffer() {
    assert_skip_preserves_play_state_over_sparse_buffer(SkipDirection::Previous, false).await;
}

/// Multi-window port of `test_previous_track_navigation`: pressed promptly,
/// Previous steps back a track; pressed later in the track, it restarts the
/// current one instead — both outcomes rebuild the decoder over the sparse
/// buffer rather than a fully-buffered file.
#[tokio::test(flavor = "multi_thread")]
async fn previous_navigation_over_sparse_buffer() {
    let mut playback = MultiWindowPlayback::new("multi-window-previous-nav").await;
    let first = playback.track_ids[0].clone();
    let second = playback.track_ids[1].clone();
    playback.play_and_wait(&first).await;

    playback.playback_handle.next();
    wait_for_state_on(
        &mut playback.progress_rx,
        |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == second),
        Duration::from_secs(25),
    )
    .await
    .expect("Next should switch to the second track");

    // Pressed promptly (well inside the 3s window): Previous steps back.
    playback.playback_handle.seek(Duration::from_secs(1));
    position_after(&mut playback.progress_rx, Duration::from_millis(500)).await;
    playback.playback_handle.previous();
    wait_for_state_on(
        &mut playback.progress_rx,
        |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == first),
        Duration::from_secs(25),
    )
    .await
    .expect("Previous called early in the track should go to the previous track");

    // Pressed late in the track: Previous restarts the current one.
    playback.playback_handle.seek(Duration::from_secs(10));
    position_after(&mut playback.progress_rx, Duration::from_millis(500)).await;
    playback.playback_handle.previous();
    wait_for_state_on(
        &mut playback.progress_rx,
        |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == first),
        Duration::from_secs(25),
    )
    .await
    .expect("Previous called late in the track should restart it");
    let restart_position =
        position_after(&mut playback.progress_rx, Duration::from_millis(800)).await;
    assert!(
        restart_position < 3_000,
        "restart should reset position near 0, got {restart_position}ms",
    );
}

/// Multi-window port of `seek_preserves_staged_next_for_a_gapless_advance`:
/// a seek rebuilds the stream, but the staged gapless next must survive it —
/// exercised here where re-staging must also survive the sparse buffer's
/// on-demand fetch rather than reading an already-buffered preload.
#[tokio::test(flavor = "multi_thread")]
async fn seek_preserves_staged_next_over_sparse_buffer() {
    let mut playback = MultiWindowPlayback::new("multi-window-seek-preserves-next").await;
    let first = playback.track_ids[0].clone();
    let second = playback.track_ids[1].clone();
    playback.play_and_wait(&first).await;

    // Seek twice to stress the take-out/re-stage across an already-re-staged
    // source, then let the boundary arrive.
    playback.playback_handle.seek(Duration::from_secs(30));
    wait_for_seeked_on(&mut playback.progress_rx, Duration::from_secs(20))
        .await
        .expect("first seek lands");
    playback.playback_handle.seek(Duration::from_secs(56));
    wait_for_seeked_on(&mut playback.progress_rx, Duration::from_secs(20))
        .await
        .expect("second seek lands");

    let outcome = observe_boundary(
        &mut playback.progress_rx,
        &first,
        &second,
        Duration::from_secs(25),
    )
    .await;
    assert!(
        outcome.reached_incoming,
        "playback crosses into the second track after two re-staging seeks"
    );
    assert!(
        outcome.decode_stats_for_finishing && !outcome.completed_for_finishing,
        "the boundary stays gapless: the staged next survived both seeks"
    );
}

/// Multi-window port of `test_restore_emits_seeked_at_saved_position` over
/// the shared-buffer resume path: persist mid-track, restart the service, and
/// confirm the restored track resumes and streams from the saved position
/// rather than starving on a stale/cancelled buffer.
#[tokio::test(flavor = "multi_thread")]
async fn restore_at_position_over_sparse_buffer_resumes_and_advances() {
    let mut playback = MultiWindowPlayback::new("multi-window-restore").await;
    let last_track = playback.track_ids[2].clone();
    playback.play_and_wait(&last_track).await;

    let target = Duration::from_secs(20);
    playback.playback_handle.seek(target);
    wait_for_seeked_on(&mut playback.progress_rx, Duration::from_secs(20))
        .await
        .expect("the seek before shutdown lands");

    // Persist and tear the service down. The second service below starts
    // completely cold — no shared file buffer cache, no preload — the same
    // as a real app relaunch reopening the same library.
    playback.playback_handle.shutdown().await;

    let (handle, _capture_rx) = start_capture_service(
        playback.library_manager.clone(),
        playback.runtime_handle.clone(),
    );
    let mut progress_rx = handle.subscribe_progress();

    wait_for_state_on(
        &mut progress_rx,
        |s| matches!(s, PlaybackState::Paused { track_info, .. } if track_info.track_id == last_track),
        Duration::from_secs(20),
    )
    .await
    .expect("restart should restore paused at the saved track");

    handle.resume();
    wait_for_state_on(
        &mut progress_rx,
        |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == last_track),
        Duration::from_secs(20),
    )
    .await
    .expect("resume after restore should start playing");

    wait_for_position_advance(&mut progress_rx)
        .await
        .expect("resumed playback must keep advancing from the restored position");
}

/// With the "Restore on launch" preference off, a service that starts over a
/// saved `playback_state` row must come up with nothing in playback: empty
/// queue, no current track, no restored state emission. The row itself stays —
/// it's the crash-safe resume point, and flipping the preference back on
/// restores it at the next launch.
#[tokio::test(flavor = "multi_thread")]
async fn restore_off_starts_with_nothing_in_playback_and_keeps_the_row() {
    let mut playback = MultiWindowPlayback::new("multi-window-restore-off").await;
    let track = playback.track_ids[0].clone();
    playback.play_and_wait(&track).await;

    // Persist mid-track and tear the service down, like an app quit.
    playback.playback_handle.shutdown().await;
    assert!(
        matches!(
            playback
                .library_manager
                .load_playback_state()
                .await
                .expect("read the resume row"),
            bae_core::db::LoadedPlaybackState::Present(_)
        ),
        "shutdown persists a resume row while a track is current"
    );

    // Relaunch with the preference off.
    let (handle, _capture_rx) = start_capture_service_with_restore(
        playback.library_manager.clone(),
        playback.runtime_handle.clone(),
        false,
    );
    let mut progress_rx = handle.subscribe_progress();

    // No restored Paused/Playing state may surface. The bounded window is
    // generous relative to how fast a restore emits (immediately at startup).
    let restored = wait_for_state_on(
        &mut progress_rx,
        |s| {
            matches!(
                s,
                PlaybackState::Paused { .. } | PlaybackState::Playing { .. }
            )
        },
        Duration::from_secs(3),
    )
    .await;
    assert!(
        restored.is_none(),
        "restore-off must not surface a restored playback state, got {restored:?}"
    );

    let queue = handle.queue_projection().await.expect("queue projection");
    assert!(
        queue.manual.is_empty() && queue.context.is_none(),
        "restore-off starts with an empty queue"
    );

    // The row survives the skipped restore, so turning the preference on
    // restores this session at the next launch.
    assert!(
        matches!(
            playback
                .library_manager
                .load_playback_state()
                .await
                .expect("re-read the resume row"),
            bae_core::db::LoadedPlaybackState::Present(_)
        ),
        "the resume row must survive a restore-off launch"
    );
}

/// Multi-window analog of `assert_preload_refreshed_after_queue_mutation`,
/// used by all four preload-displacement mutations (`add_next`,
/// `reorder_entry`, `insert_in_queue`, `remove_entry`): after the mutation
/// displaces the stale preload, Next plays the newly-correct track AND its
/// audio actually flows over the sparse buffer — a regression that keeps the
/// queue correct but silences the promoted track fails the audio-advances
/// assertion.
async fn assert_preload_refreshed_over_sparse_buffer<F>(
    playback: &mut MultiWindowPlayback,
    initial_queue: Vec<String>,
    track0: &str,
    expected: &str,
    mutate: F,
) where
    F: FnOnce(&bae_core::playback::PlaybackHandle, &[bae_core::playback::QueueEntry]),
{
    playback.playback_handle.add_to_queue(initial_queue);
    playback.playback_handle.play(track0.to_string());

    let (played, entries) =
        wait_for_playing_capturing_queue_on(
            &playback.playback_handle,
            &mut playback.progress_rx,
            Duration::from_secs(25),
        )
            .await;
    assert!(played, "track0 should start playing");

    mutate(&playback.playback_handle, &entries);
    playback.playback_handle.next();

    let next_state = wait_for_state_on(
        &mut playback.progress_rx,
        |s| match s {
            PlaybackState::Playing { track_info, .. }
            | PlaybackState::Paused { track_info, .. } => track_info.track_id != track0,
            _ => false,
        },
        Duration::from_secs(25),
    )
    .await;
    let state = next_state.expect("Next should switch off track0 after the queue mutation");
    let playing_id = match &state {
        PlaybackState::Playing { track_info, .. } => track_info.track_id.clone(),
        PlaybackState::Paused { track_info, .. } => track_info.track_id.clone(),
        _ => unreachable!(),
    };
    assert_eq!(playing_id, expected);

    if matches!(state, PlaybackState::Playing { .. }) {
        assert_position_advances(&mut playback.progress_rx).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn add_next_displaces_preloaded_track_over_sparse_buffer() {
    let mut playback = MultiWindowPlayback::new("multi-window-add-next-displace").await;
    let (track0, track1, track2) = (
        playback.track_ids[0].clone(),
        playback.track_ids[1].clone(),
        playback.track_ids[2].clone(),
    );
    let t2 = track2.clone();
    assert_preload_refreshed_over_sparse_buffer(
        &mut playback,
        vec![track1],
        &track0,
        &track2,
        move |h, _entries| h.add_next(vec![t2]),
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn remove_entry_refreshes_preloaded_track_over_sparse_buffer() {
    let mut playback = MultiWindowPlayback::new("multi-window-remove-entry-refresh").await;
    let (track0, track1, track2) = (
        playback.track_ids[0].clone(),
        playback.track_ids[1].clone(),
        playback.track_ids[2].clone(),
    );
    assert_preload_refreshed_over_sparse_buffer(
        &mut playback,
        vec![track1, track2.clone()],
        &track0,
        &track2,
        |h, entries| h.remove_entry(entries[0].id.clone()),
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn reorder_entry_displaces_preloaded_track_over_sparse_buffer() {
    let mut playback = MultiWindowPlayback::new("multi-window-reorder-displace").await;
    let (track0, track1, track2) = (
        playback.track_ids[0].clone(),
        playback.track_ids[1].clone(),
        playback.track_ids[2].clone(),
    );
    assert_preload_refreshed_over_sparse_buffer(
        &mut playback,
        vec![track1, track2.clone()],
        &track0,
        &track2,
        |h, entries| h.reorder_entry(entries[1].id.clone(), Some(entries[0].id.clone())),
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn insert_in_queue_displaces_preloaded_track_over_sparse_buffer() {
    let mut playback = MultiWindowPlayback::new("multi-window-insert-displace").await;
    let (track0, track1, track2) = (
        playback.track_ids[0].clone(),
        playback.track_ids[1].clone(),
        playback.track_ids[2].clone(),
    );
    let t2 = track2.clone();
    assert_preload_refreshed_over_sparse_buffer(
        &mut playback,
        vec![track1],
        &track0,
        &track2,
        move |h, _entries| h.insert_in_queue(vec![t2], 0),
    )
    .await;
}

/// Multi-window port of `skip_to_entry_jumps_to_that_queue_entry`: SkipTo
/// must land on the targeted entry and actually stream it, not just project
/// it as current.
#[tokio::test(flavor = "multi_thread")]
async fn skip_to_entry_jumps_to_that_queue_entry_over_sparse_buffer() {
    let mut playback = MultiWindowPlayback::new("multi-window-skip-to-entry").await;
    let first = playback.track_ids[0].clone();
    let third = playback.track_ids[2].clone();
    playback.playback_handle.play(first.clone());
    let (played, entries) =
        wait_for_playing_capturing_queue_on(
            &playback.playback_handle,
            &mut playback.progress_rx,
            Duration::from_secs(25),
        )
            .await;
    assert!(played, "the release starts playing");
    let target = entries
        .iter()
        .find(|e| e.track_id == third)
        .expect("the third track is queued in the context");
    playback.playback_handle.skip_to_entry(target.id.clone());
    wait_for_state_on(
        &mut playback.progress_rx,
        |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == third),
        Duration::from_secs(25),
    )
    .await
    .expect("SkipTo jumps to the targeted entry");

    assert_position_advances(&mut playback.progress_rx).await;
}

/// Multi-window port of
/// `set_shuffle_permutes_and_unpermutes_the_context_lane_in_place`: rearranging
/// the lane must not disturb the currently-playing track's audio.
#[tokio::test(flavor = "multi_thread")]
async fn set_shuffle_rearranges_the_context_lane_over_sparse_buffer() {
    let mut playback = MultiWindowPlayback::new("multi-window-set-shuffle").await;
    let first = playback.track_ids[0].clone();
    playback.play_and_wait(&first).await;

    playback.playback_handle.set_shuffle(true);
    let shuffled = playback
        .playback_handle
        .queue_projection()
        .await
        .expect("queue projection");
    assert!(
        shuffled.context.as_ref().is_some_and(|c| c.shuffled),
        "SetShuffle(true) puts the lane in shuffled order"
    );

    playback.playback_handle.set_shuffle(false);
    let sequential = playback
        .playback_handle
        .queue_projection()
        .await
        .expect("queue projection");
    assert!(
        !sequential
            .context
            .expect("still a playing context")
            .shuffled,
        "SetShuffle(false) puts the lane back in sequential order"
    );

    assert_position_advances(&mut playback.progress_rx).await;
}

// ============================================================================
// Remote (cloud-path) variant: local files make the fill near-instant, so the
// tests above never exercise a real ranged read. This imports the same
// multi-window CUE album as remote-unpinned against an InMemoryCloudHome and
// deletes the local originals, so every byte the fill touches comes from an
// actual ranged cloud read — the fetch arbiter and window fetches over the real
// remote path, not just the local-disk fast path.
// ============================================================================
