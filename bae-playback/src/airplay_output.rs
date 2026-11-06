//! The AirPlay audio output: the sink that pushes decoded PCM to a receiver.
//!
//! Unlike Cast and DLNA (where the device fetches a URL), AirPlay keeps bae's
//! decode pipeline running and swaps only the *output end*: [`AirPlayOutput`]
//! implements the same [`AudioOutput`] trait cpal's sink does, so the service
//! installs it in place of the local device and everything upstream — the
//! decoder, ring buffer, gapless crossing, position and completion events — runs
//! unchanged. Its stream pulls f32 from the same [`PlaybackSource`] the DAC would,
//! converts to the 16-bit PCM a receiver takes, and hands it, as a
//! [`PcmSource`], to the [`AirPlaySink`] it was given, which paces it to the
//! receiver instead of the sound card.
//!
//! The receiver connection is behind [`AirPlaySink`] and [`AirPlayStreamControl`]:
//! bae-cast opens the RAOP or AirPlay 2 session behind them, and the service
//! tests drive the whole seam against a fake that records frames and honours
//! pause/resume — no sockets, no real device.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tracing::warn;

use crate::audio_output::{
    AudioDrain, AudioError, AudioEventSender, AudioOutput, AudioOutputControls, AudioStream,
};
use crate::resampling_drain::ResamplingDrain;
use crate::source::PlaybackSource;

/// The sample rate of the PCM a [`PcmSource`] yields: 44.1 kHz, the rate an
/// AirPlay receiver takes.
pub const PCM_SAMPLE_RATE: u32 = 44_100;
/// The channel count of the PCM a [`PcmSource`] yields: interleaved stereo.
pub const PCM_CHANNELS: u32 = 2;

/// A source of decoded PCM the AirPlay stream pulls one packet at a time:
/// interleaved 16-bit samples at [`PCM_SAMPLE_RATE`] and [`PCM_CHANNELS`]. The
/// output hands one to its [`AirPlaySink`], which streams it to the receiver.
pub trait PcmSource: Send {
    /// Fill `out` (interleaved i16, up to one packet's worth) with the next
    /// frames and return how many *frames* were produced. `0` means no audio is
    /// available right now — the stream is paused, starved, or between tracks —
    /// and the stream sends nothing this packet rather than treating it as the
    /// end: the session's lifetime, not the source, decides when streaming stops.
    fn next_frames(&mut self, out: &mut [i16]) -> usize;
}

/// What the AirPlay sender waits on while its [`PcmSource`] has no frames.
/// The player wakes it whenever the source may have frames again: a resume, a
/// decoder's push, a track placed in the source. The sender's owner wakes it
/// to stop. Read [`Self::changes`] before pulling, and when a pull yields
/// nothing, [`Self::wait_past`] what was read: a change in between ends the
/// wait at once.
#[derive(Clone)]
pub struct PcmWake {
    controls: AudioOutputControls,
}

impl PcmWake {
    /// The changes counted so far that could give the source frames.
    pub fn changes(&self) -> u64 {
        self.controls.drain_changes()
    }

    /// Wait until a change past `seen` could give the source frames.
    pub fn wait_past(&self, seen: u64) {
        self.controls.wait_for_drain_work(seen);
    }

    /// End a wait in progress: the sender is stopping.
    pub fn wake(&self) {
        self.controls.wake_drain();
    }

    /// Test-only: a wake no player notifies, for a sender test that feeds its
    /// own source and wakes it itself.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn unattached() -> Self {
        Self { controls: AudioOutputControls::new(crate::Volume::FULL) }
    }

    /// Test-only: wait until the sender is waiting for frames.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn wait_until_waiting(&self) {
        self.controls.wait_until_drain_waits();
    }
}

/// Tells playback that a stream to a receiver failed for good, so it reads
/// the live session's [`AirPlayStreamControl::failure`] and ends AirPlay. A
/// session calls it once, when its failure is first set.
#[derive(Clone)]
pub struct AirPlayFailed {
    report: Arc<dyn Fn() + Send + Sync>,
}

impl AirPlayFailed {
    pub(crate) fn new(report: impl Fn() + Send + Sync + 'static) -> Self {
        Self { report: Arc::new(report) }
    }

    pub fn report(&self) {
        (self.report)();
    }

    /// Test-only: a report that goes nowhere, for a session test with no
    /// playback behind it.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn ignored() -> Self {
        Self::new(|| {})
    }

    /// Test-only: a report that runs `report`, for a session test that checks
    /// it was told.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn reporting_to(report: impl Fn() + Send + Sync + 'static) -> Self {
        Self::new(report)
    }
}

/// The transport controls the playback service drives a live AirPlay stream
/// through: pause (FLUSH), resume/seek (re-anchor), whether the audio flow
/// failed, and the receiver latency the stream runs at. Implemented by the RAOP
/// and AirPlay 2 sessions in bae-cast, and by the test fake.
pub trait AirPlayStreamControl: Send + Sync {
    /// Stop sending and drop the receiver's buffer (pause / pre-seek).
    fn flush(&self);
    /// Re-anchor the pacing after a FLUSH (resume / post-seek).
    fn reanchor(&self);
    /// Why the audio flow to the receiver failed for good (a dead receiver),
    /// when it has, so the service should end AirPlay.
    fn failure(&self) -> Option<String>;
    /// How long audio sent now takes to be heard: the receiver latency the
    /// session paces its sends ahead by, settled during its setup. Playback
    /// holds the position, the play it records, and the track's completion
    /// back by it.
    fn latency(&self) -> Duration;
}

/// Starts pushing decoded PCM to an AirPlay receiver. bae-cast implements it
/// over a RAOP or AirPlay 2 session; tests supply a fake.
pub trait AirPlaySink: Send {
    /// Begin streaming `source`, waiting on `wake` while it has no frames, and
    /// telling `failed` once the stream fails for good. The returned session
    /// owns the receiver connection and is retained by the output stream.
    fn start(
        &self,
        source: Box<dyn PcmSource>,
        wake: PcmWake,
        failed: AirPlayFailed,
    ) -> Result<Arc<dyn AirPlayStreamControl>, AudioError>;
}

/// The playback service's handle on whichever receiver session is live. The
/// AirPlay renderer outlives each session: every output stream opens its own
/// (and owns it, so dropping the stream tears it down), and binds it here.
pub(crate) struct AirPlayControl {
    session: Mutex<Option<std::sync::Weak<dyn AirPlayStreamControl>>>,
}

impl AirPlayControl {
    pub(crate) fn new() -> Self {
        Self { session: Mutex::new(None) }
    }

    /// Make `session` the live one. The stream that opened it holds it.
    pub(crate) fn bind(&self, session: &Arc<dyn AirPlayStreamControl>) {
        *self.session.lock().unwrap() = Some(Arc::downgrade(session));
    }

    fn with_session<T>(&self, run: impl FnOnce(&dyn AirPlayStreamControl) -> T) -> Option<T> {
        self.session
            .lock()
            .unwrap()
            .as_ref()
            .and_then(std::sync::Weak::upgrade)
            .map(|session| run(session.as_ref()))
    }

    pub(crate) fn flush(&self) {
        self.with_session(|session| session.flush());
    }

    pub(crate) fn reanchor(&self) {
        self.with_session(|session| session.reanchor());
    }

    pub(crate) fn failure(&self) -> Option<String> {
        // No session bound has nothing to fail.
        self.with_session(|session| session.failure()).flatten()
    }

    /// The live session's receiver latency. With no session live, no audio is
    /// on its way to a receiver, so there is none.
    pub(crate) fn latency(&self) -> Option<Duration> {
        self.with_session(|session| session.latency())
    }
}

/// An `AudioOutput` that streams to an AirPlay receiver via `sink`.
pub struct AirPlayOutput {
    sink: Box<dyn AirPlaySink>,
    control: Arc<AirPlayControl>,
    failed: AirPlayFailed,
}

impl AirPlayOutput {
    pub(crate) fn new(
        sink: Box<dyn AirPlaySink>,
        control: Arc<AirPlayControl>,
        failed: AirPlayFailed,
    ) -> Self {
        AirPlayOutput { sink, control, failed }
    }
}

impl AudioOutput for AirPlayOutput {
    fn create_stream(
        &mut self,
        controls: AudioOutputControls,
        source: Arc<Mutex<PlaybackSource>>,
        source_sample_rate: u32,
        source_channels: u32,
        audio_events: AudioEventSender,
        position_update_interval_ms: u32,
    ) -> Result<Box<dyn AudioStream>, AudioError> {
        // Local gain IS the AirPlay volume path: the drain multiplies samples
        // by `controls`' volume before they're packetized, so the user hears
        // the change. The receiver stays at its own hardware level (the
        // session's one initial SET_PARAMETER seeds it at full); there is no
        // per-change device round-trip.
        let drain =
            AudioDrain::new(controls.clone(), source, audio_events, position_update_interval_ms);
        let wake = PcmWake { controls: controls.clone() };
        let pcm_source =
            Box::new(DrainSource::new(drain, controls, source_sample_rate, source_channels)?);
        let session = self.sink.start(pcm_source, wake, self.failed.clone())?;
        self.control.bind(&session);
        Ok(Box::new(AirPlayAudioStream { _session: session }))
    }
}

/// The [`AudioStream`] the output returns: it owns the session drop-guard, so the
/// service dropping the stream tears the receiver session down.
struct AirPlayAudioStream {
    _session: Arc<dyn AirPlayStreamControl>,
}

impl AudioStream for AirPlayAudioStream {
    fn play(&self) -> Result<(), AudioError> {
        // The RAOP threads are already running from `sink.start`.
        Ok(())
    }
}

/// Pulls f32 from the ring buffer through a [`ResamplingDrain`], which converts
/// it to the 44.1 kHz stereo of the realtime stream bae opens to a receiver
/// (the decode pipeline fills the ring at the track's native rate), and
/// converts that to the interleaved 16-bit PCM the send path packetizes. A
/// paused or drained drain yields no frames — the stream stays up (the
/// session's lifetime ends it), so pause and gapless track swaps don't tear it
/// down; the resampler's tail is flushed when the source drains.
///
/// A conversion that fails partway stops sending: the failure goes to playback
/// as an [`AudioEvent::OutputFailed`], which stops playback and reports it,
/// and nothing more reaches the receiver — no audio skipped silently.
struct DrainSource {
    converter: ResamplingDrain,
    /// One packet of 44.1 kHz stereo f32, before it becomes 16-bit.
    packet: Vec<f32>,
}

impl DrainSource {
    /// The source for a track of `source_sample_rate` and `source_channels`,
    /// or the track's format when no converter to 44.1 kHz stereo can be set
    /// up for it: then the speaker can't play the track at all.
    fn new(
        drain: AudioDrain,
        controls: AudioOutputControls,
        source_sample_rate: u32,
        source_channels: u32,
    ) -> Result<Self, AudioError> {
        let converter = ResamplingDrain::new(
            drain,
            controls,
            (source_sample_rate, source_channels),
            (PCM_SAMPLE_RATE, PCM_CHANNELS),
            None,
            "the AirPlay speaker",
        )
        .map_err(|error| {
            warn!(
                "no converter from {source_sample_rate} Hz, {source_channels} channels \
                 to the AirPlay speaker's 44.1 kHz stereo: {error}"
            );
            AudioError::UnsupportedFormat {
                sample_rate: source_sample_rate,
                channels: source_channels,
            }
        })?;
        Ok(DrainSource { converter, packet: Vec::new() })
    }
}

impl PcmSource for DrainSource {
    fn next_frames(&mut self, out: &mut [i16]) -> usize {
        self.packet.resize(out.len(), 0.0);
        let written = self.converter.read(&mut self.packet);
        for (pcm, sample) in out.iter_mut().zip(&self.packet[..written]) {
            *pcm = f32_to_i16(*sample);
        }
        written / PCM_CHANNELS as usize
    }
}

/// Convert a normalized f32 sample to 16-bit PCM.
fn f32_to_i16(sample: f32) -> i16 {
    (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16
}

#[cfg(test)]
#[path = "airplay_output_tests.rs"]
mod tests;
