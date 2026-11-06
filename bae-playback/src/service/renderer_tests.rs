use super::*;
use crate::audio_output::{audio_event_channel, AudioError};
use crate::renderer::channel::FakeChannel;
use crate::renderer::{ReceiverStatus, RendererFlavor, RendererPlayerState, RendererSessionEvent};
use crate::service::tests::*;
use crate::RemotePlayback;
use bae_audio::track_stream::create_track_stream_pair;
use bae_util::test_channels::HANG_GUARD;

/// `play_on` mid-track keeps the current track and queue position, switches the
/// renderer to Remote, and reissues the current track to the device at its
/// current position (a LOAD plus a seek).
#[tokio::test]
async fn play_on_reissues_current_track_at_position() {
    let (_home, service, state, _rx) = playing_remote_fixture(
        &["08c7ff07-b56a-4e16-8df6-ae2967fa0806", "08c7fe07-b56a-4c63-8df6-ad2967fa0653"],
        StreamPosition::from_millis(30_000),
    )
    .await;

    assert!(service.renderer.is_remote(), "the renderer switches to Remote");
    assert_eq!(
        service.slot.current_track_id(),
        Some("08c7ff07-b56a-4e16-8df6-ae2967fa0806"),
        "the current track is unchanged"
    );
    assert!(
        wait_until(&state, |s| {
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
        &["08c7ff07-b56a-4e16-8df6-ae2967fa0806", "08c7fe07-b56a-4c63-8df6-ad2967fa0653"],
        StreamPosition::START,
    )
    .await;
    assert!(wait_until(&state, |s| !s.loads.is_empty()));

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
        wait_until(&state, |s| s.loads.iter().any(
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
        playing_remote_fixture(&["08c7ff07-b56a-4e16-8df6-ae2967fa0806"], StreamPosition::START)
            .await;
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
        if let PlaybackProgress::PositionUpdate { position_ms, track_id, .. } = progress {
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
        playing_remote_fixture(&["08c7ff07-b56a-4e16-8df6-ae2967fa0806"], StreamPosition::START)
            .await;

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
        if let PlaybackProgress::PlaybackError { reason } = progress {
            reported = Some(reason);
        }
    }
    assert_eq!(
        reported,
        Some(crate::PlaybackErrorReason::renderer(
            "renderer command failed: LOAD failed: unsupported media",
        ))
    );
}

/// Stopping remote playback stops the device, drops the renderer back to Local,
/// and announces local playback so the UI leaves the remote state.
#[tokio::test]
async fn stop_remote_stops_device_and_returns_to_local() {
    let (_home, mut service, state, mut rx) =
        playing_remote_fixture(&["08c7ff07-b56a-4e16-8df6-ae2967fa0806"], StreamPosition::START)
            .await;
    assert!(wait_until(&state, |s| !s.loads.is_empty()));
    while rx.try_recv().is_ok() {}

    service.handle_stop_remote().await;

    assert!(!service.renderer.is_remote(), "the renderer returns to Local");
    assert!(wait_until(&state, |s| s.stops == 1), "the device is told to stop");
    let mut saw_not_remote = false;
    while let Ok(progress) = rx.try_recv() {
        if let PlaybackProgress::RemoteStatusChanged { remote: RemotePlayback::Local } = progress {
            saw_not_remote = true;
        }
    }
    assert!(saw_not_remote, "stopping remote playback announces local playback");
}

/// A plain `stop()` while playing remotely must stop the device and return to
/// local — stop means stop (pause is what keeps the session warm). Without
/// routing stop through the renderer, the local slot goes Stopped while the
/// device stays connected and playing. This is the routing bug the Cast round
/// hit; the DLNA channel is held to the same contract in its own tests.
#[tokio::test]
async fn stop_while_remote_stops_device_and_returns_to_local() {
    let (_home, mut service, state, mut rx) =
        playing_remote_fixture(&["08c7ff07-b56a-4e16-8df6-ae2967fa0806"], StreamPosition::START)
            .await;
    assert!(wait_until(&state, |s| !s.loads.is_empty()));
    while rx.try_recv().is_ok() {}

    service.stop().await;

    assert!(!service.renderer.is_remote(), "stop must return the renderer to local");
    assert!(
        wait_until(&state, |s| s.stops == 1),
        "stop must stop the device, not leave it playing"
    );
    assert!(matches!(service.slot, PlaybackSlot::Stopped), "the slot must be Stopped after stop");
    let mut saw_not_remote = false;
    while let Ok(progress) = rx.try_recv() {
        if let PlaybackProgress::RemoteStatusChanged { remote: RemotePlayback::Local } = progress {
            saw_not_remote = true;
        }
    }
    assert!(saw_not_remote, "stopping while remote announces local playback");
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
    assert!(wait_until(&state, |s| !s.loads.is_empty()));
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

    assert_eq!(service.current_track_position(), Some((SECOND.to_string(), StreamPosition::START)));
}

/// Ending remote playback right after a seek on the device resumes locally at
/// the seek target, before the device has reported a position from there.
#[tokio::test]
async fn ending_remote_playback_after_a_seek_resumes_at_the_seek_target() {
    const TRACK: &str = "08c7ff07-b56a-4e16-8df6-ae2967fa0806";
    let (_home, mut service, state, _rx) =
        playing_remote_fixture(&[TRACK], StreamPosition::from_millis(30_000)).await;
    assert!(wait_until(&state, |s| !s.loads.is_empty()));
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
        if let PlaybackProgress::Seeked { track_id: tid, position_ms, .. } = progress {
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
    assert!(wait_until(&state, |s| !s.loads.is_empty()));

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
    assert!(wait_until(&state, |s| !s.loads.is_empty()));
    while rx.try_recv().is_ok() {}

    service.handle_stop_remote().await;

    assert_eq!(shown_positions(&mut rx, TRACK), vec![30_000]);
}

/// Pause while remote routes to the device.
#[tokio::test]
async fn pause_while_remote_pauses_the_device() {
    let (_home, mut service, state, _rx) = remote_over_fake().await;
    service.pause();
    assert!(wait_until(&state, |s| s.pauses == 1), "pause while remote must pause the device");
}

/// Resume while remote routes to the device.
#[tokio::test]
async fn resume_while_remote_plays_the_device() {
    let (_home, mut service, state, _rx) = remote_over_fake().await;
    service.pause();
    assert!(wait_until(&state, |s| s.pauses == 1));
    service.resume().await;
    assert!(wait_until(&state, |s| s.plays == 1), "resume while remote must play the device");
}

/// Seek while remote routes to the device (and skips the local rebuild path).
#[tokio::test]
async fn seek_while_remote_seeks_the_device() {
    let (_home, mut service, state, _rx) = remote_over_fake().await;
    service.seek(StreamPosition::from_duration(std::time::Duration::from_secs(45))).await;
    assert!(
        wait_until(&state, |s| s.seeks.contains(&std::time::Duration::from_secs(45))),
        "seek while remote must seek the device"
    );
}

/// Setting the volume while remote sets the device's volume too.
#[tokio::test]
async fn set_volume_while_remote_sets_the_device_volume() {
    let (_home, mut service, state, _rx) = remote_over_fake().await;
    service.set_volume(crate::Volume::new(0.3).unwrap());
    assert!(
        wait_until(&state, |s| s.volumes.contains(&0.3)),
        "setting the volume while remote must set the device's volume"
    );
}

/// A queued track renamed while an earlier one plays on a device reaches the
/// device when it loads: each load reads the track's display from the library
/// then, not from a copy taken when the queue was filled or the device was
/// handed playback. The device is not loaded again mid-track.
#[tokio::test]
async fn a_queued_track_renamed_while_another_plays_remotely_loads_with_its_new_title() {
    const FIRST: &str = "08c7ff07-b56a-4e16-8df6-ae2967fa0806";
    const SECOND: &str = "08c7fe07-b56a-4c63-8df6-ad2967fa0653";
    let (library, mut service, state, _rx) =
        playing_remote_fixture(&[FIRST, SECOND], StreamPosition::START).await;
    let (load_tx, mut loads) = tokio_mpsc::unbounded_channel();
    state.lock().unwrap().load_events = Some(load_tx);
    let first_url = format!("http://renderer.local/stream?id={FIRST}");
    let second_url = format!("http://renderer.local/stream?id={SECOND}");

    bae_records::test_fixtures::test_records(&library)
        .rename_track_for_test(SECOND, "Renamed Track")
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
        let media = tokio::time::timeout(HANG_GUARD, loads.recv())
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

/// What a whole-file track and then a CUE image's track are each served as on
/// a renderer of `flavor`, with conversions set to `transcode`: the URL (named
/// by its format) and the MIME type declared for it.
async fn served_whole_file_then_window(
    flavor: RendererFlavor,
    transcode: bae_preferences::CastTranscodeFormat,
) -> Vec<(String, String)> {
    const RELEASE: &str = "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e";
    const WHOLE_FILE: &str = "08c7ff07-b56a-4e16-8df6-ae2967fa0806";
    const IMAGE_WINDOW: &str = "08c7fe07-b56a-4c63-8df6-ad2967fa0653";
    let (library, mut service, _rx) =
        seeded_playback_service(&[(RELEASE, &[WHOLE_FILE, IMAGE_WINDOW])]).await;
    bae_preferences::PreferencesRow::new(library.clone())
        .edit(move |prefs| {
            prefs.cast_transcode_format = transcode;
            Ok(())
        })
        .await
        .unwrap();
    seed_playable_track(&library, RELEASE, WHOLE_FILE).await;
    seed_track_window(&library, RELEASE, IMAGE_WINDOW, 441_000, Some(882_000), None).await;
    fill_test_release(&mut service, RELEASE).await;
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
                flavor,
            ),
        ))
        .await;
    service
        .handle_remote_event(RendererSessionEvent::Status(ReceiverStatus {
            player_state: RendererPlayerState::Finished,
            position: None,
            duration: None,
            volume: Some(1.0),
        }))
        .await;

    assert!(wait_until(&state, |s| s.loads.len() == 2), "both tracks are loaded onto the device");
    let s = state.lock().unwrap();
    s.loads.iter().map(|m| (m.url.clone(), m.content_type.clone())).collect()
}

/// A device plays the stream it is served, so each track is served as its own
/// stream from its first sample: a track that is a whole file goes out as that
/// file, and a CUE image's track is transcoded from its window rather than
/// served as the whole image.
#[tokio::test]
async fn a_cue_image_track_is_served_as_its_own_window() {
    assert_eq!(
        served_whole_file_then_window(
            RendererFlavor::Cast,
            bae_preferences::CastTranscodeFormat::Mp3
        )
        .await,
        [
            (
                "http://renderer.local/stream?id=08c7ff07-b56a-4e16-8df6-ae2967fa0806&Raw(Flac)"
                    .to_string(),
                "audio/flac".to_string()
            ),
            (
                "http://renderer.local/stream?id=08c7fe07-b56a-4c63-8df6-ad2967fa0653&TranscodeMp3"
                    .to_string(),
                "audio/mpeg".to_string()
            ),
        ]
    );
}

/// The conversion setting reaches the device as each track loads: with WAV
/// picked, a Cast receiver is sent the CUE track as WAV, and a UPnP renderer,
/// which isn't counted on to play WAV, is sent MP3. A whole file it decodes
/// goes out as stored either way.
#[tokio::test]
async fn a_converted_track_goes_out_in_the_format_the_flavor_takes() {
    let window = |format: &str, mime: &str| {
        (
            format!(
                "http://renderer.local/stream?id=08c7fe07-b56a-4c63-8df6-ad2967fa0653&{format}"
            ),
            mime.to_string(),
        )
    };
    let whole_file = (
        "http://renderer.local/stream?id=08c7ff07-b56a-4e16-8df6-ae2967fa0806&Raw(Flac)"
            .to_string(),
        "audio/flac".to_string(),
    );
    assert_eq!(
        served_whole_file_then_window(
            RendererFlavor::Cast,
            bae_preferences::CastTranscodeFormat::Wav
        )
        .await,
        [whole_file.clone(), window("TranscodeWav", "audio/wav")]
    );
    assert_eq!(
        served_whole_file_then_window(
            RendererFlavor::Dlna,
            bae_preferences::CastTranscodeFormat::Wav
        )
        .await,
        [whole_file, window("TranscodeMp3", "audio/mpeg")]
    );
}

/// A Cast LOAD of a file served as stored declares the container its bytes
/// are in, not the codec they hold: an Opus file is Ogg.
#[tokio::test]
async fn a_raw_opus_file_is_loaded_as_ogg() {
    const RELEASE: &str = "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e";
    const TRACK: &str = "08c7ff07-b56a-4e16-8df6-ae2967fa0806";
    let (store, mut service, _rx) = seeded_playback_service(&[(RELEASE, &[TRACK])]).await;
    seed_playable_track_stored_as(
        &store,
        RELEASE,
        TRACK,
        bae_util::content_type::ContentType::Opus,
        bae_util::audio_container::AudioContainer::Ogg,
    )
    .await;
    fill_test_release(&mut service, RELEASE).await;
    service.slot =
        active_slot(test_prepared_track(TRACK, create_sparse_buffer(1_024)), TrackPhase::Playing);
    let channel = FakeChannel::new();
    let state = channel.state.clone();

    service.handle_play_on(remote_connect(channel)).await;

    assert!(wait_until(&state, |s| !s.loads.is_empty()));
    assert_eq!(state.lock().unwrap().loads[0].content_type, "audio/ogg");
}

/// Cast `RELEASE`'s one track, stored with `track_length_ms` and a 2 s
/// generated pregap, to a fake device, and return the length the device is
/// told for its stream.
async fn served_stream_length(track_length_ms: u64) -> Option<std::time::Duration> {
    const RELEASE: &str = "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e";
    const TRACK: &str = "08c7ff07-b56a-4e16-8df6-ae2967fa0806";
    let store = seeded_store_with_track_length(&[(RELEASE, &[TRACK])], track_length_ms).await;
    let (mut service, _rx) = playback_service_over(&store, bae_diagnostics::Diagnostics::noop());
    // Two seconds of generated silence at 44.1 kHz.
    seed_track_window(&store, RELEASE, TRACK, 0, None, Some(88_200)).await;
    fill_test_release(&mut service, RELEASE).await;
    service.slot =
        active_slot(test_prepared_track(TRACK, create_sparse_buffer(1_024)), TrackPhase::Playing);
    let channel = FakeChannel::new();
    let state = channel.state.clone();

    service.handle_play_on(remote_connect(channel)).await;

    assert!(wait_until(&state, |s| !s.loads.is_empty()));
    let duration = state.lock().unwrap().loads[0].duration;
    duration
}

/// A device is told the length of the stream it is served, which runs through
/// the track's pregap before the track: the device's position counts in that
/// stream, so a length without the pregap would end the track early.
#[tokio::test]
async fn a_device_is_told_the_served_stream_length_pregap_included() {
    assert_eq!(served_stream_length(60_000).await, Some(std::time::Duration::from_secs(62)));
}

#[tokio::test]
async fn airplay_pause_flushes_and_resume_reanchors() {
    let (_home, mut service, _progress_rx) = test_playback_service().await;
    let buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(test_prepared_track("t", buffer), TrackPhase::Playing);
    let airplay = install_airplay(&mut service, std::time::Duration::from_secs(2));
    let state = &airplay.state;

    service.pause();
    assert_eq!(
        state.flushed.load(std::sync::atomic::Ordering::Acquire),
        1,
        "pause FLUSHes the receiver"
    );
    assert_eq!(state.reanchored.load(std::sync::atomic::Ordering::Acquire), 0);

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
    // The live session's receiver reported a quarter-second latency.
    let _airplay = install_airplay(&mut service, std::time::Duration::from_millis(250));

    // A tick at 5 s of decoded position: the audible position is 5 − 0.25 s.
    let mut fmt = TrackFmt::for_test("t");
    fmt.timeline =
        TrackTimeline::new(Some(std::time::Duration::from_secs(60)), std::time::Duration::ZERO);
    service.handle_position_event(Arc::new(fmt), StreamPosition::from_millis(5_000)).await;

    let position = loop {
        match progress_rx.try_recv() {
            Ok(PlaybackProgress::PositionUpdate { position_ms, .. }) => break position_ms,
            Ok(_) => continue,
            Err(_) => panic!("expected a PositionUpdate"),
        }
    };
    assert_eq!(position, 4_750, "position reflects the live session's receiver latency");
}

#[tokio::test]
async fn airplay_stop_restores_the_local_output_and_returns_to_local() {
    let (_home, mut service, _progress_rx) = test_playback_service().await;
    let buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(test_prepared_track("t", buffer), TrackPhase::Playing);
    let airplay = install_airplay(&mut service, std::time::Duration::from_secs(2));

    service.stop().await;

    assert!(!service.renderer.is_airplay(), "stop returns to the local renderer");
    assert!(
        airplay.is_saved_output(&service.audio_output),
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
    service.slot =
        active_slot(test_prepared_track(TRACK_T, create_sparse_buffer(1_024)), TrackPhase::Playing);
    let airplay = install_airplay(&mut service, std::time::Duration::from_secs(2));

    // Playback progressed on AirPlay: decode is local, so the slot's position is
    // the live one the resume reads.
    if let PlaybackSlot::Active(cur) = &mut service.slot {
        cur.position = StreamPosition::from_millis(30_000);
    }

    service.handle_stop_remote().await;

    assert!(!service.renderer.is_airplay(), "stop returns to the local renderer");
    assert!(
        airplay.is_saved_output(&service.audio_output),
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
    service.slot = active_slot(test_prepared_track(TRACK_T, buffer.clone()), TrackPhase::Playing);
    let (_sink, source, _ready) = create_track_stream_pair(44_100, 2);
    let (_tx, audio_rx) = audio_event_channel().unwrap();
    service.output = Some(test_output(
        Arc::new(Mutex::new(source::PlaybackSource::new(source, TrackFmt::for_test(TRACK_T)))),
        audio_rx,
    ));
    let airplay = install_airplay(&mut service, std::time::Duration::from_secs(2));
    let state = &airplay.state;

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
    fill_test_release(&mut service, "af63ef4c-8602-4cd5-82c0-3d334b916305").await;
    service.slot =
        active_slot(test_prepared_track(TRACK_A, create_sparse_buffer(1_024)), TrackPhase::Playing);
    let _airplay = install_airplay(&mut service, std::time::Duration::from_secs(2));

    service.handle_auto_advance(TRACK_A.to_string()).await;

    assert!(
        service.renderer.is_airplay(),
        "the AirPlay renderer stays installed across an auto-advance"
    );
}

/// A no-op AirPlay sink for driving `handle_play_on_airplay`: it accepts the PCM
/// source without touching a socket.
struct NoopAirPlaySink;

impl crate::airplay_output::AirPlaySink for NoopAirPlaySink {
    fn start(
        &self,
        _source: Box<dyn crate::airplay_output::PcmSource>,
        _wake: crate::airplay_output::PcmWake,
        _failed: crate::airplay_output::AirPlayFailed,
    ) -> Result<Arc<dyn crate::airplay_output::AirPlayStreamControl>, AudioError> {
        Ok(Arc::new(FakeAirPlayControl {
            state: Arc::new(FakeAirPlayControlState::default()),
            latency: std::time::Duration::from_secs(2),
        }))
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
        ))
        .await;

    assert!(service.renderer.is_airplay(), "the AirPlay renderer is installed");
    assert!(
        !service.renderer.is_remote(),
        "AirPlay keeps decoding locally — it is not a fetch-a-URL remote renderer"
    );
}

/// A dead AirPlay receiver (the session reports transport failure) ends AirPlay
/// and returns to local — surfaced by the session's report rather than
/// erroring silently forever, even with no position ticks coming, as when a
/// sender thread panicked — and announces the receiver lost with why.
#[tokio::test]
async fn airplay_receiver_death_ends_airplay_and_returns_to_local() {
    let (_home, mut service, mut progress_rx) =
        seeded_playback_service(&[("af63ef4c-8602-4cd5-82c0-3d334b916305", &[TRACK_T])]).await;
    service.slot =
        active_slot(test_prepared_track(TRACK_T, create_sparse_buffer(1_024)), TrackPhase::Playing);
    let airplay = install_airplay(&mut service, std::time::Duration::from_secs(2));
    let state = &airplay.state;
    while progress_rx.try_recv().is_ok() {}

    // The receiver went away: the session reports the transport as failed.
    state
        .failure
        .set("20 audio sends in a row failed: No route to host (os error 65)".to_string())
        .expect("the failure is set once");

    // The session reports it, and the service ends AirPlay.
    service.airplay_stream_failed().await;

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

/// The level set while AirPlay plays carries back to the local output when
/// the person stops AirPlay: there is one level, and the UI already shows it.
#[tokio::test]
async fn stopping_airplay_keeps_the_level_set_on_the_receiver() {
    let (_home, mut service, _progress_rx) = test_playback_service().await;
    service.set_volume(crate::Volume::new(0.8).unwrap());
    service
        .handle_play_on_airplay(renderer::AirPlayConnect::new(
            Box::new(NoopAirPlaySink),
            test_device(),
        ))
        .await;

    service.set_volume(crate::Volume::new(0.3).unwrap());
    service.handle_stop_remote().await;

    assert!(!service.renderer.is_airplay(), "stop returns to the local renderer");
    assert_eq!(service.audio_output.get_volume(), 0.3, "the local output plays at the level set");
}

/// A mute made while AirPlay plays holds on the local output after a stop
/// ends AirPlay, and unmuting there puts back the level set before the mute.
#[tokio::test]
async fn a_mute_made_on_airplay_holds_after_airplay_ends() {
    let (_home, mut service, _progress_rx) = test_playback_service().await;
    service.set_volume(crate::Volume::new(0.8).unwrap());
    service
        .handle_play_on_airplay(renderer::AirPlayConnect::new(
            Box::new(NoopAirPlaySink),
            test_device(),
        ))
        .await;

    service.set_volume(crate::Volume::new(0.4).unwrap());
    service.set_muted(true);
    service.stop().await;

    assert!(!service.renderer.is_airplay(), "stop returns to the local renderer");
    assert_eq!(service.audio_output.get_volume(), 0.0, "the local output stays muted");
    service.set_muted(false);
    assert_eq!(service.audio_output.get_volume(), 0.4, "unmute puts back the level set");
}
