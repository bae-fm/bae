fn starved_event(
    track_id: &str,
    starved_ms: u64,
    samples_decoded: u64,
    producer_finished: bool,
) -> AudioEvent {
    AudioEvent::Starved {
        fmt: Arc::new(test_track_fmt(track_id)),
        starved_ms,
        position_ms: 0,
        producer_finished,
        samples_decoded,
        decode_errors: 0,
        has_next: false,
    }
}

fn starvation_ended_event(track_id: &str) -> AudioEvent {
    AudioEvent::StarvationEnded {
        fmt: Arc::new(test_track_fmt(track_id)),
        starved_ms: 0,
        position_ms: 0,
        samples_decoded: 0,
        decode_errors: 0,
    }
}

/// A starvation episode with zero decode progress that persists past the fail
/// threshold is a genuine stall — a decoder wedged for good on a byte buffer
/// that will never produce, not a producer that's merely slow — and must
/// surface a `PlaybackError` and tear playback down rather than log forever
/// with a frozen position bar.
#[tokio::test]
async fn starvation_past_fail_threshold_with_no_progress_escalates_to_error_and_stops() {
    let (_home, mut service, mut progress_rx) = test_playback_service().await;
    let buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(test_prepared_track("t", buffer), TrackPhase::Playing);

    service
        .handle_audio_event(starved_event("t", 500, 1_000, false))
        .await;
    service
        .handle_audio_event(starved_event("t", 30_000, 1_000, false))
        .await;

    let mut saw_error = false;
    while let Ok(progress) = progress_rx.try_recv() {
        if matches!(progress, PlaybackProgress::PlaybackError { .. }) {
            saw_error = true;
        }
    }
    assert!(
        saw_error,
        "a stalled starvation episode must surface a PlaybackError"
    );
    assert!(
        matches!(service.slot, PlaybackSlot::Stopped),
        "the stalled track must stop"
    );
}

/// `samples_decoded` advancing between `Starved` events proves the producer is
/// alive (e.g. a slow cloud fetch) even though the ring is still starved —
/// this must never escalate, however long the starvation drags on.
#[tokio::test]
async fn starvation_with_advancing_samples_decoded_never_escalates() {
    let (_home, mut service, mut progress_rx) = test_playback_service().await;
    let buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(test_prepared_track("t", buffer), TrackPhase::Playing);

    service
        .handle_audio_event(starved_event("t", 500, 1_000, false))
        .await;
    service
        .handle_audio_event(starved_event("t", 30_000, 1_100, false))
        .await;
    service
        .handle_audio_event(starved_event("t", 60_000, 1_200, false))
        .await;

    let mut saw_error = false;
    while let Ok(progress) = progress_rx.try_recv() {
        if matches!(progress, PlaybackProgress::PlaybackError { .. }) {
            saw_error = true;
        }
    }
    assert!(
        !saw_error,
        "advancing samples_decoded must never escalate, regardless of starved_ms"
    );
    assert!(
        matches!(service.slot, PlaybackSlot::Active(_)),
        "the track must stay active"
    );
}

/// `producer_finished == true` is the completion path — a drained track
/// awaiting `AutoAdvance` — never the stall this watchdog targets.
#[tokio::test]
async fn starvation_with_producer_finished_never_escalates() {
    let (_home, mut service, mut progress_rx) = test_playback_service().await;
    let buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(test_prepared_track("t", buffer), TrackPhase::Playing);

    service
        .handle_audio_event(starved_event("t", 500, 1_000, true))
        .await;
    service
        .handle_audio_event(starved_event("t", 60_000, 1_000, true))
        .await;

    let mut saw_error = false;
    while let Ok(progress) = progress_rx.try_recv() {
        if matches!(progress, PlaybackProgress::PlaybackError { .. }) {
            saw_error = true;
        }
    }
    assert!(
        !saw_error,
        "producer_finished starvation is the completion path, never escalates"
    );
}

/// A `StarvationEnded` between episodes resets the watchdog clock: the next
/// episode starts fresh rather than inheriting the ended episode's duration.
/// Sabotage — drop the reset on `StarvationEnded` — and the single event below
/// (whose own `starved_ms` already exceeds the threshold) would be read as a
/// continuation of the first episode's stalled baseline and escalate
/// immediately.
#[tokio::test]
async fn starvation_ended_resets_the_episode_clock() {
    let (_home, mut service, mut progress_rx) = test_playback_service().await;
    let buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(test_prepared_track("t", buffer), TrackPhase::Playing);

    // First episode starts, then ends (the producer resumed) before ever
    // crossing the fail threshold.
    service
        .handle_audio_event(starved_event("t", 500, 1_000, false))
        .await;
    service
        .handle_audio_event(starvation_ended_event("t"))
        .await;

    // A second, independent episode begins at the same samples_decoded count.
    service
        .handle_audio_event(starved_event("t", 30_000, 1_000, false))
        .await;

    let mut saw_error = false;
    while let Ok(progress) = progress_rx.try_recv() {
        if matches!(progress, PlaybackProgress::PlaybackError { .. }) {
            saw_error = true;
        }
    }
    assert!(
        !saw_error,
        "StarvationEnded must reset the episode; the first Starved event after \
         it establishes a fresh baseline rather than escalating immediately"
    );
    assert!(
        matches!(service.slot, PlaybackSlot::Active(_)),
        "the track must stay active"
    );
}

/// `halt_on_error` is a no-op when the slot is already Stopped, so a failure
/// dispatched after a self-handled stop doesn't emit a duplicate Stopped.
#[tokio::test]
async fn halt_on_error_noops_when_stopped() {
    let (_home, mut service, mut progress_rx) = test_playback_service().await;
    // The slot starts Stopped.
    service.halt_on_error().await;
    assert!(
        progress_rx.try_recv().is_err(),
        "halting an already-stopped slot must emit nothing"
    );
}

/// Natural preview completion (a `PreviewCompleted` command) tears the preview
/// pipeline down and emits `PreviewState::Idle`. This pins the service-side
/// contract the preview listener's Completion arm feeds into: PreviewCompleted →
/// stop() → Idle, with the pipeline gone.
#[tokio::test]
async fn preview_completed_tears_down_and_emits_idle() {
    let (_home, mut service, mut progress_rx) = test_playback_service().await;

    let buffer = create_sparse_buffer(1_024);
    let prepared = test_prepared_track("preview-file", buffer.clone());
    let pipeline = test_pipeline(&prepared);
    service
        .preview
        .set_active_for_test("preview-file".to_string(), pipeline, buffer.clone());
    assert!(service.preview.is_active());

    service.preview_completed();

    assert!(
        !service.preview.is_active(),
        "completion tears the active preview down"
    );
    assert!(
        buffer.is_cancelled(),
        "completion cancels the preview buffer"
    );
    let mut saw_idle = false;
    while let Ok(progress) = progress_rx.try_recv() {
        if matches!(
            progress,
            PlaybackProgress::PreviewStateChanged(crate::playback::PreviewState::Idle)
        ) {
            saw_idle = true;
        }
    }
    assert!(saw_idle, "completion emits PreviewState::Idle");
}

/// Table-drive `playback_state()` over each slot/phase.
#[tokio::test]
async fn playback_state_mapping() {
    let (_home, mut service, _progress_rx) = test_playback_service().await;
    let buffer = create_sparse_buffer(1_024);

    service.slot = PlaybackSlot::Stopped;
    assert!(matches!(service.playback_state(), PlaybackState::Stopped));

    service.slot = PlaybackSlot::Loading {
        track_id: "t".to_string(),
        resolved: None,
    };
    assert!(matches!(
        service.playback_state(),
        PlaybackState::Loading { resolved: None, .. }
    ));

    let generation = service.next_load_generation();
    service.slot = active_slot(
        test_prepared_track("t", buffer.clone()),
        TrackPhase::Loading {
            generation,
            target: PlayTarget::Playing,
        },
    );
    assert!(matches!(
        service.playback_state(),
        PlaybackState::Loading {
            resolved: Some(_),
            ..
        }
    ));

    service.slot = active_slot(
        test_prepared_track("t", buffer.clone()),
        TrackPhase::Playing,
    );
    assert!(matches!(
        service.playback_state(),
        PlaybackState::Playing { .. }
    ));

    service.slot = active_slot(
        test_prepared_track("t", buffer.clone()),
        TrackPhase::Paused(PausePhase::Manual),
    );
    assert!(matches!(
        service.playback_state(),
        PlaybackState::Paused {
            reason: PlaybackPauseReason::Manual,
            ..
        }
    ));

    let prompt = PlaybackSidePausePrompt {
        id: "id".to_string(),
        title_key: SIDE_PAUSE_TITLE_KEY,
        side_letter: "B".to_string(),
        message_key: SIDE_PAUSE_VINYL_MESSAGE_KEY,
    };
    service.slot = active_slot(
        test_prepared_track("t", buffer),
        TrackPhase::Paused(PausePhase::SideEnded(SidePauseDecision {
            track_id: "next".to_string(),
            prompt,
        })),
    );
    assert!(matches!(
        service.playback_state(),
        PlaybackState::Paused {
            reason: PlaybackPauseReason::SideEnded(_),
            ..
        }
    ));
    // Completed is never emitted as a public state, so `playback_state` treats it
    // as unreachable rather than mapping it — no arm to assert here.
}

/// `TrackStart::Direct` deriving its position from `pregap_seek_position` is
/// exercised by its own two cases below (a positive pregap needs a seek to
/// it, no pregap needs none) — the same two cases `pregap_seek_position`
/// itself would need, so there's nothing left for a separate direct test of
/// the free function to add.
#[test]
fn track_start_position_cases() {
    use std::time::Duration;

    assert_eq!(
        TrackStart::Direct.position(Some(3000)),
        Duration::from_millis(3000)
    );
    assert_eq!(TrackStart::Direct.position(None), Duration::ZERO);
    assert_eq!(TrackStart::Natural.position(Some(3000)), Duration::ZERO);
    assert_eq!(
        TrackStart::Position(Duration::from_millis(42_000)).position(Some(3000)),
        Duration::from_millis(42_000)
    );
}

#[test]
fn resolved_audio_format_rejects_zero_channels() {
    let resolved = test_resolved_track_audio("track-id", 44_100, 0);

    let error = ensure_resolved_audio_format("track-id", &resolved)
        .expect_err("zero channels should be rejected");

    assert!(error
        .to_string()
        .contains("track track-id has unusable audio format"));
}

#[test]
fn resolved_audio_format_rejects_zero_sample_rate() {
    let resolved = test_resolved_track_audio("track-id", 0, 2);

    let error = ensure_resolved_audio_format("track-id", &resolved)
        .expect_err("zero sample rate should be rejected");

    assert!(error
        .to_string()
        .contains("track track-id has unusable audio format"));
}

#[test]
fn direct_start_skips_audio_and_generated_pregap_segments() {
    let pregap_buffer = create_sparse_buffer(1_024);
    let main_buffer = create_sparse_buffer(2_048);
    let mut prepared = test_prepared_track("track", main_buffer.clone());
    prepared.generated_pregap_samples = Some(441);
    prepared.generated_pregap_ms = Some(10);
    prepared.pregap_ms = Some(1010);
    prepared.segments = vec![
        PreparedAudioSegment {
            role: DbAudioSegmentRole::AudioPregap,
            file_id: "pregap-file".to_string(),
            buffer: pregap_buffer,
            span: crate::db::SegmentSpan {
                start_sample: 1_000,
                end_sample: None,
                start_byte: Some(100),
                end_byte: None,
            },
        },
        PreparedAudioSegment {
            role: DbAudioSegmentRole::Main,
            file_id: "main-file".to_string(),
            buffer: main_buffer.clone(),
            span: crate::db::SegmentSpan {
                start_sample: 44_100,
                end_sample: Some(88_200),
                start_byte: Some(2_000),
                end_byte: Some(4_000),
            },
        },
    ];

    let decode = prepared.decode_params(0, false);

    assert_eq!(decode.leading_silence_frames(), 0);
    assert_eq!(decode.segment_count(), 1);
    assert_eq!(decode.segment_buffer_id(0), main_buffer.id());
    assert_eq!(decode.segment_target_sample(0), 44_100);
    assert_eq!(decode.segment_seek_to_byte(0), Some(2_000));
}

#[test]
fn natural_start_includes_audio_and_generated_pregap_segments() {
    let pregap_buffer = create_sparse_buffer(1_024);
    let main_buffer = create_sparse_buffer(2_048);
    let mut prepared = test_prepared_track("track", main_buffer.clone());
    prepared.generated_pregap_samples = Some(441);
    prepared.generated_pregap_ms = Some(10);
    prepared.pregap_ms = Some(1010);
    prepared.segments = vec![
        PreparedAudioSegment {
            role: DbAudioSegmentRole::AudioPregap,
            file_id: "pregap-file".to_string(),
            buffer: pregap_buffer.clone(),
            span: crate::db::SegmentSpan {
                start_sample: 1_000,
                end_sample: None,
                start_byte: Some(100),
                end_byte: None,
            },
        },
        PreparedAudioSegment {
            role: DbAudioSegmentRole::Main,
            file_id: "main-file".to_string(),
            buffer: main_buffer,
            span: crate::db::SegmentSpan {
                start_sample: 44_100,
                end_sample: Some(88_200),
                start_byte: Some(2_000),
                end_byte: Some(4_000),
            },
        },
    ];

    let decode = prepared.decode_params(0, true);

    assert_eq!(decode.leading_silence_frames(), 441);
    assert_eq!(decode.segment_count(), 2);
    assert_eq!(decode.segment_buffer_id(0), pregap_buffer.id());
    assert_eq!(decode.segment_target_sample(0), 1_000);
    assert_eq!(decode.segment_seek_to_byte(0), Some(100));
}

#[test]
fn generated_pregap_samples_clamps_negative_sample_value() {
    let buffer = create_sparse_buffer(1_024);
    let mut prepared = test_prepared_track("track", buffer);
    prepared.generated_pregap_samples = Some(-1);
    prepared.generated_pregap_ms = Some(10);

    assert_eq!(prepared.generated_pregap_samples(), 0);
}

#[test]
fn generated_pregap_samples_clamps_negative_millisecond_value() {
    let buffer = create_sparse_buffer(1_024);
    let mut prepared = test_prepared_track("track", buffer);
    prepared.generated_pregap_samples = None;
    prepared.generated_pregap_ms = Some(-10);

    assert_eq!(prepared.generated_pregap_samples(), 0);
}
