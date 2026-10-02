// -- what a remote renderer is served ----------------------------------------
//
// Included after `remote_output.rs`, whose fakes and fixtures these use.

/// What a whole-file track and then a CUE image's track are each served as on
/// a renderer of `flavor`, with conversions set to `transcode`: the URL (named
/// by its format) and the MIME type declared for it.
async fn served_whole_file_then_window(
    flavor: RendererFlavor,
    transcode: crate::config::CastTranscodeFormat,
) -> Vec<(String, String)> {
    const RELEASE: &str = "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e";
    const WHOLE_FILE: &str = "08c7ff07-b56a-4e16-8df6-ae2967fa0806";
    const IMAGE_WINDOW: &str = "08c7fe07-b56a-4c63-8df6-ad2967fa0653";
    let (_home, mut service, _rx) =
        seeded_playback_service(&[(RELEASE, &[WHOLE_FILE, IMAGE_WINDOW])]).await;
    service
        .library_manager
        .set_cast_transcode_format(transcode)
        .await
        .unwrap();
    seed_playable_track(&service.library_manager, RELEASE, WHOLE_FILE).await;
    seed_track_window(
        &service.library_manager,
        RELEASE,
        IMAGE_WINDOW,
        441_000,
        Some(882_000),
        None,
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
                flavor,
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
    s.loads
        .iter()
        .map(|m| (m.url.clone(), m.content_type.clone()))
        .collect()
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
            crate::config::CastTranscodeFormat::Mp3
        )
        .await,
        [
            (
                "http://renderer.local/stream?id=08c7ff07-b56a-4e16-8df6-ae2967fa0806&Raw".to_string(),
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
            format!("http://renderer.local/stream?id=08c7fe07-b56a-4c63-8df6-ad2967fa0653&{format}"),
            mime.to_string(),
        )
    };
    let whole_file = (
        "http://renderer.local/stream?id=08c7ff07-b56a-4e16-8df6-ae2967fa0806&Raw".to_string(),
        "audio/flac".to_string(),
    );
    assert_eq!(
        served_whole_file_then_window(
            RendererFlavor::Cast,
            crate::config::CastTranscodeFormat::Wav
        )
        .await,
        [whole_file.clone(), window("TranscodeWav", "audio/wav")]
    );
    assert_eq!(
        served_whole_file_then_window(
            RendererFlavor::Dlna,
            crate::config::CastTranscodeFormat::Wav
        )
        .await,
        [whole_file, window("TranscodeMp3", "audio/mpeg")]
    );
}

/// A device is told the length of the stream it is served, which runs through
/// the track's pregap before the track: the device's position counts in that
/// stream, so a length without the pregap would end the track early.
#[tokio::test]
async fn a_device_is_told_the_served_stream_length_pregap_included() {
    const RELEASE: &str = "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e";
    const TRACK: &str = "08c7ff07-b56a-4e16-8df6-ae2967fa0806";
    let (_home, mut service, _rx) = seeded_playback_service(&[(RELEASE, &[TRACK])]).await;
    seed_track_window(&service.library_manager, RELEASE, TRACK, 0, None, Some(2_000)).await;
    service.playback_queue.apply(|queue| {
        queue.play_release(
            ContextSource::Release(RELEASE.to_string()),
            vec![TRACK.to_string()],
            ContextStart::Index(0),
        )
    });
    service.slot = active_slot(
        test_prepared_track(TRACK, create_sparse_buffer(1_024)),
        TrackPhase::Playing,
    );
    let channel = FakeChannel::new();
    let state = channel.state.clone();

    service.handle_play_on(remote_connect(channel)).await;

    assert!(wait_until(|| !state.lock().unwrap().loads.is_empty()));
    let resolved = service
        .library_manager
        .resolve_track_audio(TRACK)
        .await
        .unwrap();
    let track_duration = finalize_playback_track(
        TRACK.to_string(),
        &resolved,
        Vec::new(),
        crate::config::ReplayGainMode::Off,
    )
    .timeline
    .duration();
    assert_eq!(
        state.lock().unwrap().loads[0].duration,
        Some(std::time::Duration::from_secs(2) + track_duration)
    );
}
