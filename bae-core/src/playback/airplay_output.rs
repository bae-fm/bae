//! The AirPlay audio output: the sink that pushes decoded PCM to a receiver.
//!
//! Unlike Cast and DLNA (where the device fetches a URL), AirPlay keeps bae's
//! decode pipeline running and swaps only the *output end*: [`AirPlayOutput`]
//! implements the same [`AudioOutput`] trait cpal's sink does, so the service
//! installs it in place of the local device and everything upstream — the
//! decoder, ring buffer, gapless crossing, position and completion events — runs
//! unchanged. Its stream pulls f32 from the same [`PlaybackSource`] the DAC would,
//! converts to the 16-bit PCM RAOP sends, and hands it to a [`RaopSession`],
//! pacing to the receiver instead of the sound card.
//!
//! The receiver connection is behind an [`AirPlaySink`] so the service tests
//! drive the whole seam against a fake that records frames and honours
//! pause/resume — no sockets, no real device.

use std::net::IpAddr;
use std::sync::{Arc, Mutex};

use tracing::warn;

use crate::airplay::capabilities::RaopEncryption;
use crate::airplay::crypto::{apple_public_key, RaopCipher};
use crate::airplay::session::RaopSession;
use crate::airplay::stream::{PcmSource, SystemClock};
use crate::playback::audio_output::{
    AudioDrain, AudioError, AudioEventSender, AudioOutput, AudioOutputControls, AudioState,
    AudioStream, DrainStatus,
};
use crate::playback::source::PlaybackSource;

/// The transport controls the playback service drives a live AirPlay stream
/// through: pause (FLUSH), resume/seek (re-anchor), volume, and the audible
/// position (frames sent minus the receiver latency). Implemented by the RAOP
/// session's control handle, and by the test fake.
pub trait AirPlayStreamControl: Send + Sync {
    /// Stop sending and drop the receiver's buffer (pause / pre-seek).
    fn flush(&self);
    /// Re-anchor the pacing after a FLUSH (resume / post-seek).
    fn reanchor(&self);
    /// Set the receiver volume (0.0–1.0).
    fn set_volume(&self, level: f32);
    /// Frames handed to the receiver so far.
    fn frames_sent(&self) -> u64;
    /// The receiver's audio latency in frames.
    fn latency_frames(&self) -> u32;
}

/// A drop-guard for the running stream: dropping it tears the receiver session
/// and its threads down. Held by the [`AudioStream`] the output returns.
pub trait AirPlayStreamGuard: Send {}

/// A started stream: the drop-guard that owns the session and the control handle
/// the service drives it through.
pub type StartedStream = (Box<dyn AirPlayStreamGuard>, Arc<dyn AirPlayStreamControl>);

/// Starts pushing decoded PCM to an AirPlay receiver. The real implementation
/// connects a RAOP session; tests supply a fake.
pub trait AirPlaySink: Send {
    /// Begin streaming `source`, returning the drop-guard and control handle.
    fn start(&self, source: Box<dyn PcmSource>) -> Result<StartedStream, AudioError>;
}

/// A shared slot the live stream's control is published into on `create_stream`,
/// so the renderer (which is set up before the stream starts) can reach it.
pub type ControlSlot = Arc<Mutex<Option<Arc<dyn AirPlayStreamControl>>>>;

/// An `AudioOutput` that streams to an AirPlay receiver via `sink`.
pub struct AirPlayOutput {
    controls: AudioOutputControls,
    sink: Box<dyn AirPlaySink>,
    control_slot: ControlSlot,
}

impl AirPlayOutput {
    /// Build an output over `sink`, seeded to `initial_volume`. The returned
    /// `control_slot` receives the live stream's control on `create_stream`.
    pub fn new(sink: Box<dyn AirPlaySink>, initial_volume: f32) -> (Self, ControlSlot) {
        let control_slot: ControlSlot = Arc::new(Mutex::new(None));
        (
            AirPlayOutput {
                controls: AudioOutputControls::new(initial_volume),
                sink,
                control_slot: control_slot.clone(),
            },
            control_slot,
        )
    }
}

impl AudioOutput for AirPlayOutput {
    fn create_stream(
        &mut self,
        source: Arc<Mutex<PlaybackSource>>,
        _source_sample_rate: u32,
        source_channels: u32,
        audio_events: AudioEventSender,
        position_update_interval_ms: u32,
    ) -> Result<Box<dyn AudioStream>, AudioError> {
        let drain = AudioDrain::new(
            self.controls.clone(),
            source,
            audio_events,
            position_update_interval_ms,
        );
        let pcm_source = Box::new(DrainSource::new(drain, source_channels));
        let (guard, control) = self.sink.start(pcm_source)?;
        *self.control_slot.lock().unwrap() = Some(control);
        Ok(Box::new(AirPlayAudioStream { _guard: guard }))
    }

    fn set_state(&self, state: AudioState) {
        self.controls.set_state(state);
    }

    fn get_state(&self) -> AudioState {
        self.controls.get_state()
    }

    fn set_volume(&self, volume: f32) {
        // AirPlay applies volume as local gain in the drain (the receiver plays at
        // its own level); the session's SET_PARAMETER is driven separately by the
        // service through the control handle.
        self.controls.set_volume(volume);
    }

    fn get_volume(&self) -> f32 {
        self.controls.get_volume()
    }
}

/// The [`AudioStream`] the output returns: it owns the session drop-guard, so the
/// service dropping the stream tears the receiver session down.
struct AirPlayAudioStream {
    _guard: Box<dyn AirPlayStreamGuard>,
}

impl AudioStream for AirPlayAudioStream {
    fn play(&self) -> Result<(), AudioError> {
        // The RAOP threads are already running from `sink.start`.
        Ok(())
    }
}

/// Pulls f32 from the ring buffer through an [`AudioDrain`] and converts it to the
/// interleaved 16-bit stereo PCM RAOP sends. A paused or drained drain yields no
/// frames — the stream stays up (the session's lifetime ends it), so pause and
/// gapless track swaps don't tear it down.
struct DrainSource {
    drain: AudioDrain,
    source_channels: u32,
    scratch: Vec<f32>,
}

impl DrainSource {
    fn new(drain: AudioDrain, source_channels: u32) -> Self {
        DrainSource {
            drain,
            source_channels: source_channels.max(1),
            scratch: Vec::new(),
        }
    }
}

impl PcmSource for DrainSource {
    fn next_frames(&mut self, out: &mut [i16]) -> usize {
        let channels = self.source_channels as usize;
        let frames_wanted = out.len() / 2;
        self.scratch.resize(frames_wanted * channels, 0.0);

        // Apply replay-gain/volume and zero the unfilled tail, exactly as the DAC
        // path does.
        let read = match self
            .drain
            .drain_iteration(&mut self.scratch, true, true, None)
        {
            DrainStatus::Samples { read } => read,
            DrainStatus::Idle => return 0,
        };

        let frames_read = read / channels;
        for f in 0..frames_read {
            let (left, right) = if channels == 1 {
                let mono = self.scratch[f];
                (mono, mono)
            } else {
                (self.scratch[f * channels], self.scratch[f * channels + 1])
            };
            out[f * 2] = f32_to_i16(left);
            out[f * 2 + 1] = f32_to_i16(right);
        }
        frames_read
    }
}

/// Convert a normalized f32 sample to 16-bit PCM.
fn f32_to_i16(sample: f32) -> i16 {
    (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16
}

// -- The real receiver connection: a RAOP session --

/// The parameters an [`AirPlaySink`] needs to open a RAOP session to a receiver.
pub struct RaopSink {
    pub receiver: IpAddr,
    pub rtsp_port: u16,
    /// The encryption the receiver requires — `None` for `et=0`, `RsaAes` for
    /// `et=1`.
    pub encryption: RaopEncryption,
    /// The receiver's reported audio latency in frames, if known.
    pub latency_frames: Option<u32>,
    pub initial_volume: f32,
}

impl AirPlaySink for RaopSink {
    fn start(&self, source: Box<dyn PcmSource>) -> Result<StartedStream, AudioError> {
        let cipher = match self.encryption {
            RaopEncryption::None => RaopCipher::none(),
            RaopEncryption::RsaAes => RaopCipher::new_aes(&apple_public_key())
                .map_err(|e| AudioError::StreamBuildError(format!("RAOP key wrap: {e}")))?,
        };
        let session = RaopSession::start(
            self.receiver,
            self.rtsp_port,
            source,
            cipher,
            self.latency_frames,
            self.initial_volume,
            Arc::new(SystemClock::new()),
        )
        .map_err(|e| AudioError::StreamBuildError(e.to_string()))?;

        let control: Arc<dyn AirPlayStreamControl> = Arc::new(RaopControlOps(session.control()));
        Ok((Box::new(RaopGuard(session)), control))
    }
}

/// Holds the session alive; dropping it tears down the receiver and threads. The
/// field is never read — it exists only for its `Drop`.
struct RaopGuard(#[allow(dead_code)] RaopSession);
impl AirPlayStreamGuard for RaopGuard {}

/// Adapts the RAOP control handle (whose ops return `Result`) to the infallible
/// [`AirPlayStreamControl`] the service drives, logging transport failures.
struct RaopControlOps(crate::airplay::session::RaopControl);

impl AirPlayStreamControl for RaopControlOps {
    fn flush(&self) {
        if let Err(e) = self.0.flush() {
            warn!("airplay FLUSH failed: {e}");
        }
    }

    fn reanchor(&self) {
        self.0.reanchor();
    }

    fn set_volume(&self, level: f32) {
        if let Err(e) = self.0.set_volume(level) {
            warn!("airplay volume failed: {e}");
        }
    }

    fn frames_sent(&self) -> u64 {
        self.0.frames_sent()
    }

    fn latency_frames(&self) -> u32 {
        self.0.latency_frames()
    }
}

// -- The AirPlay 2 receiver connection --

/// The parameters an [`AirPlaySink`] needs to open an AirPlay 2 session: pair
/// (transient) then push ChaCha-encrypted audio to a HomePod-class receiver.
pub struct Ap2Sink {
    pub receiver: IpAddr,
    /// The receiver's AirPlay control port (`_airplay._tcp`, usually 7000).
    pub airplay_port: u16,
    pub latency_frames: Option<u32>,
}

impl AirPlaySink for Ap2Sink {
    fn start(&self, source: Box<dyn PcmSource>) -> Result<StartedStream, AudioError> {
        let session = crate::airplay::ap2_session::Ap2Session::start(
            self.receiver,
            self.airplay_port,
            source,
            self.latency_frames,
            Arc::new(SystemClock::new()),
        )
        .map_err(|e| AudioError::StreamBuildError(e.to_string()))?;

        let control: Arc<dyn AirPlayStreamControl> = Arc::new(Ap2ControlOps(session.control()));
        Ok((Box::new(Ap2Guard(session)), control))
    }
}

/// Holds the AirPlay 2 session alive; dropping it tears the receiver down.
struct Ap2Guard(#[allow(dead_code)] crate::airplay::ap2_session::Ap2Session);
impl AirPlayStreamGuard for Ap2Guard {}

/// Adapts the AirPlay 2 session control (whose ops return `Result`) to the
/// infallible [`AirPlayStreamControl`], logging transport failures. Pause/resume
/// map to SETRATEANCHORTIME rate changes; volume is applied locally in the drain.
struct Ap2ControlOps(crate::airplay::ap2_session::Ap2SessionControl);

impl AirPlayStreamControl for Ap2ControlOps {
    fn flush(&self) {
        if let Err(e) = self.0.flush() {
            warn!("airplay 2 pause (rate 0) failed: {e}");
        }
    }

    fn reanchor(&self) {
        if let Err(e) = self.0.reanchor() {
            warn!("airplay 2 resume (rate 1) failed: {e}");
        }
    }

    fn set_volume(&self, _level: f32) {}

    fn frames_sent(&self) -> u64 {
        self.0.frames_sent()
    }

    fn latency_frames(&self) -> u32 {
        self.0.latency_frames()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::playback::audio_output::audio_event_channel;
    use crate::playback::create_track_stream_pair;
    use crate::playback::source::{PlaybackSource, TrackFmt};
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::time::Duration;

    fn track_fmt() -> TrackFmt {
        TrackFmt {
            track_id: "t".to_string(),
            duration_ms: 1_000,
            pregap_ms: None,
            position_offset: Duration::ZERO,
            replay_gain_linear: 1.0,
        }
    }

    /// A playing source of `stereo_frames` full-scale stereo frames.
    fn playing_source(stereo_frames: usize) -> Arc<Mutex<PlaybackSource>> {
        let (mut sink, stream, _ready) = create_track_stream_pair(44_100, 2);
        sink.push_samples(&vec![1.0f32; stereo_frames * 2]);
        Arc::new(Mutex::new(PlaybackSource::new(stream, track_fmt())))
    }

    /// A fake sink: it records how many frames the source produced and honours the
    /// control ops, so the seam is exercised without a socket.
    #[derive(Default)]
    struct FakeSink {
        state: Arc<FakeState>,
    }

    #[derive(Default)]
    struct FakeState {
        frames: AtomicU64,
        flushed: AtomicBool,
        reanchored: AtomicBool,
        torn_down: AtomicBool,
    }

    struct FakeGuard(Arc<FakeState>);
    impl AirPlayStreamGuard for FakeGuard {}
    impl Drop for FakeGuard {
        fn drop(&mut self) {
            self.0.torn_down.store(true, Ordering::Release);
        }
    }

    struct FakeControl(Arc<FakeState>);
    impl AirPlayStreamControl for FakeControl {
        fn flush(&self) {
            self.0.flushed.store(true, Ordering::Release);
        }
        fn reanchor(&self) {
            self.0.reanchored.store(true, Ordering::Release);
        }
        fn set_volume(&self, _level: f32) {}
        fn frames_sent(&self) -> u64 {
            self.0.frames.load(Ordering::Relaxed)
        }
        fn latency_frames(&self) -> u32 {
            88_200
        }
    }

    impl AirPlaySink for FakeSink {
        fn start(&self, mut source: Box<dyn PcmSource>) -> Result<StartedStream, AudioError> {
            // Pull one packet synchronously to record the frame conversion.
            let mut buf = vec![0i16; 8]; // 4 stereo frames
            let frames = source.next_frames(&mut buf);
            self.state.frames.store(frames as u64, Ordering::Relaxed);
            Ok((
                Box::new(FakeGuard(self.state.clone())),
                Arc::new(FakeControl(self.state.clone())),
            ))
        }
    }

    /// `create_stream` converts the drained f32 to 16-bit stereo and publishes a
    /// control into the slot; the control drives flush/reanchor and drop tears the
    /// session down.
    #[test]
    fn create_stream_drives_the_sink_and_publishes_control() {
        let source = playing_source(4);

        let sink = FakeSink::default();
        let state = sink.state.clone();
        let (mut output, slot) = AirPlayOutput::new(Box::new(sink), 1.0);
        output.set_state(AudioState::Playing);

        let (tx, _rx) = audio_event_channel();
        let stream = output
            .create_stream(source, 44_100, 2, tx, 100)
            .expect("stream builds");

        // The fake pulled 4 stereo frames.
        assert_eq!(state.frames.load(Ordering::Relaxed), 4);
        // The control was published for the renderer to drive.
        let control = slot.lock().unwrap().clone().expect("control published");
        control.flush();
        control.reanchor();
        assert!(state.flushed.load(Ordering::Acquire));
        assert!(state.reanchored.load(Ordering::Acquire));

        // Dropping the stream tears the session down.
        drop(stream);
        assert!(state.torn_down.load(Ordering::Acquire));
    }

    /// A paused output yields no frames from the drain (so the stream sends
    /// nothing), and a playing one converts full-scale f32 to i16::MAX.
    #[test]
    fn drain_source_respects_pause_and_converts_scale() {
        let (tx, _rx) = audio_event_channel();
        let controls = AudioOutputControls::new(1.0);
        controls.set_state(AudioState::Paused);
        let mut paused = DrainSource::new(
            AudioDrain::new(controls.clone(), playing_source(4), tx, 100),
            2,
        );
        let mut out = vec![0i16; 8];
        assert_eq!(paused.next_frames(&mut out), 0, "paused yields nothing");

        // Now playing: the four full-scale stereo frames convert to i16::MAX.
        let (tx2, _rx2) = audio_event_channel();
        let controls2 = AudioOutputControls::new(1.0);
        controls2.set_state(AudioState::Playing);
        let mut playing =
            DrainSource::new(AudioDrain::new(controls2, playing_source(4), tx2, 100), 2);
        let mut out2 = vec![0i16; 8];
        assert_eq!(playing.next_frames(&mut out2), 4);
        assert!(out2.iter().all(|&s| s == i16::MAX));
    }

    #[test]
    fn f32_converts_to_i16_full_scale() {
        assert_eq!(f32_to_i16(1.0), i16::MAX);
        assert_eq!(f32_to_i16(-1.0), -i16::MAX);
        assert_eq!(f32_to_i16(0.0), 0);
        // Out of range clamps.
        assert_eq!(f32_to_i16(2.0), i16::MAX);
    }
}
