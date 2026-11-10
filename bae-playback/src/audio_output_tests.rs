use super::*;
use crate::source::{PlaybackSource, TrackFmt};
use bae_audio::track_stream::create_track_stream_pair_with_capacity;

/// A playing drain reporting every 250 ms over a finished stereo source at
/// 48 kHz holding `frames` frames, and where its events go.
fn drain_over_frames(frames: usize) -> (AudioDrain, AudioEventReceiver) {
    let samples = vec![0.25_f32; frames * 2];
    let (mut sink, stream, _ready) =
        create_track_stream_pair_with_capacity(48_000, 2, samples.len());
    sink.push_samples(&samples);
    sink.mark_finished();
    let source = Arc::new(Mutex::new(PlaybackSource::new(stream, TrackFmt::for_test("t"))));
    let controls = AudioOutputControls::new(crate::Volume::FULL);
    controls.set_state(AudioState::Playing);
    let (events, received) = audio_event_channel(&bae_diagnostics::Diagnostics::noop()).unwrap();
    (AudioDrain::new(controls, source, events, 250), received)
}

/// Pull everything `drain` holds in buffers of `buffer_frames` stereo frames,
/// as fast as it gives them, and count the positions it reported.
fn positions_reported(
    drain: &mut AudioDrain,
    events: &mut AudioEventReceiver,
    buffer_frames: usize,
) -> usize {
    let mut buf = vec![0.0; buffer_frames * 2];
    while let DrainStatus::Samples { .. } = drain.drain_iteration(&mut buf, false, SourceLock::Wait)
    {
    }
    std::iter::from_fn(|| events.pop())
        .filter(|event| matches!(event, AudioEvent::Position(_)))
        .count()
}

/// A drain reports the position once per 250 ms of audio it pulls, 12 000
/// frames at 48 kHz, whatever buffers the device takes it in and however fast.
/// Sabotage: report on the wall clock, and pulling five reports' worth at full
/// speed reports at most once.
#[test]
fn a_drain_reports_once_per_interval_of_audio_however_fast_it_is_pulled() {
    for buffer_frames in [100, 4_096, 12_000] {
        let (mut drain, mut events) = drain_over_frames(60_000);

        assert_eq!(
            positions_reported(&mut drain, &mut events, buffer_frames),
            5,
            "in buffers of {buffer_frames} frames"
        );
    }
}
