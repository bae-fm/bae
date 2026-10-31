/// A track whose name isn't UTF-8 plays: playback opens the original by the
/// path the import recorded. Only Linux takes such a name.
#[cfg(target_os = "linux")]
#[tokio::test]
async fn a_track_whose_name_is_not_utf8_plays() {
    use std::os::unix::ffi::OsStrExt;

    tracing_init();
    let mut album = create_test_album();
    album.id = "non-utf8-name".to_string();
    album.tracklist.truncate(1);
    let (library_manager, imported) = imported_release_setup(
        album,
        "non-utf8-name",
        uuid::Uuid::new_v4().to_string(),
        |album_dir| {
            let fixture = bae_test_support::fixture_dir!("flac").join("01 Test Track 1.flac");
            std::fs::write(
                album_dir.join(std::ffi::OsStr::from_bytes(b"01 Caf\xE9.flac")),
                bae_test_support::read_fixture(&fixture),
            )
            .unwrap();
        },
    )
    .await
    .expect("the album imports");
    let [track_id] = imported.track_ids.as_slice() else {
        panic!("one track: {:?}", imported.track_ids);
    };
    let (playback_handle, _capture_stream_rx) = support::start_capture_playback(
        &library_manager,
        support::TestAudioDevice::RealtimeCapture,
    )
    .await;
    let mut progress_rx = playback_handle.subscribe_progress();

    playback_handle.play(track_id.clone());

    wait_for_state_on(
        &mut progress_rx,
        |state| {
            matches!(state, PlaybackState::Playing { track, .. } if track.track_id == *track_id)
        },
        Duration::from_secs(5),
    )
    .await
    .expect("the track plays");
    assert!(
        wait_for_position_advance(&mut progress_rx).await.is_some(),
        "its audio plays on"
    );
}
