fn starved_event(
    track_id: &str,
    starved_ms: u64,
    samples_decoded: u64,
    producer_finished: bool,
) -> AudioEvent {
    AudioEvent::Starved {
        fmt: Arc::new(test_track_fmt(track_id)),
        starved_ms,
        position: StreamPosition::START,
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
        position: StreamPosition::START,
        samples_decoded: 0,
        decode_errors: 0,
    }
}

/// The starvation telemetry says where in the track playback starved, as the
/// player shows it: in a 2 s pregap, 1.5 s into the stream is 0.5 s before the
/// track's start.
#[tokio::test]
async fn starvation_onset_reports_the_track_time_it_starved_at() {
    let (diagnostics, transport) = recording_diagnostics();
    let (_home, manager) = seeded_library_manager_with_diagnostics(&[], diagnostics.clone()).await;
    let (mut service, _progress_rx) = playback_service_over(manager);
    let mut prepared = test_prepared_track("t", create_sparse_buffer(1_024));
    prepared.timeline = TrackTimeline::new(Some(std::time::Duration::from_secs(10)), std::time::Duration::from_millis(2_000));
    let fmt = prepared.track_fmt(StreamPosition::START);
    service.slot = active_slot(prepared, TrackPhase::Playing);

    service
        .handle_audio_event(AudioEvent::Starved {
            fmt: Arc::new(fmt),
            starved_ms: 500,
            position: StreamPosition::from_millis(1_500),
            producer_finished: false,
            samples_decoded: 1_000,
            decode_errors: 0,
            has_next: false,
        })
        .await;

    diagnostics.flush().await.expect("flush succeeds");
    let starved: Vec<serde_json::Value> = transport
        .requests()
        .iter()
        .flat_map(|request| {
            serde_json::from_slice::<Vec<crate::diagnostics::DiagnosticEvent>>(&request.body)
                .expect("recorded request body is a diagnostic-event array")
        })
        .filter(|event| event.name == "playback_starved")
        .map(|event| event.fields["track_time"].clone())
        .collect();
    assert_eq!(starved, [serde_json::json!(-500)]);
}

/// Starvation with no decode progress past the fail threshold is a real stall,
/// so it surfaces a `PlaybackError` and stops playback instead of freezing.
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

/// An output that stops sending partway — an AirPlay conversion failing —
/// stops playback with an error and reports the bug, rather than playing on
/// with audio missing.
#[tokio::test]
async fn an_output_that_stops_sending_stops_playback_and_reports_the_bug() {
    let bugs = Arc::new(crate::diagnostics::RecordingBugReporter::default());
    let diagnostics = crate::diagnostics::Diagnostics::noop().reporting_bugs_to(bugs.clone());
    let (_home, manager) = seeded_library_manager_with_diagnostics(&[], diagnostics).await;
    let (mut service, mut progress_rx) = playback_service_over(manager);
    service.slot = active_slot(
        test_prepared_track("t", create_sparse_buffer(1_024)),
        TrackPhase::Playing,
    );

    service
        .handle_audio_event(AudioEvent::OutputFailed {
            detail: "converting a chunk for the AirPlay speaker failed: swr_convert".to_string(),
        })
        .await;

    let mut saw_error = false;
    while let Ok(progress) = progress_rx.try_recv() {
        saw_error |= matches!(progress, PlaybackProgress::PlaybackError { .. });
    }
    assert!(saw_error, "the person is told playback stopped");
    assert!(matches!(service.slot, PlaybackSlot::Stopped));
    assert_eq!(bugs.reports().len(), 1);
    assert!(bugs.reports()[0].1.contains("swr_convert"));
}

/// `samples_decoded` advancing between `Starved` events shows the producer is
/// alive (a slow cloud fetch, say), so starvation never escalates however long
/// it lasts.
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

/// `producer_finished` means a drained track awaiting `AutoAdvance`, not a stall.
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

/// A `StarvationEnded` resets the watchdog, so the next episode starts fresh.
/// Without the reset, the second `Starved` below, already past the threshold,
/// would continue the first episode and escalate.
#[tokio::test]
async fn starvation_ended_resets_the_episode_clock() {
    let (_home, mut service, mut progress_rx) = test_playback_service().await;
    let buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(test_prepared_track("t", buffer), TrackPhase::Playing);

    // The first episode ends before reaching the fail threshold.
    service
        .handle_audio_event(starved_event("t", 500, 1_000, false))
        .await;
    service
        .handle_audio_event(starvation_ended_event("t"))
        .await;

    // A second episode begins at the same samples_decoded count.
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

/// A failure reported after playback already stopped emits no second Stopped.
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

/// A preview playing to its end tears the preview down and emits
/// `PreviewState::Idle`.
#[tokio::test]
async fn preview_completed_tears_down_and_emits_idle() {
    let (_home, mut service, mut progress_rx) = test_playback_service().await;

    let buffer = create_sparse_buffer(1_024);
    let prepared = test_prepared_track("preview-file", buffer.clone());
    let pipeline = test_pipeline(&prepared, Box::new(TestAudioStream));
    service
        .preview
        .set_active_for_test("preview-file".into(), pipeline, buffer.clone());
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

/// A preview whose stream's device reported it can't go on, and which can't
/// start again because there is no device to start on, ends: the preview is
/// torn down, the UI is told it is idle, and the main player gets back the
/// resume the preview owed it. Sabotage: skip the `preview_stop` after the
/// failed restart, and the main player stays paused.
#[tokio::test]
async fn an_invalidated_preview_that_cant_restart_ends_and_resumes_main() {
    let (_home, mut service, mut progress_rx) = test_playback_service().await;
    let main_buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(
        test_prepared_track("t", main_buffer),
        TrackPhase::Paused(PausePhase::Manual),
    );

    let buffer = create_sparse_buffer(1_024);
    let prepared = test_prepared_track("preview-file", buffer.clone());
    let pipeline = test_pipeline(
        &prepared,
        Box::new(InvalidatedTestStream::new("device disconnected")),
    );
    service
        .preview
        .set_active_for_test("preview-file".into(), pipeline, buffer.clone());
    service.preview.main_player_paused();

    service.follow_preview_output_device().await;

    assert!(!service.preview.is_active(), "the preview ended");
    assert!(buffer.is_cancelled(), "its buffer is cancelled");
    assert!(
        matches!(&service.slot, PlaybackSlot::Active(cur) if matches!(cur.phase, TrackPhase::Playing)),
        "the main player got its resume back"
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
    assert!(saw_idle, "the UI is told the preview is idle");
}

/// Table-drive `playback_state()` over each slot/phase.
#[tokio::test]
async fn playback_state_mapping() {
    let (_home, mut service, _progress_rx) = test_playback_service().await;
    let buffer = create_sparse_buffer(1_024);

    service.slot = PlaybackSlot::Stopped;
    assert!(matches!(service.playback_state(), PlaybackState::Stopped));

    service.slot = PlaybackSlot::Loading(LoadingTrack::Unprepared {
        track_id: "t".to_string(),
    });
    assert!(matches!(
        service.playback_state(),
        PlaybackState::Loading {
            track: LoadingTrack::Unprepared { .. }
        }
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
            track: LoadingTrack::Prepared(_)
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

    service.slot = active_slot(
        test_prepared_track("t", buffer),
        TrackPhase::Paused(PausePhase::SideEnded(SidePauseDecision {
            track_id: "next".to_string(),
            boundary: SideBoundary {
                id: "id".to_string(),
                kind: PlaybackPauseBoundary::Side,
                side_label: "B".to_string(),
            },
            resumes_at: None,
        })),
    );
    assert!(matches!(
        service.playback_state(),
        PlaybackState::Paused {
            reason: PlaybackPauseReason::SideEnded(_),
            ..
        }
    ));
    // `playback_state` treats Completed as unreachable, so it has no case here.
}

/// A direct start lands past a positive pregap, and at the stream's start
/// without one; a natural start always at the stream's start.
#[test]
fn track_start_position_cases() {
    use std::time::Duration;
    let pregapped = TrackTimeline::new(Some(Duration::from_secs(60)), std::time::Duration::from_millis(3000));
    let no_pregap = TrackTimeline::new(Some(Duration::from_secs(60)), std::time::Duration::ZERO);

    assert_eq!(
        TrackStart::Direct.position(pregapped),
        StreamPosition::from_millis(3000)
    );
    assert_eq!(TrackStart::Direct.position(no_pregap), StreamPosition::START);
    assert_eq!(TrackStart::Natural.position(pregapped), StreamPosition::START);
    assert_eq!(
        TrackStart::Position(StreamPosition::from_millis(42_000)).position(pregapped),
        StreamPosition::from_millis(42_000)
    );
}

/// A CUE `PREGAP` directive's silence has no stored pregap behind it; the
/// track's timeline still starts past it, so the seek bar, a direct start and
/// a seek all agree on where INDEX 01 is.
#[test]
fn a_generated_pregap_starts_the_track_past_it() {
    let mut resolved = test_resolved_track_audio("track");
    resolved.generated_pregap_samples = Some(88_200);

    let prepared = finalize_playback_track(
        "track".to_string(),
        &resolved,
        Vec::new(),
        crate::config::ReplayGainMode::Off,
    );

    assert_eq!(prepared.generated_pregap_frames, 88_200);
    assert_eq!(
        prepared.timeline.track_start(),
        StreamPosition::from_millis(2000)
    );
}

/// A stored pregap is counted in samples; its length on the track's timeline
/// comes from them and the sample rate.
#[test]
fn a_stored_pregap_is_as_long_as_its_samples() {
    let mut resolved = test_resolved_track_audio("track");
    resolved.pregap_samples = Some(66_150);

    let prepared = finalize_playback_track(
        "track".to_string(),
        &resolved,
        Vec::new(),
        crate::config::ReplayGainMode::Off,
    );

    assert_eq!(prepared.generated_pregap_frames, 0);
    assert_eq!(
        prepared.timeline.track_start(),
        StreamPosition::from_millis(1500)
    );
}

/// A seek bar's fill is a fraction of the track's length. A track with no
/// length has no position at any fill, so a seek by ratio leaves it playing
/// where it is rather than jumping to a fraction of an assumed length.
#[tokio::test]
async fn a_seek_by_ratio_leaves_a_track_with_no_length_where_it_is() {
    let (_home, mut service, _progress) = test_playback_service().await;
    let mut prepared = test_prepared_track("track", create_sparse_buffer(1_024));
    prepared.timeline = TrackTimeline::new(None, std::time::Duration::ZERO);
    service.slot = active_slot(prepared, TrackPhase::Playing);
    if let PlaybackSlot::Active(cur) = &mut service.slot {
        cur.position = StreamPosition::from_millis(10_000);
    }

    service.seek_by_ratio(0.5).await;

    let PlaybackSlot::Active(cur) = &service.slot else {
        panic!("the track is still current");
    };
    assert!(matches!(cur.phase, TrackPhase::Playing));
    assert_eq!(cur.position, StreamPosition::from_millis(10_000));
}

/// A track with both a generated pregap and an audio-pregap segment before its
/// main body, returned with its two buffers so a test can tell which segment
/// decoding starts from.
fn prepared_track_with_pregap_segments(
) -> (PlaybackPreparedTrack, SharedSparseBuffer, SharedSparseBuffer) {
    let pregap_buffer = create_sparse_buffer(1_024);
    let main_buffer = create_sparse_buffer(2_048);
    let mut prepared = test_prepared_track("track", main_buffer.clone());
    prepared.generated_pregap_frames = 441;
    prepared.timeline = TrackTimeline::new(Some(std::time::Duration::from_secs(1)), std::time::Duration::from_millis(1010));
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
            scrub_points: crate::playback::ScrubPoints::new(Vec::new()),
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
            scrub_points: crate::playback::ScrubPoints::new(Vec::new()),
        },
    ];
    (prepared, pregap_buffer, main_buffer)
}

#[test]
fn direct_start_skips_audio_and_generated_pregap_segments() {
    let (prepared, _pregap_buffer, main_buffer) = prepared_track_with_pregap_segments();

    let decode = prepared.decode_params(0, false);

    assert_eq!(decode.leading_silence_frames(), 0);
    assert_eq!(decode.segment_count(), 1);
    assert_eq!(decode.segment_buffer_id(0), main_buffer.id());
    assert_eq!(decode.segment_target_sample(0), 44_100);
    assert_eq!(
        decode.segment_decode_start(0),
        bae_audio::DecodeStart::Byte(2_000)
    );
}

#[test]
fn natural_start_includes_audio_and_generated_pregap_segments() {
    let (prepared, pregap_buffer, _main_buffer) = prepared_track_with_pregap_segments();

    let decode = prepared.decode_params(0, true);

    assert_eq!(decode.leading_silence_frames(), 441);
    assert_eq!(decode.segment_count(), 2);
    assert_eq!(decode.segment_buffer_id(0), pregap_buffer.id());
    assert_eq!(decode.segment_target_sample(0), 1_000);
    assert_eq!(
        decode.segment_decode_start(0),
        bae_audio::DecodeStart::Byte(100)
    );
}
