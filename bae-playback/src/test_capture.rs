//! Playback on a capture device standing in for the audio output, for the
//! tests of the crates above: the device a test plays on, the streams it
//! captures, letting its audio play, and waiting on the progress a playback
//! reports.

use crate::{
    AudioClock, AudioOutputDevice, CaptureAudioDevice, CapturedStream, CapturedStreams,
    ManualCaptureDevice, PlaybackClockRef, PlaybackHandle, PlaybackProgress, PlaybackProgressFeed,
    PlaybackState, WallPlaybackClock,
};
use bae_util::test_channels::{next_matching, HANG_GUARD};

/// Wait for playback to report `track_id` as the playing track, discarding
/// every other progress event. Panics only if it never does.
pub async fn wait_until_playing(progress_rx: &mut PlaybackProgressFeed, track_id: &str) {
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
pub async fn wait_for_seek(progress_rx: &mut PlaybackProgressFeed, track_id: &str) {
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

/// A new device that captures at full speed, pulling as fast as the decoder
/// fills, with the receiver of the streams it captures. A track can play out
/// before a command sent after play lands, so it is for a test that only
/// reads what played.
pub fn capture_device() -> (Box<dyn AudioOutputDevice>, CaptureStreamRx) {
    let (captured, rx) = tokio::sync::mpsc::unbounded_channel();
    (Box::new(CaptureAudioDevice::new(captured)), rx)
}

/// A new device that plays only as far as the returned clock lets it, with
/// the receiver of the streams it captures.
pub fn manual_capture_device() -> (Box<dyn AudioOutputDevice>, CaptureStreamRx, AudioClock) {
    let (captured, rx) = tokio::sync::mpsc::unbounded_channel();
    let clock = AudioClock::new();
    (Box::new(ManualCaptureDevice::new(captured, clock.clone())), rx, clock)
}

/// Once `playback` has handled every command sent before, let `clock`'s
/// device play up to its next position report, and read `progress` until it
/// shows the report. Returns every event read, the report last.
pub async fn play_report(
    playback: &PlaybackHandle,
    clock: &AudioClock,
    progress: &mut PlaybackProgressFeed,
) -> Vec<PlaybackProgress> {
    playback.commands_handled().await;
    let report = clock.play_report().await;
    let mut events = Vec::new();
    let shown = next_matching(progress, HANG_GUARD, |event| {
        let reported = report.reported_as(&event);
        events.push(event);
        reported.then_some(())
    })
    .await;
    assert!(shown.is_some(), "playback never reported the position its device played to");
    events
}

/// Once `playback` has handled every command sent before, let `clock`'s
/// device play to the end of the track playing.
pub async fn play_to_track_end(playback: &PlaybackHandle, clock: &AudioClock) {
    playback.commands_handled().await;
    clock.play_to_track_end().await;
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
