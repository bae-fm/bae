//! AAudio output sink for Android.
//!
//! The in-core decode path produces interleaved f32 PCM; this sink writes it to
//! an AAudio output stream in blocking mode from a dedicated writer thread. The
//! thread owns the [`ndk::audio::AudioStream`] for its whole lifetime — AAudio
//! streams are bound to the thread that opens them, so the stream is constructed
//! inside the spawned thread and never moved across threads after creation. This
//! mirrors [`super::audio_output::CaptureAudioOutput`]'s pull loop, but writes to
//! the device instead of a capture buffer.

use super::audio_output::{
    AudioDrain, AudioError, AudioOutput, AudioOutputControls, AudioState, AudioStream,
    CompletionEvent, DrainStatus, PositionEvent,
};
use crate::playback::source::PlaybackSource;
use ndk::audio::{
    AudioDirection, AudioError as NdkAudioError, AudioFormat, AudioPerformanceMode,
    AudioStream as NdkAudioStream, AudioStreamBuilder,
};
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc as tokio_mpsc;
use tracing::{debug, error, info, warn};

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
            controls: AudioOutputControls::new(10000),
        })
    }
}

/// Build and start an AAudio output stream for the given format, logging the
/// rate and channel count the stream actually opened with. Called on the writer
/// thread at startup and again to recover from a disconnect/route change.
///
/// Uses the default performance mode, not `LowLatency`: a music player wants the
/// normal mixer path, which honors the requested sample rate and resamples to the
/// device's native rate itself — the same thing CoreAudio does on the desktop
/// side. `LowLatency` biases toward an MMAP endpoint locked to the device's
/// native rate, so feeding it source-rate PCM plays at the wrong speed with
/// noise. Low latency is for interactive audio (games, instruments), not
/// playback.
fn open_output_stream(sample_rate: u32, channels: u32) -> Result<NdkAudioStream, NdkAudioError> {
    let stream = AudioStreamBuilder::new()?
        .sample_rate(sample_rate as i32)
        .channel_count(channels as i32)
        .format(AudioFormat::PCM_Float)
        .direction(AudioDirection::Output)
        .performance_mode(AudioPerformanceMode::None)
        .open_stream()?;
    stream.request_start()?;

    info!(
        sample_rate = stream.sample_rate() as u32,
        channels = stream.channel_count() as u32,
        "AAudio output stream opened"
    );
    Ok(stream)
}

/// Write `samples` (interleaved f32 across `channels`) to the stream in blocking
/// mode. Returns `Ok` once the whole buffer is consumed, or the AAudio error
/// that interrupted the write (e.g. `Disconnected` on a route change).
fn write_all(
    stream: &NdkAudioStream,
    samples: &[f32],
    channels: u32,
    stop: &AtomicBool,
) -> Result<(), NdkAudioError> {
    let mut offset = 0usize;
    while offset < samples.len() {
        // Bail mid-buffer on shutdown so drop()/join() can't wait a whole
        // buffer (or longer, on a slow device) for the writer to return.
        if stop.load(Ordering::Acquire) {
            break;
        }
        let remaining = &samples[offset..];
        let frames = (remaining.len() / channels as usize) as i32;
        if frames == 0 {
            break;
        }
        // SAFETY: `remaining` is a valid f32 slice; AAudio reads `frames *
        // channels` f32 values (PCM_Float, interleaved) from the pointer, which
        // the slice length covers. The stream was opened for PCM_Float with this
        // channel count on this thread.
        let result = unsafe {
            stream.write(
                remaining.as_ptr() as *const c_void,
                frames,
                WRITE_TIMEOUT_NS,
            )
        };
        // ndk 0.9.0 bug: `AAudioStream_write` returns the number of frames
        // written (a positive value) on success, but ndk's wrapper runs that
        // return through its error mapping, which treats every non-zero result as
        // an error — so a successful write surfaces as `Err(__Unknown(frames))`.
        // Every real AAudio error code is negative, so a positive payload is
        // always a frame count, never an error; recover it. Without this every
        // write looks like a failure, so the writer tears the stream down and
        // rebuilds it on each buffer (~50x/second), which is heard as static.
        let written = match result {
            Ok(n) => n,
            Err(NdkAudioError::__Unknown(n)) if n > 0 => n as u32,
            Err(e) => return Err(e),
        };
        // A zero-frame return means the blocking write made no progress within
        // the timeout (a stalled stream), not an error. Re-issuing it in a tight
        // loop would spin forever and never re-check `stop`, wedging shutdown.
        // Bail to the outer loop instead, which re-checks stop/state and rebuilds
        // or exits; the unwritten tail of this buffer is dropped (a brief glitch
        // on a stalled device, far better than hanging the writer thread).
        if written == 0 {
            break;
        }
        offset += written as usize * channels as usize;
    }
    Ok(())
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
        position_tx: tokio_mpsc::UnboundedSender<PositionEvent>,
        completion_tx: tokio_mpsc::UnboundedSender<CompletionEvent>,
        position_update_interval_ms: u32,
    ) -> Result<Box<dyn AudioStream>, AudioError> {
        let controls = self.controls.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_clone = stop.clone();

        let thread = std::thread::spawn(move || {
            // Construct-and-own the AAudio stream on this thread; AAudio binds a
            // stream to its opening thread, so it is never moved out of here.
            let mut stream = match open_output_stream(source_sample_rate, source_channels) {
                Ok(s) => Some(s),
                Err(e) => {
                    error!("Failed to open AAudio output stream: {:?}", e);
                    None
                }
            };

            let mut buf = vec![0.0f32; WRITE_FRAMES * source_channels as usize];
            let mut drain = AudioDrain::new(
                controls.clone(),
                source,
                position_tx,
                completion_tx,
                position_update_interval_ms,
            );
            // Tracks whether the device stream is started; pause/stop idles it
            // and the next Playing iteration restarts it.
            let mut stream_started = stream.is_some();

            loop {
                if stop_clone.load(Ordering::Acquire) {
                    break;
                }

                if controls.get_state() != AudioState::Playing {
                    // Idle the device while paused/stopped so it doesn't drain
                    // the (now silent) buffer and burn power.
                    if stream_started {
                        if let Some(s) = &stream {
                            if let Err(e) = s.request_stop() {
                                warn!("AAudio request_stop failed while idling: {:?}", e);
                            }
                        }
                        stream_started = false;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(5));
                    continue;
                }

                // Rebuild the stream if a prior error tore it down.
                if stream.is_none() {
                    match open_output_stream(source_sample_rate, source_channels) {
                        Ok(s) => {
                            stream = Some(s);
                            stream_started = true;
                        }
                        Err(e) => {
                            warn!("AAudio stream rebuild failed; retrying: {:?}", e);
                            std::thread::sleep(std::time::Duration::from_millis(20));
                            continue;
                        }
                    }
                } else if !stream_started {
                    if let Some(s) = &stream {
                        if let Err(e) = s.request_start() {
                            warn!("AAudio request_start failed: {:?}", e);
                            std::thread::sleep(std::time::Duration::from_millis(20));
                            continue;
                        }
                    }
                    stream_started = true;
                }

                let read = match drain.drain_iteration(&mut buf, true, false, None) {
                    DrainStatus::Samples { read } => read,
                    DrainStatus::Completed => {
                        info!("AAudio writer: end of stream");
                        break;
                    }
                    DrainStatus::Idle => {
                        std::thread::sleep(std::time::Duration::from_millis(1));
                        continue;
                    }
                };

                if let Some(s) = &stream {
                    if let Err(e) = write_all(s, &buf[..read], source_channels, &stop_clone) {
                        // Disconnect (headphones (un)plugged, route change) and
                        // other stream errors are recoverable by rebuilding on
                        // the next iteration.
                        warn!("AAudio write failed; rebuilding stream: {:?}", e);
                        stream = None;
                        stream_started = false;
                    }
                }
            }

            if let Some(s) = &stream {
                if let Err(e) = s.request_stop() {
                    debug!("AAudio request_stop failed during teardown: {:?}", e);
                }
            }
        });

        Ok(Box::new(AAudioStreamHandle {
            stop,
            thread: Some(thread),
        }))
    }

    fn set_state(&self, new_state: AudioState) {
        self.controls.set_state(new_state);
    }

    fn get_state(&self) -> AudioState {
        self.controls.get_state()
    }

    fn set_volume(&self, volume: f32) {
        self.controls.set_volume(volume);
    }

    fn get_volume(&self) -> f32 {
        self.controls.get_volume()
    }
}
