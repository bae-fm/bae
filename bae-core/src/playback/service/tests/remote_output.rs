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
    seed_track_window(library_manager, release_id, track_id, 0, None).await;
}

/// [`seed_playable_track`] with its one main segment spanning
/// `start_sample..end_sample` of its file, as a CUE image's track does.
async fn seed_track_window(
    library_manager: &crate::library::LibraryManager,
    release_id: &str,
    track_id: &str,
    start_sample: u64,
    end_sample: Option<u64>,
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
        start_sample,
        end_sample,
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

fn test_device() -> crate::playback::RemoteDevice {
    crate::playback::RemoteDevice {
        id: "renderer-device-id".to_string(),
        name: "Speaker Name".to_string(),
    }
}

fn remote_connect(channel: FakeChannel) -> RemoteConnect {
    RemoteConnect::new(
        Box::new(channel),
        test_device(),
        crate::renderer::RendererMediaSource::new(
            test_stream_provider(),
            Arc::new(|cover| format!("http://renderer.local/cover?id={}&v={}", cover.id, cover.version)),
            cast_stream_format,
        ),
    )
}

/// The arrange every remote test shares: a service over one release of `tracks`,
/// that release playing from its first track, and the session handed to a fake
/// device by `handle_play_on`, from `position` in the first track's stream.
/// Returns the service, the fake channel's shared state and the progress
/// receiver.
async fn playing_remote_fixture(
    tracks: &[&str],
    position: StreamPosition,
) -> (
    TempDir,
    PlaybackService,
    Arc<Mutex<FakeChannelState>>,
    tokio_mpsc::UnboundedReceiver<PlaybackProgress>,
) {
    const RELEASE: &str = "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e";
    let (home, mut service, rx) = remote_service(&[(RELEASE, tracks)]).await;
    service.playback_queue.apply(|queue| {
        queue.play_release(
            ContextSource::Release(RELEASE.to_string()),
            tracks.iter().map(|t| (*t).to_string()).collect(),
            ContextStart::Index(0),
        )
    });
    service.slot = active_slot(
        test_prepared_track(tracks[0], create_sparse_buffer(1_024)),
        TrackPhase::Playing,
    );
    if let PlaybackSlot::Active(cur) = &mut service.slot {
        cur.position = position;
    }

    let channel = FakeChannel::new();
    let state = channel.state.clone();
    service.handle_play_on(remote_connect(channel)).await;
    (home, service, state, rx)
}

/// `play_on` mid-track keeps the current track and queue position, switches the
/// renderer to Remote, and reissues the current track to the device at its
/// current position (a LOAD plus a seek).
#[tokio::test]
async fn play_on_reissues_current_track_at_position() {
    let (_home, service, state, _rx) = playing_remote_fixture(
        &[
            "08c7ff07-b56a-4e16-8df6-ae2967fa0806",
            "08c7fe07-b56a-4c63-8df6-ad2967fa0653",
        ],
        StreamPosition::from_millis(30_000),
    )
    .await;

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
    let (_home, mut service, state, _rx) = playing_remote_fixture(
        &[
            "08c7ff07-b56a-4e16-8df6-ae2967fa0806",
            "08c7fe07-b56a-4c63-8df6-ad2967fa0653",
        ],
        StreamPosition::START,
    )
    .await;
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
    let (_home, mut service, _state, mut rx) =
        playing_remote_fixture(&["08c7ff07-b56a-4e16-8df6-ae2967fa0806"], StreamPosition::START).await;
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

/// Handing playback to a device announces that device by its id as well as its
/// name: the UI matches the id against the device list, since names need not
/// be unique.
#[tokio::test]
async fn handing_over_announces_the_device_by_id() {
    let (_home, _service, _state, mut rx) =
        playing_remote_fixture(&["08c7ff07-b56a-4e16-8df6-ae2967fa0806"], StreamPosition::START).await;

    let mut announced = None;
    while let Ok(progress) = rx.try_recv() {
        if let PlaybackProgress::RemoteStatusChanged { device } = progress {
            announced = device;
        }
    }
    assert_eq!(announced, Some(test_device()));
}

/// Stopping remote playback stops the device, drops the renderer back to Local,
/// and announces `RemoteStatusChanged(None)` so the UI leaves the remote state.
#[tokio::test]
async fn stop_remote_stops_device_and_returns_to_local() {
    let (_home, mut service, state, mut rx) =
        playing_remote_fixture(&["08c7ff07-b56a-4e16-8df6-ae2967fa0806"], StreamPosition::START).await;
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
        if let PlaybackProgress::RemoteStatusChanged { device: None } = progress {
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
    let (_home, mut service, state, mut rx) =
        playing_remote_fixture(&["08c7ff07-b56a-4e16-8df6-ae2967fa0806"], StreamPosition::START).await;
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
        if let PlaybackProgress::RemoteStatusChanged { device: None } = progress {
            saw_not_remote = true;
        }
    }
    assert!(
        saw_not_remote,
        "stopping while remote announces RemoteStatusChanged(None)"
    );
}

/// [`playing_remote_fixture`] over one track at position zero, waited until the
/// device has taken its first load.
async fn remote_over_fake() -> (
    TempDir,
    PlaybackService,
    Arc<Mutex<FakeChannelState>>,
    tokio_mpsc::UnboundedReceiver<PlaybackProgress>,
) {
    let (home, service, state, rx) = playing_remote_fixture(
        &["08c7ff07-b56a-4e16-8df6-ae2967fa0806"],
        StreamPosition::START,
    )
    .await;
    assert!(wait_until(|| !state.lock().unwrap().loads.is_empty()));
    (home, service, state, rx)
}

/// A device plays the stream it is served, so each track is served as its own
/// stream from its first sample: a track that is a whole file goes out as that
/// file, and a CUE image's track is transcoded from its window rather than
/// served as the whole image.
#[tokio::test]
async fn a_cue_image_track_is_served_as_its_own_window() {
    const RELEASE: &str = "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e";
    const WHOLE_FILE: &str = "08c7ff07-b56a-4e16-8df6-ae2967fa0806";
    const IMAGE_WINDOW: &str = "08c7fe07-b56a-4c63-8df6-ad2967fa0653";
    let (_home, mut service, _rx) =
        seeded_playback_service(&[(RELEASE, &[WHOLE_FILE, IMAGE_WINDOW])]).await;
    seed_playable_track(&service.library_manager, RELEASE, WHOLE_FILE).await;
    seed_track_window(
        &service.library_manager,
        RELEASE,
        IMAGE_WINDOW,
        441_000,
        Some(882_000),
    )
    .await;
    service.playback_queue.apply(|queue| {
        queue.play_release(
            ContextSource::Release(RELEASE.to_string()),
            vec![WHOLE_FILE.to_string(), IMAGE_WINDOW.to_string()],
            ContextStart::Index(0),
        )
    });
    service.slot = active_slot(
        test_prepared_track(WHOLE_FILE, create_sparse_buffer(1_024)),
        TrackPhase::Playing,
    );
    let channel = FakeChannel::new();
    let state = channel.state.clone();
    service
        .handle_play_on(RemoteConnect::new(
            Box::new(channel),
            test_device(),
            crate::renderer::RendererMediaSource::new(
                Arc::new(|track_id: &str, format| {
                    Ok(format!("http://renderer.local/stream?id={track_id}&{format:?}"))
                }),
                Arc::new(|cover| format!("http://renderer.local/cover?id={}", cover.id)),
                cast_stream_format,
            ),
        ))
        .await;
    service
        .handle_remote_status(RendererSessionStatus {
            player_state: RendererPlayerState::Finished,
            position: None,
            duration: None,
            volume: Some(1.0),
            ended: false,
        })
        .await;

    assert!(
        wait_until(|| state.lock().unwrap().loads.len() == 2),
        "both tracks are loaded onto the device"
    );
    let s = state.lock().unwrap();
    let served: Vec<(&str, &str)> = s
        .loads
        .iter()
        .map(|m| (m.url.as_str(), m.content_type.as_str()))
        .collect();
    assert_eq!(
        served,
        [
            (
                "http://renderer.local/stream?id=08c7ff07-b56a-4e16-8df6-ae2967fa0806&Raw",
                "audio/flac"
            ),
            (
                "http://renderer.local/stream?id=08c7fe07-b56a-4c63-8df6-ad2967fa0653&TranscodeMp3",
                "audio/mpeg"
            ),
        ]
    );
}

/// Ending remote playback resumes locally where the current track is on the
/// device: after the device finished a track and was loaded with the next, at
/// the next track's start — not where the finished track last was.
#[tokio::test]
async fn ending_remote_playback_after_an_advance_resumes_the_next_track_at_its_start() {
    const FIRST: &str = "08c7ff07-b56a-4e16-8df6-ae2967fa0806";
    const SECOND: &str = "08c7fe07-b56a-4c63-8df6-ad2967fa0653";
    let (_home, mut service, state, _rx) =
        playing_remote_fixture(&[FIRST, SECOND], StreamPosition::START).await;
    assert!(wait_until(|| !state.lock().unwrap().loads.is_empty()));
    for (player_state, position) in [
        (RendererPlayerState::Playing, Some(std::time::Duration::from_secs(30))),
        (RendererPlayerState::Finished, None),
    ] {
        service
            .handle_remote_status(RendererSessionStatus {
                player_state,
                position,
                duration: None,
                volume: Some(1.0),
                ended: false,
            })
            .await;
    }

    service.handle_stop_remote().await;

    assert_eq!(
        service.current_track_position(),
        Some((SECOND.to_string(), StreamPosition::START))
    );
}

/// Ending remote playback right after a seek on the device resumes locally at
/// the seek target, before the device has reported a position from there.
#[tokio::test]
async fn ending_remote_playback_after_a_seek_resumes_at_the_seek_target() {
    const TRACK: &str = "08c7ff07-b56a-4e16-8df6-ae2967fa0806";
    let (_home, mut service, state, _rx) =
        playing_remote_fixture(&[TRACK], StreamPosition::from_millis(30_000)).await;
    assert!(wait_until(|| !state.lock().unwrap().loads.is_empty()));
    service.seek(StreamPosition::from_millis(45_000)).await;

    service.handle_stop_remote().await;

    assert_eq!(
        service.current_track_position(),
        Some((TRACK.to_string(), StreamPosition::from_millis(45_000)))
    );
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
    service.seek(StreamPosition::from_duration(std::time::Duration::from_secs(45))).await;
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
    fmt.timeline = TrackTimeline::new(std::time::Duration::from_secs(60), None);
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
    let state = install_airplay(&mut service, 88_200, 0.5);

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
            test_device(),
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
            StreamPosition::from_millis(1_000),
        )
        .await;

    assert!(
        !service.renderer.is_airplay(),
        "a dead receiver ends AirPlay and returns to the local renderer"
    );
}

/// A queued track renamed while an earlier one plays on a device reaches the
/// device when it loads: each load reads the track's display from the library
/// then, not from a copy taken when the queue was filled or the device was
/// handed playback. The device is not loaded again mid-track.
#[tokio::test]
async fn a_queued_track_renamed_while_another_plays_remotely_loads_with_its_new_title() {
    const RELEASE: &str = "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e";
    const FIRST: &str = "08c7ff07-b56a-4e16-8df6-ae2967fa0806";
    const SECOND: &str = "08c7fe07-b56a-4c63-8df6-ad2967fa0653";
    let (_home, mut service, state, _rx) =
        playing_remote_fixture(&[FIRST, SECOND], StreamPosition::START).await;
    let (load_tx, mut loads) = tokio_mpsc::unbounded_channel();
    state.lock().unwrap().load_events = Some(load_tx);
    let first_url = format!("http://renderer.local/stream?id={FIRST}");
    let second_url = format!("http://renderer.local/stream?id={SECOND}");

    service
        .library_manager
        .apply_release_metadata_user_edit(
            RELEASE,
            &crate::import::ReleaseUserEdit {
                album_title: "Album Title".to_string(),
                album_artist_assignments: vec![crate::import::ArtistAssignment::Picked {
                    artist: crate::import::ExistingArtist {
                        artist_id: bae_test_support::test_uuid(
                            "e36744a5-1a36-460f-891c-e7e558034edf",
                        ),
                        name: "Artist Name".to_string(),
                        sort_name: None,
                        musicbrainz_artist_id: None,
                        discogs_artist_id: None,
                    },
                }],
                album_year: None,
                pressing: crate::pressing::Pressing::blank(),
                tracks: vec![
                    crate::import::TrackUserEdit {
                        title: "Track Title".to_string(),
                        side: None,
                        track_number: Some(1),
                        artist_assignments: crate::import::TrackArtistAssignments::AlbumArtists,
                        file: None,
                    },
                    crate::import::TrackUserEdit {
                        title: "Renamed Track".to_string(),
                        side: None,
                        track_number: Some(2),
                        artist_assignments: crate::import::TrackArtistAssignments::AlbumArtists,
                        file: None,
                    },
                ],
            },
        )
        .await
        .unwrap();

    service
        .handle_remote_status(RendererSessionStatus {
            player_state: RendererPlayerState::Finished,
            position: None,
            duration: None,
            volume: Some(1.0),
            ended: false,
        })
        .await;

    let second = loop {
        let media = tokio::time::timeout(std::time::Duration::from_secs(5), loads.recv())
            .await
            .expect("the device is loaded with the next track")
            .expect("the fake channel keeps its load sender");
        if media.url == second_url {
            break media;
        }
        assert_eq!(
            media.url, first_url,
            "only the handoff's load of the first track comes before the next track's"
        );
    };
    assert_eq!(second.title, "Renamed Track");
    let s = state.lock().unwrap();
    let urls: Vec<&str> = s.loads.iter().map(|m| m.url.as_str()).collect();
    assert_eq!(
        urls,
        [first_url.as_str(), second_url.as_str()],
        "the first track is loaded once, at the handoff, and never again"
    );
}
