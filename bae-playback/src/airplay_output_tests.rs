use super::*;
use crate::audio_output::{audio_event_channel, AudioState};
use crate::source::{PlaybackSource, TrackFmt};
use bae_audio::track_stream::create_track_stream_pair;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// A playing source of `stereo_frames` full-scale stereo frames.
fn playing_source(stereo_frames: usize) -> Arc<Mutex<PlaybackSource>> {
    let (mut sink, stream, _ready) = create_track_stream_pair(44_100, 2);
    sink.push_samples(&vec![1.0f32; stereo_frames * 2]);
    Arc::new(Mutex::new(PlaybackSource::new(stream, TrackFmt::for_test("t"))))
}

/// A fake sink: it records how many frames the source produced and honours the
/// control ops, so the seam is exercised without a socket.
#[derive(Default)]
struct FakeSink {
    state: Arc<FakeState>,
}

#[derive(Default)]
struct FakeState {
    frames: AtomicU64,
    flushed: AtomicBool,
    reanchored: AtomicBool,
    torn_down: AtomicBool,
    failed: Mutex<Option<AirPlayFailed>>,
}

struct FakeSession(Arc<FakeState>);
impl Drop for FakeSession {
    fn drop(&mut self) {
        self.0.torn_down.store(true, Ordering::Release);
    }
}

impl AirPlayStreamControl for FakeSession {
    fn flush(&self) {
        self.0.flushed.store(true, Ordering::Release);
    }
    fn reanchor(&self) {
        self.0.reanchored.store(true, Ordering::Release);
    }
    fn failure(&self) -> Option<String> {
        None
    }
    fn latency(&self) -> Duration {
        Duration::from_millis(250)
    }
}

impl AirPlaySink for FakeSink {
    fn start(
        &self,
        mut source: Box<dyn PcmSource>,
        _wake: PcmWake,
        failed: AirPlayFailed,
    ) -> Result<Arc<dyn AirPlayStreamControl>, AudioError> {
        *self.state.failed.lock().unwrap() = Some(failed);
        // Pull one packet synchronously to record the frame conversion.
        let mut buf = vec![0i16; 8]; // 4 stereo frames
        let frames = source.next_frames(&mut buf);
        self.state.frames.store(frames as u64, Ordering::Relaxed);
        Ok(Arc::new(FakeSession(self.state.clone())))
    }
}

/// `create_stream` converts the drained f32 to 16-bit stereo and publishes a
/// control into the slot; the control drives flush/reanchor, reads the
/// session's latency, and drop tears the session down.
#[test]
fn create_stream_drives_the_sink_and_publishes_control() {
    let source = playing_source(4);

    let sink = FakeSink::default();
    let state = sink.state.clone();
    let control = Arc::new(AirPlayControl::new());
    let reports = Arc::new(AtomicU64::new(0));
    let reported = reports.clone();
    let failed = AirPlayFailed::new(move || {
        reported.fetch_add(1, Ordering::Relaxed);
    });
    let mut output = AirPlayOutput::new(Box::new(sink), control.clone(), failed);
    let controls = AudioOutputControls::new(crate::Volume::FULL);
    controls.set_state(AudioState::Playing);

    let (tx, _rx) = audio_event_channel().unwrap();
    let stream = output.create_stream(controls, source, 44_100, 2, tx, 100).expect("stream builds");

    // The session was handed the output's failure report.
    state.failed.lock().unwrap().as_ref().expect("the sink got the failure report").report();
    assert_eq!(reports.load(Ordering::Relaxed), 1);

    // The fake pulled 4 stereo frames.
    assert_eq!(state.frames.load(Ordering::Relaxed), 4);
    // The control was published for the renderer to drive.
    control.flush();
    control.reanchor();
    assert!(state.flushed.load(Ordering::Acquire));
    assert!(state.reanchored.load(Ordering::Acquire));
    assert_eq!(control.latency(), Some(Duration::from_millis(250)));

    // Dropping the stream tears the session down; no audio is in flight.
    drop(stream);
    assert!(state.torn_down.load(Ordering::Acquire));
    assert_eq!(control.latency(), None);
}

/// A track the converter can't be set up for is refused before anything
/// is sent, rather than streamed unconverted at the wrong speed and pitch.
#[test]
fn a_converter_that_cant_be_set_up_refuses_the_stream() {
    let (tx, _rx) = audio_event_channel().unwrap();
    let controls = AudioOutputControls::new(crate::Volume::FULL);
    controls.set_state(AudioState::Playing);
    let refused = DrainSource::new(
        AudioDrain::new(controls.clone(), playing_source(4), tx, 100),
        controls,
        0,
        2,
    );
    assert!(matches!(refused, Err(AudioError::UnsupportedFormat { sample_rate: 0, channels: 2 })));
}

/// A paused output yields no frames from the drain (so the stream sends
/// nothing), and a playing one converts full-scale f32 to i16::MAX.
#[test]
fn drain_source_respects_pause_and_converts_scale() {
    let (tx, _rx) = audio_event_channel().unwrap();
    let controls = AudioOutputControls::new(crate::Volume::FULL);
    controls.set_state(AudioState::Paused);
    let mut paused = DrainSource::new(
        AudioDrain::new(controls.clone(), playing_source(4), tx, 100),
        controls.clone(),
        44_100,
        2,
    )
    .expect("a converter sets up");
    let mut out = vec![0i16; 8];
    assert_eq!(paused.next_frames(&mut out), 0, "paused yields nothing");

    // Now playing: the four full-scale stereo frames convert to i16::MAX.
    let (tx2, _rx2) = audio_event_channel().unwrap();
    let controls2 = AudioOutputControls::new(crate::Volume::FULL);
    controls2.set_state(AudioState::Playing);
    let mut playing = DrainSource::new(
        AudioDrain::new(controls2.clone(), playing_source(4), tx2, 100),
        controls2,
        44_100,
        2,
    )
    .expect("a converter sets up");
    let mut out2 = vec![0i16; 8];
    assert_eq!(playing.next_frames(&mut out2), 4);
    assert!(out2.iter().all(|&s| s == i16::MAX));
}

/// A 48 kHz source through `DrainSource` comes out at 44.1 kHz: streaming a
/// whole 48 kHz second yields ~44 100 frames, not 48 000, so playback isn't
/// slowed and pitched down. (Without the resampler this yields 48 000.)
#[test]
fn drain_source_resamples_to_the_receiver_rate() {
    // A ring large enough to hold the whole input without a concurrent reader.
    let (mut sink, stream, _ready) =
        bae_audio::track_stream::create_track_stream_pair_with_capacity(48_000, 2, 48_000 * 2);
    // 20 000 frames of 48 kHz stereo, then finish so the source drains.
    sink.push_samples(&vec![0.0f32; 20_000 * 2]);
    sink.mark_finished();
    let source = Arc::new(Mutex::new(PlaybackSource::new(stream, TrackFmt::for_test("t"))));

    let controls = AudioOutputControls::new(crate::Volume::FULL);
    controls.set_state(AudioState::Playing);
    let (tx, _rx) = audio_event_channel().unwrap();
    let mut ds =
        DrainSource::new(AudioDrain::new(controls.clone(), source, tx, 100), controls, 48_000, 2)
            .expect("a converter sets up");

    let mut total = 0usize;
    let mut out = vec![0i16; 352 * 2];
    // Pull until the source drains and the resampler tail is flushed.
    for _ in 0..1000 {
        let f = ds.next_frames(&mut out);
        if f == 0 {
            break;
        }
        total += f;
    }
    // 20 000 frames at 48 kHz → 20 000 × 44100/48000 = 18 375 at 44.1 kHz.
    assert!(
        (total as i64 - 18_375).abs() < 400,
        "expected ~18375 output frames at 44.1 kHz, got {total}"
    );
}

#[test]
fn f32_converts_to_i16_full_scale() {
    assert_eq!(f32_to_i16(1.0), i16::MAX);
    assert_eq!(f32_to_i16(-1.0), -i16::MAX);
    assert_eq!(f32_to_i16(0.0), 0);
    // Out of range clamps.
    assert_eq!(f32_to_i16(2.0), i16::MAX);
}
