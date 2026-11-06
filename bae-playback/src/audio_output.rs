//! Platform-neutral audio output seam.
//!
//! Every sink (cpal on desktop, AAudio on Android, the test capture buffer)
//! pulls f32 samples from a [`PlaybackSource`] and sends them somewhere. The
//! [`AudioOutput`]/[`AudioStream`] traits and the shared [`AudioState`] atomic
//! shape are defined here so the concrete sinks in `cpal_output`/`aaudio_output`
//! depend only on this module, not on each other or on cpal.

use crate::source::{PlaybackSource, TrackCrossing, TrackFmt};
use bae_audio::stream_wake::StreamWake;
use rtrb::{Consumer, Producer, RingBuffer};
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::sync::{Mutex, PoisonError};

use bae_util::task_failure::caught_sync;

const AUDIO_EVENT_CAPACITY: usize = 2048;

/// What the audio callback does each buffer. Held as a shared atomic the service
/// writes and the callback reads, so play/pause needs no command channel — and
/// nothing on the callback path blocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum AudioState {
    /// Silence; the stream is inactive.
    Stopped = 0,
    /// Pull samples from the source.
    Playing = 1,
    /// Silence, but the stream stays ready.
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
pub struct AudioOutputControls {
    state: Arc<AtomicU8>,
    volume: Arc<AtomicF32>,
    /// What a drain on its own thread waits on while it has nothing to do. A
    /// state change notifies it, and so does every track its source holds.
    wake: StreamWake,
}

impl AudioOutputControls {
    pub(crate) fn new(initial_volume: crate::Volume) -> Self {
        Self {
            state: Arc::new(AtomicU8::new(AudioState::Stopped as u8)),
            volume: Arc::new(AtomicF32::new(initial_volume.get())),
            wake: StreamWake::new(),
        }
    }

    /// Set the state and wake a drain waiting for it. Never from the
    /// real-time callback: the wake takes a lock.
    pub(crate) fn set_state(&self, new_state: AudioState) {
        self.state.store(new_state as u8, Ordering::Relaxed);
        self.wake.notify();
    }

    /// The drain stopping its own player when the last track drained. It is
    /// the only waiter, so nothing is woken, and the real-time callback may
    /// call it.
    fn stop_from_drain(&self) {
        self.state.store(AudioState::Stopped as u8, Ordering::Relaxed);
    }

    /// Have `source`'s tracks, and each one placed in it, wake a drain
    /// waiting on these controls.
    fn wake_drain_of(&self, source: &mut PlaybackSource) {
        source.wake_on_change(self.wake.clone());
    }

    /// The changes counted so far that could give this player's drain work.
    /// A drain on its own thread reads it before it looks for work.
    #[cfg(any(target_os = "android", test, feature = "test-utils"))]
    pub(crate) fn drain_changes(&self) -> u64 {
        self.wake.changes()
    }

    /// Wait until a change past `seen` could give the drain work.
    #[cfg(any(target_os = "android", test, feature = "test-utils"))]
    pub(crate) fn wait_for_drain_work(&self, seen: u64) {
        self.wake.wait_past(seen);
    }

    /// Wake a waiting drain: its owner stopped it.
    #[cfg(any(target_os = "android", test, feature = "test-utils"))]
    pub(crate) fn wake_drain(&self) {
        self.wake.notify();
    }

    /// Test-only: wait until a drain is waiting for work.
    #[cfg(test)]
    pub(crate) fn wait_until_drain_waits(&self) {
        self.wake.wait_until_waiting();
    }

    pub(crate) fn get_state(&self) -> AudioState {
        AudioState::from_u8(self.state.load(Ordering::Relaxed))
    }

    pub(crate) fn set_volume(&self, volume: crate::Volume) {
        self.volume.store(volume.get());
    }

    pub(crate) fn get_volume(&self) -> f32 {
        self.volume.load()
    }
}

struct AtomicF32 {
    bits: AtomicU32,
}

impl AtomicF32 {
    fn new(value: f32) -> Self {
        Self { bits: AtomicU32::new(value.to_bits()) }
    }

    fn store(&self, value: f32) {
        self.bits.store(value.to_bits(), Ordering::Relaxed);
    }

    fn load(&self) -> f32 {
        f32::from_bits(self.bits.load(Ordering::Relaxed))
    }
}

/// How a drain takes the source's lock.
pub(crate) enum SourceLock<'a> {
    /// A drain on its own thread waits while the service holds the lock.
    Wait,
    /// The real-time callback never waits: a miss yields nothing this buffer,
    /// and is logged.
    Try(&'a mut AudioLockMissLog),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DrainStatus {
    Idle,
    Samples { read: usize },
}

#[cfg(any(test, feature = "test-utils"))]
pub(crate) fn sleep_realtime_buffer(read: usize, channels: u32, sample_rate: u32) {
    let frames = (read / channels as usize) as u64;
    std::thread::sleep(bae_util::sample_time::frame_start(frames, sample_rate));
}

/// The audio callback missing the source's lock, as it reports it.
pub(crate) struct AudioLockMissLog {
    stall: bae_util::stall::Stall,
}

impl AudioLockMissLog {
    #[cfg(not(target_os = "android"))]
    pub(crate) fn new() -> Self {
        Self { stall: bae_util::stall::Stall::default() }
    }

    fn acquired(&mut self, audio_events: &mut AudioEventSender) {
        if let Some(missed) = self.stall.ended(std::time::Instant::now()) {
            audio_events
                .push(AudioEvent::SourceLockReacquired { missed_ms: duration_millis(missed) });
        }
    }

    fn missed(&mut self, audio_events: &mut AudioEventSender) {
        if let Some(missed) = self.stall.waiting(std::time::Instant::now()) {
            audio_events.push(AudioEvent::SourceLockMissed { missed_ms: duration_millis(missed) });
        }
    }
}

pub(crate) struct AudioDrain {
    controls: AudioOutputControls,
    source: Arc<Mutex<PlaybackSource>>,
    audio_events: AudioEventSender,
    position_update_interval: std::time::Duration,
    last_position_update: std::time::Instant,
    /// Set once a pull from the source panics; from then on the drain yields
    /// nothing and never touches the source again.
    failed: bool,
}

impl AudioDrain {
    pub(crate) fn new(
        controls: AudioOutputControls,
        source: Arc<Mutex<PlaybackSource>>,
        audio_events: AudioEventSender,
        position_update_interval_ms: u32,
    ) -> Self {
        controls.wake_drain_of(&mut source.lock().unwrap_or_else(PoisonError::into_inner));
        Self {
            controls,
            source,
            audio_events,
            position_update_interval: std::time::Duration::from_millis(
                position_update_interval_ms as u64,
            ),
            last_position_update: std::time::Instant::now(),
            failed: false,
        }
    }

    /// Tell playback the output stopped sending, for the reason `detail` states.
    pub(crate) fn report_output_failure(&mut self, detail: String) {
        self.audio_events.push_required(AudioEvent::OutputFailed { detail });
    }

    pub(crate) fn drain_iteration(
        &mut self,
        buf: &mut [f32],
        apply_gain: bool,
        zero_unfilled: bool,
        lock: SourceLock<'_>,
    ) -> DrainStatus {
        if self.failed || self.controls.get_state() != AudioState::Playing {
            if zero_unfilled {
                buf.fill(0.0);
            }
            return DrainStatus::Idle;
        }

        let locked = match lock {
            SourceLock::Wait => self.source.lock().map_err(|poisoned| {
                // A panic while the lock was held leaves the source in an
                // unknown state: stop reading it, and say why.
                self.failed = true;
                self.audio_events
                    .push_required(AudioEvent::OutputFailed { detail: poisoned.to_string() });
            }),
            SourceLock::Try(lock_miss) => match self.source.try_lock() {
                Ok(guard) => {
                    lock_miss.acquired(&mut self.audio_events);
                    Ok(guard)
                }
                Err(_) => {
                    lock_miss.missed(&mut self.audio_events);
                    Err(())
                }
            },
        };
        let Ok(mut source_guard) = locked else {
            if zero_unfilled {
                buf.fill(0.0);
            }
            return DrainStatus::Idle;
        };

        // Caught while the guard is held, so a panicking pull leaves the
        // source's lock unpoisoned for the playback service, which stops on
        // the failure and discards the source.
        let pulled = caught_sync("pull samples from the playing track", || {
            source_guard.pull_samples(buf, &mut self.audio_events)
        });
        let read = match pulled {
            Ok(read) => read,
            Err(failed) => {
                self.failed = true;
                self.audio_events
                    .push_required(AudioEvent::OutputFailed { detail: failed.to_string() });
                if zero_unfilled {
                    buf.fill(0.0);
                }
                return DrainStatus::Idle;
            }
        };

        if read == 0 {
            // One Completion per drained track: the source's latch resets on a
            // crossing or a `replace`, so a persistent stream playing many tracks
            // still reports each one exactly once.
            if let Some(event) = source_guard.take_completion_event() {
                self.controls.stop_from_drain();
                self.audio_events.push_required(AudioEvent::Completion(event));
            }
            if zero_unfilled {
                buf.fill(0.0);
            }
            return DrainStatus::Idle;
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
            self.audio_events.push(AudioEvent::Position(source_guard.position_event()));
            self.last_position_update = std::time::Instant::now();
        }

        DrainStatus::Samples { read }
    }
}

/// Why an audio output couldn't start or keep playing. The first three are the
/// device's or the speaker's doing, which the person can act on; `Unexpected`
/// is bae's own failure, a bug.
#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    /// There is no output device, or the one there was went away: unplugged,
    /// switched off, or taken by the system or another app.
    #[error("the audio device is unavailable")]
    DeviceUnavailable,
    /// The device doesn't take the track's sample rate and channel count.
    #[error("the audio device can't play {sample_rate} Hz with {channels} channels")]
    UnsupportedFormat { sample_rate: u32, channels: u32 },
    /// The AirPlay speaker couldn't be reached or didn't accept the session.
    #[error("the AirPlay speaker didn't accept the connection: {0}")]
    AirPlayRefused(String),
    /// Anything else: a failure bae didn't expect from its own audio path.
    #[error("{0}")]
    Unexpected(String),
}

/// A running audio stream. Drop to stop.
pub trait AudioStream: 'static {
    fn play(&self) -> Result<(), AudioError>;

    /// The device's report, if it made one, that this stream can't go on as
    /// built: the system routed audio somewhere the stream doesn't follow, or
    /// the stream's device went away. The owner replaces the stream with a new
    /// one, which plays on the current default device or fails to build when
    /// there is none. Each report is taken once.
    ///
    /// Only a cpal stream makes such a report. The AAudio writer reopens its
    /// device stream itself, and a receiver or a test capture has no device to
    /// lose, so they report nothing.
    fn take_invalidation(&self) -> Option<String> {
        None
    }
}

/// A position tick, tagged with the formatting context of the track the
/// position belongs to. The audio callback supplies the fmt by cloning the
/// source's `current_fmt` at emit time — across a track boundary the next
/// tick automatically carries the new track's fmt.
pub type PositionEvent = (Arc<TrackFmt>, crate::StreamPosition);

/// A completion signal, tagged with the finishing track's identity + decode
/// stats. Fires once when the source's last track drains; gaplessly-advanced
/// tracks signal via `AudioEvent::TrackCrossing` instead.
pub type CompletionEvent = (Arc<TrackFmt>, u32, u64);

#[derive(Debug)]
pub(crate) enum AudioEvent {
    Position(PositionEvent),
    Completion(CompletionEvent),
    TrackCrossing(TrackCrossing),
    SourceLockMissed {
        missed_ms: u64,
    },
    SourceLockReacquired {
        missed_ms: u64,
    },
    Starved {
        fmt: Arc<TrackFmt>,
        starved_ms: u64,
        position: crate::StreamPosition,
        producer_finished: bool,
        samples_decoded: u64,
        decode_errors: u32,
        has_next: bool,
    },
    StarvationEnded {
        fmt: Arc<TrackFmt>,
        starved_ms: u64,
        position: crate::StreamPosition,
        samples_decoded: u64,
        decode_errors: u32,
    },
    /// The output stopped sending partway through, for the reason `detail`
    /// states — bae's own failure, which stops playback and is reported.
    OutputFailed {
        detail: String,
    },
}

pub struct AudioEventSender {
    producer: Producer<AudioEvent>,
    dropped_events: Arc<AtomicU64>,
    dropped_required_events: Arc<AtomicU64>,
}

impl AudioEventSender {
    pub(crate) fn push(&mut self, event: AudioEvent) {
        if self.producer.push(event).is_err() {
            self.dropped_events.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub(crate) fn push_required(&mut self, event: AudioEvent) {
        if self.producer.push(event).is_err() {
            self.dropped_required_events.fetch_add(1, Ordering::Relaxed);
        }
    }
}

pub(crate) struct AudioEventReceiver {
    consumer: Consumer<AudioEvent>,
    dropped_events: Arc<AtomicU64>,
    dropped_required_events: Arc<AtomicU64>,
}

impl AudioEventReceiver {
    pub(crate) fn pop(&mut self) -> Option<AudioEvent> {
        self.consumer.pop().ok()
    }

    pub(crate) fn take_dropped_count(&self) -> u64 {
        self.dropped_events.swap(0, Ordering::Relaxed)
    }

    pub(crate) fn take_dropped_required_count(&self) -> u64 {
        self.dropped_required_events.swap(0, Ordering::Relaxed)
    }
}

pub(crate) fn audio_event_channel() -> (AudioEventSender, AudioEventReceiver) {
    let (producer, consumer) = RingBuffer::new(AUDIO_EVENT_CAPACITY);
    let dropped_events = Arc::new(AtomicU64::new(0));
    let dropped_required_events = Arc::new(AtomicU64::new(0));
    (
        AudioEventSender {
            producer,
            dropped_events: dropped_events.clone(),
            dropped_required_events: dropped_required_events.clone(),
        },
        AudioEventReceiver { consumer, dropped_events, dropped_required_events },
    )
}

pub(crate) fn duration_millis(duration: std::time::Duration) -> u64 {
    duration.as_millis().min(u64::MAX as u128) as u64
}

/// A sink that pulls samples from a `PlaybackSource` and sends them somewhere: a
/// real device, a capture buffer, a discard-and-pace probe.
pub trait AudioOutput: Send + 'static {
    /// Build a stream over `source` whose callback plays by `controls`: the
    /// state and volume the player sets.
    fn create_stream(
        &mut self,
        controls: AudioOutputControls,
        source: Arc<Mutex<PlaybackSource>>,
        source_sample_rate: u32,
        source_channels: u32,
        audio_events: AudioEventSender,
        position_update_interval_ms: u32,
    ) -> Result<Box<dyn AudioStream>, AudioError>;

    /// Called after the persistent output's `PlaybackSource` is swapped in place
    /// (`PlaybackSource::replace`) for a same-format track transition — the stream
    /// keeps running, only what its callback reads changed. Real devices don't
    /// care (the default no-op); the test capture sinks start a new captured
    /// stream so each non-gapless transition still yields one stream.
    fn on_source_replaced(&mut self) {}

    /// The playback through this output ended: it stopped, or moved to a
    /// receiver. An output that keeps something of the device across its
    /// streams lets it go here — the cpal output on a Mac keeps the device in
    /// the last track's format, so the next track's stream switches it
    /// directly. The others keep nothing.
    fn playback_ended(&mut self) {}
}

/// One player's output and the state and volume its streams play by. The
/// player sets them here; every stream the output builds reads them, so a
/// change reaches the stream playing without a command to it.
pub(crate) struct PlayerOutput {
    output: Box<dyn AudioOutput>,
    controls: AudioOutputControls,
}

impl PlayerOutput {
    /// `output`, its streams playing by `controls`.
    pub(crate) fn new(output: Box<dyn AudioOutput>, controls: AudioOutputControls) -> Self {
        Self { output, controls }
    }

    /// Build a stream over `source` on the output, playing by this player's
    /// state and volume.
    pub(crate) fn create_stream(
        &mut self,
        source: Arc<Mutex<PlaybackSource>>,
        source_sample_rate: u32,
        source_channels: u32,
        audio_events: AudioEventSender,
        position_update_interval_ms: u32,
    ) -> Result<Box<dyn AudioStream>, AudioError> {
        self.output.create_stream(
            self.controls.clone(),
            source,
            source_sample_rate,
            source_channels,
            audio_events,
            position_update_interval_ms,
        )
    }

    /// See [`AudioOutput::on_source_replaced`].
    pub(crate) fn on_source_replaced(&mut self) {
        self.output.on_source_replaced();
    }

    /// See [`AudioOutput::playback_ended`].
    pub(crate) fn playback_ended(&mut self) {
        self.output.playback_ended();
    }

    pub(crate) fn set_state(&self, state: AudioState) {
        self.controls.set_state(state);
    }

    pub(crate) fn get_state(&self) -> AudioState {
        self.controls.get_state()
    }

    pub(crate) fn set_volume(&self, volume: crate::Volume) {
        self.controls.set_volume(volume);
    }

    /// The level the streams play at, for a test of what a volume change
    /// set.
    #[cfg(test)]
    pub(crate) fn get_volume(&self) -> f32 {
        self.controls.get_volume()
    }

    /// Whether the streams play by `controls`, for a test that follows which
    /// output is in place.
    #[cfg(test)]
    pub(crate) fn plays_by(&self, controls: &AudioOutputControls) -> bool {
        Arc::ptr_eq(&self.controls.state, &controls.state)
    }
}

/// What the playback service opens its audio outputs from. The service plays
/// through two independent local outputs — the main player's persistent one and
/// the preview player's second one, which runs while the main player is paused —
/// so what it is handed is the device that opens them, not a single output.
///
/// In production this is the system's default output device; a test substitutes
/// a device whose outputs capture samples or refuse to build, so no test reaches
/// for real audio hardware on either player.
pub trait AudioOutputDevice: Send {
    fn open_output(&self) -> Result<Box<dyn AudioOutput>, AudioError>;
}

// -- Capture audio output for tests --

/// Captures raw f32 samples for ground-truth comparison in tests.
///
/// Each non-gapless transition — a `create_stream` (rebuild) or an
/// `on_source_replaced` (same-format swap) — starts a new [`CapturedStream`],
/// sent, one per transition in order, to where `new()` was told. Volume is
/// NOT applied: samples land exactly as the decoder produced them.
#[cfg(any(test, feature = "test-utils"))]
pub struct CaptureAudioOutput {
    core: CaptureOutputCore,
}

/// `CaptureAudioOutput`, but paces its pulls to real time.
#[cfg(any(test, feature = "test-utils"))]
pub struct RealtimeCaptureAudioOutput {
    core: CaptureOutputCore,
}

/// Where a capture output sends the samples it pulls: the current
/// transition's stream.
#[cfg(any(test, feature = "test-utils"))]
type CaptureSender = tokio::sync::mpsc::UnboundedSender<Vec<f32>>;

/// The streams of a capture output's transitions, in order, as the test
/// receives them.
#[cfg(any(test, feature = "test-utils"))]
pub type CapturedStreams = tokio::sync::mpsc::UnboundedReceiver<CapturedStream>;

/// Where a capture output sends the stream of each transition it starts.
#[cfg(any(test, feature = "test-utils"))]
pub type CapturedStreamSender = tokio::sync::mpsc::UnboundedSender<CapturedStream>;

/// One non-gapless transition's samples, as a test receives them: the capture
/// thread sends each run it pulls, and the test keeps its own copy, so nothing
/// the output holds is shared with the test.
#[cfg(any(test, feature = "test-utils"))]
pub struct CapturedStream {
    runs: tokio::sync::mpsc::UnboundedReceiver<Vec<f32>>,
    samples: Vec<f32>,
}

#[cfg(any(test, feature = "test-utils"))]
impl CapturedStream {
    /// Every sample this transition has sent so far.
    pub fn samples(&mut self) -> Vec<f32> {
        while let Ok(run) = self.runs.try_recv() {
            self.samples.extend(run);
        }
        self.samples.clone()
    }

    /// Every sample this transition has sent once at least `at_least` have
    /// arrived, waiting on each run the capture thread sends. Returns what
    /// arrived if the stream ends first or none ever completes the count
    /// (`HANG_GUARD`), so the caller can assert against whatever was captured.
    pub async fn wait_for_samples(&mut self, at_least: usize) -> Vec<f32> {
        let deadline = tokio::time::Instant::now() + bae_util::test_channels::HANG_GUARD;
        while self.samples.len() < at_least {
            match tokio::time::timeout_at(deadline, self.runs.recv()).await {
                Ok(Some(run)) => self.samples.extend(run),
                // The stream ended, or no more came: the caller asserts on
                // what arrived.
                Ok(None) | Err(_) => break,
            }
        }
        self.samples()
    }
}

#[cfg(any(test, feature = "test-utils"))]
struct CaptureOutputCore {
    notify_tx: CapturedStreamSender,
    /// Where the capture thread sends what it pulls. A `create_stream` or an
    /// `on_source_replaced` starts a new stream and points this at it, so each
    /// non-gapless transition is captured on its own — the one-stream-per-
    /// transition contract the fixtures await via `next_capture_stream`.
    active: Arc<Mutex<CaptureSender>>,
}

#[cfg(any(test, feature = "test-utils"))]
impl CaptureOutputCore {
    /// A core publishing its streams to `notify_tx`, so every output a
    /// capture device opens reports into the one receiver the test holds.
    fn publishing_to(notify_tx: CapturedStreamSender) -> Self {
        // Nothing is captured before the first transition starts a stream.
        let (before_first, _) = tokio::sync::mpsc::unbounded_channel();
        Self { notify_tx, active: Arc::new(Mutex::new(before_first)) }
    }

    /// Start a new stream, point the capture thread at it, and publish it to
    /// the test's receiver.
    fn rotate_capture_buffer(&self) -> Result<(), AudioError> {
        let (runs_tx, runs) = tokio::sync::mpsc::unbounded_channel();
        *self.active.lock().unwrap() = runs_tx;
        self.notify_tx
            .send(CapturedStream { runs, samples: Vec::new() })
            .map_err(|error| AudioError::Unexpected(format!("capture receiver dropped: {error}")))
    }

    /// The one capture thread keeps running across a same-format swap; start
    /// a new stream so this transition's samples are captured separately, as
    /// the fixtures' one-stream-per-transition await expects.
    fn on_source_replaced(&self) {
        if let Err(e) = self.rotate_capture_buffer() {
            tracing::warn!("starting a capture stream failed on source replace: {e}");
        }
    }
}

/// Where a capture thread sends the samples it pulls: the stream of the
/// transition `active` points at, and the run on to `after_samples`.
#[cfg(any(test, feature = "test-utils"))]
fn send_to_active(
    active: Arc<Mutex<CaptureSender>>,
    mut after_samples: impl FnMut(usize) + Send + 'static,
) -> impl FnMut(&[f32]) + Send + 'static {
    move |samples| {
        // Re-read the active stream each run: a same-format source replace
        // rotates it under us, so post-swap samples land in the new track's
        // stream.
        let sent = active.lock().unwrap().send(samples.to_vec());
        if sent.is_err() {
            // The test let go of this transition's stream, so no one reads
            // these samples; playback drains on.
        }
        after_samples(samples.len());
    }
}

#[cfg(any(test, feature = "test-utils"))]
macro_rules! impl_capture_output_constructor {
    ($type:ty) => {
        impl $type {
            /// An output sending `captured` one stream per non-gapless
            /// transition (a `create_stream` rebuild or an
            /// `on_source_replaced` swap), in order. A capture device opens
            /// each output it hands out over its own sender, so the main
            /// player's and the preview player's land on one receiver.
            pub fn new(captured: CapturedStreamSender) -> Self {
                Self { core: CaptureOutputCore::publishing_to(captured) }
            }
        }
    };
}

/// Define a capture *device*: what a test hands the playback service in place of
/// the system's audio device. Every output it opens sends its streams to the
/// one sender `new()` takes, one stream per non-gapless transition across all
/// of them, in order.
#[cfg(any(test, feature = "test-utils"))]
macro_rules! impl_capture_audio_device {
    ($device:ident, $output:ident) => {
        pub struct $device {
            notify_tx: CapturedStreamSender,
        }

        impl $device {
            pub fn new(captured: CapturedStreamSender) -> Self {
                Self { notify_tx: captured }
            }
        }

        impl AudioOutputDevice for $device {
            fn open_output(&self) -> Result<Box<dyn AudioOutput>, AudioError> {
                Ok(Box::new($output::new(self.notify_tx.clone())))
            }
        }
    };
}

#[cfg(any(test, feature = "test-utils"))]
impl_capture_output_constructor!(CaptureAudioOutput);

#[cfg(any(test, feature = "test-utils"))]
impl_capture_output_constructor!(RealtimeCaptureAudioOutput);

#[cfg(any(test, feature = "test-utils"))]
impl_capture_audio_device!(CaptureAudioDevice, CaptureAudioOutput);

#[cfg(any(test, feature = "test-utils"))]
impl_capture_audio_device!(RealtimeCaptureAudioDevice, RealtimeCaptureAudioOutput);

#[cfg(any(test, feature = "test-utils"))]
impl AudioOutput for CaptureAudioOutput {
    fn create_stream(
        &mut self,
        controls: AudioOutputControls,
        source: Arc<Mutex<PlaybackSource>>,
        _source_sample_rate: u32,
        _source_channels: u32,
        audio_events: AudioEventSender,
        position_update_interval_ms: u32,
    ) -> Result<Box<dyn AudioStream>, AudioError> {
        self.core.rotate_capture_buffer()?;
        Ok(Box::new(CaptureStream::spawn(
            AudioDrain::new(controls, source, audio_events, position_update_interval_ms),
            4096,
            false,
            send_to_active(self.core.active.clone(), |_| {}),
        )))
    }

    fn on_source_replaced(&mut self) {
        self.core.on_source_replaced();
    }
}

/// A test audio output with no backing device: `create_stream` always fails.
/// Two uses — exercising the stream-build failure path shared by both players,
/// and standing in for a real (cpal) output in actor-level tests that drive the
/// playback service for its queue/command behavior and never actually play, so
/// they need no hardware.
#[cfg(any(test, feature = "test-utils"))]
pub(crate) struct FailingAudioOutput;

/// A device with no hardware behind it: every output it opens fails to create
/// a stream. What the actor-level tests hand the service — they
/// drive its queue/command behavior and never play, so neither the main player
/// nor a preview may reach for a real device.
#[cfg(any(test, feature = "test-utils"))]
pub struct FailingAudioDevice;

#[cfg(any(test, feature = "test-utils"))]
impl AudioOutputDevice for FailingAudioDevice {
    fn open_output(&self) -> Result<Box<dyn AudioOutput>, AudioError> {
        Ok(Box::new(FailingAudioOutput))
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl AudioOutput for FailingAudioOutput {
    fn create_stream(
        &mut self,
        _controls: AudioOutputControls,
        _source: Arc<Mutex<PlaybackSource>>,
        _source_sample_rate: u32,
        _source_channels: u32,
        _audio_events: AudioEventSender,
        _position_update_interval_ms: u32,
    ) -> Result<Box<dyn AudioStream>, AudioError> {
        Err(AudioError::Unexpected("stream build failure".to_string()))
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl AudioOutput for RealtimeCaptureAudioOutput {
    fn create_stream(
        &mut self,
        controls: AudioOutputControls,
        source: Arc<Mutex<PlaybackSource>>,
        source_sample_rate: u32,
        source_channels: u32,
        audio_events: AudioEventSender,
        position_update_interval_ms: u32,
    ) -> Result<Box<dyn AudioStream>, AudioError> {
        self.core.rotate_capture_buffer()?;
        let channels = source_channels.max(1);
        Ok(Box::new(CaptureStream::spawn(
            AudioDrain::new(controls, source, audio_events, position_update_interval_ms),
            4096,
            false,
            send_to_active(self.core.active.clone(), move |read| {
                sleep_realtime_buffer(read, channels, source_sample_rate)
            }),
        )))
    }

    fn on_source_replaced(&mut self) {
        self.core.on_source_replaced();
    }
}

/// A test output's stream: a thread draining the source into `on_samples`
/// until the stream is dropped. Persistent, like a device's: a paused, starved
/// or drained source idles the thread, which waits on the player's controls
/// until something could give it work.
#[cfg(any(test, feature = "test-utils"))]
struct CaptureStream {
    stop: Arc<std::sync::atomic::AtomicBool>,
    controls: AudioOutputControls,
    thread: Option<std::thread::JoinHandle<()>>,
}

#[cfg(any(test, feature = "test-utils"))]
impl CaptureStream {
    fn spawn(
        mut drain: AudioDrain,
        buf_len: usize,
        apply_gain: bool,
        mut on_samples: impl FnMut(&[f32]) + Send + 'static,
    ) -> Self {
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let controls = drain.controls.clone();
        let running = stop.clone();
        let thread = std::thread::spawn(move || {
            let mut buf = vec![0.0f32; buf_len];
            loop {
                // Read before the stop flag and the drain, so a stop or new
                // work that lands after this read ends the wait at once.
                let seen = drain.controls.drain_changes();
                if running.load(Ordering::Acquire) {
                    break;
                }
                match drain.drain_iteration(&mut buf, apply_gain, false, SourceLock::Wait) {
                    DrainStatus::Idle => drain.controls.wait_for_drain_work(seen),
                    DrainStatus::Samples { read } => on_samples(&buf[..read]),
                }
            }
        });
        Self { stop, controls, thread: Some(thread) }
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl AudioStream for CaptureStream {
    fn play(&self) -> Result<(), AudioError> {
        Ok(()) // The capture thread is already running from create_stream.
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl Drop for CaptureStream {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.controls.wake_drain();
        if let Some(handle) = self.thread.take() {
            if let Err(e) = handle.join() {
                tracing::warn!("capture audio thread panicked during shutdown: {:?}", e);
            }
        }
    }
}

// -- Real-time CPU probe output for tests --

/// Drives playback at real time and discards the samples, for measuring
/// steady-state playback CPU. Runs the same per-buffer work the cpal sink does —
/// `pull_samples`, the replay-gain/volume multiply, position ticks, completion —
/// but paces itself by sleeping one buffer's wall-clock duration after each pull
/// instead of being clocked by an audio device.
///
/// That pacing is the point: it makes the decoder fill-then-park as it does in
/// production, so the measured CPU reflects real playback (per-buffer drain,
/// gain, ~20 Hz tick fan-out, decoder wakeups) rather than a full-speed decode,
/// which is an order of magnitude cheaper. Needs no audio device, so the
/// measurement runs headless on any platform.
#[cfg(any(test, feature = "test-utils"))]
pub struct RealtimeProbeOutput;

/// The device the CPU probe measures through: every output it opens paces to
/// real time and discards its samples.
#[cfg(any(test, feature = "test-utils"))]
pub struct RealtimeProbeDevice;

#[cfg(any(test, feature = "test-utils"))]
impl AudioOutputDevice for RealtimeProbeDevice {
    fn open_output(&self) -> Result<Box<dyn AudioOutput>, AudioError> {
        Ok(Box::new(RealtimeProbeOutput))
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl AudioOutput for RealtimeProbeOutput {
    fn create_stream(
        &mut self,
        controls: AudioOutputControls,
        source: Arc<Mutex<PlaybackSource>>,
        source_sample_rate: u32,
        source_channels: u32,
        audio_events: AudioEventSender,
        position_update_interval_ms: u32,
    ) -> Result<Box<dyn AudioStream>, AudioError> {
        let channels = source_channels.max(1);
        // 1024 frames/buffer — the pacing granularity, not the total work.
        Ok(Box::new(CaptureStream::spawn(
            AudioDrain::new(controls, source, audio_events, position_update_interval_ms),
            1024 * channels as usize,
            true,
            move |samples| sleep_realtime_buffer(samples.len(), channels, source_sample_rate),
        )))
    }
}
