#[tokio::test]
async fn retiring_preloaded_next_stops_decoder_but_keeps_buffer_alive() {
    let (_home, mut service, _progress) = test_playback_service().await;
    let buffer = create_sparse_buffer(1_024);
    let (_sink, source, _ready) = create_track_stream_pair(44_100, 2);
    let cancel_token = Arc::new(std::sync::atomic::AtomicBool::new(false));
    service.preloaded_next = Some(PreloadedNext {
        prepared: test_prepared_track("next-track", buffer.clone()),
        decoder_handle: finished_decoder_handle(),
        cancel_token: cancel_token.clone(),
        source: PreloadedNextSource::Held(source),
    });

    assert!(service.retire_preloaded_track());

    assert!(service.preloaded_next.is_none());
    assert_eq!(
        service.file_buffers.retired_track_ids(),
        ["next-track"],
        "the prepared track is retained until buffer release"
    );
    assert!(cancel_token.load(std::sync::atomic::Ordering::Acquire));
    // The buffer stays alive: whether it survives is the caller's release
    // decision (`FileBuffers::release` / stop), not the discard's.
    assert!(!buffer.is_cancelled());
}

#[tokio::test]
async fn retiring_preloaded_next_removes_staged_source() {
    let (_home, mut service, _progress) = test_playback_service().await;
    let (_current_sink, current_source, _current_ready) = create_track_stream_pair(44_100, 2);
    let (_next_sink, next_source, _next_ready) = create_track_stream_pair(44_100, 2);
    let gapless = Arc::new(Mutex::new(source::PlaybackSource::new(
        current_source,
        test_track_fmt("current-track"),
    )));
    gapless
        .lock()
        .unwrap()
        .stage_next(next_source, test_track_fmt("next-track"));

    let buffer = create_sparse_buffer(1_024);
    service.preloaded_next = Some(PreloadedNext {
        prepared: test_prepared_track("next-track", buffer.clone()),
        decoder_handle: finished_decoder_handle(),
        cancel_token: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        source: PreloadedNextSource::Staged,
    });

    let (_audio_tx, audio_events) = audio_event_channel();
    service.output = Some(test_output(gapless.clone(), audio_events));
    assert!(service.retire_preloaded_track());

    assert!(service.preloaded_next.is_none());
    assert_eq!(service.file_buffers.retired_track_ids(), ["next-track"]);
    assert!(!gapless.lock().unwrap().has_next());
    assert!(!buffer.is_cancelled());
}

/// Whether any `PlaybackError` reached the UI. A read failure that surfaces one
/// halts playback (the progress self-subscription turns it into `HaltOnError`),
/// so its absence is what says the playing track was left alone.
fn drained_a_playback_error(
    progress_rx: &mut tokio_mpsc::UnboundedReceiver<PlaybackProgress>,
) -> bool {
    let mut saw_error = false;
    while let Ok(progress) = progress_rx.try_recv() {
        if matches!(progress, PlaybackProgress::PlaybackError { .. }) {
            saw_error = true;
        }
    }
    saw_error
}

/// A preloaded next track whose bytes stop arriving — its release was deleted,
/// its cloud fetch failed — breaks nothing the user is hearing, so the playing
/// track keeps playing and no error reaches the UI. The preload is discarded
/// instead: its buffer is cancelled by the time this runs, so a gapless crossing
/// into it would play a truncated track.
#[tokio::test]
async fn read_failure_on_the_preloaded_next_discards_it_and_keeps_playing() {
    let (_home, mut service, mut progress_rx) = test_playback_service().await;
    let current_buffer = create_sparse_buffer(1_024);
    let preload_buffer = create_sparse_buffer(1_024);
    service
        .file_buffers
        .cache_for_test("preload-file", preload_buffer.clone());
    service.slot = active_slot(
        test_prepared_track_with_file("current-track", "current-file", current_buffer),
        TrackPhase::Playing,
    );
    let (_next_sink, next_source, _next_ready) = create_track_stream_pair(44_100, 2);
    service.preloaded_next = Some(PreloadedNext {
        prepared: test_prepared_track_with_file(
            "next-track",
            "preload-file",
            preload_buffer.clone(),
        ),
        decoder_handle: finished_decoder_handle(),
        cancel_token: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        source: PreloadedNextSource::Held(next_source),
    });

    service
        .handle_read_failed(
            preload_buffer.id(),
            PlaybackError::not_found("release file", "preload-file"),
        )
        .await;

    assert!(
        matches!(
            &service.slot,
            PlaybackSlot::Active(cur)
                if cur.prepared.track_info.track_id == "current-track"
                    && matches!(cur.phase, TrackPhase::Playing)
        ),
        "the playing track is untouched by the next track's read failure"
    );
    assert!(
        !drained_a_playback_error(&mut progress_rx),
        "a preload's read failure must not surface an error that halts playback"
    );
    assert!(
        service.preloaded_next.is_none(),
        "the preload whose bytes are gone is discarded"
    );
    assert!(
        !service.file_buffers.holds("preload-file"),
        "its cancelled buffer leaves the shared cache rather than being reused dead"
    );
}

/// The playing track's own bytes stopping is fatal: nothing can play, so the
/// error surfaces and the halt path tears playback down.
#[tokio::test]
async fn read_failure_on_the_playing_track_reports_the_error() {
    let (_home, mut service, mut progress_rx) = test_playback_service().await;
    let current_buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(
        test_prepared_track_with_file("current-track", "current-file", current_buffer.clone()),
        TrackPhase::Playing,
    );

    service
        .handle_read_failed(
            current_buffer.id(),
            PlaybackError::not_found("release file", "current-file"),
        )
        .await;

    assert!(
        drained_a_playback_error(&mut progress_rx),
        "the playing track's read failure surfaces as a playback error"
    );
}

/// A buffer that serves neither the current track nor the preload left the
/// pipeline before its failure surfaced (released on a track change, cancelled
/// by a stop). There is nothing to halt, and halting would kill whatever the
/// user started in the meantime.
#[tokio::test]
async fn read_failure_on_a_buffer_out_of_play_is_ignored() {
    let (_home, mut service, mut progress_rx) = test_playback_service().await;
    let abandoned_buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(
        test_prepared_track_with_file("current-track", "current-file", create_sparse_buffer(1_024)),
        TrackPhase::Playing,
    );

    service
        .handle_read_failed(
            abandoned_buffer.id(),
            PlaybackError::not_found("release file", "abandoned-file"),
        )
        .await;

    assert!(
        matches!(
            &service.slot,
            PlaybackSlot::Active(cur) if cur.prepared.track_info.track_id == "current-track"
        ),
        "a failure from a buffer out of play leaves the current track alone"
    );
    assert!(
        !drained_a_playback_error(&mut progress_rx),
        "a failure from a buffer out of play surfaces no error"
    );
}

#[tokio::test]
async fn seek_drains_pending_gapless_crossing_before_reading_current_track() {
    let (_home, mut service, mut progress_rx) = test_playback_service().await;
    service.playback_queue.apply(|queue| {
        queue.play_release(
            ContextSource::Release("release-id".to_string()),
            vec!["finished-track".to_string(), "incoming-track".to_string()],
            ContextStart::Index(0),
        )
    });

    let finished_buffer = create_sparse_buffer(1_024);
    let incoming_buffer = create_sparse_buffer(1_024);
    let (mut audio_tx, audio_rx) = audio_event_channel();
    audio_tx.push_required(AudioEvent::TrackCrossing(TrackCrossing {
        finished_fmt: Arc::new(test_track_fmt("finished-track")),
        decode_error_count: 0,
        samples_decoded: 44_100,
        incoming_fmt: Arc::new(test_track_fmt("incoming-track")),
    }));
    let (_sink, source, _ready) = create_track_stream_pair(44_100, 2);
    let source = Arc::new(Mutex::new(source::PlaybackSource::new(
        source,
        test_track_fmt("finished-track"),
    )));
    // The crossing event lives in the persistent output's audio-events receiver;
    // the source and receiver survive the track transition.
    service.output = Some(test_output(source, audio_rx));
    service.slot = PlaybackSlot::Active(CurrentTrack {
        prepared: test_prepared_track("finished-track", finished_buffer.clone()),
        decoder: test_decoder(),
        phase: TrackPhase::Playing,
    });
    service.current_position_shared =
        Arc::new(std::sync::Mutex::new(Some(std::time::Duration::ZERO)));
    service.preloaded_next = Some(PreloadedNext {
        prepared: test_prepared_track("incoming-track", incoming_buffer),
        decoder_handle: finished_decoder_handle(),
        cancel_token: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        source: PreloadedNextSource::Staged,
    });

    service.seek(std::time::Duration::ZERO).await;

    assert_eq!(service.slot.current_track_id().unwrap(), "incoming-track");
    let mut saw_incoming_seek = false;
    while let Ok(progress) = progress_rx.try_recv() {
        if let PlaybackProgress::Seeked { track_id, .. } = progress {
            saw_incoming_seek = track_id == "incoming-track";
        }
    }
    assert!(
        saw_incoming_seek,
        "seek should emit for the crossed-into track"
    );
    assert!(finished_buffer.is_cancelled());
}

/// After a track drains naturally, the decoder-completion callback flips the
/// shared audio-state atomic to `Stopped` while the track's bookkeeping is
/// retained (so AutoAdvance / the side-pause decision can still read it). A seek
/// arriving in that window must resume audible playback at the seek target — not
/// rebuild a stream that stays silent because the atomic is still `Stopped`.
#[tokio::test]
async fn seek_after_natural_completion_resumes_audibly() {
    let (_home, mut service, _progress_rx) = test_playback_service().await;
    service.playback_queue.apply(|queue| {
        queue.play_release(
            ContextSource::Release("release-id".to_string()),
            vec!["finished-track".to_string()],
            ContextStart::Index(0),
        )
    });

    let buffer = create_sparse_buffer(1_024);
    // The track drained: phase is Completed with its bookkeeping retained, and
    // the audio callback already flipped the atomic to Stopped.
    service.slot = active_slot(
        test_prepared_track("finished-track", buffer.clone()),
        TrackPhase::Completed,
    );
    service.current_position_shared =
        Arc::new(std::sync::Mutex::new(Some(std::time::Duration::ZERO)));
    service
        .audio_output
        .set_state(crate::playback::audio_output::AudioState::Stopped);

    service.seek(std::time::Duration::from_millis(500)).await;

    assert_eq!(
        service.audio_output.get_state(),
        crate::playback::audio_output::AudioState::Playing,
        "seeking after a track finished naturally should resume audible playback"
    );
}

/// Skipping to the next track after the current one drained naturally must
/// resume audible playback, not carry the completion's Stopped atomic forward as
/// a silent/paused next track. Drives the real `handle_next` over a Completed
/// slot (the phase a track sits in briefly after natural completion, before
/// AutoAdvance runs) with a preloaded next so no DB lookup is needed. This locks
/// the `TrackPhase::Completed` arm of `current_play_target`; reverting that arm
/// to a paused/stopped target turns the atomic assertion red.
#[tokio::test]
async fn next_after_natural_completion_resumes_audibly() {
    let (_home, mut service, _progress_rx) = test_playback_service().await;
    service.playback_queue.apply(|queue| {
        queue.play_release(
            ContextSource::Release("release-id".to_string()),
            vec!["finished-track".to_string(), "next-track".to_string()],
            ContextStart::Index(0),
        )
    });

    // The current track drained naturally: phase Completed, and the audio
    // callback already flipped the atomic to Stopped.
    let finished_buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(
        test_prepared_track("finished-track", finished_buffer),
        TrackPhase::Completed,
    );
    service.current_position_shared =
        Arc::new(std::sync::Mutex::new(Some(std::time::Duration::ZERO)));
    service
        .audio_output
        .set_state(crate::playback::audio_output::AudioState::Stopped);

    // A next track is preloaded and ready to play without a fresh decode.
    let (_next_sink, next_source, _next_ready) = create_track_stream_pair(44_100, 2);
    let next_buffer = create_sparse_buffer(1_024);
    service.preloaded_next = Some(PreloadedNext {
        prepared: test_prepared_track("next-track", next_buffer),
        decoder_handle: finished_decoder_handle(),
        cancel_token: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        source: PreloadedNextSource::Held(next_source),
    });

    service.handle_next().await;

    assert_eq!(
        service.audio_output.get_state(),
        crate::playback::audio_output::AudioState::Playing,
        "Next after natural completion should resume audible playback on the new track"
    );
    assert!(
        matches!(
            &service.slot,
            PlaybackSlot::Active(cur)
                if cur.prepared.track_info.track_id == "next-track"
                    && matches!(cur.phase, TrackPhase::Playing)
        ),
        "the next track should be current and Playing"
    );
}

#[tokio::test]
async fn gapless_crossing_evicts_finished_track_file_buffer() {
    let (_home, mut service, _progress_rx) = test_playback_service().await;
    service.playback_queue.apply(|queue| {
        queue.play_release(
            ContextSource::Release("release-id".to_string()),
            vec!["finished-track".to_string(), "incoming-track".to_string()],
            ContextStart::Index(0),
        )
    });

    let finished_buffer = create_sparse_buffer(1_024);
    let incoming_buffer = create_sparse_buffer(1_024);
    service
        .file_buffers
        .cache_for_test("finished-file", finished_buffer.clone());
    service
        .file_buffers
        .cache_for_test("incoming-file", incoming_buffer.clone());
    service.slot = active_slot(
        test_prepared_track_with_file("finished-track", "finished-file", finished_buffer.clone()),
        TrackPhase::Playing,
    );
    service.preloaded_next = Some(PreloadedNext {
        prepared: test_prepared_track_with_file("incoming-track", "incoming-file", incoming_buffer),
        decoder_handle: finished_decoder_handle(),
        cancel_token: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        source: PreloadedNextSource::Staged,
    });

    service
        .handle_track_crossed(TrackCrossing {
            finished_fmt: Arc::new(test_track_fmt("finished-track")),
            decode_error_count: 0,
            samples_decoded: 44_100,
            incoming_fmt: Arc::new(test_track_fmt("incoming-track")),
        })
        .await;

    assert!(!service.file_buffers.holds("finished-file"));
    assert!(service.file_buffers.holds("incoming-file"));
    assert!(finished_buffer.is_cancelled());
}

#[tokio::test]
async fn gapless_crossing_keeps_file_buffer_used_by_incoming_track() {
    let (_home, mut service, _progress_rx) = test_playback_service().await;
    service.playback_queue.apply(|queue| {
        queue.play_release(
            ContextSource::Release("release-id".to_string()),
            vec!["finished-track".to_string(), "incoming-track".to_string()],
            ContextStart::Index(0),
        )
    });

    let shared_buffer = create_sparse_buffer(1_024);
    service
        .file_buffers
        .cache_for_test("shared-file", shared_buffer.clone());
    service.slot = active_slot(
        test_prepared_track_with_file("finished-track", "shared-file", shared_buffer.clone()),
        TrackPhase::Playing,
    );
    service.preloaded_next = Some(PreloadedNext {
        prepared: test_prepared_track_with_file(
            "incoming-track",
            "shared-file",
            shared_buffer.clone(),
        ),
        decoder_handle: finished_decoder_handle(),
        cancel_token: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        source: PreloadedNextSource::Staged,
    });

    service
        .handle_track_crossed(TrackCrossing {
            finished_fmt: Arc::new(test_track_fmt("finished-track")),
            decode_error_count: 0,
            samples_decoded: 44_100,
            incoming_fmt: Arc::new(test_track_fmt("incoming-track")),
        })
        .await;

    assert!(service.file_buffers.holds("shared-file"));
    assert!(!shared_buffer.is_cancelled());
}

/// A `TrackReady` from a superseded load (same track id, replayed through a
/// fresh load) carries the old generation and must be dropped; only the live
/// load's generation resolves the phase and emits. A same-id replay is exactly
/// the case load identity must reject.
#[tokio::test]
async fn track_ready_with_stale_generation_is_ignored() {
    let (_home, mut service, mut progress_rx) = test_playback_service().await;
    let stale = service.next_load_generation();
    let live = service.next_load_generation();
    let buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(
        test_prepared_track("t", buffer),
        TrackPhase::Loading {
            generation: live,
            target: PlayTarget::Playing,
        },
    );

    service.resolve_track_ready("t".to_string(), stale);
    assert!(
        progress_rx.try_recv().is_err(),
        "a stale-generation TrackReady must not emit"
    );
    assert!(
        matches!(
            &service.slot,
            PlaybackSlot::Active(cur) if matches!(cur.phase, TrackPhase::Loading { .. })
        ),
        "the phase must stay Loading after a stale signal"
    );

    service.resolve_track_ready("t".to_string(), live);
    assert!(
        matches!(
            progress_rx.try_recv(),
            Ok(PlaybackProgress::StateChanged {
                state: PlaybackState::Playing { .. }
            })
        ),
        "the live load resolves to Playing and emits"
    );
}

/// Pausing during a load collapses the Loading phase to Paused (emitting Paused
/// once); the pending `TrackReady` then no longer matches the phase and is
/// dropped rather than re-emitting a second Paused.
#[tokio::test]
async fn pause_during_load_emits_paused_and_supersedes_track_ready() {
    let (_home, mut service, mut progress_rx) = test_playback_service().await;
    let generation = service.next_load_generation();
    let buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(
        test_prepared_track("t", buffer),
        TrackPhase::Loading {
            generation,
            target: PlayTarget::Playing,
        },
    );

    service.pause();
    assert!(
        matches!(
            progress_rx.try_recv(),
            Ok(PlaybackProgress::StateChanged {
                state: PlaybackState::Paused {
                    reason: PlaybackPauseReason::Manual,
                    ..
                }
            })
        ),
        "pause during a load emits Paused(Manual)"
    );

    service.resolve_track_ready("t".to_string(), generation);
    assert!(
        progress_rx.try_recv().is_err(),
        "the collapsed load's TrackReady must not emit a second state"
    );
}
