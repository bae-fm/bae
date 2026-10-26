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

use crate::playback::audio_output::{
    AudioDrain, AudioError, AudioEventSender, AudioOutput, AudioOutputControls, AudioState,
    AudioStream, DrainStatus,
};
use crate::playback::source::PlaybackSource;

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
    /// Begin streaming `source`. The returned session owns the receiver
    /// connection and is retained by the output stream.
    fn start(
        &self,
        source: Box<dyn PcmSource>,
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
        Self {
            session: Mutex::new(None),
        }
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
    controls: AudioOutputControls,
    sink: Box<dyn AirPlaySink>,
    control: Arc<AirPlayControl>,
}

impl AirPlayOutput {
    pub(crate) fn new(
        sink: Box<dyn AirPlaySink>,
        initial_volume: f32,
        control: Arc<AirPlayControl>,
    ) -> Self {
        AirPlayOutput {
            controls: AudioOutputControls::new(initial_volume),
            sink,
            control,
        }
    }
}

impl AudioOutput for AirPlayOutput {
    fn create_stream(
        &mut self,
        source: Arc<Mutex<PlaybackSource>>,
        source_sample_rate: u32,
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
        let pcm_source = Box::new(DrainSource::new(
            drain,
            self.controls.clone(),
            source_sample_rate,
            source_channels,
        )?);
        let session = self.sink.start(pcm_source)?;
        self.control.bind(&session);
        Ok(Box::new(AirPlayAudioStream { _session: session }))
    }

    /// Local gain IS the AirPlay volume path: the drain multiplies samples by
    /// this before they're packetized, so the user hears the change. The
    /// receiver stays at its own hardware level (the session's one initial
    /// SET_PARAMETER seeds it at full); there is no per-change device round-trip.
    fn controls(&self) -> &AudioOutputControls {
        &self.controls
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

/// Source frames pulled from the ring per drain iteration while filling a packet.
const PULL_FRAMES: usize = 512;

/// Pulls f32 from the ring buffer through an [`AudioDrain`], resamples it to the
/// 44.1 kHz stereo a receiver expects (the decode pipeline fills the ring at the
/// track's native rate), and converts to the interleaved 16-bit PCM the send path
/// packetizes. A paused or drained drain yields no frames — the stream stays up
/// (the session's lifetime ends it), so pause and gapless track swaps don't tear
/// it down; the resampler's tail is flushed when the source drains.
///
/// A conversion that fails partway stops sending: the failure goes to playback
/// as an [`AudioEvent::OutputFailed`], which stops playback and reports it,
/// and nothing more reaches the receiver — no audio skipped silently.
struct DrainSource {
    drain: AudioDrain,
    /// A clone of the output's controls, to tell a paused drain from a drained one.
    controls: AudioOutputControls,
    source_channels: usize,
    /// `None` when the source is already 44.1 kHz stereo (pass-through).
    resampler: Option<bae_audio::Resampler>,
    /// Resampled 44.1 kHz stereo interleaved samples awaiting emission.
    out_buf: Vec<f32>,
    /// Source samples pulled per drain iteration.
    in_scratch: Vec<f32>,
    /// Whether the resampler's tail has been flushed for the current drain.
    flushed: bool,
    /// Set once a conversion fails; from then on the source yields nothing.
    failed: bool,
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
        let source_channels = source_channels.max(1);
        let resampler = if source_sample_rate == PCM_SAMPLE_RATE && source_channels == PCM_CHANNELS
        {
            None
        } else {
            let resampler = bae_audio::Resampler::new(
                source_sample_rate,
                source_channels,
                PCM_SAMPLE_RATE,
                PCM_CHANNELS,
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
            Some(resampler)
        };
        Ok(DrainSource {
            drain,
            controls,
            source_channels: source_channels as usize,
            resampler,
            out_buf: Vec::new(),
            in_scratch: Vec::new(),
            flushed: false,
            failed: false,
        })
    }

    /// Stop sending after a conversion failed, and tell playback why.
    fn conversion_failed(&mut self, what: &str, error: String) -> Pull {
        self.failed = true;
        self.out_buf.clear();
        self.drain.report_output_failure(format!(
            "converting {what} for the AirPlay speaker failed: {error}"
        ));
        Pull::Idle
    }

    /// Pull one drain iteration and append its (resampled) 44.1 kHz stereo output
    /// to `out_buf`. Returns whether anything was produced or the source is idle.
    fn fill(&mut self) -> Pull {
        if self.failed {
            return Pull::Idle;
        }
        self.in_scratch
            .resize(PULL_FRAMES * self.source_channels, 0.0);
        match self
            .drain
            .drain_iteration(&mut self.in_scratch, true, true, None)
        {
            DrainStatus::Samples { read } => {
                self.flushed = false;
                let input = &self.in_scratch[..read];
                let converted = match &mut self.resampler {
                    Some(r) => r.convert(input),
                    None => Ok(input.to_vec()),
                };
                match converted {
                    Ok(out) => self.out_buf.extend_from_slice(&out),
                    Err(error) => return self.conversion_failed("a chunk", error),
                }
                Pull::Produced
            }
            // A drained source (completion latched → Stopped) flushes the
            // resampler's tail once; a paused source just waits.
            DrainStatus::Idle => {
                if !self.flushed && self.controls.get_state() == AudioState::Stopped {
                    self.flushed = true;
                    if let Some(r) = &mut self.resampler {
                        match r.flush() {
                            Ok(tail) => {
                                let produced = !tail.is_empty();
                                self.out_buf.extend_from_slice(&tail);
                                if produced {
                                    return Pull::Produced;
                                }
                            }
                            Err(error) => {
                                return self.conversion_failed("the end of the track", error)
                            }
                        }
                    }
                }
                Pull::Idle
            }
        }
    }
}

/// The outcome of one [`DrainSource::fill`].
enum Pull {
    Produced,
    Idle,
}

impl PcmSource for DrainSource {
    fn next_frames(&mut self, out: &mut [i16]) -> usize {
        let want = out.len() / 2;
        while self.out_buf.len() < want * 2 {
            if matches!(self.fill(), Pull::Idle) {
                break;
            }
        }
        let take = want.min(self.out_buf.len() / 2);
        for f in 0..take {
            out[f * 2] = f32_to_i16(self.out_buf[f * 2]);
            out[f * 2 + 1] = f32_to_i16(self.out_buf[f * 2 + 1]);
        }
        self.out_buf.drain(..take * 2);
        take
    }
}

/// Convert a normalized f32 sample to 16-bit PCM.
fn f32_to_i16(sample: f32) -> i16 {
    (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::playback::audio_output::audio_event_channel;
    use crate::playback::source::{PlaybackSource, TrackFmt};
    use bae_audio::track_stream::create_track_stream_pair;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

    fn track_fmt() -> TrackFmt {
        TrackFmt {
            track_id: "t".to_string(),
            timeline: crate::playback::TrackTimeline::new(
                Some(std::time::Duration::from_secs(1)),
                std::time::Duration::ZERO,
            ),
            starts_at: crate::playback::StreamPosition::START,
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

    struct FakeSession(Arc<FakeState>);
    impl Drop for FakeSession {
        fn drop(&mut self) {
            self.0.torn_down.store(true, Ordering::Release);
        }
    }

    impl AirPlayStreamControl for FakeSession {
        fn flush(&self) {
            self.0.flushed.store(true, Ordering::Release);
        }
        fn reanchor(&self) {
            self.0.reanchored.store(true, Ordering::Release);
        }
        fn failure(&self) -> Option<String> {
            None
        }
        fn latency(&self) -> Duration {
            Duration::from_millis(250)
        }
    }

    impl AirPlaySink for FakeSink {
        fn start(
            &self,
            mut source: Box<dyn PcmSource>,
        ) -> Result<Arc<dyn AirPlayStreamControl>, AudioError> {
            // Pull one packet synchronously to record the frame conversion.
            let mut buf = vec![0i16; 8]; // 4 stereo frames
            let frames = source.next_frames(&mut buf);
            self.state.frames.store(frames as u64, Ordering::Relaxed);
            Ok(Arc::new(FakeSession(self.state.clone())))
        }
    }

    /// `create_stream` converts the drained f32 to 16-bit stereo and publishes a
    /// control into the slot; the control drives flush/reanchor, reads the
    /// session's latency, and drop tears the session down.
    #[test]
    fn create_stream_drives_the_sink_and_publishes_control() {
        let source = playing_source(4);

        let sink = FakeSink::default();
        let state = sink.state.clone();
        let control = Arc::new(AirPlayControl::new());
        let mut output = AirPlayOutput::new(Box::new(sink), 1.0, control.clone());
        output.set_state(AudioState::Playing);

        let (tx, _rx) = audio_event_channel();
        let stream = output
            .create_stream(source, 44_100, 2, tx, 100)
            .expect("stream builds");

        // The fake pulled 4 stereo frames.
        assert_eq!(state.frames.load(Ordering::Relaxed), 4);
        // The control was published for the renderer to drive.
        control.flush();
        control.reanchor();
        assert!(state.flushed.load(Ordering::Acquire));
        assert!(state.reanchored.load(Ordering::Acquire));
        assert_eq!(control.latency(), Some(Duration::from_millis(250)));

        // Dropping the stream tears the session down; no audio is in flight.
        drop(stream);
        assert!(state.torn_down.load(Ordering::Acquire));
        assert_eq!(control.latency(), None);
    }

    /// A track the converter can't be set up for is refused before anything
    /// is sent, rather than streamed unconverted at the wrong speed and pitch.
    #[test]
    fn a_converter_that_cant_be_set_up_refuses_the_stream() {
        let (tx, _rx) = audio_event_channel();
        let controls = AudioOutputControls::new(1.0);
        controls.set_state(AudioState::Playing);
        let refused = DrainSource::new(
            AudioDrain::new(controls.clone(), playing_source(4), tx, 100),
            controls,
            0,
            2,
        );
        assert!(matches!(
            refused,
            Err(AudioError::UnsupportedFormat {
                sample_rate: 0,
                channels: 2
            })
        ));
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
            controls.clone(),
            44_100,
            2,
        )
        .expect("a converter sets up");
        let mut out = vec![0i16; 8];
        assert_eq!(paused.next_frames(&mut out), 0, "paused yields nothing");

        // Now playing: the four full-scale stereo frames convert to i16::MAX.
        let (tx2, _rx2) = audio_event_channel();
        let controls2 = AudioOutputControls::new(1.0);
        controls2.set_state(AudioState::Playing);
        let mut playing = DrainSource::new(
            AudioDrain::new(controls2.clone(), playing_source(4), tx2, 100),
            controls2,
            44_100,
            2,
        )
        .expect("a converter sets up");
        let mut out2 = vec![0i16; 8];
        assert_eq!(playing.next_frames(&mut out2), 4);
        assert!(out2.iter().all(|&s| s == i16::MAX));
    }

    /// A 48 kHz source through `DrainSource` comes out at 44.1 kHz: streaming a
    /// whole 48 kHz second yields ~44 100 frames, not 48 000, so playback isn't
    /// slowed and pitched down. (Without the resampler this yields 48 000.)
    #[test]
    fn drain_source_resamples_to_the_receiver_rate() {
        // A ring large enough to hold the whole input without a concurrent reader.
        let (mut sink, stream, _ready) =
            bae_audio::track_stream::create_track_stream_pair_with_capacity(48_000, 2, 48_000 * 2);
        // 20 000 frames of 48 kHz stereo, then finish so the source drains.
        sink.push_samples(&vec![0.0f32; 20_000 * 2]);
        sink.mark_finished();
        let source = Arc::new(Mutex::new(PlaybackSource::new(stream, track_fmt())));

        let controls = AudioOutputControls::new(1.0);
        controls.set_state(AudioState::Playing);
        let (tx, _rx) = audio_event_channel();
        let mut ds = DrainSource::new(
            AudioDrain::new(controls.clone(), source, tx, 100),
            controls,
            48_000,
            2,
        )
        .expect("a converter sets up");

        let mut total = 0usize;
        let mut out = vec![0i16; 352 * 2];
        // Pull until the source drains and the resampler tail is flushed.
        for _ in 0..1000 {
            let f = ds.next_frames(&mut out);
            if f == 0 {
                break;
            }
            total += f;
        }
        // 20 000 frames at 48 kHz → 20 000 × 44100/48000 = 18 375 at 44.1 kHz.
        assert!(
            (total as i64 - 18_375).abs() < 400,
            "expected ~18375 output frames at 44.1 kHz, got {total}"
        );
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
