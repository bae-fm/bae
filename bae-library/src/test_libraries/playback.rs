//! Playback over a library a test opened, running on a capture device.

use bae_playback::test_capture::{
    capture_device, manual_capture_device, wall_playback_clock, CaptureStreamRx,
};
use bae_playback::AudioClock;

/// Start a playback service over `library`, on the calling test's runtime,
/// backed by a device that captures at full speed, resuming the stored queue,
/// and return its handle beside the device's captured streams. A track can
/// play out before a command sent after play lands, so it is for a test that
/// only reads what played.
pub async fn start_capture_playback(
    library: &super::TestLibrary,
) -> (bae_playback::PlaybackHandle, CaptureStreamRx) {
    let (device, capture_stream_rx) = capture_device();
    let handle = library.start_playback_on(device, wall_playback_clock(), true).await;
    (handle, capture_stream_rx)
}

/// Start a playback service over `library`, on the calling test's runtime,
/// backed by a capture device whose audio plays only as the returned clock
/// lets it, resuming the stored queue, beside the device's captured streams.
pub async fn start_manual_playback(
    library: &super::TestLibrary,
) -> (bae_playback::PlaybackHandle, CaptureStreamRx, AudioClock) {
    start_manual_playback_with_clock(library, wall_playback_clock()).await
}

/// [`start_manual_playback`] on a playback clock the test supplies, so the
/// test decides when a side-pause countdown runs out.
pub async fn start_manual_playback_with_clock(
    library: &super::TestLibrary,
    clock: bae_playback::PlaybackClockRef,
) -> (bae_playback::PlaybackHandle, CaptureStreamRx, AudioClock) {
    let (device, capture_stream_rx, audio_clock) = manual_capture_device();
    let handle = library.start_playback_on(device, clock, true).await;
    (handle, capture_stream_rx, audio_clock)
}
