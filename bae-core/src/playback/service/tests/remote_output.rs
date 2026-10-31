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
    seed_track_window(library_manager, release_id, track_id, 0, None, None).await;
}

/// [`seed_playable_track`] with its one main segment spanning
/// `start_sample..end_sample` of its file, as a CUE image's track does, after
/// `generated_pregap_samples` of silence a CUE `PREGAP` generates.
async fn seed_track_window(
    library_manager: &crate::library::LibraryManager,
    release_id: &str,
    track_id: &str,
    start_sample: u64,
    end_sample: Option<u64>,
    generated_pregap_samples: Option<u64>,
) {
    use crate::db::{DbAudioFormat, DbAudioSegment, DbAudioSegmentRole, NewReleaseFile};
    use bae_util::content_type::ContentType;
    let now = chrono::Utc::now();
    let file_id = bae_test_support::test_uuid(&format!("{track_id}-file"));
    let file = NewReleaseFile::new(
        release_id,
        "track.flac",
        ContentType::Flac,
        file_id.clone(),
        now,
    );
    library_manager.add_file(&file, 4_096).await.unwrap();
    let audio_format_id = bae_test_support::test_uuid(&format!("{track_id}-af"));
    let audio_format = DbAudioFormat::new(
        track_id,
        ContentType::Flac,
        std::num::NonZeroU32::new(44_100).unwrap(),
        Some(std::num::NonZeroU32::new(16).unwrap()),
        std::num::NonZeroU32::new(2).unwrap(),
        audio_format_id.clone(),
        now,
    )
    .with_pregaps(None, generated_pregap_samples);
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
            RendererFlavor::Cast,
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
    remote_fixture_from(tracks, position, TrackPhase::Playing).await
}

/// [`playing_remote_fixture`] with the first track in `phase` when it is
/// handed to the device.
async fn remote_fixture_from(
    tracks: &[&str],
    position: StreamPosition,
    phase: TrackPhase,
) -> (
    TempDir,
    PlaybackService,
    Arc<Mutex<FakeChannelState>>,
    tokio_mpsc::UnboundedReceiver<PlaybackProgress>,
) {
    const RELEASE: &str = "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e";
    let (home, mut service, rx) = remote_service(&[(RELEASE, tracks)]).await;
    fill_test_release(&mut service, RELEASE).await;
    service.slot = active_slot(
        test_prepared_track(tracks[0], create_sparse_buffer(1_024)),
        phase,
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
        .handle_remote_event(RendererSessionEvent::Status(ReceiverStatus {
            player_state: RendererPlayerState::Finished,
            position: None,
            duration: None,
            volume: Some(1.0),
        }))
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
        .handle_remote_event(RendererSessionEvent::Status(ReceiverStatus {
            player_state: RendererPlayerState::Playing,
            position: Some(std::time::Duration::from_secs(30)),
            duration: Some(std::time::Duration::from_secs(180)),
            volume: Some(1.0),
        }))
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
        if let PlaybackProgress::RemoteStatusChanged { remote } = progress {
            announced = Some(remote);
        }
    }
    assert_eq!(announced, Some(RemotePlayback::On(test_device())));
}

/// A device-side end — the link lost — resumes local playback and announces
/// the device that was lost with why, so the cast surfaces can say what
/// happened rather than quietly falling back.
#[tokio::test]
async fn a_lost_link_returns_to_local_and_says_why() {
    let (_home, mut service, _state, mut rx) = remote_over_fake().await;
    while rx.try_recv().is_ok() {}

    service
        .handle_remote_event(RendererSessionEvent::Ended(
            crate::renderer::RendererError::Connection("receiver closed the session".to_string()),
        ))
        .await;

    assert!(!service.renderer.is_remote(), "the renderer returns to Local");
    let mut announced = None;
    while let Ok(progress) = rx.try_recv() {
        if let PlaybackProgress::RemoteStatusChanged { remote } = progress {
            announced = Some(remote);
        }
    }
    assert_eq!(
        announced,
        Some(RemotePlayback::Ended {
            device: test_device(),
            detail: "renderer connection failed: receiver closed the session".to_string(),
        })
    );
}

/// A command the device refused reaches the person as a playback error in the
/// renderer's category, with the device's reason as its detail; the session
/// stays on the device.
#[tokio::test]
async fn a_refused_command_is_reported_and_the_device_kept() {
    let (_home, mut service, _state, mut rx) = remote_over_fake().await;
    while rx.try_recv().is_ok() {}

    service
        .handle_remote_event(RendererSessionEvent::CommandFailed(
            crate::renderer::RendererError::Command("LOAD failed: unsupported media".to_string()),
        ))
        .await;

    assert!(service.renderer.is_remote(), "the session stays on the device");
    let mut reported = None;
    while let Ok(progress) = rx.try_recv() {
        if let PlaybackProgress::PlaybackError {
            reason: crate::ui::PlaybackErrorReason::Diagnostic { error },
        } = progress
        {
            reported = Some(error);
        }
    }
    assert_eq!(
        reported,
        Some(crate::ui::UiError::diagnostic(
            crate::ui::UiErrorCategory::Renderer,
            "renderer command failed: LOAD failed: unsupported media",
        ))
    );
}

/// Stopping remote playback stops the device, drops the renderer back to Local,
/// and announces local playback so the UI leaves the remote state.
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
        if let PlaybackProgress::RemoteStatusChanged {
            remote: RemotePlayback::Local,
        } = progress
        {
            saw_not_remote = true;
        }
    }
    assert!(
        saw_not_remote,
        "stopping remote playback announces local playback"
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
        if let PlaybackProgress::RemoteStatusChanged {
            remote: RemotePlayback::Local,
        } = progress
        {
            saw_not_remote = true;
        }
    }
    assert!(
        saw_not_remote,
        "stopping while remote announces local playback"
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
            .handle_remote_event(RendererSessionEvent::Status(ReceiverStatus {
                player_state,
                position,
                duration: None,
                volume: Some(1.0),
            }))
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

/// The `Seeked` positions `rx` holds for `track_id`, in track time.
fn shown_positions(
    rx: &mut tokio_mpsc::UnboundedReceiver<PlaybackProgress>,
    track_id: &str,
) -> Vec<i64> {
    let mut shown = Vec::new();
    while let Ok(progress) = rx.try_recv() {
        if let PlaybackProgress::Seeked {
            track_id: tid,
            position_ms,
            ..
        } = progress
        {
            if tid == track_id {
                shown.push(position_ms);
            }
        }
    }
    shown
}

/// A paused track handed to a device loads there paused, and shows where it
/// is: nothing plays to report the position.
#[tokio::test]
async fn handing_over_a_paused_track_shows_its_position() {
    const TRACK: &str = "08c7ff07-b56a-4e16-8df6-ae2967fa0806";
    let (_home, service, state, mut rx) = remote_fixture_from(
        &[TRACK],
        StreamPosition::from_millis(30_000),
        TrackPhase::Paused(PausePhase::Manual),
    )
    .await;
    assert!(wait_until(|| !state.lock().unwrap().loads.is_empty()));

    assert!(matches!(
        &service.slot,
        PlaybackSlot::Active(cur) if matches!(cur.phase, TrackPhase::Paused(_))
    ));
    assert_eq!(shown_positions(&mut rx, TRACK), vec![30_000]);
}

/// Ending remote playback resumes locally paused, and shows where.
#[tokio::test]
async fn ending_remote_playback_shows_the_resumed_position() {
    const TRACK: &str = "08c7ff07-b56a-4e16-8df6-ae2967fa0806";
    let (_home, mut service, state, mut rx) =
        playing_remote_fixture(&[TRACK], StreamPosition::from_millis(30_000)).await;
    assert!(wait_until(|| !state.lock().unwrap().loads.is_empty()));
    while rx.try_recv().is_ok() {}

    service.handle_stop_remote().await;

    assert_eq!(shown_positions(&mut rx, TRACK), vec![30_000]);
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
                album_artist_assignments: vec![crate::import::Credited::plain(crate::import::ArtistAssignment::Picked {
                    credit_name: None,
                    artist: crate::import::ExistingArtist {
                        artist_id: bae_test_support::test_uuid(
                            "e36744a5-1a36-460f-891c-e7e558034edf",
                        ),
                        name: "Artist Name".to_string(),
                        sort_name: None,
                        musicbrainz_artist_id: None,
                        discogs_artist_id: None,
                    },
                })],
                album_year: None,
                pressing: bae_types::pressing::Pressing::blank(),
                tracks: vec![
                    crate::import::TrackUserEdit {
                        title: "Track Title".to_string(),
                        artist_assignments: crate::import::TrackArtistAssignments::AlbumArtists,
                        file: None,
                    },
                    crate::import::TrackUserEdit {
                        title: "Renamed Track".to_string(),
                        artist_assignments: crate::import::TrackArtistAssignments::AlbumArtists,
                        file: None,
                    },
                ],
            },
        )
        .await
        .unwrap();

    service
        .handle_remote_event(RendererSessionEvent::Status(ReceiverStatus {
            player_state: RendererPlayerState::Finished,
            position: None,
            duration: None,
            volume: Some(1.0),
        }))
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
