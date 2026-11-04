//! Playback on a capture device standing in for the audio output, for the
//! tests of the crates above: which device a test plays on, the streams it
//! captures, and waiting on the progress a playback reports.

use crate::{
    AudioOutputDevice, CaptureAudioDevice, CapturedStream, CapturedStreams, PlaybackClockRef,
    PlaybackProgress, PlaybackState, RealtimeCaptureAudioDevice, WallPlaybackClock,
};
use bae_util::test_channels::{next_matching, HANG_GUARD};

/// Wait for playback to report `track_id` as the playing track, discarding
/// every other progress event. Panics only if it never does.
pub async fn wait_until_playing(progress_rx: &mut crate::PlaybackProgressFeed, track_id: &str) {
    let playing = next_matching(progress_rx, HANG_GUARD, |event| match event {
        PlaybackProgress::StateChanged { state: PlaybackState::Playing { track } }
            if track.track_id == track_id =>
        {
            Some(())
        }
        _ => None,
    })
    .await;
    assert!(playing.is_some(), "track {track_id} never reached Playing");
}

/// Wait for playback to confirm a seek on `track_id`, discarding every other
/// progress event. Panics only if it never does.
pub async fn wait_for_seek(progress_rx: &mut crate::PlaybackProgressFeed, track_id: &str) {
    let seeked = next_matching(progress_rx, HANG_GUARD, |event| match event {
        PlaybackProgress::Seeked { track_id: seeked_id, .. } if seeked_id == track_id => Some(()),
        _ => None,
    })
    .await;
    assert!(seeked.is_some(), "Should receive Seeked event");
}

/// The capture sink's streams, one per non-gapless transition. Holding
/// the receiver is what lets stream creation succeed, so a fixture keeps it for
/// the playback service's lifetime even when it never inspects the samples.
pub type CaptureStreamRx = CapturedStreams;

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

/// A new `device`, with the receiver of the streams it captures.
pub fn capture_device(device: TestAudioDevice) -> (Box<dyn AudioOutputDevice>, CaptureStreamRx) {
    match device {
        TestAudioDevice::Capture => {
            let (captured, rx) = tokio::sync::mpsc::unbounded_channel();
            (Box::new(CaptureAudioDevice::new(captured)), rx)
        }
        TestAudioDevice::RealtimeCapture => {
            let (captured, rx) = tokio::sync::mpsc::unbounded_channel();
            (Box::new(RealtimeCaptureAudioDevice::new(captured)), rx)
        }
    }
}

/// The playback time source over the wall clock, as the app's.
pub fn wall_playback_clock() -> PlaybackClockRef {
    std::sync::Arc::new(WallPlaybackClock::new(std::sync::Arc::new(coven::SystemClock)))
}

/// Awaits the next captured stream a transition started. Streams are
/// yielded in creation order; tests that exercise auto-advance, seek, or next
/// call this once per stream they want to inspect.
pub async fn next_capture_stream(capture_stream_rx: &mut CaptureStreamRx) -> CapturedStream {
    match tokio::time::timeout(HANG_GUARD, capture_stream_rx.recv()).await {
        Ok(Some(stream)) => stream,
        Ok(None) => panic!("capture stream channel closed before a stream was created"),
        Err(_) => panic!("no capture stream was ever created"),
    }
}
