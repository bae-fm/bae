use super::*;
use crate::audio_output::{audio_event_channel, AudioEvent, AudioEventReceiver};
use crate::source::{PlaybackSource, TrackFmt};
use bae_audio::track_stream::create_track_stream_pair;
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

const SAMPLES: usize = 4_096;

fn track_fmt() -> TrackFmt {
    TrackFmt {
        track_id: "t".to_string(),
        timeline: crate::TrackTimeline::new(
            Some(std::time::Duration::from_secs(1)),
            std::time::Duration::ZERO,
        ),
        starts_at: crate::StreamPosition::START,
        replay_gain_linear: 1.0,
    }
}

/// Stereo samples that each differ, so a sample lost or played twice shows.
fn ramp() -> Vec<f32> {
    (0..SAMPLES).map(|i| i as f32 / SAMPLES as f32).collect()
}

/// A drain over a source holding `ramp()`, playing.
fn playing_drain() -> (AudioOutputControls, AudioDrain, AudioEventReceiver) {
    let (mut sink, stream, _ready) = create_track_stream_pair(44_100, 2);
    sink.push_samples(&ramp());
    let source = Arc::new(Mutex::new(PlaybackSource::new(stream, track_fmt())));
    let controls = AudioOutputControls::new(crate::Volume::FULL);
    controls.set_state(AudioState::Playing);
    let (tx, rx) = audio_event_channel();
    let drain = AudioDrain::new(controls.clone(), source, tx, 100);
    (controls, drain, rx)
}

/// What every stream of one test played, in order: the speaker.
type Heard = Arc<Mutex<Vec<f32>>>;

/// A device stream that adds what it takes to `heard` and disconnects on
/// cue: after taking `take_before_disconnect` samples, or once
/// `disconnected` is set.
struct FakeStream {
    heard: Heard,
    take_before_disconnect: Cell<Option<usize>>,
    disconnected: Arc<AtomicBool>,
}

impl FakeStream {
    fn new(heard: &Heard, take_before_disconnect: Option<usize>) -> Self {
        Self {
            heard: heard.clone(),
            take_before_disconnect: Cell::new(take_before_disconnect),
            disconnected: Arc::new(AtomicBool::new(false)),
        }
    }

    fn connected(&self) -> Result<(), DeviceStreamError> {
        if self.disconnected.load(Ordering::Relaxed) {
            return Err(DeviceStreamError::Disconnected);
        }
        Ok(())
    }
}

impl DeviceStream for FakeStream {
    fn start(&self) -> Result<(), DeviceStreamError> {
        self.connected()
    }

    fn stop(&self) -> Result<(), DeviceStreamError> {
        self.connected()
    }

    fn write(&self, samples: &[f32]) -> Result<usize, DeviceStreamError> {
        self.connected()?;
        let taking = match self.take_before_disconnect.get() {
            None => samples.len(),
            Some(0) => {
                self.disconnected.store(true, Ordering::Relaxed);
                return Err(DeviceStreamError::Disconnected);
            }
            Some(left) => {
                let taking = left.min(samples.len());
                self.take_before_disconnect.set(Some(left - taking));
                taking
            }
        };
        self.heard.lock().unwrap().extend_from_slice(&samples[..taking]);
        Ok(taking)
    }
}

/// Opens connected streams onto `heard`, counting the opens.
fn reopening(heard: &Heard) -> (Arc<AtomicUsize>, impl FnMut() -> Result<FakeStream, String>) {
    let opens = Arc::new(AtomicUsize::new(0));
    let counted = opens.clone();
    let heard = heard.clone();
    (opens, move || {
        counted.fetch_add(1, Ordering::Relaxed);
        Ok(FakeStream::new(&heard, None))
    })
}

/// Step until `heard` holds the whole source.
fn play_out<O>(writer: &mut DeviceWriter<FakeStream, O>, heard: &Heard)
where
    O: FnMut() -> Result<FakeStream, String>,
{
    for _ in 0..1_000 {
        if heard.lock().unwrap().len() >= SAMPLES {
            return;
        }
        assert_ne!(writer.step(), WriterStep::Ended, "the output ended");
    }
    panic!("the source didn't play out within 1000 steps");
}

fn output_failure(rx: &mut AudioEventReceiver) -> Option<String> {
    while let Some(event) = rx.pop() {
        if let AudioEvent::OutputFailed { detail } = event {
            return Some(detail);
        }
    }
    None
}

/// A disconnect partway through a write (the device was unplugged, or
/// another became the default) moves playback to a stream opened on the
/// current default, which takes first what the old one didn't: nothing is
/// lost or played twice. Sabotage: drop the untaken samples on a reopen,
/// and what was heard misses them.
#[test]
fn a_disconnect_mid_write_continues_on_a_reopened_stream() {
    let (controls, drain, _rx) = playing_drain();
    let heard = Heard::default();
    let (opens, open) = reopening(&heard);
    let mut writer =
        DeviceWriter::new(FakeStream::new(&heard, Some(300)), open, controls, drain, 1_024);

    play_out(&mut writer, &heard);

    assert_eq!(opens.load(Ordering::Relaxed), 1, "one reopen for one disconnect");
    assert_eq!(*heard.lock().unwrap(), ramp());
}

/// The device changing while playback is paused disconnects the stopped
/// stream, and AAudio then refuses to start it. Resuming opens a stream on
/// the current default and plays on from where the pause left off.
/// Sabotage: retry the refused start on the same stream, and resume plays
/// nothing.
#[test]
fn a_device_change_while_paused_reopens_on_resume() {
    let (controls, drain, _rx) = playing_drain();
    let heard = Heard::default();
    let (opens, open) = reopening(&heard);
    let first = FakeStream::new(&heard, None);
    let first_disconnected = first.disconnected.clone();
    let mut writer = DeviceWriter::new(first, open, controls.clone(), drain, 1_024);

    assert_eq!(writer.step(), WriterStep::Wrote);
    assert_eq!(heard.lock().unwrap().len(), 1_024);

    controls.set_state(AudioState::Paused);
    assert_eq!(writer.step(), WriterStep::Paused);
    first_disconnected.store(true, Ordering::Relaxed);
    controls.set_state(AudioState::Playing);
    play_out(&mut writer, &heard);

    assert_eq!(opens.load(Ordering::Relaxed), 1);
    assert_eq!(*heard.lock().unwrap(), ramp());
}

/// A replacement stream that can't be opened ends the output and tells
/// playback why, rather than leaving it playing nothing.
#[test]
fn a_reopen_that_fails_ends_the_output_with_the_reason() {
    let (controls, drain, mut rx) = playing_drain();
    let heard = Heard::default();
    let mut writer = DeviceWriter::new(
        FakeStream::new(&heard, Some(0)),
        || Err::<FakeStream, _>("no output device".to_string()),
        controls,
        drain,
        1_024,
    );

    assert_eq!(writer.step(), WriterStep::Ended);

    let detail = output_failure(&mut rx).expect("playback is told the output stopped");
    assert!(detail.contains("no output device"), "{detail}");
}

/// A stream disconnected before it ever started isn't a change of device:
/// the device it was opened on refuses it. The output ends instead of
/// opening stream after stream.
#[test]
fn a_stream_disconnected_before_it_started_ends_the_output() {
    let (controls, drain, mut rx) = playing_drain();
    let heard = Heard::default();
    let opens = Arc::new(AtomicUsize::new(0));
    let counted = opens.clone();
    let refused = heard.clone();
    let mut writer = DeviceWriter::new(
        FakeStream::new(&heard, Some(0)),
        move || {
            counted.fetch_add(1, Ordering::Relaxed);
            let stream = FakeStream::new(&refused, None);
            stream.disconnected.store(true, Ordering::Relaxed);
            Ok(stream)
        },
        controls,
        drain,
        1_024,
    );

    assert_eq!(writer.step(), WriterStep::Wrote, "the disconnect reopens");
    assert_eq!(writer.step(), WriterStep::Ended, "the new stream's refusal ends it");
    assert_eq!(opens.load(Ordering::Relaxed), 1);
    assert!(output_failure(&mut rx).is_some());
}

/// A device failure other than a disconnect is not a change of device: the
/// output ends with the device's reason, and nothing is reopened.
#[test]
fn a_write_failure_ends_the_output_without_reopening() {
    struct FailingStream;
    impl DeviceStream for FailingStream {
        fn start(&self) -> Result<(), DeviceStreamError> {
            Ok(())
        }
        fn stop(&self) -> Result<(), DeviceStreamError> {
            Ok(())
        }
        fn write(&self, _samples: &[f32]) -> Result<usize, DeviceStreamError> {
            Err(DeviceStreamError::Failed("Internal".to_string()))
        }
    }

    let (controls, drain, mut rx) = playing_drain();
    let mut writer = DeviceWriter::new(
        FailingStream,
        || -> Result<FailingStream, String> { panic!("a failed write must not reopen") },
        controls,
        drain,
        1_024,
    );

    assert_eq!(writer.step(), WriterStep::Ended);
    let detail = output_failure(&mut rx).expect("playback is told the output stopped");
    assert!(detail.contains("Internal"), "{detail}");
}
