//! The devices a test plays on in place of the system's: outputs that capture
//! what they pull, and the one that discards it at real time for the CPU
//! probe.
//!
//! A capture device either pulls as fast as the decoder fills
//! ([`CaptureAudioDevice`]), or plays only as far as the test lets it
//! ([`ManualCaptureDevice`]): its audio time is an [`AudioClock`] the test
//! holds, so a command the test sends after play lands where the test
//! decided, however busy the machine is.

use crate::audio_output::{
    AudioDrain, AudioError, AudioEventSender, AudioOutput, AudioOutputControls, AudioOutputDevice,
    AudioStream, DrainStatus, PositionEvent, SourceLock,
};
use crate::source::{PlaybackSource, TrackFmt};
use crate::timeline::StreamPosition;
use crate::PlaybackProgress;
use bae_util::test_channels::HANG_GUARD;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, PoisonError};

/// Captures raw f32 samples for ground-truth comparison in tests, pulling as
/// fast as the decoder fills.
///
/// Each non-gapless transition — a `create_stream` (rebuild) or an
/// `on_source_replaced` (same-format swap) — starts a new [`CapturedStream`],
/// sent, one per transition in order, to where `new()` was told. Volume is
/// NOT applied: samples land exactly as the decoder produced them.
pub struct CaptureAudioOutput {
    core: CaptureOutputCore,
}

/// A capture output that plays only as far as its [`AudioClock`] lets it.
/// Its streams are captured as [`CaptureAudioOutput`]'s are.
pub struct ManualCaptureAudioOutput {
    core: CaptureOutputCore,
    clock: AudioClock,
}

/// Where a capture output sends the samples it pulls: the current
/// transition's stream.
type CaptureSender = tokio::sync::mpsc::UnboundedSender<Vec<f32>>;

/// The streams of a capture output's transitions, in order, as the test
/// receives them.
pub type CapturedStreams = tokio::sync::mpsc::UnboundedReceiver<CapturedStream>;

/// Where a capture output sends the stream of each transition it starts.
pub type CapturedStreamSender = tokio::sync::mpsc::UnboundedSender<CapturedStream>;

/// One non-gapless transition's samples, as a test receives them: the capture
/// thread sends each run it pulls, and the test keeps its own copy, so nothing
/// the output holds is shared with the test.
pub struct CapturedStream {
    runs: tokio::sync::mpsc::UnboundedReceiver<Vec<f32>>,
    samples: Vec<f32>,
    /// The audio clock of the device that captured it, when the test plays
    /// it: waiting for samples then lets the device play them.
    clock: Option<AudioClock>,
}

impl CapturedStream {
    /// Every sample this transition has sent so far.
    pub fn samples(&mut self) -> Vec<f32> {
        while let Ok(run) = self.runs.try_recv() {
            self.samples.extend(run);
        }
        self.samples.clone()
    }

    /// Every sample this transition has sent once at least `at_least` have
    /// arrived. On a manual device the device plays report by report until
    /// they have, and a report nothing plays panics. Otherwise this waits on
    /// each run the capture thread sends, and returns what arrived if the
    /// stream ends first or none ever completes the count (`HANG_GUARD`), so
    /// the caller can assert against whatever was captured.
    pub async fn wait_for_samples(&mut self, at_least: usize) -> Vec<f32> {
        match self.clock.clone() {
            Some(clock) => {
                while self.samples().len() < at_least {
                    clock.play_report().await;
                }
            }
            None => {
                let deadline = tokio::time::Instant::now() + HANG_GUARD;
                while self.samples.len() < at_least {
                    match tokio::time::timeout_at(deadline, self.runs.recv()).await {
                        Ok(Some(run)) => self.samples.extend(run),
                        // The stream ended, or no more came: the caller
                        // asserts on what arrived.
                        Ok(None) | Err(_) => break,
                    }
                }
            }
        }
        self.samples()
    }
}

struct CaptureOutputCore {
    notify_tx: CapturedStreamSender,
    /// Where the capture thread sends what it pulls. A `create_stream` or an
    /// `on_source_replaced` starts a new stream and points this at it, so each
    /// non-gapless transition is captured on its own — the one-stream-per-
    /// transition contract the fixtures await via `next_capture_stream`.
    active: Arc<Mutex<CaptureSender>>,
    /// The clock the output plays by, when the test plays it.
    clock: Option<AudioClock>,
}

impl CaptureOutputCore {
    /// A core publishing its streams to `notify_tx`, so every output a
    /// capture device opens reports into the one receiver the test holds.
    fn publishing_to(notify_tx: CapturedStreamSender, clock: Option<AudioClock>) -> Self {
        // Nothing is captured before the first transition starts a stream.
        let (before_first, _) = tokio::sync::mpsc::unbounded_channel();
        Self { notify_tx, active: Arc::new(Mutex::new(before_first)), clock }
    }

    /// Start a new stream, point the capture thread at it, and publish it to
    /// the test's receiver.
    fn rotate_capture_buffer(&self) -> Result<(), AudioError> {
        let (runs_tx, runs) = tokio::sync::mpsc::unbounded_channel();
        *self.active.lock().unwrap() = runs_tx;
        self.notify_tx
            .send(CapturedStream { runs, samples: Vec::new(), clock: self.clock.clone() })
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

    /// Where a capture thread sends the samples it pulls: the stream of the
    /// transition `active` points at.
    fn send_to_active(&self) -> impl FnMut(&[f32]) + Send + 'static {
        let active = self.active.clone();
        move |samples| {
            // Re-read the active stream each run: a same-format source replace
            // rotates it under us, so post-swap samples land in the new track's
            // stream.
            let sent = active.lock().unwrap().send(samples.to_vec());
            if sent.is_err() {
                // The test let go of this transition's stream, so no one reads
                // these samples; playback drains on.
            }
        }
    }
}

impl CaptureAudioOutput {
    /// An output sending `captured` one stream per non-gapless transition (a
    /// `create_stream` rebuild or an `on_source_replaced` swap), in order.
    pub fn new(captured: CapturedStreamSender) -> Self {
        Self { core: CaptureOutputCore::publishing_to(captured, None) }
    }
}

impl ManualCaptureAudioOutput {
    /// An output playing as far as `clock` lets it, sending `captured` one
    /// stream per non-gapless transition, in order.
    pub fn new(captured: CapturedStreamSender, clock: AudioClock) -> Self {
        Self { core: CaptureOutputCore::publishing_to(captured, Some(clock.clone())), clock }
    }
}

/// What a test hands the playback service in place of the system's audio
/// device: every output it opens captures at full speed, sending its streams
/// to the one sender `new()` takes, so the main player's and the preview
/// player's land on one receiver.
pub struct CaptureAudioDevice {
    notify_tx: CapturedStreamSender,
}

impl CaptureAudioDevice {
    pub fn new(captured: CapturedStreamSender) -> Self {
        Self { notify_tx: captured }
    }
}

impl AudioOutputDevice for CaptureAudioDevice {
    fn open_output(&self) -> Result<Box<dyn AudioOutput>, AudioError> {
        Ok(Box::new(CaptureAudioOutput::new(self.notify_tx.clone())))
    }
}

/// [`CaptureAudioDevice`], but every output it opens plays by one
/// [`AudioClock`]: the main player's and the preview player's audio both
/// move only as the test lets them.
pub struct ManualCaptureDevice {
    notify_tx: CapturedStreamSender,
    clock: AudioClock,
}

impl ManualCaptureDevice {
    pub fn new(captured: CapturedStreamSender, clock: AudioClock) -> Self {
        Self { notify_tx: captured, clock }
    }
}

impl AudioOutputDevice for ManualCaptureDevice {
    fn open_output(&self) -> Result<Box<dyn AudioOutput>, AudioError> {
        Ok(Box::new(ManualCaptureAudioOutput::new(self.notify_tx.clone(), self.clock.clone())))
    }
}

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
            controls,
            source,
            audio_events,
            position_update_interval_ms,
            4096,
            false,
            self.core.send_to_active(),
        )))
    }

    fn on_source_replaced(&mut self) {
        self.core.on_source_replaced();
    }
}

impl AudioOutput for ManualCaptureAudioOutput {
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
        // A report is due every interval of audio the device plays, as on a
        // device that plays in real time.
        let frames = bae_util::sample_time::frame_at(
            std::time::Duration::from_millis(u64::from(position_update_interval_ms)),
            source_sample_rate,
        );
        let interval = usize::try_from(frames).expect("a report interval's frames fit usize")
            * source_channels.max(1) as usize;
        Ok(Box::new(CaptureStream::spawn_manual(
            controls,
            source,
            audio_events,
            interval,
            self.clock.clone(),
            self.core.send_to_active(),
        )))
    }

    fn on_source_replaced(&mut self) {
        self.core.on_source_replaced();
    }
}

/// What a manual capture device may play now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Allowed {
    /// Nothing: the device holds where it is.
    Nothing,
    /// Up to its next position report.
    Report,
    /// Up to the end of the track playing: the pull that crosses into the
    /// next track, or the source running out.
    TrackEnd,
}

struct ClockState {
    allowed: Allowed,
    /// The report the last [`Allowed::Report`] ended with.
    report: Option<PositionEvent>,
}

struct ClockShared {
    state: Mutex<ClockState>,
    /// What a capture thread waits on while the clock lets it play nothing.
    drains: Condvar,
    /// What a test waits on while the device plays what it was let play.
    played: tokio::sync::Notify,
}

/// The audio time of a [`ManualCaptureDevice`]: the device plays nothing
/// until the test lets it, and then only up to the point the test names —
/// its next position report, or the end of the track playing. The device's
/// outputs all play by the one clock.
#[derive(Clone)]
pub struct AudioClock {
    shared: Arc<ClockShared>,
}

impl AudioClock {
    /// A clock that lets nothing play yet.
    pub fn new() -> Self {
        Self {
            shared: Arc::new(ClockShared {
                state: Mutex::new(ClockState { allowed: Allowed::Nothing, report: None }),
                drains: Condvar::new(),
                played: tokio::sync::Notify::new(),
            }),
        }
    }

    /// Let the device play up to its next position report, and wait until it
    /// has: the report it made. Waits through a decoder that hasn't filled
    /// yet and through the end of one track into the next one played; panics
    /// when nothing plays it within `HANG_GUARD` (playback is paused,
    /// stopped, or has nothing left).
    pub async fn play_report(&self) -> PlayedReport {
        self.let_play(Allowed::Report, "its next position report").await;
        let report = self.lock().report.take().expect("a played report leaves the report it made");
        PlayedReport { fmt: report.0, position: report.1 }
    }

    /// Let the device play to the end of the track playing, and wait until it
    /// has: the pull that crossed into the next track, or the one that found
    /// the source run out. Panics when nothing plays it within `HANG_GUARD`.
    pub async fn play_to_track_end(&self) {
        self.let_play(Allowed::TrackEnd, "the end of the track playing").await;
    }

    async fn let_play(&self, allowed: Allowed, what: &str) {
        {
            let mut state = self.lock();
            assert_eq!(state.allowed, Allowed::Nothing, "the device still plays what it was let");
            state.allowed = allowed;
        }
        self.shared.drains.notify_all();
        let played = tokio::time::timeout(HANG_GUARD, async {
            loop {
                let notified = self.shared.played.notified();
                tokio::pin!(notified);
                notified.as_mut().enable();
                if self.lock().allowed == Allowed::Nothing {
                    return;
                }
                notified.await;
            }
        })
        .await;
        assert!(played.is_ok(), "the device never played {what}: playback isn't playing");
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, ClockState> {
        self.shared.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Wait until the clock lets the device play, and say how far; `None`
    /// once `stop` is set.
    fn wait_until_allowed(&self, stop: &AtomicBool) -> Option<Allowed> {
        let mut state = self.lock();
        loop {
            if stop.load(Ordering::Acquire) {
                return None;
            }
            if state.allowed != Allowed::Nothing {
                return Some(state.allowed);
            }
            state = self.shared.drains.wait(state).unwrap_or_else(PoisonError::into_inner);
        }
    }

    /// The device made the report it was let play to.
    fn reported(&self, report: PositionEvent) {
        let mut state = self.lock();
        state.allowed = Allowed::Nothing;
        state.report = Some(report);
        drop(state);
        self.shared.played.notify_waiters();
    }

    /// The device played to the end of the track it was let play out.
    fn played_to_track_end(&self) {
        self.lock().allowed = Allowed::Nothing;
        self.shared.played.notify_waiters();
    }

    /// Wake every capture thread waiting on the clock: one of them stops.
    fn wake_drains(&self) {
        let _state = self.lock();
        self.shared.drains.notify_all();
    }
}

/// A position report a manual capture device made.
pub struct PlayedReport {
    fmt: Arc<TrackFmt>,
    position: StreamPosition,
}

impl PlayedReport {
    /// Whether `event` is what playback reports for this position on the
    /// track playing.
    pub fn reported_as(&self, event: &PlaybackProgress) -> bool {
        matches!(
            event,
            PlaybackProgress::PositionUpdate { track_id, position_ms, .. }
                if *track_id == self.fmt.track_id
                    && *position_ms == self.fmt.timeline.track_time(self.position).as_millis()
        )
    }
}

/// A test output's stream: a thread draining the source into `on_samples`
/// until the stream is dropped. Persistent, like a device's: a paused, starved
/// or drained source idles the thread, which waits on the player's controls
/// until something could give it work. A manual output's thread also waits on
/// its clock.
struct CaptureStream {
    stop: Arc<AtomicBool>,
    controls: AudioOutputControls,
    clock: Option<AudioClock>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl CaptureStream {
    /// A stream draining `source` by `controls` as fast as it fills.
    fn spawn(
        controls: AudioOutputControls,
        source: Arc<Mutex<PlaybackSource>>,
        audio_events: AudioEventSender,
        position_update_interval_ms: u32,
        buf_len: usize,
        apply_gain: bool,
        mut on_samples: impl FnMut(&[f32]) + Send + 'static,
    ) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let mut drain =
            AudioDrain::new(controls.clone(), source, audio_events, position_update_interval_ms);
        let thread_controls = controls.clone();
        let running = stop.clone();
        #[expect(
            clippy::disallowed_methods,
            reason = "test support with no Diagnostics: the stream joins its thread when dropped"
        )]
        let thread = std::thread::spawn(move || {
            let mut buf = vec![0.0f32; buf_len];
            loop {
                // Read before the stop flag and the drain, so a stop or new
                // work that lands after this read ends the wait at once.
                let seen = thread_controls.drain_changes();
                if running.load(Ordering::Acquire) {
                    break;
                }
                match drain.drain_iteration(&mut buf, apply_gain, SourceLock::Wait) {
                    DrainStatus::Idle => thread_controls.wait_for_drain_work(seen),
                    DrainStatus::Samples { read, .. } => on_samples(&buf[..read]),
                }
            }
        });
        Self { stop, controls, clock: None, thread: Some(thread) }
    }

    /// A stream draining `source` by `controls` only while `clock` lets it:
    /// to the report it was let play to, a report every `interval` samples,
    /// or to the end of the track playing.
    fn spawn_manual(
        controls: AudioOutputControls,
        source: Arc<Mutex<PlaybackSource>>,
        audio_events: AudioEventSender,
        interval: usize,
        clock: AudioClock,
        mut on_samples: impl FnMut(&[f32]) + Send + 'static,
    ) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let mut drain =
            AudioDrain::reporting_every(controls.clone(), source.clone(), audio_events, interval);
        let thread_controls = controls.clone();
        let running = stop.clone();
        let thread_clock = clock.clone();
        #[expect(
            clippy::disallowed_methods,
            reason = "test support with no Diagnostics: the stream joins its thread when dropped"
        )]
        let thread = std::thread::spawn(move || {
            // One pull covers a report.
            let mut buf = vec![0.0f32; interval];
            loop {
                let seen = thread_controls.drain_changes();
                let Some(allowed) = thread_clock.wait_until_allowed(&running) else {
                    break;
                };
                let playing = track_playing(&source);
                match drain.drain_iteration(&mut buf, false, SourceLock::Wait) {
                    DrainStatus::Idle => {
                        let ran_out = source.lock().unwrap_or_else(PoisonError::into_inner);
                        if allowed == Allowed::TrackEnd && ran_out.is_finished() {
                            drop(ran_out);
                            thread_clock.played_to_track_end();
                        } else {
                            drop(ran_out);
                            thread_controls.wait_for_drain_work(seen);
                        }
                    }
                    DrainStatus::Samples { read, reported } => {
                        on_samples(&buf[..read]);
                        match (allowed, reported) {
                            (Allowed::Report, Some(report)) => thread_clock.reported(report),
                            (Allowed::TrackEnd, _)
                                if !Arc::ptr_eq(&playing, &track_playing(&source)) =>
                            {
                                thread_clock.played_to_track_end();
                            }
                            _ => {}
                        }
                    }
                }
            }
        });
        Self { stop, controls, clock: Some(clock), thread: Some(thread) }
    }
}

/// The track `source` plays now.
fn track_playing(source: &Mutex<PlaybackSource>) -> Arc<TrackFmt> {
    source.lock().unwrap_or_else(PoisonError::into_inner).position_event().0
}

impl AudioStream for CaptureStream {
    fn play(&self) -> Result<(), AudioError> {
        Ok(()) // The capture thread is already running from create_stream.
    }
}

impl Drop for CaptureStream {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.controls.wake_drain();
        if let Some(clock) = &self.clock {
            clock.wake_drains();
        }
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
pub struct RealtimeProbeOutput;

/// The device the CPU probe measures through: every output it opens paces to
/// real time and discards its samples.
pub struct RealtimeProbeDevice;

impl AudioOutputDevice for RealtimeProbeDevice {
    fn open_output(&self) -> Result<Box<dyn AudioOutput>, AudioError> {
        Ok(Box::new(RealtimeProbeOutput))
    }
}

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
            controls,
            source,
            audio_events,
            position_update_interval_ms,
            1024 * channels as usize,
            true,
            move |samples| {
                let frames = (samples.len() / channels as usize) as u64;
                std::thread::sleep(bae_util::sample_time::frame_start(frames, source_sample_rate));
            },
        )))
    }
}

#[cfg(test)]
#[path = "capture_output_tests.rs"]
mod tests;
