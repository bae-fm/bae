//! Platform-neutral audio output seam.
//!
//! Every sink (cpal on desktop, AAudio on Android, the test capture buffer)
//! pulls f32 samples from a [`PlaybackSource`] and sends them somewhere. The
//! [`AudioOutput`]/[`AudioStream`] traits and the shared [`AudioState`] atomic
//! shape are defined here so the concrete sinks in `cpal_output`/`aaudio_output`
//! depend only on this module, not on each other or on cpal.

use crate::playback::source::{PlaybackSource, TrackFmt};
use std::fmt::{Display, Formatter, Result as FmtResult};
use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};
use std::sync::Arc;
use std::sync::Mutex;
use tokio::sync::mpsc as tokio_mpsc;
use tracing::{debug, warn};

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

#[derive(Clone)]
pub(crate) struct AudioOutputControls {
    state: Arc<AtomicU8>,
    volume: Arc<AtomicU32>,
}

impl AudioOutputControls {
    pub(crate) fn new(initial_volume: u32) -> Self {
        Self {
            state: Arc::new(AtomicU8::new(AudioState::Stopped as u8)),
            volume: Arc::new(AtomicU32::new(initial_volume)),
        }
    }

    pub(crate) fn set_state(&self, new_state: AudioState) {
        self.state.store(new_state as u8, Ordering::Relaxed);
    }

    pub(crate) fn get_state(&self) -> AudioState {
        AudioState::from_u8(self.state.load(Ordering::Relaxed))
    }

    pub(crate) fn set_volume(&self, volume: f32) {
        self.volume
            .store((volume.clamp(0.0, 1.0) * 10000.0) as u32, Ordering::Relaxed);
    }

    pub(crate) fn get_volume(&self) -> f32 {
        self.volume.load(Ordering::Relaxed) as f32 / 10000.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DrainStatus {
    Idle,
    Samples { read: usize },
    Completed,
}

#[cfg(feature = "test-utils")]
pub(crate) fn sleep_realtime_buffer(read: usize, channels: u32, sample_rate: u32) {
    let frames = (read as u32 / channels) as f64;
    std::thread::sleep(std::time::Duration::from_secs_f64(
        frames / sample_rate as f64,
    ));
}

pub(crate) struct AudioLockMissLog {
    started: Option<std::time::Instant>,
    last_log: Option<std::time::Instant>,
}

impl AudioLockMissLog {
    pub(crate) fn new() -> Self {
        Self {
            started: None,
            last_log: None,
        }
    }

    fn acquired(&mut self) {
        if let Some(started) = self.started.take() {
            let missed = started.elapsed();
            if missed >= std::time::Duration::from_millis(250) {
                debug!(
                    missed_ms = missed.as_millis(),
                    "audio callback reacquired playback source lock"
                );
            }
            self.last_log = None;
        }
    }

    fn missed(&mut self) {
        let now = std::time::Instant::now();
        let started = *self.started.get_or_insert(now);
        let missed = now.duration_since(started);
        if missed >= std::time::Duration::from_millis(250)
            && self
                .last_log
                .is_none_or(|last| now.duration_since(last) >= std::time::Duration::from_secs(1))
        {
            warn!(
                missed_ms = missed.as_millis(),
                "audio callback could not lock playback source while playing"
            );
            self.last_log = Some(now);
        }
    }
}

pub(crate) struct AudioDrain {
    controls: AudioOutputControls,
    source: Arc<Mutex<PlaybackSource>>,
    position_tx: tokio_mpsc::UnboundedSender<PositionEvent>,
    completion_tx: tokio_mpsc::UnboundedSender<CompletionEvent>,
    position_update_interval: std::time::Duration,
    last_position_update: std::time::Instant,
    completion_sent: bool,
}

impl AudioDrain {
    pub(crate) fn new(
        controls: AudioOutputControls,
        source: Arc<Mutex<PlaybackSource>>,
        position_tx: tokio_mpsc::UnboundedSender<PositionEvent>,
        completion_tx: tokio_mpsc::UnboundedSender<CompletionEvent>,
        position_update_interval_ms: u32,
    ) -> Self {
        Self {
            controls,
            source,
            position_tx,
            completion_tx,
            position_update_interval: std::time::Duration::from_millis(
                position_update_interval_ms as u64,
            ),
            last_position_update: std::time::Instant::now(),
            completion_sent: false,
        }
    }

    pub(crate) fn drain_iteration(
        &mut self,
        buf: &mut [f32],
        apply_gain: bool,
        zero_unfilled: bool,
        lock_miss: Option<&mut AudioLockMissLog>,
    ) -> DrainStatus {
        if self.controls.get_state() != AudioState::Playing {
            if zero_unfilled {
                buf.fill(0.0);
            }
            return DrainStatus::Idle;
        }

        let mut source_guard = match self.source.try_lock() {
            Ok(guard) => {
                if let Some(lock_miss) = lock_miss {
                    lock_miss.acquired();
                }
                guard
            }
            Err(_) => {
                if let Some(lock_miss) = lock_miss {
                    lock_miss.missed();
                }
                if zero_unfilled {
                    buf.fill(0.0);
                }
                return DrainStatus::Idle;
            }
        };

        let read = source_guard.pull_samples(buf);

        if read == 0 {
            if source_guard.is_finished() && !self.completion_sent {
                self.controls.set_state(AudioState::Stopped);
                if self
                    .completion_tx
                    .send(source_guard.completion_event())
                    .is_err()
                {
                    warn!("Failed to send completion signal");
                }
                self.completion_sent = true;
            }
            if zero_unfilled {
                buf.fill(0.0);
            }
            return if self.completion_sent {
                DrainStatus::Completed
            } else {
                DrainStatus::Idle
            };
        }

        if apply_gain {
            let combined = source_guard.current_replay_gain_linear() * self.controls.get_volume();
            for sample in &mut buf[..read] {
                *sample *= combined;
            }
        }
        if zero_unfilled {
            buf[read..].fill(0.0);
        }

        if self.last_position_update.elapsed() >= self.position_update_interval {
            if self
                .position_tx
                .send(source_guard.position_event())
                .is_err()
            {
                debug!("Position tick: receiver dropped");
            }
            self.last_position_update = std::time::Instant::now();
        }

        DrainStatus::Samples { read }
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
        position_tx: tokio_mpsc::UnboundedSender<PositionEvent>,
        completion_tx: tokio_mpsc::UnboundedSender<CompletionEvent>,
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
    controls: AudioOutputControls,
    notify_tx: tokio::sync::mpsc::UnboundedSender<Arc<Mutex<Vec<f32>>>>,
    /// When true, the drain paces itself to real time (sleeping one buffer's
    /// wall-clock duration after each pull) instead of draining at full speed.
    /// Full-speed draining lets the decoder run a whole track and gaplessly
    /// advance before a test can issue a follow-up command (e.g. a seek); a
    /// real-time drain keeps playback on the track under test, the way the cpal
    /// sink does in production. Tests that just want samples fast use `new()`.
    realtime: bool,
}

#[cfg(feature = "test-utils")]
impl CaptureAudioOutput {
    /// Returns the output and a receiver that yields one buffer per
    /// `create_stream` call, in creation order.
    pub fn new() -> (
        Self,
        tokio::sync::mpsc::UnboundedReceiver<Arc<Mutex<Vec<f32>>>>,
    ) {
        Self::with_pacing(false)
    }

    /// Like `new()`, but the drain paces to real time. Use for tests whose
    /// follow-up commands (seek, pause) must land on the track being played
    /// rather than racing a full-speed decode that has already advanced.
    pub fn new_realtime() -> (
        Self,
        tokio::sync::mpsc::UnboundedReceiver<Arc<Mutex<Vec<f32>>>>,
    ) {
        Self::with_pacing(true)
    }

    fn with_pacing(
        realtime: bool,
    ) -> (
        Self,
        tokio::sync::mpsc::UnboundedReceiver<Arc<Mutex<Vec<f32>>>>,
    ) {
        let (notify_tx, notify_rx) = tokio::sync::mpsc::unbounded_channel();
        let output = Self {
            controls: AudioOutputControls::new(10000),
            notify_tx,
            realtime,
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
        source_sample_rate: u32,
        source_channels: u32,
        position_tx: tokio_mpsc::UnboundedSender<PositionEvent>,
        completion_tx: tokio_mpsc::UnboundedSender<CompletionEvent>,
        position_update_interval_ms: u32,
    ) -> Result<Box<dyn AudioStream>, AudioError> {
        let captured = Arc::new(Mutex::new(Vec::<f32>::new()));
        let _ = self.notify_tx.send(captured.clone());

        let controls = self.controls.clone();
        let realtime = self.realtime;
        let channels = source_channels.max(1);
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop_clone = stop.clone();

        let thread = std::thread::spawn(move || {
            let mut buf = vec![0.0f32; 4096];
            let mut drain = AudioDrain::new(
                controls,
                source,
                position_tx,
                completion_tx,
                position_update_interval_ms,
            );

            loop {
                if stop_clone.load(Ordering::Acquire) {
                    break;
                }

                match drain.drain_iteration(&mut buf, false, false, None) {
                    DrainStatus::Idle => {
                        std::thread::sleep(std::time::Duration::from_millis(1));
                        continue;
                    }
                    DrainStatus::Completed => {
                        break;
                    }
                    DrainStatus::Samples { read } => {
                        if let Ok(mut guard) = captured.lock() {
                            guard.extend_from_slice(&buf[..read]);
                        } else {
                            break;
                        }

                        if realtime {
                            sleep_realtime_buffer(read, channels, source_sample_rate);
                        }
                    }
                }
            }
        });

        Ok(Box::new(CaptureStream {
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

    fn set_volume(&self, _volume: f32) {
        // Capture output ignores volume — raw samples
    }

    fn get_volume(&self) -> f32 {
        1.0 // Capture output has no volume control
    }
}

// -- Real-time CPU probe output for tests --

/// Audio output that drives playback at real time and discards the samples, for
/// measuring steady-state playback CPU. It runs the same per-buffer work the
/// cpal sink does — `pull_samples`, the replay-gain/volume multiply, position
/// ticks, completion — but paces itself by sleeping one buffer's wall-clock
/// duration after each pull instead of being clocked by an audio device.
///
/// That real-time pacing is the point: it makes the decoder fill-then-park as it
/// does in production, so the measured CPU reflects real playback (per-buffer
/// drain, gain, ~20 Hz tick fan-out, decoder wakeups) rather than a full-speed
/// decode, which is an order of magnitude cheaper. Needs no audio device, so the
/// measurement runs headless on any platform.
#[cfg(feature = "test-utils")]
pub struct RealtimeProbeOutput {
    controls: AudioOutputControls,
}

#[cfg(feature = "test-utils")]
impl RealtimeProbeOutput {
    pub fn new() -> Self {
        Self {
            controls: AudioOutputControls::new(10000),
        }
    }
}

#[cfg(feature = "test-utils")]
impl Default for RealtimeProbeOutput {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "test-utils")]
impl AudioOutput for RealtimeProbeOutput {
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
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop_clone = stop.clone();
        let channels = source_channels.max(1);

        let thread = std::thread::spawn(move || {
            // 1024 frames/buffer — the pacing granularity, not the total work.
            let mut buf = vec![0.0f32; 1024 * channels as usize];
            let mut drain = AudioDrain::new(
                controls,
                source,
                position_tx,
                completion_tx,
                position_update_interval_ms,
            );

            loop {
                if stop_clone.load(Ordering::Acquire) {
                    break;
                }

                match drain.drain_iteration(&mut buf, true, false, None) {
                    DrainStatus::Idle => {
                        std::thread::sleep(std::time::Duration::from_millis(1));
                        continue;
                    }
                    DrainStatus::Completed => {
                        break;
                    }
                    DrainStatus::Samples { read } => {
                        sleep_realtime_buffer(read, channels, source_sample_rate);
                    }
                }
            }
        });

        Ok(Box::new(CaptureStream {
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
