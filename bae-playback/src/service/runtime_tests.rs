use super::*;
use crate::audio_output::{
    audio_event_channel, AudioError, AudioEventSender, AudioOutputControls, AudioStream,
};
use crate::service::tests::*;
use bae_audio::track_stream::create_track_stream_pair;
use bae_util::test_channels::HANG_GUARD;

#[tokio::test]
async fn retiring_preloaded_next_stops_decoder_but_keeps_buffer_alive() {
    let (_home, mut service, _progress) = test_playback_service().await;
    let buffer = create_sparse_buffer(1_024);
    let (_sink, source, _ready) = create_track_stream_pair(44_100, 2);
    let cancel_token = Arc::new(std::sync::atomic::AtomicBool::new(false));
    service.preloaded_next = Some(PreloadedNext {
        cancel_token: cancel_token.clone(),
        ..test_preloaded_next(
            test_prepared_track(NEXT_TRACK, buffer.clone()),
            PreloadedNextSource::Held(source),
        )
    });

    assert!(service.retire_preloaded_track());

    assert!(service.preloaded_next.is_none());
    assert_eq!(
        service.file_buffers.retired_track_ids(),
        [NEXT_TRACK],
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
        TrackFmt::for_test("current-track"),
    )));
    gapless.lock().unwrap().stage_next(next_source, TrackFmt::for_test(NEXT_TRACK));

    let buffer = create_sparse_buffer(1_024);
    service.preloaded_next = Some(test_preloaded_next(
        test_prepared_track(NEXT_TRACK, buffer.clone()),
        PreloadedNextSource::Staged,
    ));

    let (_audio_tx, audio_events) = audio_event_channel().unwrap();
    service.output = Some(test_output(gapless.clone(), audio_events));
    assert!(service.retire_preloaded_track());

    assert!(service.preloaded_next.is_none());
    assert_eq!(service.file_buffers.retired_track_ids(), [NEXT_TRACK]);
    assert!(!gapless.lock().unwrap().has_next());
    assert!(!buffer.is_cancelled());
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
    service.file_buffers.cache_for_test("preload-file", preload_buffer.clone());
    service.slot = active_slot(
        test_prepared_track_with_file("current-track", "current-file", current_buffer),
        TrackPhase::Playing,
    );
    let (_next_sink, next_source, _next_ready) = create_track_stream_pair(44_100, 2);
    service.preloaded_next = Some(test_preloaded_next(
        test_prepared_track_with_file(NEXT_TRACK, "preload-file", preload_buffer.clone()),
        PreloadedNextSource::Held(next_source),
    ));

    service
        .handle_read_failed(
            preload_buffer.id(),
            std::sync::Arc::new(bae_track_audio::TrackReadError::OriginalMissing),
        )
        .await;

    assert!(
        matches!(
            &service.slot,
            PlaybackSlot::Active(cur)
                if cur.prepared.track_id == "current-track"
                    && matches!(cur.phase, TrackPhase::Playing)
        ),
        "the playing track is untouched by the next track's read failure"
    );
    assert!(
        !drained_a_playback_error(&mut progress_rx),
        "a preload's read failure must not surface an error that halts playback"
    );
    assert!(service.preloaded_next.is_none(), "the preload whose bytes are gone is discarded");
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
            std::sync::Arc::new(bae_track_audio::TrackReadError::OriginalMissing),
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
            std::sync::Arc::new(bae_track_audio::TrackReadError::OriginalMissing),
        )
        .await;

    assert!(
        matches!(
            &service.slot,
            PlaybackSlot::Active(cur) if cur.prepared.track_id == "current-track"
        ),
        "a failure from a buffer out of play leaves the current track alone"
    );
    assert!(
        !drained_a_playback_error(&mut progress_rx),
        "a failure from a buffer out of play surfaces no error"
    );
}

/// A failure reported after playback already stopped emits no second Stopped.
#[tokio::test]
async fn halt_on_error_noops_when_stopped() {
    let (_home, mut service, mut progress_rx) = test_playback_service().await;
    // The slot starts Stopped.
    service.halt_on_error().await;
    assert!(progress_rx.try_recv().is_err(), "halting an already-stopped slot must emit nothing");
}

/// A `Play` whose context load fails (here: the track doesn't exist) must
/// fail loud — a `PlaybackError` and nothing else — rather than silently
/// falling back to a single-track queue. `release_id` is a required column,
/// so there is no legitimate "track with no context" case to preserve a
/// fallback for.
#[tokio::test]
async fn play_context_load_failure_surfaces_error_without_touching_the_queue() {
    let (_home, mut service, mut progress_rx) = test_playback_service().await;
    let queue_before = crate::tables::queue_value(&service.store).await.unwrap();

    service.handle_play("4b6f0e2a-91c3-4d58-b7e6-0a2f3c4d5e6f".to_string()).await;

    let mut saw_error = false;
    let mut saw_playing = false;
    while let Ok(progress) = progress_rx.try_recv() {
        match progress {
            PlaybackProgress::PlaybackError { .. } => saw_error = true,
            PlaybackProgress::StateChanged { state: PlaybackState::Playing { .. } } => {
                saw_playing = true
            }
            _ => {}
        }
    }
    assert!(saw_error, "a failed context load must surface a PlaybackError");
    assert_eq!(
        crate::tables::queue_value(&service.store).await.unwrap(),
        queue_before,
        "a failed context load must not mutate the queue with a fallback single-track entry"
    );
    assert!(!saw_playing, "a failed context load must never reach Playing");
    assert!(
        matches!(service.slot, PlaybackSlot::Stopped),
        "the slot must stay Stopped, not start loading a track whose context failed"
    );
}

/// A stale `AutoAdvance` — for a track that is no longer current because the user
/// pressed Next first — must be dropped, not advance again (which would skip the
/// track Next landed on). The completed track's id no longer matches the current
/// track, so the advance is stale.
#[tokio::test]
async fn auto_advance_ignores_a_stale_track_id_after_a_manual_next() {
    let (_home, mut service, _rx) = test_playback_service().await;
    let buffer = create_sparse_buffer(1_024);
    // The user already pressed Next: B is current and Playing. A stale
    // AutoAdvance for the previously-completed A arrives afterward.
    service.slot = active_slot(test_prepared_track("B", buffer), TrackPhase::Playing);

    service.handle_auto_advance("A".to_string()).await;

    assert_eq!(
        service.slot.current_track_id(),
        Some("B"),
        "a stale AutoAdvance for a no-longer-current track must not advance"
    );
}

/// A stale `AutoAdvance` whose track IS still current but is no longer in the
/// `Completed` phase — because a seek after its completion reset the phase — must
/// also be dropped, or the queued advance would abandon the seek the user just
/// made.
#[tokio::test]
async fn auto_advance_ignores_a_matching_track_that_is_no_longer_completed() {
    let (_home, mut service, _rx) = test_playback_service().await;
    let buffer = create_sparse_buffer(1_024);
    // A is current again and Playing (a seek after its completion moved the phase
    // off Completed). The stale AutoAdvance for A must not advance.
    service.slot = active_slot(test_prepared_track("A", buffer), TrackPhase::Playing);

    service.handle_auto_advance("A".to_string()).await;

    assert_eq!(
        service.slot.current_track_id(),
        Some("A"),
        "AutoAdvance must only fire while the completed track is still Completed"
    );
}

/// A play command ships `playback_command` (the user intent), `playback_started`
/// (the new context), and `track_started` (the track it begins), driven through
/// the real command loop out to the (recording) Datadog transport. Driving it as
/// a queued command — not a direct `handle_play` — is what exercises the
/// `playback_command` emission, which lives in the loop, not the handler. The
/// seeded release has no backing audio, so preparing the track fails after all
/// three events are already emitted — the events fire at the command, not on a
/// successful decode.
#[tokio::test]
async fn a_play_command_ships_playback_command_started_and_track_started() {
    let (diagnostics, transport) = recording_diagnostics();
    let store = seeded_store(&[(
        "c61a9e19-f3ba-4728-842c-c59dbc82e238",
        &["08c80007-b56a-4fc9-8df6-af2967fa09b9", "08c7ff07-b56a-4e16-8df6-ae2967fa0806"],
    )])
    .await;
    let (mut service, _progress_rx) = playback_service_over(store.store(), diagnostics.clone());

    // Queue the play command and a shutdown, then let the real loop drain both:
    // it emits the command telemetry, runs the play, then breaks on shutdown.
    let commands = service.command_tx.clone();
    dispatch_command(
        &commands,
        PlaybackCommand::Play("08c80007-b56a-4fc9-8df6-af2967fa09b9".to_string()),
    );
    let (shutdown_tx, _shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    dispatch_command(&commands, PlaybackCommand::Shutdown(shutdown_tx));
    service.run().await;

    diagnostics.flush().await.expect("flush succeeds");
    let names = transport.event_names();
    assert!(
        names.iter().any(|n| n == "playback_command"),
        "a play command ships playback_command (got {names:?})"
    );
    assert!(
        names.iter().any(|n| n == "playback_started"),
        "a play command ships playback_started (got {names:?})"
    );
    assert!(
        names.iter().any(|n| n == "track_started"),
        "a play command ships track_started (got {names:?})"
    );
}

/// `Previous` ships `track_started` for the track it lands on — a path that
/// carried no emission until the event moved into `play_track`. The seeded
/// release has no backing audio, so preparing the previous track fails after
/// the event is already emitted: the event fires at the command, not on a
/// successful decode.
#[tokio::test]
async fn previous_ships_track_started() {
    let (diagnostics, transport) = recording_diagnostics();
    let store = seeded_store(&[(
        "c61a9e19-f3ba-4728-842c-c59dbc82e238",
        &["08c80007-b56a-4fc9-8df6-af2967fa09b9", "08c7ff07-b56a-4e16-8df6-ae2967fa0806"],
    )])
    .await;
    let (mut service, _progress_rx) = playback_service_over(store.store(), diagnostics.clone());

    // A two-track context playing from t1, so Previous steps back to t0.
    service
        .playback_queue
        .play(ContextFill::Release {
            release_id: "c61a9e19-f3ba-4728-842c-c59dbc82e238".to_string(),
            start: ReleaseStart::Track("08c7ff07-b56a-4e16-8df6-ae2967fa0806".to_string()),
        })
        .await
        .unwrap();
    let buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(
        test_prepared_track("08c7ff07-b56a-4e16-8df6-ae2967fa0806", buffer),
        TrackPhase::Playing,
    );

    service.handle_previous().await;

    diagnostics.flush().await.expect("flush succeeds");
    let names = transport.event_names();
    assert!(
        names.iter().any(|n| n == "track_started"),
        "Previous ships track_started (got {names:?})"
    );
}

/// An output whose end of playback panics, the first time: a bug in one
/// command's handling.
struct EndPanicsOutput {
    panicked: bool,
}

impl crate::audio_output::AudioOutput for EndPanicsOutput {
    fn create_stream(
        &mut self,
        _controls: crate::audio_output::AudioOutputControls,
        _source: Arc<Mutex<source::PlaybackSource>>,
        _source_sample_rate: u32,
        _source_channels: u32,
        _audio_events: AudioEventSender,
        _position_update_interval_ms: u32,
    ) -> Result<Box<dyn AudioStream>, AudioError> {
        Err(AudioError::Unexpected("this test's output builds no stream".to_string()))
    }

    fn playback_ended(&mut self) {
        if !std::mem::replace(&mut self.panicked, true) {
            panic!("the output's end failed")
        }
    }
}

/// A command that panics is reported, stops playback with an error, and the
/// service goes on to serve the next command. Sabotage: drop the boundary in
/// `run`, and the panic ends the service, so the next command is never
/// answered.
#[tokio::test]
async fn a_command_that_panics_is_reported_and_the_next_is_served() {
    let bugs = Arc::new(bae_diagnostics::RecordingBugReporter::default());
    let diagnostics = bae_diagnostics::Diagnostics::noop().reporting_bugs_to(bugs.clone());
    let store = seeded_store(&[]).await;
    let (mut service, mut progress_rx) = playback_service_over(store.store(), diagnostics.clone());
    service.audio_output = crate::audio_output::PlayerOutput::new(
        Box::new(EndPanicsOutput { panicked: false }),
        AudioOutputControls::new(crate::Volume::FULL),
    );
    let command_tx = service.command_tx.clone();
    let (saved_tx, saved_rx) = oneshot::channel();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    dispatch_command(&command_tx, PlaybackCommand::Stop);
    dispatch_command(&command_tx, PlaybackCommand::SaveState(saved_tx));
    dispatch_command(&command_tx, PlaybackCommand::Shutdown(shutdown_tx));

    service.run().await;

    saved_rx.await.expect("the next command is served");
    shutdown_rx.await.expect("the service shuts down when asked");
    let mut saw_error = false;
    while let Ok(progress) = progress_rx.try_recv() {
        saw_error |= matches!(progress, PlaybackProgress::PlaybackError { .. });
    }
    assert!(saw_error, "the person is told playback stopped");
    assert_eq!(
        bugs.reports(),
        vec![(
            "serve playback".to_string(),
            "serve playback panicked: the output's end failed".to_string()
        )]
    );
}

/// A track deleted where playback didn't ask for it — its release deleted
/// in the library, as another device's deletion arriving by sync deletes it —
/// leaves the player all the same: the playing track's row going stops
/// playback.
#[tokio::test]
async fn a_playing_track_deleted_by_sync_stops_playback() {
    const RELEASE: &str = "c61a9e19-f3ba-4728-842c-c59dbc82e238";
    let (store, mut service, mut progress_rx) =
        seeded_playback_service(&[(RELEASE, &[TRACK_A])]).await;
    fill_test_release(&mut service, RELEASE).await;
    service.slot =
        active_slot(test_prepared_track(TRACK_A, create_sparse_buffer(1_024)), TrackPhase::Playing);
    let commands = service.command_tx.clone();
    let db = store.store().clone();

    let delete_then_watch = async move {
        let records = bae_records::test_fixtures::test_records(&db);
        bae_records::test_fixtures::delete_release(&records, RELEASE).await;
        let stopped = tokio::time::timeout(HANG_GUARD, async {
            while let Some(progress) = progress_rx.recv().await {
                if matches!(
                    progress,
                    PlaybackProgress::StateChanged { state: PlaybackState::Stopped }
                ) {
                    return;
                }
            }
        })
        .await;
        let (shutdown_tx, _shutdown_rx) = oneshot::channel();
        dispatch_command(&commands, PlaybackCommand::Shutdown(shutdown_tx));
        stopped
    };
    let ((), stopped) = tokio::join!(service.run(), delete_then_watch);

    assert!(stopped.is_ok(), "deleting the playing track's row stops playback");
}

/// A track that drained into an AirPlay receiver completes once the audio
/// still in the receiver has been heard: the loop waits for that deadline on
/// the clock, with no output or tick to drive it. Sabotage: leave the audible
/// deadline out of the loop's waits, and the track never completes.
#[tokio::test]
async fn the_loop_completes_an_airplay_track_when_its_last_audio_is_heard() {
    use std::time::Duration;
    const RELEASE: &str = "c61a9e19-f3ba-4728-842c-c59dbc82e238";
    let (_home, mut service, mut progress) =
        seeded_playback_service(&[(RELEASE, &[TRACK_T])]).await;
    let clock = Arc::new(crate::ManualPlaybackClock::new(test_clock_start()));
    service.clock = clock.clone();
    let mut prepared = test_prepared_track(TRACK_T, create_sparse_buffer(1024));
    prepared.timeline = TrackTimeline::new(Some(Duration::from_secs(40)), Duration::ZERO);
    prepared.sample_rate = 100;
    prepared.channels = 1;
    let (mut sink, stream, _) =
        bae_audio::track_stream::create_track_stream_pair_with_capacity(100, 1, 4000);
    sink.push_samples(&vec![0.25; 4000]);
    sink.mark_finished();
    let mut source = source::PlaybackSource::new(stream, prepared.track_fmt(StreamPosition::START));
    service.slot = active_slot(prepared, TrackPhase::Playing);
    let _airplay = install_airplay(&mut service, Duration::from_secs(2));
    assert!(service.begin_listening().await);
    let (mut tx, _) = audio_event_channel().unwrap();
    source.pull_samples(&mut vec![0.0; 4000], &mut tx);
    service
        .handle_audio_event(AudioEvent::Completion(source.take_completion_event().unwrap()))
        .await;
    assert!(drained_track_completed_ids(&mut progress).is_empty());
    assert_eq!(service.audible_deadline(), Some(test_clock_start() + chrono::Duration::seconds(2)));

    let commands = service.command_tx.clone();
    let hear_the_tail = async move {
        clock.advance(Duration::from_secs(2));
        let completed = tokio::time::timeout(HANG_GUARD, async {
            while let Some(progress) = progress.recv().await {
                if matches!(progress, PlaybackProgress::TrackCompleted { .. }) {
                    return;
                }
            }
        })
        .await;
        let (shutdown_tx, _shutdown_rx) = oneshot::channel();
        dispatch_command(&commands, PlaybackCommand::Shutdown(shutdown_tx));
        completed
    };
    let ((), completed) = tokio::join!(service.run(), hear_the_tail);

    assert!(completed.is_ok(), "the track completes once its last audio is heard");
}

/// An event the output pushes reaches the loop with nothing polling the ring:
/// the push wakes it. Sabotage: push without the relay's wake, and the loop
/// never sees the completion.
#[tokio::test]
async fn the_loop_drains_an_event_the_moment_the_output_pushes_it() {
    const RELEASE: &str = "c61a9e19-f3ba-4728-842c-c59dbc82e238";
    let (_home, mut service, mut progress) =
        seeded_playback_service(&[(RELEASE, &[TRACK_T])]).await;
    let prepared = test_prepared_track(TRACK_T, create_sparse_buffer(1024));
    let (mut sink, stream, _) = create_track_stream_pair(44_100, 2);
    sink.mark_finished();
    let source = Arc::new(Mutex::new(source::PlaybackSource::new(
        stream,
        prepared.track_fmt(StreamPosition::START),
    )));
    service.slot = active_slot(prepared, TrackPhase::Playing);
    let (mut audio_tx, audio_rx) = audio_event_channel().unwrap();
    service.output = Some(test_output(source.clone(), audio_rx));

    let commands = service.command_tx.clone();
    let push_the_completion = async move {
        let completion = source.lock().unwrap().take_completion_event().unwrap();
        audio_tx.push_required(AudioEvent::Completion(completion));
        let completed = tokio::time::timeout(HANG_GUARD, async {
            while let Some(progress) = progress.recv().await {
                if matches!(progress, PlaybackProgress::TrackCompleted { .. }) {
                    return;
                }
            }
        })
        .await;
        let (shutdown_tx, _shutdown_rx) = oneshot::channel();
        dispatch_command(&commands, PlaybackCommand::Shutdown(shutdown_tx));
        completed
    };
    let ((), completed) = tokio::join!(service.run(), push_the_completion);

    assert!(completed.is_ok(), "the pushed completion is handled");
}
