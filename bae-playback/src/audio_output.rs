//! Platform-neutral audio output seam.
//!
//! Every sink (cpal on desktop, AAudio on Android, the test capture buffer)
//! pulls f32 samples from a [`PlaybackSource`] and sends them somewhere. The
//! [`AudioOutput`]/[`AudioStream`] traits and the shared [`AudioState`] atomic
//! shape are defined here so the concrete sinks in `cpal_output`/`aaudio_output`
//! depend only on this module, not on each other or on cpal.

use crate::event_relay::{event_relay, EventRelay, RelayWake};
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
    pub(crate) fn drain_changes(&self) -> u64 {
        self.wake.changes()
    }

    /// Wait until a change past `seen` could give the drain work.
    pub(crate) fn wait_for_drain_work(&self, seen: u64) {
        self.wake.wait_past(seen);
    }

    /// Wake a waiting drain: its owner stopped it.
    pub(crate) fn wake_drain(&self) {
        self.wake.notify();
    }

    /// Test-only: wait until a drain is waiting for work.
    #[cfg(any(test, feature = "test-utils"))]
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

/// What one pull of a drain did.
#[derive(Debug, Clone)]
pub(crate) enum DrainStatus {
    Idle,
    /// It pulled `read` samples, and reported the position they played to
    /// when one was due: what a test device whose audio time the test
    /// advances stops at.
    Samples {
        read: usize,
        #[cfg(any(test, feature = "test-utils"))]
        reported: Option<PositionEvent>,
    },
}

/// When a drain reports where its source has played to: after each
/// `interval` of the audio it pulled, counted in frames at the rate of the
/// track playing, so the reports follow what played rather than how fast it
/// was pulled.
enum PositionReports {
    /// A device that plays in real time: each pull fills what the device
    /// asked for, and a report goes out once the audio pulled since the last
    /// one reaches `interval`.
    Played { interval: std::time::Duration, since: std::time::Duration },
    /// A test device whose audio time the test advances: every `interval`
    /// samples pulled, and no pull past the next report.
    #[cfg(any(test, feature = "test-utils"))]
    Stepped { interval: usize, since: usize },
}

impl PositionReports {
    /// How many of `wanted` samples the next pull may take.
    fn pull_len(&self, wanted: usize) -> usize {
        match self {
            PositionReports::Played { .. } => wanted,
            #[cfg(any(test, feature = "test-utils"))]
            PositionReports::Stepped { interval, since } => wanted.min(interval - since),
        }
    }

    /// Count a pull of `read` samples of a track playing `channels` channels
    /// at `sample_rate`, and say whether a report is due.
    fn pulled(&mut self, read: usize, sample_rate: u32, channels: u32) -> bool {
        match self {
            PositionReports::Played { interval, since } => {
                let frames = read as u64 / u64::from(channels);
                *since += bae_util::sample_time::frame_start(frames, sample_rate);
                let due = *since >= *interval;
                if due {
                    // One report a pull: what is past the last whole interval
                    // counts toward the next, and a zero interval reports
                    // every pull.
                    let past = since.as_nanos().checked_rem(interval.as_nanos()).unwrap_or(0);
                    *since = std::time::Duration::from_nanos(
                        u64::try_from(past).expect("what is past an interval is under it"),
                    );
                }
                due
            }
            #[cfg(any(test, feature = "test-utils"))]
            PositionReports::Stepped { interval, since } => {
                *since += read;
                let due = *since >= *interval;
                if due {
                    *since = 0;
                }
                due
            }
        }
    }
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
    reports: PositionReports,
    /// Set once a pull from the source panics; from then on the drain yields
    /// nothing and never touches the source again.
    failed: bool,
}

impl AudioDrain {
    /// A drain for a device that plays in real time, reporting the position
    /// after every `position_update_interval_ms` of audio it pulls.
    pub(crate) fn new(
        controls: AudioOutputControls,
        source: Arc<Mutex<PlaybackSource>>,
        audio_events: AudioEventSender,
        position_update_interval_ms: u32,
    ) -> Self {
        let interval = std::time::Duration::from_millis(u64::from(position_update_interval_ms));
        let reports = PositionReports::Played { interval, since: std::time::Duration::ZERO };
        Self::reporting(controls, source, audio_events, reports)
    }

    /// A drain for a test device whose audio time the test advances,
    /// reporting the position every `interval` samples pulled.
    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) fn reporting_every(
        controls: AudioOutputControls,
        source: Arc<Mutex<PlaybackSource>>,
        audio_events: AudioEventSender,
        interval: usize,
    ) -> Self {
        assert!(interval > 0, "a drain reports after some audio, never before any");
        let reports = PositionReports::Stepped { interval, since: 0 };
        Self::reporting(controls, source, audio_events, reports)
    }

    fn reporting(
        controls: AudioOutputControls,
        source: Arc<Mutex<PlaybackSource>>,
        audio_events: AudioEventSender,
        reports: PositionReports,
    ) -> Self {
        controls.wake_drain_of(&mut source.lock().unwrap_or_else(PoisonError::into_inner));
        Self { controls, source, audio_events, reports, failed: false }
    }

    /// Tell playback the output stopped sending, for the reason `detail` states.
    pub(crate) fn report_output_failure(&mut self, detail: String) {
        self.audio_events.push_required(AudioEvent::OutputFailed { detail });
    }

    pub(crate) fn drain_iteration(
        &mut self,
        buf: &mut [f32],
        apply_gain: bool,
        lock: SourceLock<'_>,
    ) -> DrainStatus {
        if self.failed || self.controls.get_state() != AudioState::Playing {
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
            return DrainStatus::Idle;
        };

        // Caught while the guard is held, so a panicking pull leaves the
        // source's lock unpoisoned for the playback service, which stops on
        // the failure and discards the source.
        let pull_len = self.reports.pull_len(buf.len());
        let buf = &mut buf[..pull_len];
        let pulled = caught_sync("pull samples from the playing track", || {
            source_guard.pull_samples(buf, &mut self.audio_events)
        });
        let read = match pulled {
            Ok(read) => read,
            Err(failed) => {
                self.failed = true;
                self.audio_events
                    .push_required(AudioEvent::OutputFailed { detail: failed.to_string() });
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
            return DrainStatus::Idle;
        }

        if apply_gain {
            let combined = source_guard.current_replay_gain_linear() * self.controls.get_volume();
            for sample in &mut buf[..read] {
                *sample *= combined;
            }
        }

        #[cfg_attr(
            not(any(test, feature = "test-utils")),
            expect(unused_variables, reason = "only a test device stops at a report")
        )]
        let reported = self
            .reports
            .pulled(read, source_guard.sample_rate(), source_guard.channels())
            .then(|| {
                let position = source_guard.position_event();
                self.audio_events.push(AudioEvent::Position(position.clone()));
                position
            });

        DrainStatus::Samples {
            read,
            #[cfg(any(test, feature = "test-utils"))]
            reported,
        }
    }
}

/// Why an audio output couldn't start or keep playing. The first three are the
/// device's or the speaker's doing, which the person can act on; `Unexpected`
/// is bae's own failure, a bug.
#[derive(Clone, Debug, thiserror::Error)]
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

/// Tells playback that an output stream's device reported the stream can't go
/// on, so it asks its streams for the report
/// ([`AudioStream::take_invalidation`]) and replaces the one that made it.
/// The system device hands it to every output it opens.
#[derive(Clone)]
pub(crate) struct InvalidationReport {
    report: Arc<dyn Fn() + Send + Sync>,
}

impl InvalidationReport {
    pub(crate) fn new(report: impl Fn() + Send + Sync + 'static) -> Self {
        Self { report: Arc::new(report) }
    }

    #[cfg_attr(target_os = "android", expect(dead_code, reason = "AAudio reopens its own stream"))]
    pub(crate) fn report(&self) {
        (self.report)();
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

/// The pushing side of an output's event ring. Each push wakes the drain
/// through the relay, the one wake the real-time callback makes for it.
pub struct AudioEventSender {
    producer: Producer<AudioEvent>,
    dropped_events: Arc<AtomicU64>,
    dropped_required_events: Arc<AtomicU64>,
    relay: RelayWake,
}

impl AudioEventSender {
    pub(crate) fn push(&mut self, event: AudioEvent) {
        if self.producer.push(event).is_err() {
            self.dropped_events.fetch_add(1, Ordering::Relaxed);
        }
        self.relay.wake();
    }

    pub(crate) fn push_required(&mut self, event: AudioEvent) {
        if self.producer.push(event).is_err() {
            self.dropped_required_events.fetch_add(1, Ordering::Relaxed);
        }
        self.relay.wake();
    }
}

pub(crate) struct AudioEventReceiver {
    consumer: Consumer<AudioEvent>,
    dropped_events: Arc<AtomicU64>,
    dropped_required_events: Arc<AtomicU64>,
    relay: EventRelay,
}

impl AudioEventReceiver {
    pub(crate) fn pop(&mut self) -> Option<AudioEvent> {
        self.consumer.pop().ok()
    }

    /// Complete once events were pushed since the last wait completed: the
    /// drain's cue to pop them.
    pub(crate) fn ready(&self) -> impl std::future::Future<Output = ()> + Send + '_ {
        self.relay.ready()
    }

    pub(crate) fn take_dropped_count(&self) -> u64 {
        self.dropped_events.swap(0, Ordering::Relaxed)
    }

    pub(crate) fn take_dropped_required_count(&self) -> u64 {
        self.dropped_required_events.swap(0, Ordering::Relaxed)
    }
}

/// An output's event ring, with the relay thread that wakes its drain; the
/// error when that thread can't start.
pub(crate) fn audio_event_channel() -> std::io::Result<(AudioEventSender, AudioEventReceiver)> {
    let (producer, consumer) = RingBuffer::new(AUDIO_EVENT_CAPACITY);
    let dropped_events = Arc::new(AtomicU64::new(0));
    let dropped_required_events = Arc::new(AtomicU64::new(0));
    let (wake, relay) = event_relay()?;
    Ok((
        AudioEventSender {
            producer,
            dropped_events: dropped_events.clone(),
            dropped_required_events: dropped_required_events.clone(),
            relay: wake,
        },
        AudioEventReceiver { consumer, dropped_events, dropped_required_events, relay },
    ))
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
// -- Outputs that fail, for tests --

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

#[cfg(test)]
#[path = "audio_output_tests.rs"]
mod tests;
