//! Playback over a library a test opened, running on a capture sink.

use bae_playback::test_capture::{
    capture_device, wall_playback_clock, CaptureStreamRx, TestAudioDevice,
};

/// Start a playback service over `library`, on the calling test's runtime,
/// backed by a capture sink standing in for the audio device, resuming the
/// stored queue, and return its handle beside the sink's captured streams.
pub async fn start_capture_playback(
    library: &super::TestLibrary,
    device: TestAudioDevice,
) -> (bae_playback::PlaybackHandle, CaptureStreamRx) {
    start_capture_playback_with_clock(library, device, wall_playback_clock()).await
}

/// [`start_capture_playback`] on a playback clock the test supplies, so the
/// test decides when a side-pause countdown runs out.
pub async fn start_capture_playback_with_clock(
    library: &super::TestLibrary,
    device: TestAudioDevice,
    clock: bae_playback::PlaybackClockRef,
) -> (bae_playback::PlaybackHandle, CaptureStreamRx) {
    let (capture_device, capture_stream_rx) = capture_device(device);
    let handle = library.start_playback_on(capture_device, clock, true).await;
    (handle, capture_stream_rx)
}
