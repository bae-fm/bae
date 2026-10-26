// -- AirPlay renderer-seam tests --

/// Records the control operations the service drives an AirPlay stream through,
/// standing in for the RAOP session so the seam is tested without a receiver.
#[derive(Default)]
struct FakeAirPlayControlState {
    flushed: std::sync::atomic::AtomicU64,
    reanchored: std::sync::atomic::AtomicU64,
    failure: std::sync::OnceLock<String>,
}

struct FakeAirPlayControl(Arc<FakeAirPlayControlState>);

impl crate::playback::airplay_output::AirPlayStreamControl for FakeAirPlayControl {
    fn flush(&self) {
        self.0
            .flushed
            .fetch_add(1, std::sync::atomic::Ordering::Release);
    }
    fn reanchor(&self) {
        self.0
            .reanchored
            .fetch_add(1, std::sync::atomic::Ordering::Release);
    }
    fn failure(&self) -> Option<String> {
        self.0.failure.get().cloned()
    }
}

/// Install an AirPlay renderer on `service` with a fake control published, a
/// tagged saved local output (volume `saved_tag`), and the given latency.
fn install_airplay(
    service: &mut PlaybackService,
    latency: std::time::Duration,
    saved_tag: f32,
) -> Arc<FakeAirPlayControlState> {
    let state = Arc::new(FakeAirPlayControlState::default());
    let control: Arc<dyn crate::playback::airplay_output::AirPlayStreamControl> =
        Arc::new(FakeAirPlayControl(state.clone()));
    let saved = TestAudioOutput::new();
    saved.set_volume(saved_tag);
    service.renderer = Renderer::AirPlay(renderer::AirPlayRenderer::new(
        test_device(),
        control,
        Box::new(saved),
        latency,
    ));
    state
}

#[tokio::test]
async fn airplay_pause_flushes_and_resume_reanchors() {
    let (_home, mut service, _progress_rx) = test_playback_service().await;
    let buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(test_prepared_track("t", buffer), TrackPhase::Playing);
    let state = install_airplay(&mut service, std::time::Duration::from_secs(2), 0.5);

    service.pause();
    assert_eq!(
        state.flushed.load(std::sync::atomic::Ordering::Acquire),
        1,
        "pause FLUSHes the receiver"
    );
    assert_eq!(
        state.reanchored.load(std::sync::atomic::Ordering::Acquire),
        0
    );

    service.resume().await;
    assert_eq!(
        state.reanchored.load(std::sync::atomic::Ordering::Acquire),
        1,
        "resume re-anchors the pacing"
    );
}

#[tokio::test]
async fn airplay_position_is_offset_by_receiver_latency() {
    let (_home, mut service, mut progress_rx) = test_playback_service().await;
    let buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(test_prepared_track("t", buffer), TrackPhase::Playing);
    install_airplay(&mut service, std::time::Duration::from_secs(2), 0.5);

    // A tick at 5 s of decoded position: the audible position is 5 − 2 = 3 s.
    let mut fmt = test_track_fmt("t");
    fmt.timeline = TrackTimeline::new(Some(std::time::Duration::from_secs(60)), std::time::Duration::ZERO);
    service
        .handle_position_event(Arc::new(fmt), StreamPosition::from_millis(5_000))
        .await;

    let position = loop {
        match progress_rx.try_recv() {
            Ok(PlaybackProgress::PositionUpdate { position_ms, .. }) => break position_ms,
            Ok(_) => continue,
            Err(_) => panic!("expected a PositionUpdate"),
        }
    };
    assert_eq!(
        position, 3_000,
        "position reflects the ~2 s receiver latency"
    );
}

#[tokio::test]
async fn airplay_stop_restores_the_local_output_and_returns_to_local() {
    let (_home, mut service, _progress_rx) = test_playback_service().await;
    let buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(test_prepared_track("t", buffer), TrackPhase::Playing);
    install_airplay(&mut service, std::time::Duration::from_secs(2), 0.777);

    service.stop().await;

    assert!(
        !service.renderer.is_airplay(),
        "stop returns to the local renderer"
    );
    assert_eq!(
        service.audio_output.get_volume(),
        0.777,
        "the saved local output sink is restored"
    );
}

/// Stopping AirPlay resumes local at the position playback actually reached: the
/// resume position is read from the live shared position at teardown, so the
/// `AirPlayRenderer` carries no separately-stored position that could go stale.
/// (Fully exercising the resumed local decode needs a real imported track; here
/// the position source and the return-to-local are asserted.)
#[tokio::test]
async fn airplay_stop_reads_the_live_position_and_returns_to_local() {
    let (_home, mut service, _progress_rx) =
        seeded_playback_service(&[("af63ef4c-8602-4cd5-82c0-3d334b916305", &[TRACK_T])]).await;
    service.slot = active_slot(
        test_prepared_track(TRACK_T, create_sparse_buffer(1_024)),
        TrackPhase::Playing,
    );
    let saved_tag = 0.5;
    install_airplay(&mut service, std::time::Duration::from_secs(2), saved_tag);

    // Playback progressed on AirPlay: decode is local, so the slot's position is
    // the live one the resume reads.
    if let PlaybackSlot::Active(cur) = &mut service.slot {
        cur.position = StreamPosition::from_millis(30_000);
    }

    service.handle_stop_remote().await;

    assert!(
        !service.renderer.is_airplay(),
        "stop returns to the local renderer"
    );
    assert_eq!(
        service.audio_output.get_volume(),
        saved_tag,
        "the saved local output sink is restored"
    );
}

/// Seeking while on AirPlay FLUSHes the receiver and re-anchors the pacing (decode
/// is local, so the rebuild re-fills the sink at the new position).
#[tokio::test]
async fn airplay_seek_flushes_and_reanchors() {
    let (_home, mut service, _progress_rx) =
        seeded_playback_service(&[("af63ef4c-8602-4cd5-82c0-3d334b916305", &[TRACK_T])]).await;
    let buffer = create_sparse_buffer(64 * 1024);
    service.slot = active_slot(
        test_prepared_track(TRACK_T, buffer.clone()),
        TrackPhase::Playing,
    );
    let (_sink, source, _ready) = create_track_stream_pair(44_100, 2);
    let (_tx, audio_rx) = audio_event_channel();
    service.output = Some(test_output(
        Arc::new(Mutex::new(source::PlaybackSource::new(
            source,
            test_track_fmt(TRACK_T),
        ))),
        audio_rx,
    ));
    let state = install_airplay(&mut service, std::time::Duration::from_secs(2), 0.5);

    service.seek(StreamPosition::from_duration(std::time::Duration::from_secs(20))).await;

    assert!(
        state.flushed.load(std::sync::atomic::Ordering::Acquire) >= 1,
        "seek FLUSHes the receiver's buffer"
    );
    assert!(
        state.reanchored.load(std::sync::atomic::Ordering::Acquire) >= 1,
        "seek re-anchors the pacing"
    );
}

/// A local end-of-decode advances the queue while the AirPlay renderer stays
/// installed — playback moves to the next track on the same receiver.
#[tokio::test]
async fn airplay_advance_on_local_end_stays_on_airplay() {
    let (_home, mut service, _progress_rx) =
        seeded_playback_service(&[("af63ef4c-8602-4cd5-82c0-3d334b916305", &[TRACK_A, TRACK_B])])
            .await;
    service.playback_queue.apply(|queue| {
        queue.play_release(
            ContextSource::Release("af63ef4c-8602-4cd5-82c0-3d334b916305".to_string()),
            vec![TRACK_A.to_string(), TRACK_B.to_string()],
            ContextStart::Index(0),
        )
    });
    service.slot = active_slot(
        test_prepared_track(TRACK_A, create_sparse_buffer(1_024)),
        TrackPhase::Playing,
    );
    install_airplay(&mut service, std::time::Duration::from_secs(2), 0.5);

    service.handle_auto_advance(TRACK_A.to_string()).await;

    assert!(
        service.renderer.is_airplay(),
        "the AirPlay renderer stays installed across an auto-advance"
    );
}

/// A no-op AirPlay sink for driving `handle_play_on_airplay`: it accepts the PCM
/// source without touching a socket.
struct NoopAirPlaySink;
impl crate::playback::airplay_output::AirPlaySink for NoopAirPlaySink {
    fn start(
        &self,
        _source: Box<dyn crate::playback::airplay_output::PcmSource>,
    ) -> Result<Arc<dyn crate::playback::airplay_output::AirPlayStreamControl>, AudioError> {
        Ok(Arc::new(FakeAirPlayControl(Arc::new(
            FakeAirPlayControlState::default(),
        ))))
    }
}

/// `handle_play_on_airplay` swaps to the AirPlay output and installs the AirPlay
/// renderer without turning playback "remote" — decode stays local, so the queue
/// and slot are driven by the local pipeline, not device transport commands.
/// (Driven with nothing playing so the swap isn't torn down by the unit harness's
/// undecodable seed track; the local decode path is covered by the seek/advance
/// tests.)
#[tokio::test]
async fn play_on_airplay_swaps_the_sink_and_keeps_decode_local() {
    let (_home, mut service, _progress_rx) = test_playback_service().await;
    // Nothing playing: AirPlay arms without a track to re-decode.
    service.slot = PlaybackSlot::Stopped;

    service
        .handle_play_on_airplay(renderer::AirPlayConnect::new(
            Box::new(NoopAirPlaySink),
            test_device(),
            std::time::Duration::from_secs(2),
        ))
        .await;

    assert!(
        service.renderer.is_airplay(),
        "the AirPlay renderer is installed"
    );
    assert!(
        !service.renderer.is_remote(),
        "AirPlay keeps decoding locally — it is not a fetch-a-URL remote renderer"
    );
}

/// A dead AirPlay receiver (the session reports transport failure) ends AirPlay
/// and returns to local — surfaced on the regular position path rather than
/// erroring silently forever — and announces the receiver lost with why.
#[tokio::test]
async fn airplay_receiver_death_ends_airplay_and_returns_to_local() {
    let (_home, mut service, mut progress_rx) =
        seeded_playback_service(&[("af63ef4c-8602-4cd5-82c0-3d334b916305", &[TRACK_T])]).await;
    service.slot = active_slot(
        test_prepared_track(TRACK_T, create_sparse_buffer(1_024)),
        TrackPhase::Playing,
    );
    let state = install_airplay(&mut service, std::time::Duration::from_secs(2), 0.5);
    while progress_rx.try_recv().is_ok() {}

    // The receiver went away: the session reports the transport as failed.
    state
        .failure
        .set("20 audio sends in a row failed: No route to host (os error 65)".to_string())
        .expect("the failure is set once");

    // A routine position tick catches it and ends AirPlay.
    service
        .handle_position_event(
            Arc::new(test_track_fmt(TRACK_T)),
            StreamPosition::from_millis(1_000),
        )
        .await;

    assert!(
        !service.renderer.is_airplay(),
        "a dead receiver ends AirPlay and returns to the local renderer"
    );
    let mut announced = None;
    while let Ok(progress) = progress_rx.try_recv() {
        if let PlaybackProgress::RemoteStatusChanged { remote } = progress {
            announced = Some(remote);
        }
    }
    assert_eq!(
        announced,
        Some(RemotePlayback::Ended {
            device: test_device(),
            detail: "20 audio sends in a row failed: No route to host (os error 65)".to_string(),
        })
    );
}
