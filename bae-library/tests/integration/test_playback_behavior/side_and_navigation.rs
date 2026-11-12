use crate::test_playback_behavior::fixtures::*;
use crate::test_playback_behavior::support::*;

#[tokio::test]
async fn sided_vinyl_boundary_pauses_on_auto_advance() {
    assert_sided_boundary_pauses("Vinyl", ["A1", "A2", "B1"], 1, "A", PlaybackPauseBoundary::Side)
        .await;
}

#[tokio::test]
async fn sided_cassette_boundary_pauses_on_auto_advance() {
    assert_sided_boundary_pauses(
        "Cassette",
        ["A1", "B1", "B2"],
        0,
        "A",
        PlaybackPauseBoundary::Side,
    )
    .await;
}

async fn assert_sided_boundary_pauses(
    format: &str,
    positions: [&str; 3],
    start_track_index: usize,
    expected_side_label: &str,
    expected_boundary: PlaybackPauseBoundary,
) {
    let mut fixture =
        SidePauseTestFixture::new(format, positions, true).await.expect("side-pause fixture");
    let side_track_id = fixture.track_ids[start_track_index].clone();

    let paused =
        fixture.play_to_side_pause(&side_track_id, expected_side_label, expected_boundary).await;

    match paused {
        PlaybackState::Paused { track, .. } => {
            assert_eq!(track.track_id, side_track_id);
        }
        other => panic!("expected side-ended pause, got {other:?}"),
    }
}

#[tokio::test]
async fn same_side_auto_advance_does_not_side_pause() {
    with_fixture(
        async {
            SidePauseTestFixture::new("Vinyl", ["A1", "A2", "B1"], true)
                .await
                .expect("side-pause fixture")
        },
        async |fixture| {
            let first_side_track_id = fixture.track_ids[0].clone();
            let same_side_track_id = fixture.track_ids[1].clone();

            fixture.play_track_and_wait(&first_side_track_id).await;

            fixture.play_out_from_near_the_end().await;

            fixture
                .wait_for_playing_track(&same_side_track_id, "same side should keep playing")
                .await;
        },
    )
    .await;
}

#[tokio::test]
async fn cd_multi_disc_boundary_pauses_on_auto_advance() {
    assert_sided_boundary_pauses("CD", ["1-1", "2-1", "2-2"], 0, "1", PlaybackPauseBoundary::Disc)
        .await;
}

#[tokio::test]
async fn setting_off_auto_advances_across_sided_boundary() {
    with_fixture(
        async {
            SidePauseTestFixture::new("Vinyl", ["A1", "A2", "B1"], false)
                .await
                .expect("side-pause fixture")
        },
        async |fixture| {
            let side_a_track_id = fixture.track_ids[1].clone();
            let next_side_track_id = fixture.track_ids[2].clone();

            fixture.play_track_and_wait(&side_a_track_id).await;

            fixture.play_out_from_near_the_end().await;

            fixture
                .wait_for_playing_track(
                    &next_side_track_id,
                    "setting off should keep playing across side boundary",
                )
                .await;
        },
    )
    .await;
}

#[tokio::test]
async fn enabling_setting_mid_track_pauses_at_the_imminent_boundary() {
    // With the setting off, B1 is already staged for a gapless handoff when
    // the setting turns on; the pause must still apply at that boundary.
    with_fixture(
        async {
            SidePauseTestFixture::new("Vinyl", ["A1", "A2", "B1"], false)
                .await
                .expect("side-pause fixture")
        },
        async |fixture| {
            let side_a_track_id = fixture.track_ids[1].clone();

            fixture.play_track_and_wait(&side_a_track_id).await;

            fixture.set_pause_between_sides_mid_track(true).await;

            fixture.play_out_from_near_the_end().await;

            fixture.wait_for_side_pause("A", PlaybackPauseBoundary::Side).await;
        },
    )
    .await;
}

#[tokio::test]
async fn disabling_setting_mid_track_keeps_playing_across_the_boundary() {
    // With the setting on, B1 is held rather than staged; the boundary check
    // re-reads the setting, so turning it off still plays straight through.
    with_fixture(
        async {
            SidePauseTestFixture::new("Vinyl", ["A1", "A2", "B1"], true)
                .await
                .expect("side-pause fixture")
        },
        async |fixture| {
            let side_a_track_id = fixture.track_ids[1].clone();
            let next_side_track_id = fixture.track_ids[2].clone();

            fixture.play_track_and_wait(&side_a_track_id).await;

            fixture.set_pause_between_sides_mid_track(false).await;

            fixture.play_out_from_near_the_end().await;

            fixture
                .wait_for_playing_track(
                    &next_side_track_id,
                    "disabling the setting mid-track should keep playing across the boundary",
                )
                .await;
        },
    )
    .await;
}

#[tokio::test]
async fn repeat_track_does_not_side_pause_at_boundary() {
    with_fixture(
        async {
            SidePauseTestFixture::new("Vinyl", ["A1", "A2", "B1"], true)
                .await
                .expect("side-pause fixture")
        },
        async |fixture| {
            let repeated_track_id = fixture.track_ids[1].clone();

            fixture.play_track_and_wait(&repeated_track_id).await;
            fixture
                .playback_handle
                .set_repeat_mode(RepeatMode::Track)
                .await
                .expect("set_repeat_mode succeeds");

            fixture.play_out_from_near_the_end().await;

            fixture
                .wait_for_playing_track(
                    &repeated_track_id,
                    "repeat-track should replay the current track",
                )
                .await;
        },
    )
    .await;
}

#[tokio::test]
async fn resume_from_side_pause_starts_next_side() {
    with_fixture(
        async {
            SidePauseTestFixture::new("Vinyl", ["A1", "A2", "B1"], true)
                .await
                .expect("side-pause fixture")
        },
        async |fixture| {
            let side_a_track_id = fixture.track_ids[1].clone();
            let next_side_track_id = fixture.track_ids[2].clone();

            fixture.play_to_side_pause(&side_a_track_id, "A", PlaybackPauseBoundary::Side).await;

            fixture.playback_handle.resume();

            fixture
                .wait_for_playing_track(
                    &next_side_track_id,
                    "resume from side pause should start the next side",
                )
                .await;
        },
    )
    .await;
}

#[tokio::test]
async fn side_boundary_pause_prevents_gapless_stream_handoff() {
    with_fixture(
        async {
            SidePauseTestFixture::new("Vinyl", ["A1", "A2", "B1"], true)
                .await
                .expect("side-pause fixture")
        },
        async |fixture| {
            let side_a_track_id = fixture.track_ids[1].clone();
            let side_b_track_id = fixture.track_ids[2].clone();

            fixture.play_track_and_wait(&side_a_track_id).await;
            let _side_a_stream = fixture.next_capture_stream().await;

            fixture.play_out_from_near_the_end().await;
            fixture.wait_for_side_pause("A", PlaybackPauseBoundary::Side).await;

            fixture.playback_handle.resume();
            let _side_b_stream = fixture.next_capture_stream().await;
            fixture
                .wait_for_playing_track(
                    &side_b_track_id,
                    "next side should start from a new stream",
                )
                .await;
        },
    )
    .await;
}

/// Next and Previous keep the current play/pause state, unlike a fresh `play`,
/// which always starts playing.
async fn assert_skip_preserves_play_state(direction: SkipDirection, start_paused: bool) {
    with_fixture(PlaybackTestFixture::new(), async |fixture| {
        let first_track_id = fixture.track_ids[0].clone();
        let second_track_id = fixture.track_ids[1].clone();

        // Previous needs a track behind the current one.
        let (start_track_id, target_track_id) = match direction {
            SkipDirection::Next => (first_track_id, second_track_id),
            SkipDirection::Previous => (second_track_id, first_track_id),
        };

        fixture.playback_handle.play(start_track_id.clone());
        fixture
            .wait_for_state(|s| {
                matches!(s, PlaybackState::Playing { track, .. }
                    if track.track_id == start_track_id)
            })
            .await
            .expect("the starting track should play");

        if start_paused {
            fixture.playback_handle.pause();
            fixture
                .wait_for_state(|s| matches!(s, PlaybackState::Paused { .. }))
                .await
                .expect("playback should pause");
        }

        // Within 3 s of the track start, Previous steps back instead of restarting.
        match direction {
            SkipDirection::Next => fixture.playback_handle.next(),
            SkipDirection::Previous => fixture.playback_handle.previous(),
        }

        let landed = fixture
            .wait_for_state(|s| {
                let (track, is_paused) = match s {
                    PlaybackState::Playing { track, .. } => (track, false),
                    PlaybackState::Paused { track, .. } => (track, true),
                    _ => return false,
                };
                track.track_id == target_track_id && is_paused == start_paused
            })
            .await;
        assert!(
            landed.is_some(),
            "{} while {} should land on the adjacent track in the same play/pause state",
            direction.label(),
            if start_paused { "paused" } else { "playing" },
        );
    })
    .await;
}

#[tokio::test]
async fn next_while_paused_stays_paused() {
    assert_skip_preserves_play_state(SkipDirection::Next, true).await;
}

#[tokio::test]
async fn next_while_playing_stays_playing() {
    assert_skip_preserves_play_state(SkipDirection::Next, false).await;
}

#[tokio::test]
async fn previous_while_paused_stays_paused() {
    assert_skip_preserves_play_state(SkipDirection::Previous, true).await;
}

#[tokio::test]
async fn previous_while_playing_stays_playing() {
    assert_skip_preserves_play_state(SkipDirection::Previous, false).await;
}

/// A seek while paused stays paused and plays from the target on resume; a seek
/// while playing keeps its position across a pause and resume.
#[tokio::test]
async fn pause_and_seek_interact_in_both_orderings() {
    // Seek while paused, then resume.
    with_fixture(PlaybackTestFixture::new(), async |fixture| {
        let track_id = fixture.track_ids[0].clone();
        fixture.playback_handle.play(track_id.clone());
        fixture
            .wait_for_state(|s| matches!(s, PlaybackState::Playing { .. }))
            .await
            .expect("should start playing");

        fixture.playback_handle.pause();
        fixture
            .wait_for_state(|s| matches!(s, PlaybackState::Paused { .. }))
            .await
            .expect("should be paused");

        fixture.playback_handle.seek(TrackTime::from_duration(Duration::from_secs(2)));
        let seeked_ms =
            fixture.wait_for_seeked().await.expect("Seeked event after seeking while paused");
        assert!(seeked_ms >= 1900, "seek should land near 2s, got {seeked_ms}ms");

        // The seek holds Loading until the rebuilt decoder is ready, then settles
        // on the pause it had.
        let settled = fixture
            .wait_for_state(|s| !matches!(s, PlaybackState::Loading { .. }))
            .await
            .expect("the seek settles");
        assert!(
            matches!(settled, PlaybackState::Paused { .. }),
            "should stay paused after seek, not auto-play, got {settled:?}"
        );

        fixture.playback_handle.resume();
        fixture
            .wait_for_state(|s| matches!(s, PlaybackState::Playing { .. }))
            .await
            .expect("should resume playing");
        let advanced_ms = fixture.wait_for_position_past(seeked_ms).await.unwrap_or_else(|| {
            panic!(
                "position should advance past the seek target after resume; seeked {seeked_ms}ms"
            )
        });
        assert!(advanced_ms > seeked_ms);
    })
    .await;

    // Seek while playing, then pause and resume.
    with_fixture(PlaybackTestFixture::new(), async |fixture| {
        let track_id = fixture.track_ids[0].clone();
        fixture.playback_handle.play(track_id.clone());
        fixture
            .wait_for_state(|s| matches!(s, PlaybackState::Playing { .. }))
            .await
            .expect("should start playing");

        let seek_target = Duration::from_secs(2);
        fixture.playback_handle.seek(TrackTime::from_duration(seek_target));
        let seeked_ms = fixture.wait_for_seeked().await.expect("Seeked event");
        assert!(
            Duration::from_millis(seeked_ms).abs_diff(seek_target) < Duration::from_secs(1),
            "seek should land near 2s, got {seeked_ms}ms"
        );

        fixture.playback_handle.pause();
        fixture
            .wait_for_state(|s| matches!(s, PlaybackState::Paused { .. }))
            .await
            .expect("should be paused");
        fixture.playback_handle.resume();
        fixture
            .wait_for_state(|s| matches!(s, PlaybackState::Playing { .. }))
            .await
            .expect("should resume playing");
        let maintained_ms =
            fixture.wait_for_position_update().await.expect("position update after resume");
        assert!(
            Duration::from_millis(maintained_ms).abs_diff(seek_target) < Duration::from_secs(1),
            "position should be maintained across pause/resume, got {maintained_ms}ms"
        );
    })
    .await;
}

#[tokio::test]
async fn test_fresh_play_always_starts_playing() {
    with_fixture(PlaybackTestFixture::new(), async |fixture| {
        let first_track_id = fixture.track_ids[0].clone();
        let second_track_id = fixture.track_ids[1].clone();

        fixture.playback_handle.play(first_track_id.clone());
        let _playing_state =
            fixture.wait_for_state(|s| matches!(s, PlaybackState::Playing { .. })).await;

        fixture.playback_handle.pause();
        let paused_state =
            fixture.wait_for_state(|s| matches!(s, PlaybackState::Paused { .. })).await;
        assert!(paused_state.is_some(), "Should be paused");

        fixture.playback_handle.play(second_track_id.clone());

        let new_play_state = fixture
            .wait_for_state(|s| {
                if let PlaybackState::Playing { track, .. } = s {
                    track.track_id == second_track_id
                } else {
                    false
                }
            })
            .await;

        assert!(new_play_state.is_some(), "Fresh play should always start playing, not paused");
    })
    .await;
}

#[tokio::test]
async fn test_seek_while_playing_advances_position() {
    with_fixture(PlaybackTestFixture::new(), async |fixture| {

    let track_id = fixture.track_ids[0].clone();

    fixture.playback_handle.play(track_id.clone());
    let playing_state = fixture
        .wait_for_state(|s| matches!(s, PlaybackState::Playing { .. }))
        .await;
    assert!(playing_state.is_some(), "Should start playing");

    let seek_target = Duration::from_secs(2);
    fixture.playback_handle.seek(TrackTime::from_duration(seek_target));

    let seeked_position = fixture.wait_for_seeked().await;
    assert!(seeked_position.is_some(), "Should receive Seeked event after seeking while playing");
    let seeked_position_ms = seeked_position.unwrap();
    assert!(
        seeked_position_ms >= 1900,
        "Seeked position should be near 2s, got {}ms",
        seeked_position_ms
    );

    // Climbing past the target shows audio is playing, not only that the seek
    // landed.
    let final_position_ms = fixture
        .wait_for_position_past(seeked_position_ms)
        .await
        .unwrap_or_else(|| {
            panic!(
                "position should advance past the seek target while playing; seeked to {seeked_position_ms}ms"
            )
        });
    assert!(final_position_ms > seeked_position_ms);
    })
    .await;
}

/// A track that plays to its end with the next track staged hands off inside
/// the running stream, without a rebuild, and the next track starts near 0.
#[tokio::test]
async fn gapless_boundary_hands_off_without_rebuild() {
    with_fixture(PlaybackTestFixture::new(), async |fixture| {
        let first = fixture.track_ids[0].clone();
        let second = fixture.track_ids[1].clone();

        fixture.playback_handle.play(first.clone());
        fixture
            .wait_for_state(
                |s| matches!(s, PlaybackState::Playing { track, .. } if track.track_id == first),
            )
            .await
            .expect("the first track should play");

        // Leaves enough time before the end for the next track to be staged again.
        fixture.playback_handle.seek(TrackTime::from_duration(Duration::from_secs(3)));
        fixture.wait_for_seeked().await.expect("the seek should land");

        let outcome = observe_boundary(
            &fixture.playback_handle,
            &fixture.audio_clock,
            &mut fixture.progress_rx,
            &first,
            &second,
        )
        .await;
        assert!(
            outcome.reached_incoming,
            "playback should cross into the second track at the first track's end"
        );
        assert!(
            outcome.decode_stats_for_finishing,
            "the gapless boundary handler reports the finishing track's decode stats"
        );
        assert!(
            !outcome.completed_for_finishing,
            "a gapless handoff must not emit TrackCompleted for the finishing track — \
         that event only fires on the stream-rebuild path"
        );
        assert!(
            !outcome.loading_for_incoming,
            "a gapless handoff must not emit a Loading state for the incoming track — \
         the staged next plays straight through, only the stream-rebuild path shows Loading"
        );
        assert_eq!(
            outcome.decode_errors, 0,
            "the crossing must decode cleanly, got {} decode errors",
            outcome.decode_errors
        );

        let position_ms = wait_for_track_position(
            &fixture.playback_handle,
            &fixture.audio_clock,
            &mut fixture.progress_rx,
            &second,
        )
        .await
        .expect("a position update for the incoming track");
        assert!(
            position_ms < 1500,
            "the incoming track's position resets near 0 at the boundary, got {position_ms}ms"
        );
    })
    .await;
}

/// A gapless crossing preloads the track after the incoming one, so the next
/// crossing is gapless too.
#[tokio::test]
async fn gapless_boundary_repreloads_following_track() {
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
            .expect("the first track should play");

        fixture.playback_handle.seek(TrackTime::from_duration(Duration::from_secs(3)));
        fixture.wait_for_seeked().await.expect("the first seek should land");
        let first_cross = observe_boundary(
            &fixture.playback_handle,
            &fixture.audio_clock,
            &mut fixture.progress_rx,
            &first,
            &second,
        )
        .await;
        assert!(first_cross.reached_incoming, "playback should cross into the second track");

        fixture.playback_handle.seek(TrackTime::from_duration(Duration::from_secs(3)));
        fixture.wait_for_seeked().await.expect("the second seek should land");
        let second_cross = observe_boundary(
            &fixture.playback_handle,
            &fixture.audio_clock,
            &mut fixture.progress_rx,
            &second,
            &third,
        )
        .await;
        assert!(
            second_cross.reached_incoming,
            "the following track was re-preloaded, so the second boundary also crosses into it"
        );
        assert!(
            second_cross.decode_stats_for_finishing,
            "the second boundary reports the finishing track's decode stats"
        );
        assert!(
            !second_cross.completed_for_finishing,
            "the re-preloaded following track crosses gaplessly — no rebuild"
        );
    })
    .await;
}

/// A seek to just before the end completes the track instead of crossing
/// gaplessly; the auto-advance still plays the preloaded next track from its
/// start.
#[tokio::test]
async fn boundary_advances_to_next_track_after_late_seek() {
    with_fixture(PlaybackTestFixture::new(), async |fixture| {
        let first = fixture.track_ids[0].clone();
        let second = fixture.track_ids[1].clone();

        fixture.playback_handle.play(first.clone());
        fixture
            .wait_for_state(
                |s| matches!(s, PlaybackState::Playing { track, .. } if track.track_id == first),
            )
            .await
            .expect("the first track should play");

        // 200 ms before the end of the 5 s track.
        fixture.playback_handle.seek(TrackTime::from_duration(Duration::from_millis(4800)));
        fixture.wait_for_seeked().await.expect("the seek should land");
        fixture.play_to_track_end().await;

        fixture
            .wait_for_state(
                |s| matches!(s, PlaybackState::Playing { track, .. } if track.track_id == second),
            )
            .await
            .expect("playback should advance to the second track at the end");
        let position_ms = wait_for_track_position(
            &fixture.playback_handle,
            &fixture.audio_clock,
            &mut fixture.progress_rx,
            &second,
        )
        .await
        .expect("a position update for the second track");
        assert!(position_ms < 1500, "the next track starts from ~0, got {position_ms}ms");
    })
    .await;
}

/// The staged next track survives seeks, so the track's end still crosses
/// gaplessly.
#[tokio::test]
async fn seek_preserves_staged_next_for_a_gapless_advance() {
    with_fixture(PlaybackTestFixture::new(), async |fixture| {
        let first = fixture.track_ids[0].clone();
        let second = fixture.track_ids[1].clone();

        fixture.playback_handle.play(first.clone());
        fixture
            .wait_for_state(
                |s| matches!(s, PlaybackState::Playing { track, .. } if track.track_id == first),
            )
            .await
            .expect("the first track should play");

        // Two seeks, so the second replaces a source that already carried the
        // staged track over once.
        fixture.playback_handle.seek(TrackTime::from_duration(Duration::from_secs(1)));
        fixture.wait_for_seeked().await.expect("the first seek should land");
        fixture.playback_handle.seek(TrackTime::from_duration(Duration::from_secs(3)));
        fixture.wait_for_seeked().await.expect("the second seek should land");

        let outcome = observe_boundary(
            &fixture.playback_handle,
            &fixture.audio_clock,
            &mut fixture.progress_rx,
            &first,
            &second,
        )
        .await;
        assert!(
            outcome.reached_incoming,
            "the staged next should survive the seeks and play at the boundary"
        );
        assert!(
        !outcome.completed_for_finishing,
        "the advance after seeking must stay gapless — the staged next was preserved, not rebuilt"
    );
        assert!(
            outcome.decode_stats_for_finishing,
            "the gapless boundary handler still reports the finishing track's decode stats"
        );
    })
    .await;
}

/// A same-format seek swaps the source in place without creating a new device
/// stream, so dropping the capture receiver cannot stop playback.
#[tokio::test]
async fn seek_with_dropped_capture_receiver_keeps_playing() {
    let lib = restore_test_library().await;
    let first = lib.track_ids[0].clone();

    // Built here so the test owns the receiver it drops.
    let (capture_device, capture_stream_rx, _audio_clock) =
        bae_playback::test_capture::manual_capture_device();
    let mut capture_stream_rx = Some(capture_stream_rx);
    let handle = lib
        .library
        .start_playback_on(capture_device, bae_playback::test_capture::wall_playback_clock(), true)
        .await;
    let mut progress_rx = handle.subscribe_progress();

    handle.play(first.clone());
    wait_for_state_on(
        &mut progress_rx,
        |s| matches!(s, PlaybackState::Playing { track, .. } if track.track_id == first),
    )
    .await
    .expect("the track should start playing (first stream created)");

    capture_stream_rx.take();
    handle.seek(TrackTime::from_duration(Duration::from_secs(2)));

    let mut saw_error = false;
    // `Some(true)` means the seek landed, `Some(false)` that playback stopped.
    let outcome =
        bae_util::test_channels::next_matching(&mut progress_rx, HANG_GUARD, |event| match event {
            PlaybackProgress::Seeked { track_id, .. } if track_id == first => Some(true),
            PlaybackProgress::PlaybackError { .. } => {
                saw_error = true;
                None
            }
            PlaybackProgress::StateChanged { state: PlaybackState::Stopped } => Some(false),
            _ => None,
        })
        .await;
    let (saw_seeked, saw_stopped) = (outcome == Some(true), outcome == Some(false));
    assert!(saw_seeked, "an in-place seek should land and emit Seeked");
    assert!(!saw_error, "an in-place seek must not surface a PlaybackError");
    assert!(!saw_stopped, "an in-place seek must not stop playback");

    handle.stop();
}
