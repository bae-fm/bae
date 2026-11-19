//! The RAOP send path: interleaved PCM → 352-frame RTP audio packets → the three
//! UDP flows a receiver expects (audio, sync, timing).
//!
//! bae keeps decoding locally and pushes 16-bit little-endian PCM to the receiver
//! under the `L16` codec — the interoperable path the reference senders take
//! (pyatv, MIT), rather than compressing to ALAC (no approved reference frames
//! it, and a container-less deterministic frame is what the golden fixtures and
//! the receiver both want). Each packet carries [`FRAMES_PER_PACKET`] frames,
//! optionally AES-128-CBC encrypted ([`super::crypto::RaopCipher`]).
//!
//! Three concerns, each isolated so the timing logic is tested without a socket
//! or a real clock: [`Packetizer`] turns one packet's PCM into wire bytes
//! (deterministic — golden fixtures); [`Pacer`] decides how many packets are due
//! by a given elapsed time (driven by an injectable clock, so tests never sleep);
//! and [`RaopStream`] is the thin runtime that owns the sockets and threads.

use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use bae_diagnostics::{Diagnostics, ReportedThread};
use bae_playback::airplay_output::{AirPlayFailed, PcmSource, PcmWake, PCM_SAMPLE_RATE};
use bae_util::task_failure::Bug;
use tracing::{debug, warn};

use super::crypto::RaopCipher;
use super::rtp::{self, NtpTime};

/// Frames of audio per RTP packet — the RAOP constant echoed in the SDP fmtp.
pub(crate) const FRAMES_PER_PACKET: u32 = 352;

/// How a packet's PCM payload is encrypted — the one thing the two dialects
/// differ on in the send path. RAOP encrypts the payload in place (AES-128-CBC or
/// not at all); AirPlay 2 seals it with ChaCha20-Poly1305, the RTP header's
/// timestamp+SSRC as associated data, and the 8-byte nonce trailing the datagram.
pub(crate) enum PayloadCrypto {
    Raop(RaopCipher),
    Ap2(super::airplay2::Ap2AudioCipher),
}

/// Turns each packet's PCM into RTP audio bytes, advancing the sequence and
/// timestamp and encrypting the payload. Pure and deterministic.
pub(crate) struct Packetizer {
    ssrc: u32,
    crypto: PayloadCrypto,
    channels: u32,
    sequence: u16,
    timestamp: u32,
    first: bool,
}

impl Packetizer {
    /// Start at `initial_timestamp` (the RECORD anchor). `first` marks the first
    /// packet after RECORD/FLUSH, which carries the RTP marker bit.
    pub(crate) fn new(
        ssrc: u32,
        crypto: PayloadCrypto,
        channels: u32,
        initial_sequence: u16,
        initial_timestamp: u32,
    ) -> Self {
        Packetizer {
            ssrc,
            crypto,
            channels,
            sequence: initial_sequence,
            timestamp: initial_timestamp,
            first: true,
        }
    }

    /// Serialize one packet from `frames` interleaved i16 samples (already
    /// zero-padded to a whole packet). Advances the sequence (wrapping) and the
    /// timestamp by the frame count.
    pub(crate) fn packet(&mut self, frames: &[i16]) -> Vec<u8> {
        let mut payload = Vec::with_capacity(frames.len() * 2);
        for sample in frames {
            payload.extend_from_slice(&sample.to_le_bytes());
        }

        let packet = match &mut self.crypto {
            PayloadCrypto::Raop(cipher) => {
                cipher.encrypt_packet(&mut payload);
                rtp::audio_packet(self.first, self.sequence, self.timestamp, self.ssrc, &payload)
            }
            PayloadCrypto::Ap2(cipher) => {
                // The AAD is the header's timestamp+SSRC (bytes 4..12); build the
                // header first, seal against it, then append the sealed payload
                // (ciphertext+tag+8-byte nonce).
                let mut packet =
                    rtp::audio_packet(self.first, self.sequence, self.timestamp, self.ssrc, &[]);
                let sealed = cipher.seal(&packet[4..12], &payload);
                packet.extend_from_slice(&sealed);
                packet
            }
        };

        self.first = false;
        self.sequence = self.sequence.wrapping_add(1);
        self.timestamp = self.timestamp.wrapping_add(frames.len() as u32 / self.channels.max(1));
        packet
    }

    /// Re-anchor after a FLUSH: the next packet carries the marker bit again so
    /// the receiver treats the resumed stream as a fresh start. The RTP timestamp
    /// keeps advancing — the sync packet re-establishes the mapping.
    pub(crate) fn reanchor(&mut self) {
        self.first = true;
    }
}

/// Decides how many packets are due by a moment in the stream, so the sender runs
/// slightly ahead of real time to keep the receiver's buffer full. Timing only —
/// no I/O — so a test drives it with virtual elapsed durations.
pub(crate) struct Pacer {
    sample_rate: u32,
    latency_frames: u32,
    frames_sent: u64,
}

impl Pacer {
    /// `latency_frames` is how far ahead of the playback clock the sender stays
    /// (the receiver's reported audio latency, ~2 s).
    pub(crate) fn new(sample_rate: u32, latency_frames: u32) -> Self {
        Pacer { sample_rate, latency_frames, frames_sent: 0 }
    }

    /// Whole packets still owed at `elapsed` since the anchor: the target is the
    /// latency lead plus the frames the playback clock has reached, minus what's
    /// already been sent.
    pub(crate) fn packets_due(&self, elapsed: Duration) -> u64 {
        let played = bae_util::sample_time::frame_at(elapsed, self.sample_rate);
        let target = u64::from(self.latency_frames) + played;
        target.saturating_sub(self.frames_sent) / u64::from(FRAMES_PER_PACKET)
    }

    /// Account for one packet just sent.
    pub(crate) fn record_packet(&mut self) {
        self.frames_sent += u64::from(FRAMES_PER_PACKET);
    }

    /// Total frames handed to the receiver so far.
    #[cfg(test)]
    pub(crate) fn frames_sent(&self) -> u64 {
        self.frames_sent
    }
}

/// A monotonic clock the pacing loop reads elapsed time from, and re-anchors on a
/// FLUSH. Injectable so tests don't sleep.
pub(crate) trait MonotonicClock: Send + Sync {
    /// Time since the clock was created or last [`reset`](MonotonicClock::reset).
    fn elapsed(&self) -> Duration;
    /// Restart the elapsed count from now (the re-anchor point after a FLUSH).
    fn reset(&self);
}

/// The real clock: elapsed since the last reset.
pub(crate) struct SystemClock {
    start: Mutex<Instant>,
}

impl SystemClock {
    pub(crate) fn new() -> Self {
        SystemClock { start: Mutex::new(Instant::now()) }
    }
}

impl MonotonicClock for SystemClock {
    fn elapsed(&self) -> Duration {
        self.start.lock().unwrap().elapsed()
    }

    fn reset(&self) {
        *self.start.lock().unwrap() = Instant::now();
    }
}

/// The UDP endpoints a SETUP response resolves to.
pub(crate) struct StreamEndpoints {
    pub receiver: IpAddr,
    /// Where audio datagrams go.
    pub audio_port: u16,
    /// Where sync datagrams go.
    pub control_port: u16,
}

/// The RTP audio flow a session hands the stream: the synchronization source id
/// its packets are tagged with and the PCM format they carry. Its timeline
/// starts at [`RtpPosition::START`].
pub(super) struct RtpAudio {
    pub(super) ssrc: u32,
    pub(super) sample_rate: u32,
    pub(super) channels: u32,
}

/// A packet's place on a stream's RTP timeline: its sequence number and
/// timestamp, which RECORD and FLUSH name in their `RTP-Info` header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RtpPosition {
    pub(crate) sequence: u16,
    pub(crate) timestamp: u32,
}

impl RtpPosition {
    /// The first packet of every stream bae sends.
    pub(crate) const START: RtpPosition = RtpPosition { sequence: 0, timestamp: 0 };

    /// The `RTP-Info` header value that names this packet.
    pub(crate) fn rtp_info(&self) -> String {
        format!("seq={};rtptime={}", self.sequence, self.timestamp)
    }

    /// The packet `frames` frames after this one. Every packet carries
    /// [`FRAMES_PER_PACKET`] frames, since the sender pads the last.
    fn after(self, frames: u64) -> Self {
        RtpPosition {
            sequence: self.sequence.wrapping_add((frames / u64::from(FRAMES_PER_PACKET)) as u16),
            timestamp: self.timestamp.wrapping_add(frames as u32),
        }
    }
}

/// The sockets a session opens before SETUP so it can announce their ports:
/// the timing responder answers on one from the start, since a receiver
/// syncs its clock with bae while it sets the session up (a Mac's AirPlay
/// Receiver holds SETUP (stream) until it has, as OwnTone's and pyatv's
/// timing servers, which run before any SETUP, allow), and sync packets leave
/// from `control`.
pub(super) struct StreamSockets {
    timing: TimingResponder,
    control: UdpSocket,
}

impl StreamSockets {
    /// Bind both sockets on any local port and start answering timing
    /// requests. A responder that panics fails the stream through `failure`.
    pub(super) fn open(
        diagnostics: &Diagnostics,
        failure: &StreamFailure,
    ) -> std::io::Result<Self> {
        let timing = UdpSocket::bind((IpAddr::from([0, 0, 0, 0]), 0))?;
        Ok(Self {
            timing: TimingResponder::start(diagnostics, timing, failure.clone())?,
            control: UdpSocket::bind((IpAddr::from([0, 0, 0, 0]), 0))?,
        })
    }

    /// The local port the receiver sends timing requests to.
    pub(super) fn timing_port(&self) -> u16 {
        self.timing.port
    }

    /// The local port sync packets leave from.
    pub(super) fn control_port(&self) -> std::io::Result<u16> {
        Ok(self.control.local_addr()?.port())
    }
}

/// The thread that answers a receiver's timing requests, from when the
/// session opens its sockets until it ends. Dropping it stops the thread.
struct TimingResponder {
    port: u16,
    stop: Arc<AtomicBool>,
    /// Joined only once its read was woken; see `Drop`.
    thread: Option<ReportedThread>,
}

impl TimingResponder {
    fn start(
        diagnostics: &Diagnostics,
        socket: UdpSocket,
        failure: StreamFailure,
    ) -> std::io::Result<Self> {
        let port = socket.local_addr()?.port();
        let stop = Arc::new(AtomicBool::new(false));
        let thread = spawn_timing_thread(diagnostics, socket, failure, stop.clone())?;
        Ok(TimingResponder { port, stop, thread: Some(thread) })
    }
}

impl Drop for TimingResponder {
    /// Stop the responder: its read blocks until a datagram arrives, so one
    /// goes to its socket once `stop` is set.
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let woken = UdpSocket::bind((IpAddr::from([127, 0, 0, 1]), 0))
            .and_then(|socket| socket.send_to(&[], (IpAddr::from([127, 0, 0, 1]), self.port)));
        if let Some(thread) = self.thread.take() {
            match woken {
                Ok(_) => thread.join(),
                // Nothing would end its read but the receiver's next timing
                // request, so it is left to exit on that rather than block
                // the teardown.
                Err(error) => {
                    warn!("couldn't wake the airplay timing responder to stop it: {error}")
                }
            }
        }
    }
}

/// How often sync packets go out.
const SYNC_INTERVAL: Duration = Duration::from_secs(1);

/// The audio latency in frames a session paces by when its receiver doesn't
/// report one: the ~2 s buffer AirPort Express expects.
pub(super) const DEFAULT_LATENCY_FRAMES: u32 = 88_200;

/// The error for a control connection a panic left partway through a request:
/// it can't be read in step with its replies again, so it takes no more
/// requests.
pub(super) fn poisoned_connection<T>(poisoned: std::sync::PoisonError<T>) -> std::io::Error {
    std::io::Error::other(format!(
        "a panic left the control connection partway through a request: {poisoned}"
    ))
}

/// Where the audio sender's frames come from, and what it waits on while the
/// source has none.
pub(crate) struct PcmFeed {
    source: Box<dyn PcmSource>,
    wake: PcmWake,
}

impl PcmFeed {
    pub(crate) fn new(source: Box<dyn PcmSource>, wake: PcmWake) -> Self {
        Self { source, wake }
    }
}

/// The live push-audio stream shared by both dialects: the UDP audio flow, the
/// periodic sync packets that anchor the receiver's timeline, and the timing
/// responder the session started. Dropping it stops every thread. Created by
/// a session after its setup.
pub(crate) struct RaopStream {
    stop: Arc<AtomicBool>,
    /// What the audio sender waits on for frames: woken after `stop` is set,
    /// so a waiting sender sees it.
    wake: PcmWake,
    /// Dropped to end the sync sender's wait between packets.
    sync_stop: Option<mpsc::Sender<()>>,
    threads: Vec<ReportedThread>,
    /// Stopped after the other threads, when the stream's fields drop.
    _timing: TimingResponder,
}

/// Why a stream failed for good, once it has, and the playback output told
/// of it once: shared by the timing responder, which runs from before SETUP,
/// and the stream's threads.
#[derive(Clone)]
pub(super) struct StreamFailure {
    cause: Arc<OnceLock<String>>,
    failed: AirPlayFailed,
}

impl StreamFailure {
    /// `failed` is told when the stream fails.
    pub(super) fn new(failed: AirPlayFailed) -> Self {
        Self { cause: Arc::new(OnceLock::new()), failed }
    }

    /// The audio flow has failed persistently — the receiver is unreachable —
    /// the last send failing with `cause`. The first cause is kept and warned;
    /// later ones repeat it.
    fn mark_failed(&self, sends: u32, cause: &std::io::Error) {
        let failure = format!("{sends} audio sends in a row failed: {cause}");
        if self.cause.set(failure.clone()).is_ok() {
            warn!("airplay receiver unreachable: {failure}");
            self.failed.report();
        }
    }

    /// One of the stream's threads panicked, so the receiver gets no more
    /// audio, sync or timing answers from it: the stream has failed.
    fn mark_panicked(&self, failed: Bug) {
        if self.cause.set(failed.to_string()).is_ok() {
            self.failed.report();
        } else {
            debug!("airplay stream already failed before {failed}");
        }
    }
}

/// The shared state a running stream is observed and steered through: how far the
/// send has got, the re-anchor request, why the audio flow died if it has, and
/// the negotiated receiver latency the pacing lead and the audible position both
/// use. A session keeps one and hands a clone to the threads.
#[derive(Clone)]
pub(super) struct RaopStreamControl {
    frames_sent: Arc<AtomicU64>,
    reanchor: Arc<AtomicU64>,
    failure: StreamFailure,
    latency_frames: u32,
}

impl RaopStreamControl {
    /// `latency_frames` is the receiver's audio latency (its reported value, else
    /// the dialect's ~2 s default); `failure` is where the stream says it
    /// failed.
    pub(super) fn new(latency_frames: u32, failure: StreamFailure) -> Self {
        Self {
            frames_sent: Arc::new(AtomicU64::new(0)),
            reanchor: Arc::new(AtomicU64::new(0)),
            failure,
            latency_frames,
        }
    }

    pub(super) fn frames_sent(&self) -> u64 {
        self.frames_sent.load(Ordering::Relaxed)
    }

    /// The packet the sender sends next.
    pub(super) fn next_packet(&self) -> RtpPosition {
        RtpPosition::START.after(self.frames_sent())
    }

    pub(super) fn reanchor(&self) {
        self.reanchor.fetch_add(1, Ordering::Release);
    }

    /// Why the audio flow to the receiver failed for good, when it has.
    pub(super) fn failure(&self) -> Option<String> {
        self.failure.cause.get().cloned()
    }

    /// The receiver's audio latency in frames — how far ahead of the playback
    /// clock the sender paces, and the offset between frames sent and what is
    /// audible.
    pub(super) fn latency_frames(&self) -> u32 {
        self.latency_frames
    }

    /// [`latency_frames`](Self::latency_frames) as time, at the rate the
    /// stream carries PCM: what playback holds its audible position back by.
    pub(super) fn latency(&self) -> Duration {
        bae_util::sample_time::frame_start(u64::from(self.latency_frames), PCM_SAMPLE_RATE)
    }

    /// The re-anchor counter the audio thread watches: a change means restart the
    /// pacing lead and re-mark the stream.
    fn reanchor_epoch(&self) -> u64 {
        self.reanchor.load(Ordering::Acquire)
    }

    /// Account for frames just handed to the receiver.
    fn record_frames(&self, frames: u64) {
        self.frames_sent.fetch_add(frames, Ordering::Relaxed);
    }

    /// The audio flow has failed persistently; see [`StreamFailure`].
    fn mark_failed(&self, sends: u32, cause: &std::io::Error) {
        self.failure.mark_failed(sends, cause);
    }

    /// One of the stream's threads panicked; see [`StreamFailure`].
    fn mark_panicked(&self, failed: Bug) {
        self.failure.mark_panicked(failed);
    }
}

impl RaopStream {
    /// Spawn the audio and sync threads over the sockets the session opened,
    /// whose timing responder already runs. Both dialects anchor the
    /// receiver's timeline with sync packets, as OwnTone and pyatv do for an
    /// AirPlay 2 realtime stream. A thread that panics fails the stream, which
    /// ends the session.
    pub(super) fn spawn(
        diagnostics: &Diagnostics,
        feed: PcmFeed,
        crypto: PayloadCrypto,
        endpoints: StreamEndpoints,
        audio: RtpAudio,
        sockets: StreamSockets,
        clock: Arc<dyn MonotonicClock>,
        control: RaopStreamControl,
    ) -> std::io::Result<Self> {
        let audio_socket = UdpSocket::bind((IpAddr::from([0, 0, 0, 0]), 0))?;
        let audio_dst = SocketAddr::new(endpoints.receiver, endpoints.audio_port);
        let control_dst = SocketAddr::new(endpoints.receiver, endpoints.control_port);

        let PcmFeed { source, wake } = feed;
        let StreamSockets { timing, control: control_socket } = sockets;
        let (sync_stop, sync_stopped) = mpsc::channel();
        // Built before any thread starts, so a thread that fails to start
        // drops it, which stops the ones already running.
        let mut stream = RaopStream {
            stop: Arc::new(AtomicBool::new(false)),
            wake: wake.clone(),
            sync_stop: Some(sync_stop),
            threads: Vec::new(),
            _timing: timing,
        };
        let stop = stream.stop.clone();
        stream.threads.push(spawn_audio_thread(
            diagnostics,
            AudioThread {
                source,
                wake: wake.clone(),
                packetizer: Packetizer::new(
                    audio.ssrc,
                    crypto,
                    audio.channels,
                    RtpPosition::START.sequence,
                    RtpPosition::START.timestamp,
                ),
                pacer: Pacer::new(audio.sample_rate, control.latency_frames()),
                sample_rate: audio.sample_rate,
                socket: audio_socket,
                dst: audio_dst,
                clock: clock.clone(),
                stop: stop.clone(),
                control: control.clone(),
            },
        )?);
        stream.threads.push(spawn_sync_thread(
            diagnostics,
            control_socket,
            control_dst,
            control,
            sync_stopped,
        )?);

        Ok(stream)
    }
}

impl Drop for RaopStream {
    /// Stop the audio and sync threads and wait for each: the audio sender
    /// by its wake, the sync sender by its channel closing. The timing
    /// responder stops when its field drops.
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.wake.wake();
        self.sync_stop.take();
        for thread in self.threads.drain(..) {
            thread.join();
        }
    }
}

/// The state one audio-sender thread owns.
struct AudioThread {
    source: Box<dyn PcmSource>,
    wake: PcmWake,
    packetizer: Packetizer,
    pacer: Pacer,
    sample_rate: u32,
    socket: UdpSocket,
    dst: SocketAddr,
    clock: Arc<dyn MonotonicClock>,
    stop: Arc<AtomicBool>,
    control: RaopStreamControl,
}

/// Pull PCM one packet at a time, pace by the clock, and send. Persistent: a
/// source with nothing to give (paused, starved, between tracks) yields no
/// packet but the thread lives on, waiting on the feed's wake until the source
/// may have frames again — only [`RaopStream`] being dropped (the stop flag)
/// ends it, so pause and gapless track swaps don't tear the stream down. The
/// pacing clock starts with the thread. A re-anchor resets the pacing lead
/// and clock and re-marks the next packet.
fn spawn_audio_thread(
    diagnostics: &Diagnostics,
    mut t: AudioThread,
) -> std::io::Result<ReportedThread> {
    let channels = 2usize; // L16 stereo; the session negotiates 44.1/16/2.
                           // Consecutive audio-send failures before the receiver is declared dead — a
                           // handful of packets (~50 ms) so a transient blip doesn't end the session.
    const FAILURE_THRESHOLD: u32 = 20;
    let on_panic = t.control.clone();
    diagnostics.spawn_thread_reported(
        "airplay audio sender",
        move || {
            // The pacing counts from now, when the audio starts, not from
            // when the session began its setup: a receiver can hold SETUP
            // for seconds while its owner allows the sender, and the time
            // before the stream started is owed no audio.
            t.clock.reset();
            let mut buf = vec![0i16; FRAMES_PER_PACKET as usize * channels];
            let mut seen_reanchor = t.control.reanchor_epoch();
            let mut consecutive_failures = 0u32;
            loop {
                // Read before the stop flag and the pulls, so a stop or new
                // frames that land after this read end the wait below at once.
                let seen = t.wake.changes();
                if t.stop.load(Ordering::Acquire) {
                    break;
                }
                let epoch = t.control.reanchor_epoch();
                if epoch != seen_reanchor {
                    t.pacer = Pacer::new(t.sample_rate, t.control.latency_frames());
                    t.clock.reset();
                    t.packetizer.reanchor();
                    seen_reanchor = epoch;
                }

                let due = t.pacer.packets_due(t.clock.elapsed());
                let mut starved = false;
                for _ in 0..due {
                    if t.stop.load(Ordering::Acquire) {
                        break;
                    }
                    buf.iter_mut().for_each(|s| *s = 0);
                    if t.source.next_frames(&mut buf) == 0 {
                        // Nothing to send this packet; wait rather than end.
                        starved = true;
                        break;
                    }
                    let packet = t.packetizer.packet(&buf);
                    match t.socket.send_to(&packet, t.dst) {
                        Ok(_) => consecutive_failures = 0,
                        Err(e) => {
                            // Each failed packet is noted; the run of them that
                            // declares the receiver dead is warned once, with why.
                            debug!("airplay audio send failed: {e}");
                            consecutive_failures += 1;
                            if consecutive_failures >= FAILURE_THRESHOLD {
                                // The receiver is unreachable — surface the death rather
                                // than erroring silently forever.
                                t.control.mark_failed(consecutive_failures, &e);
                            }
                        }
                    }
                    t.pacer.record_packet();
                    t.control.record_frames(u64::from(FRAMES_PER_PACKET));
                }
                if starved {
                    t.wake.wait_past(seen);
                } else if due == 0 {
                    // No packet is due yet: pace to the clock.
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
            debug!("airplay audio thread ended");
        },
        move |failed| on_panic.mark_panicked(failed),
    )
}

/// Send a sync packet immediately (extension bit set) and then every second, so
/// the receiver keeps its clock anchored to ours, until `stopped`'s sender is
/// dropped.
fn spawn_sync_thread(
    diagnostics: &Diagnostics,
    socket: UdpSocket,
    dst: SocketAddr,
    control: RaopStreamControl,
    stopped: mpsc::Receiver<()>,
) -> std::io::Result<ReportedThread> {
    let on_panic = control.clone();
    diagnostics.spawn_thread_reported(
        "airplay sync sender",
        move || {
            let latency_frames = control.latency_frames();
            let mut first = true;
            loop {
                // The sender runs the latency ahead of what the receiver
                // plays: the next frame it sends plays a latency from now.
                let now_ts = control.next_packet().timestamp;
                let played = now_ts.wrapping_sub(latency_frames);
                let packet = rtp::sync_packet(first, played, NtpTime::now(), now_ts);
                if let Err(e) = socket.send_to(&packet, dst) {
                    debug!("airplay sync send failed: {e}");
                }
                first = false;
                match stopped.recv_timeout(SYNC_INTERVAL) {
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
        },
        move |failed| on_panic.mark_panicked(failed),
    )
}

/// Answer the receiver's timing requests until stopped. The read blocks until
/// a datagram arrives; the responder's drop sends one once `stop` is set.
fn spawn_timing_thread(
    diagnostics: &Diagnostics,
    socket: UdpSocket,
    failure: StreamFailure,
    stop: Arc<AtomicBool>,
) -> std::io::Result<ReportedThread> {
    diagnostics.spawn_thread_reported(
        "airplay timing responder",
        move || {
            let mut buf = [0u8; 64];
            loop {
                let received = socket.recv_from(&mut buf);
                if stop.load(Ordering::Acquire) {
                    break;
                }
                match received {
                    Ok((len, from)) => {
                        let received = NtpTime::now();
                        if let Some(response) =
                            rtp::timing_response(&buf[..len], received, NtpTime::now())
                        {
                            if let Err(e) = socket.send_to(&response, from) {
                                debug!("airplay timing reply failed: {e}");
                            }
                        }
                    }
                    Err(e) => {
                        debug!("airplay timing socket error: {e}");
                        break;
                    }
                }
            }
        },
        move |failed| failure.mark_panicked(failed),
    )
}

#[cfg(test)]
#[path = "stream_tests.rs"]
mod tests;
