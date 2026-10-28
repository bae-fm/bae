//! The writer that feeds an output whose device takes samples by blocking
//! write — AAudio's, on Android — from a
//! [`PlaybackSource`](crate::playback::source::PlaybackSource), and moves it to
//! the system's current default output when the device it plays on goes away.
//!
//! AAudio reports a change of output device by disconnecting the stream: the
//! device it played on was unplugged, or another one became the primary output.
//! A disconnected stream can't be used again; a stream opened without naming a
//! device plays on the current primary one. So the writer closes the
//! disconnected stream, opens a new one and goes on writing where it stopped:
//! the samples the old stream didn't take go to the new one first, and the
//! source's position never moved past them.
//!
//! The device stream is behind [`DeviceStream`], so the writer's reaction to a
//! disconnect runs in tests against streams that disconnect on cue.

use super::audio_output::{AudioDrain, AudioOutputControls, AudioState, DrainStatus};
use std::ops::Range;
use tracing::{debug, info};

/// Why a device stream refused an operation.
#[derive(Debug)]
pub(crate) enum DeviceStreamError {
    /// The stream's device went away or stopped being the primary output. The
    /// stream can't be used again; one opened now plays on the current default.
    Disconnected,
    /// Any other failure, as the device described it.
    Failed(String),
}

/// A device output stream as the writer drives it.
pub(crate) trait DeviceStream {
    fn start(&self) -> Result<(), DeviceStreamError>;
    fn stop(&self) -> Result<(), DeviceStreamError>;
    /// Write interleaved samples, blocking up to the stream's write timeout.
    /// Returns how many of them the device took, a whole number of frames;
    /// zero when it took none within the timeout.
    fn write(&self, samples: &[f32]) -> Result<usize, DeviceStreamError>;
}

/// What a writer step did, which says what the writer thread does next.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum WriterStep {
    /// Wrote to the device, or reopened it: step again.
    Wrote,
    /// Playback isn't playing, and the device is stopped until it is.
    Paused,
    /// Playing, but the source had nothing to give.
    Starved,
    /// The output can't go on; playback was told why. The thread exits.
    Ended,
}

/// Where the writer's device stream is in its life.
#[derive(Clone, Copy, PartialEq, Eq)]
enum StreamPhase {
    /// Opened and never started. A disconnect now isn't a change of device: the
    /// device it was just opened on is refusing it, and opening another would
    /// only repeat that.
    Opened,
    Started,
    /// Started before, and stopped while playback isn't playing.
    Stopped,
}

/// The device stream the writer holds, and its phase.
struct OpenStream<S> {
    stream: S,
    phase: StreamPhase,
}

/// Feeds a device stream from the drain, reopening it on the current default
/// device when the one it plays on disconnects.
pub(crate) struct DeviceWriter<S: DeviceStream, O> {
    open: O,
    current: OpenStream<S>,
    controls: AudioOutputControls,
    drain: AudioDrain,
    buf: Vec<f32>,
    /// The part of `buf` drained from the source and not yet taken by the
    /// device. Written before anything new is drained, so a reopened stream
    /// starts with what the disconnected one didn't take.
    unwritten: Range<usize>,
}

impl<S, O> DeviceWriter<S, O>
where
    S: DeviceStream,
    O: FnMut() -> Result<S, String>,
{
    /// A writer over `stream`, already opened, that opens its replacements
    /// with `open`. `buf_len` is the number of samples drained at a time.
    pub(crate) fn new(
        stream: S,
        open: O,
        controls: AudioOutputControls,
        drain: AudioDrain,
        buf_len: usize,
    ) -> Self {
        Self {
            open,
            current: OpenStream {
                stream,
                phase: StreamPhase::Opened,
            },
            controls,
            drain,
            buf: vec![0.0; buf_len],
            unwritten: 0..0,
        }
    }

    /// One pass of the writer: follow the play state, then write what the
    /// device hasn't taken yet, or drain more and write that.
    pub(crate) fn step(&mut self) -> WriterStep {
        if self.controls.get_state() != AudioState::Playing {
            // Idle the device while paused or stopped so it doesn't drain a
            // silent buffer and burn power.
            if self.current.phase == StreamPhase::Started {
                if let Err(error) = self.current.stream.stop() {
                    // A disconnected stream refuses to stop too; the start on
                    // resume finds it disconnected and reopens.
                    debug!("stopping the device stream while paused failed: {error:?}");
                }
                self.current.phase = StreamPhase::Stopped;
            }
            return WriterStep::Paused;
        }

        if self.current.phase != StreamPhase::Started {
            match self.current.stream.start() {
                Ok(()) => self.current.phase = StreamPhase::Started,
                Err(error) => return self.stream_failed("start the device stream", error),
            }
        }

        if self.unwritten.is_empty() {
            match self.drain.drain_iteration(&mut self.buf, true, false, None) {
                DrainStatus::Samples { read } => self.unwritten = 0..read,
                // Persistent output: a drained track idles the writer rather
                // than ending it; the next track's source replace resumes it.
                DrainStatus::Idle => return WriterStep::Starved,
            }
        }

        match self.current.stream.write(&self.buf[self.unwritten.clone()]) {
            // A write that took nothing within the timeout leaves the samples
            // unwritten; the next step re-checks the play state and tries
            // them again.
            Ok(taken) => {
                self.unwritten.start += taken;
                WriterStep::Wrote
            }
            Err(error) => self.stream_failed("write to the device stream", error),
        }
    }

    /// The stream refused `operation`. A disconnect means its device changed:
    /// close it and open one on the current default, keeping what it didn't
    /// take. Anything else, or a disconnect of a stream that never started,
    /// ends the output and tells playback why.
    fn stream_failed(&mut self, operation: &str, error: DeviceStreamError) -> WriterStep {
        match error {
            DeviceStreamError::Disconnected if self.current.phase != StreamPhase::Opened => {
                self.reopen()
            }
            DeviceStreamError::Disconnected => self.end(format!(
                "{operation}: the device it was opened on disconnected it"
            )),
            DeviceStreamError::Failed(detail) => self.end(format!("{operation}: {detail}")),
        }
    }

    /// Close the disconnected stream and open its replacement on the current
    /// default device, which the next step starts.
    fn reopen(&mut self) -> WriterStep {
        if let Err(error) = self.current.stream.stop() {
            // AAudio asks for a disconnected stream to be stopped before it is
            // closed, and may refuse the stop.
            debug!("stopping the disconnected device stream failed: {error:?}");
        }
        match (self.open)() {
            Ok(stream) => {
                info!("the output device changed; playing on the current default device");
                self.current = OpenStream {
                    stream,
                    phase: StreamPhase::Opened,
                };
                WriterStep::Wrote
            }
            Err(detail) => self.end(format!(
                "reopen the device stream after its device changed: {detail}"
            )),
        }
    }

    /// Tell playback the output stopped, for the reason `detail` states.
    fn end(&mut self, detail: String) -> WriterStep {
        self.drain.report_output_failure(detail);
        WriterStep::Ended
    }
}

impl<S: DeviceStream, O> Drop for DeviceWriter<S, O> {
    /// Stop the device stream as the writer exits.
    fn drop(&mut self) {
        if self.current.phase == StreamPhase::Started {
            if let Err(error) = self.current.stream.stop() {
                debug!("stopping the device stream at teardown failed: {error:?}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::playback::audio_output::{audio_event_channel, AudioEvent, AudioEventReceiver};
    use crate::playback::source::{PlaybackSource, TrackFmt};
    use bae_audio::track_stream::create_track_stream_pair;
    use std::cell::Cell;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    const SAMPLES: usize = 4_096;

    fn track_fmt() -> TrackFmt {
        TrackFmt {
            track_id: "t".to_string(),
            timeline: crate::playback::TrackTimeline::new(
                Some(std::time::Duration::from_secs(1)),
                std::time::Duration::ZERO,
            ),
            starts_at: crate::playback::StreamPosition::START,
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
        let controls = AudioOutputControls::new(1.0);
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
            self.heard
                .lock()
                .unwrap()
                .extend_from_slice(&samples[..taking]);
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
        let mut writer = DeviceWriter::new(
            FakeStream::new(&heard, Some(300)),
            open,
            controls,
            drain,
            1_024,
        );

        play_out(&mut writer, &heard);

        assert_eq!(
            opens.load(Ordering::Relaxed),
            1,
            "one reopen for one disconnect"
        );
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
        assert_eq!(
            writer.step(),
            WriterStep::Ended,
            "the new stream's refusal ends it"
        );
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
}
