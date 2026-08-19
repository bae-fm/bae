#[tokio::test]
async fn sided_vinyl_boundary_pauses_on_auto_advance() {
    assert_sided_boundary_pauses(
        "Vinyl",
        ["A1", "A2", "B1"],
        1,
        "A",
        SIDE_PAUSE_VINYL_MESSAGE_KEY,
    )
    .await;
}

#[tokio::test]
async fn sided_cassette_boundary_pauses_on_auto_advance() {
    assert_sided_boundary_pauses(
        "Cassette",
        ["A1", "B1", "B2"],
        0,
        "A",
        SIDE_PAUSE_CASSETTE_MESSAGE_KEY,
    )
    .await;
}

async fn assert_sided_boundary_pauses(
    format: &str,
    positions: [&str; 3],
    start_track_index: usize,
    expected_side_letter: &str,
    expected_message_key: &str,
) {
    let mut fixture = SidePauseTestFixture::new(format, positions, true)
        .await
        .expect("side-pause fixture");
    let side_track_id = fixture.track_ids[start_track_index].clone();

    let paused = fixture
        .play_to_side_pause(
            start_track_index,
            &side_track_id,
            expected_side_letter,
            expected_message_key,
        )
        .await;

    match paused {
        PlaybackState::Paused { track_info, .. } => {
            assert_eq!(track_info.track_id, side_track_id);
        }
        other => panic!("expected side-ended pause, got {other:?}"),
    }
}

#[tokio::test]
async fn same_side_auto_advance_does_not_side_pause() {
    let mut fixture = SidePauseTestFixture::new("Vinyl", ["A1", "A2", "B1"], true)
        .await
        .expect("side-pause fixture");
    let first_side_track_id = fixture.track_ids[0].clone();
    let same_side_track_id = fixture.track_ids[1].clone();

    fixture.play_track_and_wait(0, &first_side_track_id).await;

    fixture.seek_to_auto_advance();

    fixture
        .wait_for_playing_track(
            &same_side_track_id,
            Duration::from_secs(10),
            "same side should keep playing",
        )
        .await;
}

#[tokio::test]
async fn cd_multi_disc_auto_advance_does_not_side_pause() {
    let mut fixture = SidePauseTestFixture::new("CD", ["1-1", "2-1", "2-2"], true)
        .await
        .expect("side-pause fixture");
    let first_disc_track_id = fixture.track_ids[0].clone();
    let next_disc_track_id = fixture.track_ids[1].clone();

    fixture.play_track_and_wait(0, &first_disc_track_id).await;

    fixture.seek_to_auto_advance();

    fixture
        .wait_for_playing_track(
            &next_disc_track_id,
            Duration::from_secs(10),
            "CD disc boundary should keep playing",
        )
        .await;
}

#[tokio::test]
async fn setting_off_auto_advances_across_sided_boundary() {
    let mut fixture = SidePauseTestFixture::new("Vinyl", ["A1", "A2", "B1"], false)
        .await
        .expect("side-pause fixture");
    let side_a_track_id = fixture.track_ids[1].clone();
    let next_side_track_id = fixture.track_ids[2].clone();

    fixture.play_track_and_wait(1, &side_a_track_id).await;

    fixture.seek_to_auto_advance();

    fixture
        .wait_for_playing_track(
            &next_side_track_id,
            Duration::from_secs(10),
            "setting off should keep playing across side boundary",
        )
        .await;
}

#[tokio::test]
async fn enabling_setting_mid_track_pauses_at_the_imminent_boundary() {
    // The setting starts OFF: A2 preloads B1 and stages it gapless into the
    // live audio chain immediately after A2 starts. Turning the setting on
    // while A2 is still playing must still catch that already-staged
    // boundary — the natural way anyone would try the feature.
    let mut fixture = SidePauseTestFixture::new("Vinyl", ["A1", "A2", "B1"], false)
        .await
        .expect("side-pause fixture");
    let side_a_track_id = fixture.track_ids[1].clone();

    fixture.play_track_and_wait(1, &side_a_track_id).await;

    fixture.set_pause_between_sides_mid_track(true);

    fixture.seek_to_auto_advance();

    fixture
        .wait_for_side_pause("A", SIDE_PAUSE_VINYL_MESSAGE_KEY)
        .await;
}

#[tokio::test]
async fn disabling_setting_mid_track_keeps_playing_across_the_boundary() {
    // Regression guard for the direction that already worked: the setting
    // starts ON, so B1 is held (not staged) rather than gaplessly chained.
    // Turning the setting off mid-track must let it play straight through —
    // the drain-time gate re-reads the config before the boundary fires.
    let mut fixture = SidePauseTestFixture::new("Vinyl", ["A1", "A2", "B1"], true)
        .await
        .expect("side-pause fixture");
    let side_a_track_id = fixture.track_ids[1].clone();
    let next_side_track_id = fixture.track_ids[2].clone();

    fixture.play_track_and_wait(1, &side_a_track_id).await;

    fixture.set_pause_between_sides_mid_track(false);

    fixture.seek_to_auto_advance();

    fixture
        .wait_for_playing_track(
            &next_side_track_id,
            Duration::from_secs(10),
            "disabling the setting mid-track should keep playing across the boundary",
        )
        .await;
}

#[tokio::test]
async fn repeat_track_does_not_side_pause_at_boundary() {
    let mut fixture = SidePauseTestFixture::new("Vinyl", ["A1", "A2", "B1"], true)
        .await
        .expect("side-pause fixture");
    let repeated_track_id = fixture.track_ids[1].clone();

    fixture.play_track_and_wait(1, &repeated_track_id).await;
    fixture.playback_handle.set_repeat_mode(RepeatMode::Track);

    fixture.seek_to_auto_advance();

    fixture
        .wait_for_playing_track(
            &repeated_track_id,
            Duration::from_secs(10),
            "repeat-track should replay the current track",
        )
        .await;
}

#[tokio::test]
async fn resume_from_side_pause_starts_next_side() {
    let mut fixture = SidePauseTestFixture::new("Vinyl", ["A1", "A2", "B1"], true)
        .await
        .expect("side-pause fixture");
    let side_a_track_id = fixture.track_ids[1].clone();
    let next_side_track_id = fixture.track_ids[2].clone();

    fixture
        .play_to_side_pause(1, &side_a_track_id, "A", SIDE_PAUSE_VINYL_MESSAGE_KEY)
        .await;

    fixture.playback_handle.resume();

    fixture
        .wait_for_playing_track(
            &next_side_track_id,
            Duration::from_secs(5),
            "resume from side pause should start the next side",
        )
        .await;
}

#[tokio::test]
async fn side_boundary_pause_prevents_gapless_stream_handoff() {
    let mut fixture = SidePauseTestFixture::new("Vinyl", ["A1", "A2", "B1"], true)
        .await
        .expect("side-pause fixture");
    let side_a_track_id = fixture.track_ids[1].clone();
    let side_b_track_id = fixture.track_ids[2].clone();

    fixture.play_track_and_wait(1, &side_a_track_id).await;
    let _side_a_stream = fixture.next_capture_stream().await;

    fixture.seek_to_auto_advance();
    fixture
        .wait_for_side_pause("A", SIDE_PAUSE_VINYL_MESSAGE_KEY)
        .await;

    fixture.playback_handle.resume();
    let _side_b_stream = fixture.next_capture_stream().await;
    fixture
        .wait_for_playing_track(
            &side_b_track_id,
            Duration::from_secs(5),
            "next side should start from a new stream",
        )
        .await;
}

#[derive(Clone, Copy)]
enum SkipDirection {
    Next,
    Previous,
}

impl SkipDirection {
    fn label(self) -> &'static str {
        match self {
            SkipDirection::Next => "Next",
            SkipDirection::Previous => "Previous",
        }
    }
}

/// Next and Previous preserve the current play/pause state: pressing either
/// while paused lands on the adjacent track still paused; while playing, still
/// playing. (Fresh `play` always starts playing — that's the deliberate
/// exception, pinned by `test_fresh_play_always_starts_playing`.)
async fn assert_skip_preserves_play_state(direction: SkipDirection, start_paused: bool) {
    let mut fixture = PlaybackTestFixture::new().await;
    let first_track_id = fixture.track_ids[0].clone();
    let second_track_id = fixture.track_ids[1].clone();

    // Previous needs a track behind the cursor, so start on the second track;
    // Next starts on the first. The target is the adjacent track either way.
    let (start_track_id, target_track_id) = match direction {
        SkipDirection::Next => (first_track_id, second_track_id),
        SkipDirection::Previous => (second_track_id, first_track_id),
    };

    fixture.playback_handle.play(start_track_id.clone());
    fixture
        .wait_for_state(
            |s| {
                matches!(s, PlaybackState::Playing { track_info, .. }
                    if track_info.track_id == start_track_id)
            },
            Duration::from_secs(5),
        )
        .await
        .expect("the starting track should play");

    if start_paused {
        fixture.playback_handle.pause();
        fixture
            .wait_for_state(
                |s| matches!(s, PlaybackState::Paused { .. }),
                Duration::from_secs(2),
            )
            .await
            .expect("playback should pause");
    }

    // Press promptly (well inside Previous's 3s window) so Previous steps back a
    // track rather than restarting the current one.
    match direction {
        SkipDirection::Next => fixture.playback_handle.next(),
        SkipDirection::Previous => fixture.playback_handle.previous(),
    }

    let landed = fixture
        .wait_for_state(
            |s| {
                let (track_info, is_paused) = match s {
                    PlaybackState::Playing { track_info, .. } => (track_info, false),
                    PlaybackState::Paused { track_info, .. } => (track_info, true),
                    _ => return false,
                };
                track_info.track_id == target_track_id && is_paused == start_paused
            },
            Duration::from_secs(5),
        )
        .await;
    assert!(
        landed.is_some(),
        "{} while {} should land on the adjacent track in the same play/pause state",
        direction.label(),
        if start_paused { "paused" } else { "playing" },
    );
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

/// Pause and seek interact correctly in both orderings, which exercise different
/// code paths. Seek issued WHILE paused (the is_playing-not-set-after-seek-while-
/// paused regression: seek clears is_playing, and if only Pause is re-sent it
/// stays false so audio never resumes): the seek lands without auto-playing, and
/// once resumed the position advances — audio actually flows. Seek issued while
/// PLAYING, then paused: the position is preserved across the pause and resume.
#[tokio::test]
async fn pause_and_seek_interact_in_both_orderings() {
    // Ordering 1 — seek while paused, then resume: position advances.
    let mut fixture = PlaybackTestFixture::new().await;
    let track_id = fixture.track_ids[0].clone();
    fixture.playback_handle.play(track_id.clone());
    fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { .. }),
            Duration::from_secs(5),
        )
        .await
        .expect("should start playing");

    fixture.playback_handle.pause();
    fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Paused { .. }),
            Duration::from_secs(2),
        )
        .await
        .expect("should be paused");

    fixture.playback_handle.seek(Duration::from_secs(2));
    let seeked_ms = fixture
        .wait_for_seeked(Duration::from_secs(5))
        .await
        .expect("Seeked event after seeking while paused");
    assert!(
        seeked_ms >= 1900,
        "seek should land near 2s, got {seeked_ms}ms"
    );

    // A seek while paused must not auto-play.
    assert!(
        fixture
            .wait_for_state(
                |s| matches!(s, PlaybackState::Playing { .. }),
                Duration::from_millis(200),
            )
            .await
            .is_none(),
        "should stay paused after seek, not auto-play"
    );

    fixture.playback_handle.resume();
    fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { .. }),
            Duration::from_secs(2),
        )
        .await
        .expect("should resume playing");
    let advanced_ms = fixture
        .wait_for_position_past(seeked_ms, Duration::from_secs(5))
        .await
        .unwrap_or_else(|| {
            panic!(
                "position should advance past the seek target after resume; seeked {seeked_ms}ms"
            )
        });
    assert!(advanced_ms > seeked_ms);

    // Ordering 2 — seek while playing, then pause and resume: position maintained.
    let mut fixture = PlaybackTestFixture::new().await;
    let track_id = fixture.track_ids[0].clone();
    fixture.playback_handle.play(track_id.clone());
    fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { .. }),
            Duration::from_secs(5),
        )
        .await
        .expect("should start playing");

    let seek_target = Duration::from_secs(2);
    fixture.playback_handle.seek(seek_target);
    let seeked_ms = fixture
        .wait_for_seeked(Duration::from_secs(2))
        .await
        .expect("Seeked event");
    assert!(
        Duration::from_millis(seeked_ms).abs_diff(seek_target) < Duration::from_secs(1),
        "seek should land near 2s, got {seeked_ms}ms"
    );

    fixture.playback_handle.pause();
    fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Paused { .. }),
            Duration::from_secs(2),
        )
        .await
        .expect("should be paused");
    fixture.playback_handle.resume();
    fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { .. }),
            Duration::from_secs(2),
        )
        .await
        .expect("should resume playing");
    let maintained_ms = fixture
        .wait_for_position_update(Duration::from_secs(2))
        .await
        .expect("position update after resume");
    assert!(
        Duration::from_millis(maintained_ms).abs_diff(seek_target) < Duration::from_secs(1),
        "position should be maintained across pause/resume, got {maintained_ms}ms"
    );
}

#[tokio::test]
async fn test_fresh_play_always_starts_playing() {
    // Fresh play should always start playing, even if previously paused

    let mut fixture = PlaybackTestFixture::new().await;

    let first_track_id = fixture.track_ids[0].clone();
    let second_track_id = fixture.track_ids[1].clone();

    // Start playing first track
    fixture.playback_handle.play(first_track_id.clone());
    let _playing_state = fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { .. }),
            Duration::from_secs(5),
        )
        .await;

    // Pause
    fixture.playback_handle.pause();
    let paused_state = fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Paused { .. }),
            Duration::from_secs(2),
        )
        .await;
    assert!(paused_state.is_some(), "Should be paused");

    // Fresh play of a different track should start Playing (not Paused)
    fixture.playback_handle.play(second_track_id.clone());

    let new_play_state = fixture
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
        new_play_state.is_some(),
        "Fresh play should always start playing, not paused"
    );
}

/// Test that seeking while playing continues playback and advances position.
///
/// This is the counterpart to test_pause_seek_resume_advances_position.
/// When seeking while playing, playback should continue and position should advance.
#[tokio::test]
async fn test_seek_while_playing_advances_position() {
    let mut fixture = PlaybackTestFixture::new().await;

    let track_id = fixture.track_ids[0].clone();

    // Start playing
    fixture.playback_handle.play(track_id.clone());
    let playing_state = fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { .. }),
            Duration::from_secs(5),
        )
        .await;
    assert!(playing_state.is_some(), "Should start playing");

    // Seek while playing (to 2 seconds)
    let seek_target = Duration::from_secs(2);
    fixture.playback_handle.seek(seek_target);

    // Wait for seek to complete
    let seeked_position = fixture.wait_for_seeked(Duration::from_secs(5)).await;
    assert!(
        seeked_position.is_some(),
        "Should receive Seeked event after seeking while playing"
    );
    let seeked_position_ms = seeked_position.unwrap();
    assert!(
        seeked_position_ms >= 1900,
        "Seeked position should be near 2s, got {}ms",
        seeked_position_ms
    );

    // Position must climb past the seek target — the signal that audio is
    // actually playing, not just that the seek landed.
    let final_position_ms = fixture
        .wait_for_position_past(seeked_position_ms, Duration::from_secs(5))
        .await
        .unwrap_or_else(|| {
            panic!(
                "position should advance past the seek target while playing; seeked to {seeked_position_ms}ms"
            )
        });
    assert!(final_position_ms > seeked_position_ms);
}

/// The true gapless handoff: a track played to its natural end with the next
/// track staged crosses the boundary inside the running stream
/// (boundary_rx → handle_track_crossed → advance_to_preloaded), never rebuilding
/// via the TrackCompleted → AutoAdvance path. The signature is that the
/// finishing track emits its DecodeStats (reported by the boundary handler) but
/// not TrackCompleted, and the incoming track's position resets to 0.
#[tokio::test]
async fn gapless_boundary_hands_off_without_rebuild() {
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
        .expect("the first track should play");

    // Seek partway to bring the natural end sooner while leaving ample runway:
    // the staged next is re-staged well before the decoder reaches EOF. (Seeking
    // right up against the end instead is the not-ready-fallback case below.)
    fixture.playback_handle.seek(Duration::from_secs(3));
    fixture
        .wait_for_seeked(Duration::from_secs(5))
        .await
        .expect("the seek should land");

    let outcome = observe_boundary(
        &mut fixture.progress_rx,
        &first,
        &second,
        Duration::from_secs(8),
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

    let position_ms =
        wait_for_track_position(&mut fixture.progress_rx, &second, Duration::from_secs(2))
            .await
            .expect("a position update for the incoming track");
    assert!(
        position_ms < 1500,
        "the incoming track's position resets near 0 at the boundary, got {position_ms}ms"
    );
}

/// The boundary handler re-preloads the *following* track, so a chain of natural
/// ends crosses gaplessly the whole way down. Play track 0 → track 1 (first
/// crossing, which re-preloads track 2) → track 2: the second crossing is also
/// gapless, which it can only be if track 2 was staged during the first.
#[tokio::test]
async fn gapless_boundary_repreloads_following_track() {
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
        .expect("the first track should play");

    fixture.playback_handle.seek(Duration::from_secs(3));
    fixture
        .wait_for_seeked(Duration::from_secs(5))
        .await
        .expect("the first seek should land");
    let first_cross = observe_boundary(
        &mut fixture.progress_rx,
        &first,
        &second,
        Duration::from_secs(8),
    )
    .await;
    assert!(
        first_cross.reached_incoming,
        "playback should cross into the second track"
    );

    // Shorten the second track too, then let it end. If the third track was
    // re-preloaded and staged during the first crossing, this crosses gaplessly.
    fixture.playback_handle.seek(Duration::from_secs(3));
    fixture
        .wait_for_seeked(Duration::from_secs(5))
        .await
        .expect("the second seek should land");
    let second_cross = observe_boundary(
        &mut fixture.progress_rx,
        &second,
        &third,
        Duration::from_secs(8),
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
}

/// The rebuild advance: seeking right up against the end leaves the post-seek
/// decoder with ~no samples, so it hits EOF and completes rather than crossing
/// the staged boundary — driving TrackCompleted → AutoAdvance →
/// advance_and_play_preloaded, which recovers the preloaded next and plays it
/// (advance.rs ~407, the has_preloaded_next branch). This is the non-gapless
/// counterpart to the two handoff tests above: the next track still plays from
/// its start via the preloaded decoder.
///
/// (The sibling not-ready fallback at ~413 — play_track when the staged source
/// was already consumed — is a defensive race between a boundary crossing and a
/// concurrent Next/AutoAdvance; the serial command loop keeps those from
/// interleaving, so it isn't deterministically reachable from a black-box test.)
#[tokio::test]
async fn boundary_advances_to_next_track_after_late_seek() {
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
        .expect("the first track should play");

    // Tracks run ~5s; seek to 4.8s leaves ~0.2s before the end.
    fixture.playback_handle.seek(Duration::from_millis(4800));
    fixture
        .wait_for_seeked(Duration::from_secs(5))
        .await
        .expect("the seek should land");

    fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == second),
            Duration::from_secs(10),
        )
        .await
        .expect("playback should advance to the second track at the end");
    let position_ms =
        wait_for_track_position(&mut fixture.progress_rx, &second, Duration::from_secs(3))
            .await
            .expect("a position update for the second track");
    assert!(
        position_ms < 1500,
        "the next track starts from ~0, got {position_ms}ms"
    );
}

/// A seek rebuilds the stream, but the staged gapless next must survive it
/// (seek.rs take_next → re-stage_next): the subsequent natural end still crosses
/// gaplessly. Seek twice to stress the take-out/re-stage across an
/// already-re-staged source, then let the track end and assert the boundary is
/// gapless (finishing track emits DecodeStats but not TrackCompleted).
#[tokio::test]
async fn seek_preserves_staged_next_for_a_gapless_advance() {
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
        .expect("the first track should play");

    // Two seeks: each takes the staged next out of the old stream and re-stages
    // it into the rebuilt one. Both land with runway to spare before the end.
    fixture.playback_handle.seek(Duration::from_secs(1));
    fixture
        .wait_for_seeked(Duration::from_secs(5))
        .await
        .expect("the first seek should land");
    fixture.playback_handle.seek(Duration::from_secs(3));
    fixture
        .wait_for_seeked(Duration::from_secs(5))
        .await
        .expect("the second seek should land");

    let outcome = observe_boundary(
        &mut fixture.progress_rx,
        &first,
        &second,
        Duration::from_secs(8),
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
}

/// A same-format seek swaps the persistent output's source in place
/// (`PlaybackSource::replace`) — it never rebuilds the device stream, so it has
/// no `create_stream` step that can fail. Dropping the capture sink's stream
/// receiver (which under the old per-seek-rebuild model made the seek's
/// `create_stream` error and stopped playback) now only fails the non-fatal,
/// logged capture-buffer rotation: the seek still lands and playback continues.
/// This pins that a dropped test observer can't tear playback down.
#[tokio::test]
async fn seek_with_dropped_capture_receiver_keeps_playing() {
    let lib = restore_test_library().await;
    let first = lib.track_ids[0].clone();

    // Build a capture-backed service directly so the test owns the stream
    // receiver and can drop it mid-session (real-time paced so the track doesn't
    // race to its end before we seek).
    let (capture_device, capture_stream_rx) =
        bae_core::playback::RealtimeCaptureAudioDevice::new();
    let mut capture_stream_rx = Some(capture_stream_rx);
    let handle = lib
        .library_manager
        .start_playback_service_with_audio_device(
            lib.runtime_handle,
            100,
            true,
            Box::new(capture_device),
        );
    let mut progress_rx = handle.subscribe_progress();

    handle.play(first.clone());
    wait_for_state_on(
        &mut progress_rx,
        |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == first),
        Duration::from_secs(5),
    )
    .await
    .expect("the track should start playing (first stream created)");

    // Drop the receiver: under the old per-seek rebuild this made create_stream
    // fail; the persistent output uses replace, so the seek must still land.
    capture_stream_rx.take();
    handle.seek(Duration::from_secs(2));

    // The seek lands (Seeked for the same track); no error, no stop.
    let mut saw_seeked = false;
    let mut saw_error = false;
    let mut saw_stopped = false;
    let deadline = Instant::now() + Duration::from_secs(8);
    while Instant::now() < deadline {
        match timeout(Duration::from_millis(200), progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::Seeked { track_id, .. })) if track_id == first => {
                saw_seeked = true;
                break;
            }
            Ok(Some(PlaybackProgress::PlaybackError { .. })) => saw_error = true,
            Ok(Some(PlaybackProgress::StateChanged {
                state: PlaybackState::Stopped,
            })) => {
                saw_stopped = true;
                break;
            }
            Ok(Some(_)) => continue,
            Ok(None) => break,
            Err(_) => continue,
        }
    }
    assert!(saw_seeked, "an in-place seek should land and emit Seeked");
    assert!(
        !saw_error,
        "an in-place seek must not surface a PlaybackError"
    );
    assert!(!saw_stopped, "an in-place seek must not stop playback");

    handle.stop();
}
