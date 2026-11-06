use super::*;
use crate::audio_output::{audio_event_channel, AudioEvent, AudioEventReceiver, AudioState};
use bae_audio::track_stream::create_track_stream_pair_with_capacity;

/// One second of stereo at 44.1 kHz.
const TRACK_SAMPLES: usize = 44_100 * 2;
/// A report every 100 ms: 4,410 stereo frames.
const REPORT_SAMPLES: usize = 4_410 * 2;

/// A manual output playing a one-second track whose decode has finished, as
/// a test holds it: the stream it built, the transition stream it captures
/// into, the clock it plays by, and the events its drain reports.
struct Playing {
    _stream: Box<dyn AudioStream>,
    captured: CapturedStream,
    clock: AudioClock,
    events: AudioEventReceiver,
}

async fn playing_one_second() -> Playing {
    // A ring the whole track fits in, so its decode is done before it plays.
    let (mut sink, track, _ready) =
        create_track_stream_pair_with_capacity(44_100, 2, TRACK_SAMPLES);
    let pushed = sink.push_samples(&(0..TRACK_SAMPLES).map(|i| i as f32).collect::<Vec<_>>());
    assert_eq!(pushed, TRACK_SAMPLES);
    sink.mark_finished();
    let source = Arc::new(Mutex::new(PlaybackSource::new(track, TrackFmt::for_test("t"))));
    let (captured_tx, mut captured_rx) = tokio::sync::mpsc::unbounded_channel();
    let clock = AudioClock::new();
    let mut output = ManualCaptureAudioOutput::new(captured_tx, clock.clone());
    let controls = AudioOutputControls::new(crate::Volume::FULL);
    controls.set_state(AudioState::Playing);
    let (events_tx, events) = audio_event_channel().unwrap();
    let stream = output.create_stream(controls, source, 44_100, 2, events_tx, 100).unwrap();
    let captured = captured_rx.recv().await.expect("the stream's capture");
    Playing { _stream: stream, captured, clock, events }
}

/// The device plays exactly what the clock lets it, one report at a time,
/// and nothing past it: two reports are two intervals of samples, at the
/// positions they report.
#[tokio::test]
async fn a_manual_device_plays_one_report_at_a_time() {
    let mut playing = playing_one_second().await;

    let first = playing.clock.play_report().await;
    assert_eq!(playing.captured.samples().len(), REPORT_SAMPLES);
    assert_eq!(first.position, StreamPosition::from_millis(100));

    let second = playing.clock.play_report().await;
    assert_eq!(playing.captured.samples().len(), 2 * REPORT_SAMPLES);
    assert_eq!(second.position, StreamPosition::from_millis(200));
    let samples = playing.captured.samples();
    assert!(
        samples.iter().enumerate().all(|(i, sample)| *sample == i as f32),
        "the device plays the track in order, nothing skipped or played twice"
    );
}

/// Playing to the end of the track plays all of it and holds there: the
/// drain has reported the track complete.
#[tokio::test]
async fn a_manual_device_plays_a_track_out() {
    let mut playing = playing_one_second().await;

    playing.clock.play_to_track_end().await;

    assert_eq!(playing.captured.samples().len(), TRACK_SAMPLES);
    let completed = std::iter::from_fn(|| playing.events.pop())
        .any(|event| matches!(event, AudioEvent::Completion(_)));
    assert!(completed, "the track ran out, and the drain said so");
}

/// Waiting for samples on a manual device lets it play report by report
/// until they have played, and no further than the report that brought them.
#[tokio::test]
async fn waiting_for_samples_plays_them() {
    let mut playing = playing_one_second().await;

    let samples = playing.captured.wait_for_samples(REPORT_SAMPLES + 1).await;

    assert_eq!(samples.len(), 2 * REPORT_SAMPLES);
}
