//! Waiting on a progress stream, and playback running over a capture sink.

/// Read events off `rx` until `matches` returns a value, discarding the ones it
/// declines, and give up after `timeout_duration` (or when the channel closes)
/// with `None`.
///
/// The drain loop every playback and import test wants. `matches` takes each
/// event by value, so it can move data out of the one it accepts, and it is
/// `FnMut`, so it can also accumulate across the events it declines — which is
/// what a "watch the whole crossing, then report what was seen" wait needs.
pub async fn next_matching<E, T>(
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<E>,
    timeout_duration: std::time::Duration,
    mut matches: impl FnMut(E) -> Option<T>,
) -> Option<T> {
    let deadline = std::time::Instant::now() + timeout_duration;
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return None;
        }
        match tokio::time::timeout(remaining, rx.recv()).await {
            Ok(Some(event)) => {
                if let Some(found) = matches(event) {
                    return Some(found);
                }
            }
            // The channel closed: no later event can arrive, so waiting out the
            // deadline would only spin.
            Ok(None) => return None,
            Err(_) => return None,
        }
    }
}

/// Wait for playback to report `track_id` as the playing track, discarding
/// every other progress event, and return whether it arrived in time.
pub async fn wait_until_playing(
    progress_rx: &mut tokio::sync::mpsc::UnboundedReceiver<bae_core::playback::PlaybackProgress>,
    track_id: &str,
    timeout_duration: std::time::Duration,
) -> bool {
    use bae_core::playback::{PlaybackProgress, PlaybackState};
    next_matching(progress_rx, timeout_duration, |event| match event {
        PlaybackProgress::StateChanged {
            state: PlaybackState::Playing { track_info, .. },
        } if track_info.track_id == track_id => Some(()),
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
    let seeked = next_matching(
        progress_rx,
        std::time::Duration::from_secs(10),
        |event| match event {
            PlaybackProgress::Seeked {
                track_id: seeked_id,
                ..
            } if seeked_id == track_id => Some(()),
            _ => None,
        },
    )
    .await;
    assert!(seeked.is_some(), "Should receive Seeked event");
}

/// The capture sink's stream buffers, minted one per `create_stream`. Holding
/// the receiver is what lets stream creation succeed, so a fixture keeps it for
/// the playback service's lifetime even when it never inspects the samples.
pub type CaptureStreamRx =
    tokio::sync::mpsc::UnboundedReceiver<std::sync::Arc<std::sync::Mutex<Vec<f32>>>>;

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

/// Start a playback service over `library_manager` on the calling test's
/// runtime, backed by a capture sink standing in for the audio device, and
/// return its handle beside the sink's capture buffers.
///
/// A tuple rather than a fixture struct: a struct retaining a playback handle
/// is an owner, and `scripts/owner-dependency-boundary.sh` refuses to let a
/// public one expose its fields. Each test binary keeps its own (private)
/// fixture around what this returns.
pub fn start_capture_playback(
    library_manager: &bae_core::library::LibraryManager,
    device: TestAudioDevice,
) -> (bae_core::playback::PlaybackHandle, CaptureStreamRx) {
    let (capture_device, capture_stream_rx): (
        Box<dyn bae_core::playback::AudioOutputDevice>,
        CaptureStreamRx,
    ) = match device {
        TestAudioDevice::Capture => {
            let (device, rx) = bae_core::playback::CaptureAudioDevice::new();
            (Box::new(device), rx)
        }
        TestAudioDevice::RealtimeCapture => {
            let (device, rx) = bae_core::playback::RealtimeCaptureAudioDevice::new();
            (Box::new(device), rx)
        }
    };
    let handle = library_manager.start_playback_service_with_audio_device(
        tokio::runtime::Handle::current(),
        100,
        true,
        capture_device,
    );
    (handle, capture_stream_rx)
}

/// Awaits the next capture buffer minted by `create_stream`. Buffers are
/// yielded in creation order; tests that exercise auto-advance, seek, or next
/// call this once per stream they want to inspect.
pub async fn next_capture_stream(
    capture_stream_rx: &mut CaptureStreamRx,
) -> std::sync::Arc<std::sync::Mutex<Vec<f32>>> {
    match tokio::time::timeout(std::time::Duration::from_secs(5), capture_stream_rx.recv()).await {
        Ok(Some(buffer)) => buffer,
        Ok(None) => panic!("capture stream channel closed before a stream was created"),
        Err(_) => panic!("no capture stream created within 5s"),
    }
}
