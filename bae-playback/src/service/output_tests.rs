use super::*;
use crate::audio_output::{
    audio_event_channel, AudioError, AudioEvent, AudioEventSender, AudioOutputControls, AudioStream,
};
use crate::service::tests::*;
use bae_audio::track_stream::create_track_stream_pair;

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
    service.audio_output = test_player_output(output);

    // First attach: nothing is attached yet, so it builds the stream.
    let (_s1, ts1, _r1) = create_track_stream_pair(44_100, 2);
    service
        .attach_track(
            ts1,
            TrackFmt::for_test("08c7ff07-b56a-4e16-8df6-ae2967fa0806"),
            44_100,
            2,
            StagedNextOnReplace::Discard,
        )
        .await
        .expect("the first attach builds a stream");
    assert_eq!(builds.load(Ordering::Relaxed), 1, "first attach builds once");
    assert_eq!(replaces.load(Ordering::Relaxed), 0, "no swap on the first attach");

    // Same format: swap in place, no rebuild.
    let (_s2, ts2, _r2) = create_track_stream_pair(44_100, 2);
    service
        .attach_track(
            ts2,
            TrackFmt::for_test("08c7fe07-b56a-4c63-8df6-ad2967fa0653"),
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
        .attach_track(ts3, TrackFmt::for_test("t3"), 96_000, 2, StagedNextOnReplace::Discard)
        .await
        .expect("a format-change attach rebuilds the stream");
    assert_eq!(builds.load(Ordering::Relaxed), 2, "a format change rebuilds the device stream");
    assert_eq!(replaces.load(Ordering::Relaxed), 1, "a rebuild is not a source swap");
}

/// A stream whose device reported it can't go on — the default device changed
/// on Windows, or its device was unplugged — is rebuilt over the SAME
/// `PlaybackSource` on the current default device, at the next drain of the
/// output's events, so playback follows without losing position or state.
/// Builds go up by one, no source swap (the source is reused, not replaced),
/// and the callback's play state is untouched. Sabotage: stop asking the stream
/// for its device's report in `follow_output_devices`, and no rebuild
/// happens.
#[tokio::test]
async fn an_invalidated_stream_is_rebuilt_over_the_same_source() {
    use std::sync::atomic::Ordering;

    let (_home, mut service, _rx) = test_playback_service().await;
    let output = TestAudioOutput::new();
    let builds = output.build_count.clone();
    let replaces = output.replace_count.clone();
    service.audio_output = test_player_output(output);
    service.audio_output.set_state(crate::audio_output::AudioState::Playing);

    // Attach a track so there's a live output stream to move.
    let (_s1, ts1, _r1) = create_track_stream_pair(44_100, 2);
    service
        .attach_track(
            ts1,
            TrackFmt::for_test("08c7ff07-b56a-4e16-8df6-ae2967fa0806"),
            44_100,
            2,
            StagedNextOnReplace::Discard,
        )
        .await
        .expect("the first attach builds a stream");
    assert_eq!(builds.load(Ordering::Relaxed), 1, "one build so far");
    let source_before = service.output.as_ref().unwrap().source.clone();

    // The device reports the stream can't go on.
    service.output.as_mut().unwrap().stream =
        Box::new(InvalidatedTestStream::new("the default device changed"));
    service.follow_output_devices().await;

    assert_eq!(builds.load(Ordering::Relaxed), 2, "an invalidated stream is rebuilt");
    assert_eq!(
        replaces.load(Ordering::Relaxed),
        0,
        "the rebuild reuses the source, it is not a swap"
    );
    let source_after = service.output.as_ref().unwrap().source.clone();
    assert!(
        Arc::ptr_eq(&source_before, &source_after),
        "the rebuild reuses the very same PlaybackSource so position/state survive"
    );
    assert_eq!(
        service.audio_output.get_state(),
        crate::audio_output::AudioState::Playing,
        "playback keeps playing across the device switch"
    );

    // The rebuilt stream reported nothing, so the next report leaves it alone.
    service.follow_output_devices().await;
    assert_eq!(builds.load(Ordering::Relaxed), 2, "one report, one rebuild");
}

/// A stream invalidated with no default device to rebuild on stops playback
/// and says the audio device isn't available, rather than leaving playback
/// "playing" through a stream that plays nothing. Sabotage: ignore the build
/// failure, and playback stays active with no output.
#[tokio::test]
async fn an_invalidated_stream_with_no_device_to_move_to_stops_playback() {
    let (_home, mut service, mut progress_rx) = test_playback_service().await;
    struct NoDeviceOutput;
    impl AudioOutput for NoDeviceOutput {
        fn create_stream(
            &mut self,
            _controls: crate::audio_output::AudioOutputControls,
            _source: Arc<Mutex<source::PlaybackSource>>,
            _source_sample_rate: u32,
            _source_channels: u32,
            _audio_events: AudioEventSender,
            _position_update_interval_ms: u32,
        ) -> Result<Box<dyn AudioStream>, AudioError> {
            Err(AudioError::DeviceUnavailable)
        }
    }
    service.audio_output = crate::audio_output::PlayerOutput::new(
        Box::new(NoDeviceOutput),
        AudioOutputControls::new(crate::Volume::FULL),
    );

    let (_sink, stream, _r) = create_track_stream_pair(44_100, 2);
    let source = Arc::new(Mutex::new(source::PlaybackSource::new(stream, TrackFmt::for_test("t"))));
    let (_tx, rx) = audio_event_channel().unwrap();
    let mut out = test_output(source, rx);
    out.stream = Box::new(InvalidatedTestStream::new("device disconnected"));
    service.output = Some(out);
    let buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(test_prepared_track("t", buffer), TrackPhase::Playing);

    service.follow_output_devices().await;

    assert!(service.output.is_none(), "no stream is left behind");
    assert!(!matches!(service.slot, PlaybackSlot::Active(_)), "playback stopped");
    let mut reasons = Vec::new();
    while let Ok(progress) = progress_rx.try_recv() {
        if let PlaybackProgress::PlaybackError { reason } = progress {
            reasons.push(reason);
        }
    }
    assert!(
        reasons.iter().any(|reason| matches!(
            reason,
            crate::PlaybackErrorReason::AudioOutput {
                problem: crate::AudioOutputProblem::DeviceUnavailable
            }
        )),
        "the person is told the audio device isn't available: {reasons:?}"
    );
}

/// Spawn a stand-in decoder that fills `sink`'s ring until the sink is
/// cancelled, then exits. A stub output never drains the ring, so
/// the thread parks on a full ring in `push_samples_blocking` — the common
/// steady-state condition, and the only way to prove the source was cancelled
/// (the AVIO cancel token does not unpark a write-blocked decoder; only the
/// sink's cancel flag does).
fn spawn_ring_filling_decoder(
    mut sink: bae_audio::track_stream::TrackSink,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let chunk = vec![0.0f32; 4096];
        while !sink.is_cancelled() {
            sink.push_samples_blocking(&chunk);
        }
    })
}

/// Wait for the decoder thread to exit.
async fn await_decoder_exit(handle: std::thread::JoinHandle<()>, why: &str) {
    tokio::time::timeout(
        bae_util::test_channels::HANG_GUARD,
        bae_util::test_blocking::on_blocking_pool(move || handle.join().unwrap()),
    )
    .await
    .unwrap_or_else(|_| panic!("{why}"));
}

/// `stop()` must cancel the output's `PlaybackSource` before dropping it, so the
/// outgoing decoder — which `teardown_local_playback` only stopped via its AVIO
/// token — is unparked and exits even when it's blocked writing a full ring.
/// Dropping the source alone abandons the ring but never sets the sink's cancel
/// flag, so a ring-parked decoder would spin forever (leaking its thread and
/// FFmpeg contexts). Sabotage — drop the source-cancel — and this hangs.
#[tokio::test]
async fn stop_cancels_the_output_source_so_a_ring_parked_decoder_exits() {
    let (_home, mut service, _rx) = test_playback_service().await;

    let (sink, track_stream, _ready) = create_track_stream_pair(44_100, 2);
    let handle = spawn_ring_filling_decoder(sink);
    let source =
        Arc::new(Mutex::new(source::PlaybackSource::new(track_stream, TrackFmt::for_test("t"))));
    let (_tx, audio_rx) = audio_event_channel().unwrap();
    service.output = Some(test_output(source, audio_rx));
    let buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(test_prepared_track("t", buffer), TrackPhase::Playing);

    service.stop().await;

    await_decoder_exit(
        handle,
        "stop() must cancel the output source so a ring-parked decoder exits",
    )
    .await;
}

/// A format-change rebuild (`attach_track` into a different sample rate/channel
/// count) discards the old output's `PlaybackSource`; it must cancel that source
/// first so its ring-parked decoder exits, for the same reason as `stop()`.
#[tokio::test]
async fn format_change_rebuild_cancels_the_old_source_so_its_decoder_exits() {
    let (_home, mut service, _rx) = test_playback_service().await;

    // An old output at 44.1kHz whose decoder is parked filling a full ring.
    let (sink, track_stream, _ready) = create_track_stream_pair(44_100, 2);
    let handle = spawn_ring_filling_decoder(sink);
    let source =
        Arc::new(Mutex::new(source::PlaybackSource::new(track_stream, TrackFmt::for_test("t"))));
    let (_tx, audio_rx) = audio_event_channel().unwrap();
    service.output = Some(test_output(source, audio_rx));

    // Attach a track in a DIFFERENT format, forcing a rebuild that drops the old
    // output's source.
    let (_new_sink, new_stream, _new_ready) = create_track_stream_pair(96_000, 2);
    service
        .attach_track(
            new_stream,
            TrackFmt::for_test("08c7fe07-b56a-4c63-8df6-ad2967fa0653"),
            96_000,
            2,
            StagedNextOnReplace::Discard,
        )
        .await
        .expect("the format-change attach rebuilds the stream");

    await_decoder_exit(
        handle,
        "a format-change rebuild must cancel the discarded source so its decoder exits",
    )
    .await;
}

fn queued_completion(track_id: &str) -> AudioEvent {
    AudioEvent::Completion((Arc::new(TrackFmt::for_test(track_id)), 0, 44_100))
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
    let source =
        Arc::new(Mutex::new(source::PlaybackSource::new(stream_a, TrackFmt::for_test("A"))));
    // A Completion for A is already in the receiver, not yet drained.
    let (mut tx, rx) = audio_event_channel().unwrap();
    tx.push_required(queued_completion("A"));
    service.output = Some(test_output(source, rx));
    let buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(test_prepared_track("A", buffer), TrackPhase::Playing);

    // A same-format switch to B swaps the source in place.
    let (_sink_b, stream_b, _r_b) = create_track_stream_pair(44_100, 2);
    service
        .attach_track(stream_b, TrackFmt::for_test("B"), 44_100, 2, StagedNextOnReplace::Discard)
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

/// The rebuild of an invalidated stream mints a fresh audio-events channel but
/// reuses the SAME source. A `Completion` the callback queued before the old
/// stream was dropped must be carried onto the new channel — it can never
/// re-fire (the source's completion latch is already set), so losing it wedges
/// auto-advance at the end of that track. Sabotage — drop the old receiver
/// without carrying its events — and no `TrackCompleted` survives the rebuild.
#[tokio::test]
async fn an_invalidated_stream_rebuild_carries_a_queued_completion_forward() {
    let (_home, mut service, mut progress_rx) = test_playback_service().await;

    let (_sink, stream, _r) = create_track_stream_pair(44_100, 2);
    let source = Arc::new(Mutex::new(source::PlaybackSource::new(stream, TrackFmt::for_test("t"))));
    let (mut tx, rx) = audio_event_channel().unwrap();
    tx.push_required(queued_completion("t"));
    let out = test_output(source, rx);
    let buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(test_prepared_track("t", buffer), TrackPhase::Playing);

    service.follow_output_device(out, "the default device changed").await;
    service.drain_current_audio_events().await;

    assert_eq!(
        drained_track_completed_ids(&mut progress_rx),
        vec!["t".to_string()],
        "a queued Completion must survive a device-change rebuild so auto-advance still fires"
    );
}

/// An output that stops sending partway — an AirPlay conversion failing —
/// stops playback with an error and reports the bug, rather than playing on
/// with audio missing.
#[tokio::test]
async fn an_output_that_stops_sending_stops_playback_and_reports_the_bug() {
    let bugs = Arc::new(bae_diagnostics::RecordingBugReporter::default());
    let diagnostics = bae_diagnostics::Diagnostics::noop().reporting_bugs_to(bugs.clone());
    let store = seeded_store(&[]).await;
    let (mut service, mut progress_rx) = playback_service_over(store.store(), diagnostics.clone());
    service.slot =
        active_slot(test_prepared_track("t", create_sparse_buffer(1_024)), TrackPhase::Playing);

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

/// A level asked for past full or below silent is held to that range where
/// it is set, so the output, the device and what is stored and shown all
/// hold one level in range.
#[tokio::test]
async fn a_level_out_of_range_is_held_to_silent_and_full_where_it_is_set() {
    let (_home, mut service, mut progress_rx) = test_playback_service().await;
    let mut emitted = Vec::new();
    for asked in [1.5, -0.5] {
        service.set_volume(crate::Volume::new(asked).unwrap());
        while let Ok(progress) = progress_rx.try_recv() {
            if let PlaybackProgress::VolumeChanged { volume } = progress {
                emitted.push(volume);
            }
        }
    }
    assert_eq!(emitted, vec![1.0, 0.0]);
    assert_eq!(service.audio_output.get_volume(), 0.0);
}
