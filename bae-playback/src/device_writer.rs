//! The writer that feeds an output whose device takes samples by blocking
//! write — AAudio's, on Android — from a
//! [`PlaybackSource`](crate::source::PlaybackSource), and moves it to
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

use super::audio_output::{AudioDrain, AudioOutputControls, AudioState, DrainStatus, SourceLock};
use bae_util::task_failure::caught_sync;
use std::ops::Range;
use std::sync::atomic::{AtomicBool, Ordering};
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
            current: OpenStream { stream, phase: StreamPhase::Opened },
            controls,
            drain,
            buf: vec![0.0; buf_len],
            unwritten: 0..0,
        }
    }

    /// Step until the output ends or `stop` is set. While paused or starved
    /// the thread waits on the player's wake, which a resume, a decoder's push
    /// or finish, a track placed in the source, and the owner's stop all
    /// notify. A step that panics ends the output, which playback reports.
    pub(crate) fn run(mut self, stop: &AtomicBool) {
        loop {
            // Read before the stop flag and the step, so a stop, a resume or a
            // push that lands after this read ends the wait below at once.
            let seen = self.controls.drain_changes();
            if stop.load(Ordering::Acquire) {
                return;
            }
            let step = caught_sync("write to the device stream", || self.step())
                .unwrap_or_else(|failed| self.end(failed.to_string()));
            match step {
                WriterStep::Wrote => {}
                WriterStep::Paused | WriterStep::Starved => self.controls.wait_for_drain_work(seen),
                WriterStep::Ended => return,
            }
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
            match self.drain.drain_iteration(&mut self.buf, true, SourceLock::Wait) {
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
            DeviceStreamError::Disconnected => {
                self.end(format!("{operation}: the device it was opened on disconnected it"))
            }
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
                self.current = OpenStream { stream, phase: StreamPhase::Opened };
                WriterStep::Wrote
            }
            Err(detail) => {
                self.end(format!("reopen the device stream after its device changed: {detail}"))
            }
        }
    }

    /// Tell playback the output stopped, for the reason `detail` states: a
    /// device stream that can't go on, or a step that panicked.
    pub(crate) fn end(&mut self, detail: String) -> WriterStep {
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
#[path = "device_writer_tests.rs"]
mod tests;
