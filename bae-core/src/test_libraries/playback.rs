//! Playback over a library a test opened, running on a capture sink.

use bae_playback::test_capture::{
    capture_device, wall_playback_clock, CaptureStreamRx, TestAudioDevice,
};

/// Start a playback service over `library_manager` and its store, `database`,
/// on the calling test's runtime, backed by a capture sink standing in for the
/// audio device, and return its handle beside the sink's captured streams.
///
/// A tuple rather than a fixture struct: a struct retaining a playback handle
/// is an owner, and `scripts/owner-dependency-boundary.sh` refuses to let a
/// public one expose its fields. Each test binary keeps its own (private)
/// fixture around what this returns.
pub async fn start_capture_playback(
    library_manager: &crate::library::LibraryManager,
    database: &bae_db::Database,
    device: TestAudioDevice,
) -> (bae_playback::PlaybackHandle, CaptureStreamRx) {
    start_capture_playback_with_clock(library_manager, database, device, wall_playback_clock())
        .await
}

/// [`start_capture_playback`] on a playback clock the test supplies, so the
/// test decides when a side-pause countdown runs out.
pub async fn start_capture_playback_with_clock(
    library_manager: &crate::library::LibraryManager,
    database: &bae_db::Database,
    device: TestAudioDevice,
    clock: bae_playback::PlaybackClockRef,
) -> (bae_playback::PlaybackHandle, CaptureStreamRx) {
    let (capture_device, capture_stream_rx) = capture_device(device);
    let handle = start_playback_on(library_manager, database, capture_device, clock).await;
    (handle, capture_stream_rx)
}

/// Start a playback service over `library_manager` and its store, `database`,
/// on the calling test's runtime, playing on `device` and timing side-pause
/// countdowns on `clock`. It resumes the stored queue and reports its position
/// every 100 ms.
pub async fn start_playback_on(
    library_manager: &crate::library::LibraryManager,
    database: &bae_db::Database,
    device: Box<dyn bae_playback::AudioOutputDevice>,
    clock: bae_playback::PlaybackClockRef,
) -> bae_playback::PlaybackHandle {
    library_manager.settle_playback_queue(true).await.expect("settle the playback queue");
    bae_playback::PlaybackService::start_with_audio_device(
        database.clone(),
        std::sync::Arc::new(coven::UuidIds),
        bae_playback::tables::PlaybackQueueTables::new(database.clone()),
        bae_playback::tables::PlaybackNowRow::new(database.clone()),
        bae_playback::tables::PlaybackTimelineRow::new(database.clone()),
        library_manager.subscribe_track_deletions(),
        bae_diagnostics::Diagnostics::noop(),
        tokio::runtime::Handle::current(),
        100,
        true,
        device,
        clock,
    )
}
