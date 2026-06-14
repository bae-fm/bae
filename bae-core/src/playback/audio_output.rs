//! Platform-neutral audio output seam.
//!
//! Every sink (cpal on desktop, AAudio on Android, the test capture buffer)
//! pulls f32 samples from a [`PlaybackSource`] and sends them somewhere. The
//! [`AudioOutput`]/[`AudioStream`] traits and the shared [`AudioState`] atomic
//! shape are defined here so the concrete sinks in `cpal_output`/`aaudio_output`
//! depend only on this module, not on each other or on cpal.

use crate::playback::source::{PlaybackSource, TrackFmt};
use std::fmt::{Display, Formatter, Result as FmtResult};
#[cfg(feature = "test-utils")]
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::sync::{mpsc, Mutex};

/// Audio output state - directly controls what the audio callback does.
///
/// This is a shared atomic that both the service and audio callback access.
/// No command channel needed - just set the state directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum AudioState {
    /// Output silence, stream inactive
    Stopped = 0,
    /// Output samples from buffer
    Playing = 1,
    /// Output silence, but stream remains ready
    Paused = 2,
}

impl AudioState {
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => AudioState::Playing,
            2 => AudioState::Paused,
            _ => AudioState::Stopped,
        }
    }
}

#[derive(Debug)]
pub enum AudioError {
    DeviceNotFound,
    StreamConfigError(String),
    StreamBuildError(String),
}
impl Display for AudioError {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            AudioError::DeviceNotFound => write!(f, "Audio device not found"),
            AudioError::StreamConfigError(msg) => {
                write!(f, "Stream config error: {}", msg)
            }
            AudioError::StreamBuildError(msg) => write!(f, "Stream build error: {}", msg),
        }
    }
}
impl std::error::Error for AudioError {}

/// A running audio stream. Drop to stop.
pub trait AudioStream: 'static {
    fn play(&self) -> Result<(), AudioError>;
}

/// A position tick, tagged with the formatting context of the track the
/// position belongs to. The audio callback supplies the fmt by cloning the
/// source's `current_fmt` at emit time — across a track boundary the next
/// tick automatically carries the new track's fmt.
pub type PositionEvent = (Arc<TrackFmt>, std::time::Duration);

/// A completion signal, tagged with the finishing track's identity + decode
/// stats. Fires once when the source's last track drains; gaplessly-advanced
/// tracks signal via the boundary channel instead.
pub type CompletionEvent = (Arc<TrackFmt>, u32, u64);

/// Audio output abstraction. Implementations pull samples from a PlaybackSource
/// and send them somewhere (real device, capture buffer, etc).
pub trait AudioOutput: Send + 'static {
    fn create_stream(
        &mut self,
        source: Arc<Mutex<PlaybackSource>>,
        source_sample_rate: u32,
        source_channels: u32,
        position_tx: mpsc::Sender<PositionEvent>,
        completion_tx: mpsc::Sender<CompletionEvent>,
        position_update_interval_ms: u32,
    ) -> Result<Box<dyn AudioStream>, AudioError>;

    fn set_state(&self, state: AudioState);
    fn get_state(&self) -> AudioState;
    fn is_paused(&self) -> bool {
        self.get_state() == AudioState::Paused
    }
    fn set_volume(&self, volume: f32);
    fn get_volume(&self) -> f32;
}

// -- Capture audio output for tests --

/// Audio output that captures raw f32 samples for ground truth comparison in tests.
///
/// Each call to `create_stream` mints a fresh capture buffer; the receiver
/// returned from `new()` yields one `Arc<Mutex<Vec<f32>>>` per stream in
/// creation order. Volume is NOT applied — samples are captured exactly as
/// the decoder produced them.
#[cfg(feature = "test-utils")]
pub struct CaptureAudioOutput {
    state: Arc<AtomicU8>,
    notify_tx: tokio::sync::mpsc::UnboundedSender<Arc<Mutex<Vec<f32>>>>,
}

#[cfg(feature = "test-utils")]
impl CaptureAudioOutput {
    /// Returns the output and a receiver that yields one buffer per
    /// `create_stream` call, in creation order.
    pub fn new() -> (
        Self,
        tokio::sync::mpsc::UnboundedReceiver<Arc<Mutex<Vec<f32>>>>,
    ) {
        let (notify_tx, notify_rx) = tokio::sync::mpsc::unbounded_channel();
        let output = Self {
            state: Arc::new(AtomicU8::new(AudioState::Stopped as u8)),
            notify_tx,
        };
        (output, notify_rx)
    }
}

/// Polls `buffer` every 50 ms until it holds at least `at_least` samples or
/// `timeout` elapses. Returns the snapshot at exit so the caller can assert
/// against whatever was captured.
#[cfg(feature = "test-utils")]
pub async fn wait_for_samples(
    buffer: &Arc<Mutex<Vec<f32>>>,
    at_least: usize,
    timeout: std::time::Duration,
) -> Vec<f32> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        {
            let guard = buffer.lock().unwrap();
            if guard.len() >= at_least {
                return guard.clone();
            }
        }
        if std::time::Instant::now() >= deadline {
            return buffer.lock().unwrap().clone();
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

#[cfg(feature = "test-utils")]
struct CaptureStream {
    stop: Arc<std::sync::atomic::AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

#[cfg(feature = "test-utils")]
impl AudioStream for CaptureStream {
    fn play(&self) -> Result<(), AudioError> {
        Ok(()) // Already running from create_stream
    }
}

#[cfg(feature = "test-utils")]
impl Drop for CaptureStream {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(handle) = self.thread.take() {
            if let Err(e) = handle.join() {
                tracing::warn!("capture audio thread panicked during shutdown: {:?}", e);
            }
        }
    }
}

#[cfg(feature = "test-utils")]
impl AudioOutput for CaptureAudioOutput {
    fn create_stream(
        &mut self,
        source: Arc<Mutex<PlaybackSource>>,
        _source_sample_rate: u32,
        _source_channels: u32,
        position_tx: mpsc::Sender<PositionEvent>,
        completion_tx: mpsc::Sender<CompletionEvent>,
        position_update_interval_ms: u32,
    ) -> Result<Box<dyn AudioStream>, AudioError> {
        let captured = Arc::new(Mutex::new(Vec::<f32>::new()));
        let _ = self.notify_tx.send(captured.clone());

        let state = self.state.clone();
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop_clone = stop.clone();

        let thread = std::thread::spawn(move || {
            let mut buf = vec![0.0f32; 4096];
            let position_update_interval =
                std::time::Duration::from_millis(position_update_interval_ms as u64);
            let mut last_position_update = std::time::Instant::now();
            let mut completion_sent = false;

            loop {
                if stop_clone.load(Ordering::Acquire) {
                    break;
                }

                let current_state = AudioState::from_u8(state.load(Ordering::Relaxed));
                if current_state != AudioState::Playing {
                    std::thread::sleep(std::time::Duration::from_millis(1));
                    continue;
                }

                let mut source_guard = match source.try_lock() {
                    Ok(guard) => guard,
                    Err(_) => {
                        std::thread::sleep(std::time::Duration::from_millis(1));
                        continue;
                    }
                };

                let read = source_guard.pull_samples(&mut buf);

                if read == 0 {
                    if source_guard.is_finished() && !completion_sent {
                        state.store(AudioState::Stopped as u8, Ordering::Relaxed);
                        if completion_tx.send(source_guard.completion_event()).is_err() {
                            tracing::warn!("Failed to send completion signal");
                        }
                        completion_sent = true;
                    }
                    drop(source_guard);
                    if completion_sent {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(1));
                    continue;
                }

                // Capture raw samples (no volume applied)
                if let Ok(mut guard) = captured.lock() {
                    guard.extend_from_slice(&buf[..read]);
                } else {
                    break; // Test ended, mutex poisoned
                }

                if last_position_update.elapsed() >= position_update_interval {
                    if position_tx.send(source_guard.position_event()).is_err() {
                        tracing::debug!("Position tick: receiver dropped");
                    }
                    last_position_update = std::time::Instant::now();
                }
            }
        });

        Ok(Box::new(CaptureStream {
            stop,
            thread: Some(thread),
        }))
    }

    fn set_state(&self, new_state: AudioState) {
        self.state.store(new_state as u8, Ordering::Relaxed);
    }

    fn get_state(&self) -> AudioState {
        AudioState::from_u8(self.state.load(Ordering::Relaxed))
    }

    fn set_volume(&self, _volume: f32) {
        // Capture output ignores volume — raw samples
    }

    fn get_volume(&self) -> f32 {
        1.0 // Capture output has no volume control
    }
}
