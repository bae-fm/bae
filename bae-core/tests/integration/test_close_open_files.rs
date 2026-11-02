//! Closing a library leaves none of its files open, whatever its services
//! were doing. Windows refuses to delete a file a process holds open without
//! sharing deletion: std's handles share it, FFmpeg's don't. So a handle left
//! behind can fail removing the library there, and elsewhere passes unseen;
//! listing this process's open files catches it on every platform.

use bae_test_support as support;
use std::time::Duration;

/// Copy the three fixture tracks into `dir`.
fn write_tracks(dir: &std::path::Path) {
    let fixtures = support::fixture_dir!("flac");
    for name in ["01 Test Track 1.flac", "02 Test Track 2.flac", "03 Test Track 3.flac"] {
        std::fs::copy(fixtures.join(name), dir.join(name)).expect("copy a fixture track");
    }
}

/// A library playing a track when it closes holds none of its files once the
/// close returns: not the track playback was reading, not its store.
#[tokio::test(flavor = "multi_thread")]
async fn closing_a_playing_library_leaves_none_of_its_files_open() {
    let release = support::discogs_test_release(
        "close-files-release",
        "Close Test Album",
        &[("Test Track 1", "0:10"), ("Test Track 2", "0:10"), ("Test Track 3", "0:10")],
    );
    let ((manager, database, catalogs, closing), imported) = support::imported_release_setup(
        release,
        "test",
        uuid::Uuid::new_v4().to_string(),
        write_tracks,
    )
    .await
    .expect("import the release");
    let (device, _capture) = bae_core::playback::RealtimeCaptureAudioDevice::new();
    let clock: coven::ClockRef = std::sync::Arc::new(coven::SystemClock);
    let services = bae_core::app::start_library_services(
        manager,
        database,
        catalogs.clone(),
        closing,
        clock.clone(),
        std::sync::Arc::new(coven::UuidIds),
        bae_core::diagnostics::Diagnostics::noop(),
        tokio::runtime::Handle::current(),
        bae_catalogs::remote_image::RemoteImageCache::for_test(bae_util::http::Http::for_test()),
        100,
        false,
        std::sync::Arc::new(bae_core::playback::WallPlaybackClock::new(clock)),
        bae_core::app::PlaybackOutput::Device(Box::new(device)),
    )
    .await
    .expect("start the library's services");
    let mut progress = services.subscribe_playback_progress();

    services.playback_play_release(imported.release_id.clone(), None, false);
    assert!(
        support::wait_until_playing(&mut progress, &imported.track_ids[0], Duration::from_secs(30))
            .await,
        "the first track plays"
    );
    assert!(
        !coven::open_files_under(imported.temp_dir.path()).is_empty(),
        "a playing library holds files open"
    );

    services.close().await.unwrap();

    coven::assert_no_open_files_under(imported.temp_dir.path());
}
