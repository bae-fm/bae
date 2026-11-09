//! Closing a library leaves none of its files open, whatever its services
//! were doing. Windows refuses to delete a file a process holds open without
//! sharing deletion: std's handles share it, FFmpeg's don't. So a handle left
//! behind can fail removing the library there, and elsewhere passes unseen;
//! listing this process's open files catches it on every platform.

/// Copy the three fixture tracks into `dir`.
fn write_tracks(dir: &std::path::Path) {
    let fixtures = bae_import::test_fixtures::flac_dir();
    for name in ["01 Test Track 1.flac", "02 Test Track 2.flac", "03 Test Track 3.flac"] {
        std::fs::copy(fixtures.join(name), dir.join(name)).expect("copy a fixture track");
    }
}

/// A library playing a track when it closes holds none of its files once the
/// close returns: not the track playback was reading, not its store.
#[tokio::test(flavor = "multi_thread")]
async fn closing_a_playing_library_leaves_none_of_its_files_open() {
    let release = bae_catalogs::test_releases::discogs_test_release(
        "close-files-release",
        "Close Test Album",
        &[("Test Track 1", "0:10"), ("Test Track 2", "0:10"), ("Test Track 3", "0:10")],
    );
    let (library, imported) = bae_library::test_libraries::imported_release_setup(
        release,
        "test",
        coven::IdSource::new_id(&coven::UuidIds).to_string(),
        write_tracks,
    )
    .await
    .expect("import the release");
    let (device, _capture, _audio_clock) = bae_playback::test_capture::manual_capture_device();
    let services = bae_app::test_services::start_services_on(&library, device)
        .await
        .expect("start the library's services");
    let mut progress = services.subscribe_playback_progress();

    let played = services
        .playback_play_release(imported.release_id.clone(), None, false)
        .await
        .expect("the play is answered");
    assert_eq!(played, bae_playback::PlayOutcome::Playing);
    bae_playback::test_capture::wait_until_playing(&mut progress, &imported.track_ids[0]).await;
    assert!(
        !coven::open_files_under(imported.temp_dir.path()).is_empty(),
        "a playing library holds files open"
    );

    services.close().await.unwrap();

    coven::assert_no_open_files_under(imported.temp_dir.path());
}
