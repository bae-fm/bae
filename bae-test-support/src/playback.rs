//! Waiting on a progress stream, and playback running over a capture sink.

/// Wait for playback to report `track_id` as the playing track, discarding
/// every other progress event, and return whether it arrived in time.
pub async fn wait_until_playing(
    progress_rx: &mut tokio::sync::mpsc::UnboundedReceiver<bae_core::playback::PlaybackProgress>,
    track_id: &str,
    timeout_duration: std::time::Duration,
) -> bool {
    use bae_core::playback::{PlaybackProgress, PlaybackState};
    bae_util::test_channels::next_matching(progress_rx, timeout_duration, |event| match event {
        PlaybackProgress::StateChanged { state: PlaybackState::Playing { track } }
            if track.track_id == track_id =>
        {
            Some(())
        }
        _ => None,
    })
    .await
    .is_some()
}

/// Wait up to ten seconds for playback to confirm a seek on `track_id`,
/// discarding every other progress event, and assert the confirmation arrived.
pub async fn wait_for_seek(
    progress_rx: &mut tokio::sync::mpsc::UnboundedReceiver<bae_core::playback::PlaybackProgress>,
    track_id: &str,
) {
    use bae_core::playback::PlaybackProgress;
    let seeked = bae_util::test_channels::next_matching(
        progress_rx,
        std::time::Duration::from_secs(10),
        |event| match event {
            PlaybackProgress::Seeked { track_id: seeked_id, .. } if seeked_id == track_id => {
                Some(())
            }
            _ => None,
        },
    )
    .await;
    assert!(seeked.is_some(), "Should receive Seeked event");
}

/// The capture sink's streams, one per non-gapless transition. Holding
/// the receiver is what lets stream creation succeed, so a fixture keeps it for
/// the playback service's lifetime even when it never inspects the samples.
pub type CaptureStreamRx = bae_core::playback::CapturedStreams;

/// Which stand-in audio device a test's playback service runs on.
pub enum TestAudioDevice {
    /// Full speed: the drain pulls as fast as the decoder fills. Fast, but a
    /// track can fully decode and gaplessly advance before a follow-up command
    /// lands.
    Capture,
    /// Wall-clock paced, like a real device: the drain sleeps each buffer's own
    /// duration, so the decoder fills the ring and parks instead of racing whole
    /// tracks ahead. Required whenever a test plays and then issues a command
    /// (seek, pause) that has to land on the track under test.
    RealtimeCapture,
}

/// Start a playback service over `library_manager` and its store, `database`,
/// on the calling test's runtime, backed by a capture sink standing in for the
/// audio device, and return its handle beside the sink's captured streams.
///
/// A tuple rather than a fixture struct: a struct retaining a playback handle
/// is an owner, and `scripts/owner-dependency-boundary.sh` refuses to let a
/// public one expose its fields. Each test binary keeps its own (private)
/// fixture around what this returns.
pub async fn start_capture_playback(
    library_manager: &bae_core::library::LibraryManager,
    database: &bae_db::Database,
    device: TestAudioDevice,
) -> (bae_core::playback::PlaybackHandle, CaptureStreamRx) {
    start_capture_playback_with_clock(library_manager, database, device, wall_playback_clock())
        .await
}

/// [`start_capture_playback`] on a playback clock the test supplies, so the
/// test decides when a side-pause countdown runs out.
pub async fn start_capture_playback_with_clock(
    library_manager: &bae_core::library::LibraryManager,
    database: &bae_db::Database,
    device: TestAudioDevice,
    clock: bae_core::playback::PlaybackClockRef,
) -> (bae_core::playback::PlaybackHandle, CaptureStreamRx) {
    let (capture_device, capture_stream_rx) = capture_device(device);
    let handle = start_playback_on(library_manager, database, capture_device, clock).await;
    (handle, capture_stream_rx)
}

/// Start a playback service over `library_manager` and its store, `database`,
/// on the calling test's runtime, playing on `device` and timing side-pause
/// countdowns on `clock`. It resumes the stored queue and reports its position
/// every 100 ms.
pub async fn start_playback_on(
    library_manager: &bae_core::library::LibraryManager,
    database: &bae_db::Database,
    device: Box<dyn bae_core::playback::AudioOutputDevice>,
    clock: bae_core::playback::PlaybackClockRef,
) -> bae_core::playback::PlaybackHandle {
    library_manager.settle_playback_queue(true).await.expect("settle the playback queue");
    bae_core::playback::PlaybackService::start_with_audio_device(
        database.clone(),
        std::sync::Arc::new(coven::UuidIds),
        bae_core::playback::tables::PlaybackQueueTables::new(database.clone()),
        bae_core::playback::tables::PlaybackNowRow::new(database.clone()),
        bae_core::playback::tables::PlaybackTimelineRow::new(database.clone()),
        library_manager.subscribe_track_deletions(),
        bae_diagnostics::Diagnostics::noop(),
        tokio::runtime::Handle::current(),
        100,
        true,
        device,
        clock,
    )
}

/// The playback time source over the wall clock, as the app's.
pub fn wall_playback_clock() -> bae_core::playback::PlaybackClockRef {
    std::sync::Arc::new(bae_core::playback::WallPlaybackClock::new(std::sync::Arc::new(
        coven::SystemClock,
    )))
}

fn capture_device(
    device: TestAudioDevice,
) -> (Box<dyn bae_core::playback::AudioOutputDevice>, CaptureStreamRx) {
    match device {
        TestAudioDevice::Capture => {
            let (device, rx) = bae_core::playback::CaptureAudioDevice::new();
            (Box::new(device), rx)
        }
        TestAudioDevice::RealtimeCapture => {
            let (device, rx) = bae_core::playback::RealtimeCaptureAudioDevice::new();
            (Box::new(device), rx)
        }
    }
}

/// Awaits the next captured stream a transition started. Streams are
/// yielded in creation order; tests that exercise auto-advance, seek, or next
/// call this once per stream they want to inspect.
pub async fn next_capture_stream(
    capture_stream_rx: &mut CaptureStreamRx,
) -> bae_core::playback::CapturedStream {
    match tokio::time::timeout(std::time::Duration::from_secs(5), capture_stream_rx.recv()).await {
        Ok(Some(stream)) => stream,
        Ok(None) => panic!("capture stream channel closed before a stream was created"),
        Err(_) => panic!("no capture stream created within 5s"),
    }
}
