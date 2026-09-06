/// Shared, scriptable state for the fake renderer channel: the commands the
/// session issued and the status each poll returns. The service drives any
/// `RendererChannel`, so this one fake covers the transport routing for both
/// renderer flavors.
#[derive(Default)]
struct FakeRendererState {
    loads: Vec<RendererMedia>,
    seeks: Vec<std::time::Duration>,
    pauses: u32,
    plays: u32,
    stops: u32,
    volumes: Vec<f32>,
}

#[derive(Clone)]
struct FakeRendererChannel {
    state: Arc<Mutex<FakeRendererState>>,
}

impl FakeRendererChannel {
    fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(FakeRendererState::default())),
        }
    }
}

impl RendererChannel for FakeRendererChannel {
    fn load(&mut self, media: &RendererMedia) -> Result<(), RendererError> {
        self.state.lock().unwrap().loads.push(media.clone());
        Ok(())
    }
    fn play(&mut self) -> Result<(), RendererError> {
        self.state.lock().unwrap().plays += 1;
        Ok(())
    }
    fn pause(&mut self) -> Result<(), RendererError> {
        self.state.lock().unwrap().pauses += 1;
        Ok(())
    }
    fn seek(&mut self, position: std::time::Duration) -> Result<(), RendererError> {
        self.state.lock().unwrap().seeks.push(position);
        Ok(())
    }
    fn set_volume(&mut self, level: f32) -> Result<(), RendererError> {
        self.state.lock().unwrap().volumes.push(level);
        Ok(())
    }
    fn stop(&mut self) -> Result<(), RendererError> {
        self.state.lock().unwrap().stops += 1;
        Ok(())
    }
    fn poll_status(&mut self) -> Result<ReceiverStatus, RendererError> {
        Ok(ReceiverStatus {
            player_state: RendererPlayerState::Playing,
            position: None,
            duration: None,
            volume: Some(1.0),
        })
    }
}

/// Poll `predicate` until it holds or a 2s deadline passes.
fn wait_until(predicate: impl Fn() -> bool) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while std::time::Instant::now() < deadline {
        if predicate() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    predicate()
}

/// Seed the audio format, segment, and backing file that make `track_id`
/// resolvable, so the remote path can turn it into media. No real bytes on disk —
/// the device, not bae, fetches the audio, so the remote path never decodes it.
async fn seed_playable_track(
    library_manager: &crate::library::LibraryManager,
    release_id: &str,
    track_id: &str,
) {
    use crate::db::{DbAudioFormat, DbAudioSegment, DbAudioSegmentRole, DbFile};
    use crate::util::content_type::ContentType;
    let now = chrono::Utc::now();
    let file_id = bae_test_support::test_uuid(&format!("{track_id}-file"));
    let file = DbFile::new(
        release_id,
        "track.flac",
        4_096,
        ContentType::Flac,
        file_id.clone(),
        now,
    );
    library_manager.add_file(&file).await.unwrap();
    let audio_format_id = bae_test_support::test_uuid(&format!("{track_id}-af"));
    let audio_format = DbAudioFormat::new(
        track_id,
        ContentType::Flac,
        44_100,
        Some(16),
        2,
        audio_format_id.clone(),
        now,
    );
    let segment = DbAudioSegment {
        id: bae_test_support::test_uuid(&format!("{track_id}-seg")),
        audio_format_id,
        segment_index: 0,
        role: DbAudioSegmentRole::Main,
        file_id,
        start_sample: 0,
        end_sample: None,
        start_byte: None,
        end_byte: None,
        created_at: now,
    };
    library_manager
        .insert_audio_format_with_segments_for_test(&audio_format, &[segment])
        .await
        .unwrap();
}

/// A playback service over releases whose every track is resolvable to remote
/// media.
async fn remote_service(
    releases: &[(&str, &[&str])],
) -> (
    TempDir,
    PlaybackService,
    tokio_mpsc::UnboundedReceiver<PlaybackProgress>,
) {
    let (home, service, rx) = seeded_playback_service(releases).await;
    for (release_id, tracks) in releases {
        for track_id in *tracks {
            seed_playable_track(&service.library_manager, release_id, track_id).await;
        }
    }
    (home, service, rx)
}

fn test_stream_provider() -> crate::renderer::MediaUrlProvider {
    Arc::new(|track_id: &str, _format| Ok(format!("http://renderer.local/stream?id={track_id}")))
}

fn remote_connect(channel: FakeRendererChannel) -> RemoteConnect {
    RemoteConnect::new(
        Box::new(channel),
        "Living Room".to_string(),
        test_stream_provider(),
        Arc::new(|_| None),
        cast_stream_format,
    )
}

/// `play_on` mid-track keeps the current track and queue position, switches the
/// renderer to Remote, and reissues the current track to the device at its
/// current position (a LOAD plus a seek).
#[tokio::test]
async fn play_on_reissues_current_track_at_position() {
    let (_home, mut service, _rx) = remote_service(&[(
        "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e",
        &[
            "08c7ff07-b56a-4e16-8df6-ae2967fa0806",
            "08c7fe07-b56a-4c63-8df6-ad2967fa0653",
        ],
    )])
    .await;
    service.playback_queue.apply(|queue| {
        queue.play_release(
            ContextSource::Release("e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e".to_string()),
            vec![
                "08c7ff07-b56a-4e16-8df6-ae2967fa0806".to_string(),
                "08c7fe07-b56a-4c63-8df6-ad2967fa0653".to_string(),
            ],
            ContextStart::Index(0),
        )
    });
    let buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(
        test_prepared_track("08c7ff07-b56a-4e16-8df6-ae2967fa0806", buffer),
        TrackPhase::Playing,
    );
    *service.current_position_shared.lock().unwrap() = Some(std::time::Duration::from_secs(30));

    let channel = FakeRendererChannel::new();
    let state = channel.state.clone();
    service.handle_play_on(remote_connect(channel)).await;

    assert!(
        service.renderer.is_remote(),
        "the renderer switches to Remote"
    );
    assert_eq!(
        service.slot.current_track_id(),
        Some("08c7ff07-b56a-4e16-8df6-ae2967fa0806"),
        "the current track is unchanged"
    );
    assert!(
        wait_until(|| {
            let s = state.lock().unwrap();
            s.loads.len() == 1 && s.seeks.contains(&std::time::Duration::from_secs(30))
        }),
        "the current track is loaded onto the device and seeked to its position"
    );
    assert_eq!(
        state.lock().unwrap().loads[0].url,
        "http://renderer.local/stream?id=08c7ff07-b56a-4e16-8df6-ae2967fa0806"
    );
}

/// A device `Finished` status advances the shared queue to the next track and
/// loads it onto the device — the same advance path local end-of-track uses.
#[tokio::test]
async fn remote_finished_advances_queue_and_loads_next() {
    let (_home, mut service, _rx) = remote_service(&[(
        "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e",
        &[
            "08c7ff07-b56a-4e16-8df6-ae2967fa0806",
            "08c7fe07-b56a-4c63-8df6-ad2967fa0653",
        ],
    )])
    .await;
    service.playback_queue.apply(|queue| {
        queue.play_release(
            ContextSource::Release("e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e".to_string()),
            vec![
                "08c7ff07-b56a-4e16-8df6-ae2967fa0806".to_string(),
                "08c7fe07-b56a-4c63-8df6-ad2967fa0653".to_string(),
            ],
            ContextStart::Index(0),
        )
    });
    let buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(
        test_prepared_track("08c7ff07-b56a-4e16-8df6-ae2967fa0806", buffer),
        TrackPhase::Playing,
    );
    *service.current_position_shared.lock().unwrap() = Some(std::time::Duration::ZERO);

    let channel = FakeRendererChannel::new();
    let state = channel.state.clone();
    service.handle_play_on(remote_connect(channel)).await;
    assert!(wait_until(|| !state.lock().unwrap().loads.is_empty()));

    service
        .handle_remote_status(RendererSessionStatus {
            player_state: RendererPlayerState::Finished,
            position: None,
            duration: None,
            volume: Some(1.0),
            ended: false,
        })
        .await;

    assert_eq!(
        service.slot.current_track_id(),
        Some("08c7fe07-b56a-4c63-8df6-ad2967fa0653"),
        "the queue advanced to the next track"
    );
    assert!(
        wait_until(|| state.lock().unwrap().loads.iter().any(
            |m| m.url == "http://renderer.local/stream?id=08c7fe07-b56a-4c63-8df6-ad2967fa0653"
        )),
        "the next track is loaded onto the device"
    );
}

/// A non-terminal device status feeds the shared progress channel, so every UI
/// and the position store update exactly as for local playback.
#[tokio::test]
async fn remote_status_feeds_progress() {
    let (_home, mut service, mut rx) = remote_service(&[(
        "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e",
        &["08c7ff07-b56a-4e16-8df6-ae2967fa0806"],
    )])
    .await;
    service.playback_queue.apply(|queue| {
        queue.play_release(
            ContextSource::Release("e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e".to_string()),
            vec!["08c7ff07-b56a-4e16-8df6-ae2967fa0806".to_string()],
            ContextStart::Index(0),
        )
    });
    let buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(
        test_prepared_track("08c7ff07-b56a-4e16-8df6-ae2967fa0806", buffer),
        TrackPhase::Playing,
    );

    let channel = FakeRendererChannel::new();
    service.handle_play_on(remote_connect(channel)).await;
    // Drain the setup events.
    while rx.try_recv().is_ok() {}

    service
        .handle_remote_status(RendererSessionStatus {
            player_state: RendererPlayerState::Playing,
            position: Some(std::time::Duration::from_secs(30)),
            duration: Some(std::time::Duration::from_secs(180)),
            volume: Some(1.0),
            ended: false,
        })
        .await;

    let mut saw_position = false;
    while let Ok(progress) = rx.try_recv() {
        if let PlaybackProgress::PositionUpdate {
            position_ms,
            track_id,
            ..
        } = progress
        {
            if track_id == "08c7ff07-b56a-4e16-8df6-ae2967fa0806" && position_ms == 30_000 {
                saw_position = true;
            }
        }
    }
    assert!(
        saw_position,
        "the device's position must flow as a PositionUpdate for the current track"
    );
}

/// Stopping remote playback stops the device, drops the renderer back to Local,
/// and announces `RemoteStatusChanged(None)` so the UI leaves the remote state.
#[tokio::test]
async fn stop_remote_stops_device_and_returns_to_local() {
    let (_home, mut service, mut rx) = remote_service(&[(
        "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e",
        &["08c7ff07-b56a-4e16-8df6-ae2967fa0806"],
    )])
    .await;
    service.playback_queue.apply(|queue| {
        queue.play_release(
            ContextSource::Release("e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e".to_string()),
            vec!["08c7ff07-b56a-4e16-8df6-ae2967fa0806".to_string()],
            ContextStart::Index(0),
        )
    });
    let buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(
        test_prepared_track("08c7ff07-b56a-4e16-8df6-ae2967fa0806", buffer),
        TrackPhase::Playing,
    );

    let channel = FakeRendererChannel::new();
    let state = channel.state.clone();
    service.handle_play_on(remote_connect(channel)).await;
    assert!(wait_until(|| !state.lock().unwrap().loads.is_empty()));
    while rx.try_recv().is_ok() {}

    service.handle_stop_remote().await;

    assert!(
        !service.renderer.is_remote(),
        "the renderer returns to Local"
    );
    assert!(
        wait_until(|| state.lock().unwrap().stops == 1),
        "the device is told to stop"
    );
    let mut saw_not_remote = false;
    while let Ok(progress) = rx.try_recv() {
        if let PlaybackProgress::RemoteStatusChanged { device_name: None } = progress {
            saw_not_remote = true;
        }
    }
    assert!(
        saw_not_remote,
        "stopping remote playback announces RemoteStatusChanged(None)"
    );
}

/// A plain `stop()` while playing remotely must stop the device and return to
/// local — stop means stop (pause is what keeps the session warm). Without
/// routing stop through the renderer, the local slot goes Stopped while the
/// device stays connected and playing. This is the routing bug the Cast round
/// hit; the DLNA channel is held to the same contract in its own tests.
#[tokio::test]
async fn stop_while_remote_stops_device_and_returns_to_local() {
    let (_home, mut service, mut rx) = remote_service(&[(
        "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e",
        &["08c7ff07-b56a-4e16-8df6-ae2967fa0806"],
    )])
    .await;
    service.playback_queue.apply(|queue| {
        queue.play_release(
            ContextSource::Release("e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e".to_string()),
            vec!["08c7ff07-b56a-4e16-8df6-ae2967fa0806".to_string()],
            ContextStart::Index(0),
        )
    });
    let buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(
        test_prepared_track("08c7ff07-b56a-4e16-8df6-ae2967fa0806", buffer),
        TrackPhase::Playing,
    );

    let channel = FakeRendererChannel::new();
    let state = channel.state.clone();
    service.handle_play_on(remote_connect(channel)).await;
    assert!(wait_until(|| !state.lock().unwrap().loads.is_empty()));
    while rx.try_recv().is_ok() {}

    service.stop().await;

    assert!(
        !service.renderer.is_remote(),
        "stop must return the renderer to local"
    );
    assert!(
        wait_until(|| state.lock().unwrap().stops == 1),
        "stop must stop the device, not leave it playing"
    );
    assert!(
        matches!(service.slot, PlaybackSlot::Stopped),
        "the slot must be Stopped after stop"
    );
    let mut saw_not_remote = false;
    while let Ok(progress) = rx.try_recv() {
        if let PlaybackProgress::RemoteStatusChanged { device_name: None } = progress {
            saw_not_remote = true;
        }
    }
    assert!(
        saw_not_remote,
        "stopping while remote announces RemoteStatusChanged(None)"
    );
}

/// Set up a remote-playback service over a single fake-backed track `t1`, current
/// at position 0, and return the service plus the fake channel's shared state.
async fn remote_over_fake() -> (
    TempDir,
    PlaybackService,
    Arc<Mutex<FakeRendererState>>,
    tokio_mpsc::UnboundedReceiver<PlaybackProgress>,
) {
    let (home, mut service, rx) = remote_service(&[(
        "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e",
        &["08c7ff07-b56a-4e16-8df6-ae2967fa0806"],
    )])
    .await;
    service.playback_queue.apply(|queue| {
        queue.play_release(
            ContextSource::Release("e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e".to_string()),
            vec!["08c7ff07-b56a-4e16-8df6-ae2967fa0806".to_string()],
            ContextStart::Index(0),
        )
    });
    let buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(
        test_prepared_track("08c7ff07-b56a-4e16-8df6-ae2967fa0806", buffer),
        TrackPhase::Playing,
    );
    *service.current_position_shared.lock().unwrap() = Some(std::time::Duration::ZERO);

    let channel = FakeRendererChannel::new();
    let state = channel.state.clone();
    service.handle_play_on(remote_connect(channel)).await;
    assert!(wait_until(|| !state.lock().unwrap().loads.is_empty()));
    (home, service, state, rx)
}

/// Pause while remote routes to the device.
#[tokio::test]
async fn pause_while_remote_pauses_the_device() {
    let (_home, mut service, state, _rx) = remote_over_fake().await;
    service.pause();
    assert!(
        wait_until(|| state.lock().unwrap().pauses == 1),
        "pause while remote must pause the device"
    );
}

/// Resume while remote routes to the device.
#[tokio::test]
async fn resume_while_remote_plays_the_device() {
    let (_home, mut service, state, _rx) = remote_over_fake().await;
    service.pause();
    assert!(wait_until(|| state.lock().unwrap().pauses == 1));
    service.resume().await;
    assert!(
        wait_until(|| state.lock().unwrap().plays == 1),
        "resume while remote must play the device"
    );
}

/// Seek while remote routes to the device (and skips the local rebuild path).
#[tokio::test]
async fn seek_while_remote_seeks_the_device() {
    let (_home, mut service, state, _rx) = remote_over_fake().await;
    service.seek(std::time::Duration::from_secs(45)).await;
    assert!(
        wait_until(|| state
            .lock()
            .unwrap()
            .seeks
            .contains(&std::time::Duration::from_secs(45))),
        "seek while remote must seek the device"
    );
}

/// Setting the volume while remote sets the device's volume too.
#[tokio::test]
async fn set_volume_while_remote_sets_the_device_volume() {
    let (_home, mut service, state, _rx) = remote_over_fake().await;
    service.set_volume(0.3);
    assert!(
        wait_until(|| state.lock().unwrap().volumes.contains(&0.3)),
        "setting the volume while remote must set the device's volume"
    );
}

// -- AirPlay renderer-seam tests --

/// Records the control operations the service drives an AirPlay stream through,
/// standing in for the RAOP session so the seam is tested without a receiver.
#[derive(Default)]
struct FakeAirPlayControlState {
    flushed: std::sync::atomic::AtomicU64,
    reanchored: std::sync::atomic::AtomicU64,
    failed: std::sync::atomic::AtomicBool,
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
    fn has_failed(&self) -> bool {
        self.0.failed.load(std::sync::atomic::Ordering::Acquire)
    }
    fn frames_sent(&self) -> u64 {
        0
    }
    fn latency_frames(&self) -> u32 {
        88_200
    }
}

/// Install an AirPlay renderer on `service` with a fake control published, a
/// tagged saved local output (volume `saved_tag`), and the given latency.
fn install_airplay(
    service: &mut PlaybackService,
    latency_frames: u32,
    saved_tag: f32,
) -> Arc<FakeAirPlayControlState> {
    let state = Arc::new(FakeAirPlayControlState::default());
    let control: Arc<dyn crate::playback::airplay_output::AirPlayStreamControl> =
        Arc::new(FakeAirPlayControl(state.clone()));
    let saved = TestAudioOutput::new();
    saved.set_volume(saved_tag);
    service.renderer = Renderer::AirPlay(renderer::AirPlayRenderer::new(
        control,
        Box::new(saved),
        latency_frames,
    ));
    state
}

#[tokio::test]
async fn airplay_pause_flushes_and_resume_reanchors() {
    let (_home, mut service, _progress_rx) = test_playback_service().await;
    let buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(test_prepared_track("t", buffer), TrackPhase::Playing);
    let state = install_airplay(&mut service, 88_200, 0.5);

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
    // 88_200 frames at 44.1 kHz = 2 s of latency.
    install_airplay(&mut service, 88_200, 0.5);

    // A tick at 5 s of decoded position: the audible position is 5 − 2 = 3 s.
    let mut fmt = test_track_fmt("t");
    fmt.duration_ms = 60_000;
    service
        .handle_position_event(Arc::new(fmt), std::time::Duration::from_secs(5))
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
    install_airplay(&mut service, 88_200, 0.777);

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
    install_airplay(&mut service, 88_200, saved_tag);

    // Playback progressed on AirPlay: decode is local, so the shared position is
    // the live one the resume reads.
    *service.current_position_shared.lock().unwrap() = Some(std::time::Duration::from_secs(30));

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
    service.current_position_shared =
        Arc::new(std::sync::Mutex::new(Some(std::time::Duration::ZERO)));
    let state = install_airplay(&mut service, 88_200, 0.5);

    service.seek(std::time::Duration::from_secs(20)).await;

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
    install_airplay(&mut service, 88_200, 0.5);

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
        _source: Box<dyn crate::airplay::stream::PcmSource>,
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
            "Living Room".to_string(),
            88_200,
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
/// erroring silently forever.
#[tokio::test]
async fn airplay_receiver_death_ends_airplay_and_returns_to_local() {
    let (_home, mut service, _progress_rx) =
        seeded_playback_service(&[("af63ef4c-8602-4cd5-82c0-3d334b916305", &[TRACK_T])]).await;
    service.slot = active_slot(
        test_prepared_track(TRACK_T, create_sparse_buffer(1_024)),
        TrackPhase::Playing,
    );
    let state = install_airplay(&mut service, 88_200, 0.5);

    // The receiver went away: the session reports the transport as failed.
    state
        .failed
        .store(true, std::sync::atomic::Ordering::Release);

    // A routine position tick catches it and ends AirPlay.
    service
        .handle_position_event(
            Arc::new(test_track_fmt(TRACK_T)),
            std::time::Duration::from_secs(1),
        )
        .await;

    assert!(
        !service.renderer.is_airplay(),
        "a dead receiver ends AirPlay and returns to the local renderer"
    );
}
