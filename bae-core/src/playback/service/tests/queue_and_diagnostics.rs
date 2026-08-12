/// A multi-release play concatenates each release's tracks in the input order the
/// releases were chosen, and reports the releases that contributed as the source.
#[tokio::test]
async fn play_releases_concatenates_tracks_in_input_order() {
    let (_home, service, _rx) = seeded_playback_service(&[
        (
            "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e",
            &[
                "5634d119-43be-4435-8432-575baddc4705",
                "5634ce19-43be-4f1c-8432-545baddc41ec",
            ],
        ),
        (
            "e6cdc0f3-3a7b-458b-86aa-fd093cc5e79b",
            &["5630a919-43ba-4766-842e-6f5badd886f6"],
        ),
    ])
    .await;

    let (playable, tracks) = service
        .load_release_set_tracks(vec![
            "e6cdc0f3-3a7b-458b-86aa-fd093cc5e79b".to_string(),
            "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e".to_string(),
        ])
        .await;

    assert_eq!(
        playable,
        vec![
            "e6cdc0f3-3a7b-458b-86aa-fd093cc5e79b",
            "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e"
        ]
    );
    assert_eq!(
        tracks,
        vec![
            "5630a919-43ba-4766-842e-6f5badd886f6",
            "5634d119-43be-4435-8432-575baddc4705",
            "5634ce19-43be-4f1c-8432-545baddc41ec"
        ]
    );
}

/// A release with no tracks (deleted, or never existed) is skipped; the remaining
/// releases still play in order.
#[tokio::test]
async fn play_releases_skips_a_release_without_tracks() {
    let (_home, service, _rx) = seeded_playback_service(&[
        (
            "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e",
            &["5634d119-43be-4435-8432-575baddc4705"],
        ),
        (
            "e6cdc0f3-3a7b-458b-86aa-fd093cc5e79b",
            &["5630a919-43ba-4766-842e-6f5badd886f6", TRACK_T2B],
        ),
    ])
    .await;

    let (playable, tracks) = service
        .load_release_set_tracks(vec![
            "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e".to_string(),
            "rel-gone".to_string(),
            "e6cdc0f3-3a7b-458b-86aa-fd093cc5e79b".to_string(),
        ])
        .await;

    assert_eq!(
        playable,
        vec![
            "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e",
            "e6cdc0f3-3a7b-458b-86aa-fd093cc5e79b"
        ]
    );
    assert_eq!(
        tracks,
        vec![
            "5634d119-43be-4435-8432-575baddc4705",
            "5630a919-43ba-4766-842e-6f5badd886f6",
            TRACK_T2B
        ]
    );
}

/// The shuffle/restore re-fetch of a multi-release source concatenates each
/// release's current tracks in source order — the same order the initial play
/// built, so a shuffle toggle re-derives over the whole multi-album order.
#[tokio::test]
async fn fetch_source_tracks_concatenates_a_releases_source() {
    let (_home, service, _rx) = seeded_playback_service(&[
        (
            "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e",
            &[
                "5634d119-43be-4435-8432-575baddc4705",
                "5634ce19-43be-4f1c-8432-545baddc41ec",
            ],
        ),
        (
            "e6cdc0f3-3a7b-458b-86aa-fd093cc5e79b",
            &["5630a919-43ba-4766-842e-6f5badd886f6"],
        ),
    ])
    .await;

    let source = ContextSource::Releases(vec![
        "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e".to_string(),
        "e6cdc0f3-3a7b-458b-86aa-fd093cc5e79b".to_string(),
    ]);
    let tracks = service.fetch_source_tracks(&source).await.unwrap();

    assert_eq!(
        tracks,
        vec![
            "5634d119-43be-4435-8432-575baddc4705",
            "5634ce19-43be-4f1c-8432-545baddc41ec",
            "5630a919-43ba-4766-842e-6f5badd886f6"
        ]
    );
}

/// A `Play` whose context load fails (here: the track doesn't exist) must
/// fail loud — a `PlaybackError` and nothing else — rather than silently
/// falling back to a single-track queue. `get_play_context`'s `Err` covers
/// only DB failures and data-inconsistency (a track missing, or absent from
/// its own release's track list — `release_id` is a required column, so
/// there is no legitimate "track with no context" case), so there is no
/// absence value to preserve a fallback for.
///
/// The discriminator from the old silently-degrading behavior: the old code
/// unconditionally changed the queue after the match (mutating it to a
/// queue to a bogus single-track entry even on failure); the fix returns
/// before ever touching the queue.
#[tokio::test]
async fn play_context_load_failure_surfaces_error_without_touching_the_queue() {
    let (_home, mut service, mut progress_rx) = test_playback_service().await;
    let queue_revision = service.playback_queue.revision();

    service.handle_play("missing-track".to_string()).await;

    let mut saw_error = false;
    let mut saw_playing = false;
    while let Ok(progress) = progress_rx.try_recv() {
        match progress {
            PlaybackProgress::PlaybackError { .. } => saw_error = true,
            PlaybackProgress::StateChanged {
                state: PlaybackState::Playing { .. },
            } => saw_playing = true,
            _ => {}
        }
    }
    assert!(
        saw_error,
        "a failed context load must surface a PlaybackError"
    );
    assert!(
        service.playback_queue.revision() == queue_revision,
        "a failed context load must not mutate the queue with a fallback single-track entry"
    );
    assert!(
        !saw_playing,
        "a failed context load must never reach Playing"
    );
    assert!(
        matches!(service.slot, PlaybackSlot::Stopped),
        "the slot must stay Stopped, not start loading a track whose context failed"
    );
}

/// `attach_track` reuses the one persistent output stream for a same-format
/// transition (swapping the source in place, which fires `on_source_replaced`)
/// and rebuilds the device stream only when the format changes. Drives it
/// directly through a `TestAudioOutput` that counts builds vs replaces.
#[tokio::test]
async fn attach_track_reuses_stream_on_same_format_and_rebuilds_on_change() {
    use std::sync::atomic::Ordering;

    let (_home, mut service, _rx) = test_playback_service().await;
    let output = TestAudioOutput::new();
    let builds = output.build_count.clone();
    let replaces = output.replace_count.clone();
    service.audio_output = Box::new(output);

    // First attach: nothing is attached yet, so it builds the stream.
    let (_s1, ts1, _r1) = create_track_stream_pair(44_100, 2);
    service
        .attach_track(
            ts1,
            test_track_fmt("08c7ff07-b56a-4e16-8df6-ae2967fa0806"),
            44_100,
            2,
            StagedNextOnReplace::Discard,
        )
        .await
        .expect("the first attach builds a stream");
    assert_eq!(
        builds.load(Ordering::Relaxed),
        1,
        "first attach builds once"
    );
    assert_eq!(
        replaces.load(Ordering::Relaxed),
        0,
        "no swap on the first attach"
    );

    // Same format: swap in place, no rebuild.
    let (_s2, ts2, _r2) = create_track_stream_pair(44_100, 2);
    service
        .attach_track(
            ts2,
            test_track_fmt("08c7fe07-b56a-4c63-8df6-ad2967fa0653"),
            44_100,
            2,
            StagedNextOnReplace::Discard,
        )
        .await
        .expect("a same-format attach replaces in place");
    assert_eq!(
        builds.load(Ordering::Relaxed),
        1,
        "a same-format attach reuses the one persistent stream"
    );
    assert_eq!(
        replaces.load(Ordering::Relaxed),
        1,
        "a same-format attach swaps the source (on_source_replaced)"
    );

    // Format change: drop the old stream and build a fresh one.
    let (_s3, ts3, _r3) = create_track_stream_pair(96_000, 2);
    service
        .attach_track(
            ts3,
            test_track_fmt("t3"),
            96_000,
            2,
            StagedNextOnReplace::Discard,
        )
        .await
        .expect("a format-change attach rebuilds the stream");
    assert_eq!(
        builds.load(Ordering::Relaxed),
        2,
        "a format change rebuilds the device stream"
    );
    assert_eq!(
        replaces.load(Ordering::Relaxed),
        1,
        "a rebuild is not a source swap"
    );
}

/// A default-output-device change rebuilds the persistent stream over the SAME
/// `PlaybackSource` (re-resolving the device), so playback follows the new
/// default without losing position or state — the only path that rebuilds a
/// live stream mid-playback. Builds go up by one, no source swap (the source is
/// reused, not replaced), and the callback's play state is untouched.
#[cfg(target_os = "macos")]
#[tokio::test]
async fn output_device_changed_rebuilds_over_the_same_source() {
    use std::sync::atomic::Ordering;

    let (_home, mut service, _rx) = test_playback_service().await;
    let output = TestAudioOutput::new();
    let builds = output.build_count.clone();
    let replaces = output.replace_count.clone();
    service.audio_output = Box::new(output);
    service
        .audio_output
        .set_state(crate::playback::audio_output::AudioState::Playing);

    // Attach a track so there's a live output stream to move.
    let (_s1, ts1, _r1) = create_track_stream_pair(44_100, 2);
    service
        .attach_track(
            ts1,
            test_track_fmt("08c7ff07-b56a-4e16-8df6-ae2967fa0806"),
            44_100,
            2,
            StagedNextOnReplace::Discard,
        )
        .await
        .expect("the first attach builds a stream");
    assert_eq!(builds.load(Ordering::Relaxed), 1, "one build so far");
    let source_before = service.output.as_ref().unwrap().source.clone();

    // The default device changed: rebuild in place.
    service.handle_output_device_changed().await;

    assert_eq!(
        builds.load(Ordering::Relaxed),
        2,
        "a device change rebuilds the device stream"
    );
    assert_eq!(
        replaces.load(Ordering::Relaxed),
        0,
        "a device-change rebuild reuses the source, it is not a swap"
    );
    let source_after = service.output.as_ref().unwrap().source.clone();
    assert!(
        Arc::ptr_eq(&source_before, &source_after),
        "the rebuild reuses the very same PlaybackSource so position/state survive"
    );
    assert_eq!(
        service.audio_output.get_state(),
        crate::playback::audio_output::AudioState::Playing,
        "playback keeps playing across the device switch"
    );
}

/// A device change with nothing playing has no stream to move, so it's a no-op:
/// no build, no source, no error.
#[cfg(target_os = "macos")]
#[tokio::test]
async fn output_device_changed_is_a_noop_when_stopped() {
    use std::sync::atomic::Ordering;

    let (_home, mut service, mut progress_rx) = test_playback_service().await;
    let output = TestAudioOutput::new();
    let builds = output.build_count.clone();
    service.audio_output = Box::new(output);

    service.handle_output_device_changed().await;

    assert_eq!(
        builds.load(Ordering::Relaxed),
        0,
        "no stream to rebuild when nothing is playing"
    );
    assert!(service.output.is_none(), "still no output");
    assert!(
        progress_rx.try_recv().is_err(),
        "a no-op device change emits nothing"
    );
}

/// Spawn a stand-in decoder that fills `sink`'s ring until the sink is
/// cancelled, then flags that it exited. A stub output never drains the ring, so
/// the thread parks on a full ring in `push_samples_blocking` — the common
/// steady-state condition, and the only way to prove the source was cancelled
/// (the AVIO cancel token does not unpark a write-blocked decoder; only the
/// sink's cancel flag does).
fn spawn_ring_filling_decoder(
    mut sink: crate::playback::track_stream::TrackSink,
) -> (
    Arc<std::sync::atomic::AtomicBool>,
    std::thread::JoinHandle<()>,
) {
    use std::sync::atomic::{AtomicBool, Ordering};
    let exited = Arc::new(AtomicBool::new(false));
    let exited_in_thread = exited.clone();
    let handle = std::thread::spawn(move || {
        let chunk = vec![0.0f32; 4096];
        while !sink.is_cancelled() {
            sink.push_samples_blocking(&chunk);
        }
        exited_in_thread.store(true, Ordering::Release);
    });
    (exited, handle)
}

async fn await_decoder_exit(exited: &Arc<std::sync::atomic::AtomicBool>) -> bool {
    use std::sync::atomic::Ordering;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while !exited.load(Ordering::Acquire) && std::time::Instant::now() < deadline {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    exited.load(Ordering::Acquire)
}

/// `stop()` must cancel the output's `PlaybackSource` before dropping it, so the
/// outgoing decoder — which `teardown_current_track` only stopped via its AVIO
/// token — is unparked and exits even when it's blocked writing a full ring.
/// Dropping the source alone abandons the ring but never sets the sink's cancel
/// flag, so a ring-parked decoder would spin forever (leaking its thread and
/// FFmpeg contexts). Sabotage — drop the source-cancel — and this hangs.
#[tokio::test]
async fn stop_cancels_the_output_source_so_a_ring_parked_decoder_exits() {
    let (_home, mut service, _rx) = test_playback_service().await;

    let (sink, track_stream, _ready) = create_track_stream_pair(44_100, 2);
    let (exited, handle) = spawn_ring_filling_decoder(sink);
    let source = Arc::new(Mutex::new(source::PlaybackSource::new(
        track_stream,
        test_track_fmt("t"),
    )));
    let (_tx, audio_rx) = audio_event_channel();
    service.output = Some(test_output(source, audio_rx));
    let buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(test_prepared_track("t", buffer), TrackPhase::Playing);

    service.stop().await;

    assert!(
        await_decoder_exit(&exited).await,
        "stop() must cancel the output source so a ring-parked decoder exits"
    );
    handle.join().unwrap();
}

/// A format-change rebuild (`attach_track` into a different sample rate/channel
/// count) discards the old output's `PlaybackSource`; it must cancel that source
/// first so its ring-parked decoder exits, for the same reason as `stop()`.
#[tokio::test]
async fn format_change_rebuild_cancels_the_old_source_so_its_decoder_exits() {
    let (_home, mut service, _rx) = test_playback_service().await;

    // An old output at 44.1kHz whose decoder is parked filling a full ring.
    let (sink, track_stream, _ready) = create_track_stream_pair(44_100, 2);
    let (exited, handle) = spawn_ring_filling_decoder(sink);
    let source = Arc::new(Mutex::new(source::PlaybackSource::new(
        track_stream,
        test_track_fmt("t"),
    )));
    let (_tx, audio_rx) = audio_event_channel();
    service.output = Some(test_output(source, audio_rx));

    // Attach a track in a DIFFERENT format, forcing a rebuild that drops the old
    // output's source.
    let (_new_sink, new_stream, _new_ready) = create_track_stream_pair(96_000, 2);
    service
        .attach_track(
            new_stream,
            test_track_fmt("08c7fe07-b56a-4c63-8df6-ad2967fa0653"),
            96_000,
            2,
            StagedNextOnReplace::Discard,
        )
        .await
        .expect("the format-change attach rebuilds the stream");

    assert!(
        await_decoder_exit(&exited).await,
        "a format-change rebuild must cancel the discarded source so its decoder exits"
    );
    handle.join().unwrap();
}

fn queued_completion(track_id: &str) -> AudioEvent {
    AudioEvent::Completion((Arc::new(test_track_fmt(track_id)), 0, 44_100))
}

fn drained_track_completed_ids(
    progress_rx: &mut tokio_mpsc::UnboundedReceiver<PlaybackProgress>,
) -> Vec<String> {
    let mut ids = Vec::new();
    while let Ok(progress) = progress_rx.try_recv() {
        if let PlaybackProgress::TrackCompleted { track_id } = progress {
            ids.push(track_id);
        }
    }
    ids
}

/// A same-format switch swaps the source in place but keeps the one persistent
/// audio-events receiver. Events queued for the outgoing track before the swap
/// must be dropped under the same lock the swap takes, or a later drain would
/// pop the outgoing track's `Completion` and stamp the incoming track
/// `Completed` (muting it) and fire a spurious auto-advance. Sabotage — skip the
/// drain in `attach_track`'s replace branch — and the stale `TrackCompleted`
/// fires.
#[tokio::test]
async fn same_format_replace_drops_events_queued_for_the_outgoing_track() {
    let (_home, mut service, mut progress_rx) = test_playback_service().await;

    let (_sink_a, stream_a, _r_a) = create_track_stream_pair(44_100, 2);
    let source = Arc::new(Mutex::new(source::PlaybackSource::new(
        stream_a,
        test_track_fmt("A"),
    )));
    // A Completion for A is already in the receiver, not yet drained.
    let (mut tx, rx) = audio_event_channel();
    tx.push_required(queued_completion("A"));
    service.output = Some(test_output(source, rx));
    let buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(test_prepared_track("A", buffer), TrackPhase::Playing);

    // A same-format switch to B swaps the source in place.
    let (_sink_b, stream_b, _r_b) = create_track_stream_pair(44_100, 2);
    service
        .attach_track(
            stream_b,
            test_track_fmt("B"),
            44_100,
            2,
            StagedNextOnReplace::Discard,
        )
        .await
        .expect("a same-format attach replaces in place");

    // Draining now must find nothing stale — the outgoing track's Completion is
    // gone, so no TrackCompleted (which would drive a spurious advance).
    service.drain_current_audio_events().await;
    assert!(
        drained_track_completed_ids(&mut progress_rx).is_empty(),
        "a same-format swap must drop events queued for the outgoing track"
    );
}

/// A default-device rebuild mints a fresh audio-events channel but reuses the
/// SAME source. A `Completion` queued when the device changed must be carried
/// onto the new channel — it can never re-fire (the source's completion latch is
/// already set), so losing it wedges auto-advance at the end of that track.
/// Sabotage — drop the old receiver without carrying its events — and no
/// `TrackCompleted` survives the rebuild.
#[cfg(target_os = "macos")]
#[tokio::test]
async fn device_change_carries_a_queued_completion_forward() {
    let (_home, mut service, mut progress_rx) = test_playback_service().await;

    let (_sink, stream, _r) = create_track_stream_pair(44_100, 2);
    let source = Arc::new(Mutex::new(source::PlaybackSource::new(
        stream,
        test_track_fmt("t"),
    )));
    let (mut tx, rx) = audio_event_channel();
    tx.push_required(queued_completion("t"));
    service.output = Some(test_output(source, rx));
    let buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(test_prepared_track("t", buffer), TrackPhase::Playing);

    service.handle_output_device_changed().await;
    service.drain_current_audio_events().await;

    assert_eq!(
        drained_track_completed_ids(&mut progress_rx),
        vec!["t".to_string()],
        "a queued Completion must survive a device-change rebuild so auto-advance still fires"
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
    let (_home, manager) = seeded_library_manager_with_diagnostics(
        &[(
            "c61a9e19-f3ba-4728-842c-c59dbc82e238",
            &[
                "08c80007-b56a-4fc9-8df6-af2967fa09b9",
                "08c7ff07-b56a-4e16-8df6-ae2967fa0806",
            ],
        )],
        diagnostics.clone(),
    )
    .await;
    let (mut service, _progress_rx) = playback_service_over(manager);

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
    let (_home, manager) = seeded_library_manager_with_diagnostics(
        &[(
            "c61a9e19-f3ba-4728-842c-c59dbc82e238",
            &[
                "08c80007-b56a-4fc9-8df6-af2967fa09b9",
                "08c7ff07-b56a-4e16-8df6-ae2967fa0806",
            ],
        )],
        diagnostics.clone(),
    )
    .await;
    let (mut service, _progress_rx) = playback_service_over(manager);

    // A two-track context playing from t1, so Previous steps back to t0.
    service.playback_queue.play_release(
        ContextSource::Release("c61a9e19-f3ba-4728-842c-c59dbc82e238".to_string()),
        vec![
            "08c80007-b56a-4fc9-8df6-af2967fa09b9".to_string(),
            "08c7ff07-b56a-4e16-8df6-ae2967fa0806".to_string(),
        ],
        ContextStart::Index(1),
    );
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

/// A user-intent command maps to its telemetry kind; internal/system commands,
/// queries, and continuous inputs map to `None` and ship nothing.
#[test]
fn playback_command_kind_maps_user_intent_only() {
    use super::playback_command_kind;
    assert!(matches!(
        playback_command_kind(&PlaybackCommand::Play(
            "08c80007-b56a-4fc9-8df6-af2967fa09b9".to_string()
        )),
        Some(PlaybackCommandKind::Play)
    ));
    assert!(matches!(
        playback_command_kind(&PlaybackCommand::SetRepeatMode(RepeatMode::Off)),
        Some(PlaybackCommandKind::SetRepeat)
    ));
    // A continuous input and an internal command ship nothing.
    assert!(playback_command_kind(&PlaybackCommand::SetVolume(0.5)).is_none());
    assert!(playback_command_kind(&PlaybackCommand::AutoAdvance {
        track_id: "08c80007-b56a-4fc9-8df6-af2967fa09b9".to_string()
    })
    .is_none());
}

/// A track's natural completion ships `track_completed` carrying the decode-error
/// count — the quality signal.
#[tokio::test]
async fn track_completion_ships_track_completed_with_decode_error_count() {
    let (diagnostics, transport) = recording_diagnostics();
    let (_home, manager) = seeded_library_manager_with_diagnostics(
        &[(
            "c61a9e19-f3ba-4728-842c-c59dbc82e238",
            &["08c80007-b56a-4fc9-8df6-af2967fa09b9"],
        )],
        diagnostics.clone(),
    )
    .await;
    let (mut service, _progress_rx) = playback_service_over(manager);

    // A track must be active for the completion to mark its phase.
    let buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(
        test_prepared_track("08c80007-b56a-4fc9-8df6-af2967fa09b9", buffer),
        TrackPhase::Playing,
    );

    service.handle_completion_event(
        Arc::new(test_track_fmt("08c80007-b56a-4fc9-8df6-af2967fa09b9")),
        2,
        44_100,
    );

    diagnostics.flush().await.expect("flush succeeds");
    let bodies = transport.requests();
    let events: Vec<crate::diagnostics::DiagnosticEvent> = bodies
        .iter()
        .flat_map(|r| {
            serde_json::from_slice::<Vec<crate::diagnostics::DiagnosticEvent>>(&r.body).unwrap()
        })
        .collect();
    let completed = events
        .iter()
        .find(|e| e.name == "track_completed")
        .expect("track completion ships track_completed");
    assert_eq!(completed.fields["decode_errors"], serde_json::json!(2));
    assert_eq!(
        completed.fields["track_id"],
        serde_json::json!("08c80007-b56a-4fc9-8df6-af2967fa09b9")
    );
}

/// A corrupt resume row, driven through the real restore path, ships
/// `anomaly{resume_cache_corrupt}` and clears the row. A negative `position_ms`
/// is an out-of-domain value `from_row` rejects — the row is our own write, so
/// out of range means a corrupted local cache.
#[tokio::test]
async fn corrupt_resume_row_ships_resume_cache_corrupt_anomaly() {
    let (diagnostics, transport) = recording_diagnostics();
    let (_home, manager) = seeded_library_manager_with_diagnostics(
        &[(
            "c61a9e19-f3ba-4728-842c-c59dbc82e238",
            &["08c80007-b56a-4fc9-8df6-af2967fa09b9"],
        )],
        diagnostics.clone(),
    )
    .await;

    // Persist a row whose position is out of domain; `from_row` discards it.
    manager
        .save_playback_state(&crate::db::DbPlaybackState {
            context: None,
            manual: "[]".to_string(),
            repeat: "off".to_string(),
            current_track_id: Some("08c80007-b56a-4fc9-8df6-af2967fa09b9".to_string()),
            position_ms: Some(-1),
            volume: 1.0,
            is_muted: false,
        })
        .await
        .expect("save the corrupt row");

    let (mut service, _progress_rx) = playback_service_over(manager);
    service.restore_from_cache(true).await;

    diagnostics.flush().await.expect("flush succeeds");
    let events: Vec<crate::diagnostics::DiagnosticEvent> = transport
        .requests()
        .iter()
        .flat_map(|r| {
            serde_json::from_slice::<Vec<crate::diagnostics::DiagnosticEvent>>(&r.body).unwrap()
        })
        .collect();
    let anomaly = events
        .iter()
        .find(|e| e.name == "anomaly")
        .expect("a corrupt resume row ships an anomaly");
    assert_eq!(
        anomaly.fields["kind"],
        serde_json::json!("resume_cache_corrupt")
    );
}

// -- renderer seam: remote playback -------------------------------------------

use crate::renderer::{
    cast_stream_format, ReceiverStatus, RendererChannel, RendererError, RendererMedia,
    RendererPlayerState, RendererSessionStatus,
};
