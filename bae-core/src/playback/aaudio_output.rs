//! AAudio output sink for Android.
//!
//! The in-core decode path produces interleaved f32 PCM; this sink writes it to
//! an AAudio output stream in blocking mode from a dedicated writer thread. The
//! thread owns the [`ndk::audio::AudioStream`] for its whole lifetime — AAudio
//! streams are bound to the thread that opens them, so the stream is constructed
//! inside the spawned thread and never moved across threads after creation. The
//! thread runs a [`DeviceWriter`], which reopens the stream on the current
//! default device when AAudio disconnects it because the output device changed.

use super::audio_output::{
    AudioDrain, AudioError, AudioEventSender, AudioOutput, AudioOutputControls, AudioStream,
};
use super::device_writer::{DeviceStream, DeviceStreamError, DeviceWriter, WriterStep};
use crate::playback::source::PlaybackSource;
use ndk::audio::{
    AudioDirection, AudioError as NdkAudioError, AudioFormat, AudioPerformanceMode,
    AudioStream as NdkAudioStream, AudioStreamBuilder,
};
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;
use tracing::{info, warn};

/// Number of frames pulled and written per loop iteration. Interleaved across
/// channels, so the f32 scratch buffer holds `WRITE_FRAMES * channels` samples.
const WRITE_FRAMES: usize = 1024;

/// Blocking-write timeout. The writer thread is dedicated to this stream, so a
/// bounded block is fine; the timeout keeps the thread responsive to state
/// changes (pause/stop) and shutdown rather than parking indefinitely on a
/// stalled device.
const WRITE_TIMEOUT_NS: i64 = 100_000_000; // 100 ms

/// Audio output writing decoded PCM to an AAudio output stream.
pub struct AAudioOutput {
    controls: AudioOutputControls,
}

impl AAudioOutput {
    pub fn new() -> Result<Self, AudioError> {
        Ok(Self {
            controls: AudioOutputControls::new(1.0),
        })
    }
}

/// An AAudio output stream as the [`DeviceWriter`] drives it.
struct AAudioDeviceStream {
    stream: NdkAudioStream,
    channels: u32,
}

/// Open an AAudio output stream for the given format on the system's current
/// primary output (no device is named), logging the rate, channel count and
/// device it opened with. The writer starts it. Called on the writer thread
/// when the output is built and again each time the device changes.
///
/// Uses the default performance mode, not `LowLatency`: a music player wants the
/// normal mixer path, which honors the requested sample rate and resamples to the
/// device's native rate itself — the same thing CoreAudio does on the desktop
/// side. `LowLatency` biases toward an MMAP endpoint locked to the device's
/// native rate, so feeding it source-rate PCM plays at the wrong speed with
/// noise. Low latency is for interactive audio (games, instruments), not
/// playback.
fn open_output_stream(
    sample_rate: u32,
    channels: u32,
) -> Result<AAudioDeviceStream, NdkAudioError> {
    let stream = AudioStreamBuilder::new()?
        .sample_rate(sample_rate as i32)
        .channel_count(channels as i32)
        .format(AudioFormat::PCM_Float)
        .direction(AudioDirection::Output)
        .performance_mode(AudioPerformanceMode::None)
        .open_stream()?;

    info!(
        sample_rate = stream.sample_rate() as u32,
        channels = stream.channel_count() as u32,
        device_id = stream.device_id(),
        "AAudio output stream opened"
    );
    Ok(AAudioDeviceStream { stream, channels })
}

/// What the person is told when the stream for `sample_rate` and `channels`
/// couldn't be opened.
fn open_error(error: NdkAudioError, sample_rate: u32, channels: u32) -> AudioError {
    match error {
        NdkAudioError::InvalidFormat | NdkAudioError::InvalidRate => {
            AudioError::UnsupportedFormat {
                sample_rate,
                channels,
            }
        }
        NdkAudioError::Disconnected => AudioError::DeviceUnavailable,
        other => AudioError::Unexpected(format!("opening the AAudio stream: {other:?}")),
    }
}

/// A disconnect is a change of device; any other AAudio error is the stream
/// failing.
fn stream_error(error: NdkAudioError) -> DeviceStreamError {
    match error {
        NdkAudioError::Disconnected => DeviceStreamError::Disconnected,
        other => DeviceStreamError::Failed(format!("{other:?}")),
    }
}

impl DeviceStream for AAudioDeviceStream {
    fn start(&self) -> Result<(), DeviceStreamError> {
        self.stream.request_start().map_err(|error| match error {
            // AAudio refuses to start a disconnected stream with InvalidState,
            // not Disconnected (`AudioStream::systemStart` checks
            // `isDisconnected()` first). The writer starts only a stream it just
            // opened or stopped itself, which AAudio otherwise starts.
            NdkAudioError::InvalidState => DeviceStreamError::Disconnected,
            other => stream_error(other),
        })
    }

    fn stop(&self) -> Result<(), DeviceStreamError> {
        self.stream.request_stop().map_err(stream_error)
    }

    fn write(&self, samples: &[f32]) -> Result<usize, DeviceStreamError> {
        let frames = (samples.len() / self.channels as usize) as i32;
        // SAFETY: `samples` is a valid f32 slice; AAudio reads `frames *
        // channels` f32 values (PCM_Float, interleaved) from the pointer, which
        // the slice length covers. The stream was opened for PCM_Float with this
        // channel count on this thread.
        let result = unsafe {
            self.stream
                .write(samples.as_ptr() as *const c_void, frames, WRITE_TIMEOUT_NS)
        };
        // ndk 0.9.0 bug: `AAudioStream_write` returns the number of frames
        // written (a positive value) on success, but ndk's wrapper runs that
        // return through its error mapping, which treats every non-zero result as
        // an error — so a successful write surfaces as `Err(__Unknown(frames))`.
        // Every real AAudio error code is negative, so a positive payload is
        // always a frame count, never an error; recover it. Without this every
        // write looks like a failure.
        let written = match result {
            Ok(n) => n as usize,
            Err(NdkAudioError::__Unknown(n)) if n > 0 => n as usize,
            Err(error) => return Err(stream_error(error)),
        };
        Ok(written * self.channels as usize)
    }
}

/// Returned to the service as the `AudioStream`. Its only job is to stop the
/// writer thread when dropped.
struct AAudioStreamHandle {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl AudioStream for AAudioStreamHandle {
    fn play(&self) -> Result<(), AudioError> {
        Ok(()) // The writer thread is already running from create_stream.
    }
}

impl Drop for AAudioStreamHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(handle) = self.thread.take() {
            if let Err(e) = handle.join() {
                warn!("AAudio writer thread panicked during shutdown: {:?}", e);
            }
        }
    }
}

impl AudioOutput for AAudioOutput {
    fn create_stream(
        &mut self,
        source: Arc<Mutex<PlaybackSource>>,
        source_sample_rate: u32,
        source_channels: u32,
        audio_events: AudioEventSender,
        position_update_interval_ms: u32,
    ) -> Result<Box<dyn AudioStream>, AudioError> {
        let controls = self.controls.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_clone = stop.clone();
        // The writer thread opens the stream (AAudio binds it to the opening
        // thread) and says here whether it could, so a stream that can't open
        // fails this build rather than a later write.
        let (opened_tx, opened_rx) = mpsc::sync_channel(1);

        let thread = std::thread::spawn(move || {
            let opened = open_output_stream(source_sample_rate, source_channels)
                .map_err(|error| open_error(error, source_sample_rate, source_channels));
            let stream = match opened {
                Ok(stream) => stream,
                Err(error) => {
                    if opened_tx.send(Err(error)).is_err() {
                        warn!("the AAudio output stopped waiting for its stream to open");
                    }
                    return;
                }
            };
            if opened_tx.send(Ok(())).is_err() {
                warn!("the AAudio output stopped waiting for its stream to open");
                return;
            }

            let drain = AudioDrain::new(
                controls.clone(),
                source,
                audio_events,
                position_update_interval_ms,
            );
            let mut writer = DeviceWriter::new(
                stream,
                move || {
                    open_output_stream(source_sample_rate, source_channels)
                        .map_err(|error| format!("{error:?}"))
                },
                controls,
                drain,
                WRITE_FRAMES * source_channels as usize,
            );
            while !stop_clone.load(Ordering::Acquire) {
                match writer.step() {
                    WriterStep::Wrote => {}
                    WriterStep::Paused => std::thread::sleep(Duration::from_millis(5)),
                    WriterStep::Starved => std::thread::sleep(Duration::from_millis(1)),
                    WriterStep::Ended => break,
                }
            }
        });

        let handle = AAudioStreamHandle {
            stop,
            thread: Some(thread),
        };
        match opened_rx.recv() {
            Ok(Ok(())) => Ok(Box::new(handle)),
            Ok(Err(error)) => Err(error),
            Err(_) => Err(AudioError::Unexpected(
                "the AAudio writer thread exited before opening its stream".to_string(),
            )),
        }
    }

    fn controls(&self) -> &AudioOutputControls {
        &self.controls
    }
}
