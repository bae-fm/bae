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
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use tracing::{debug, warn};

use super::crypto::RaopCipher;
use super::rtp::{self, NtpTime};

/// Frames of audio per RTP packet — the RAOP constant echoed in the SDP fmtp.
pub const FRAMES_PER_PACKET: u32 = 352;

/// A source of decoded PCM the stream pulls one packet at a time. In production
/// this adapts the playback ring buffer; tests drive it with a scripted ramp.
pub trait PcmSource: Send {
    /// Fill `out` (interleaved i16, up to one packet's worth) with the next
    /// frames and return how many *frames* were produced. `0` means no audio is
    /// available right now — the stream is paused, starved, or between tracks —
    /// and the stream sends nothing this packet rather than treating it as the
    /// end: the session's lifetime, not the source, decides when streaming stops.
    fn next_frames(&mut self, out: &mut [i16]) -> usize;
}

/// How a packet's PCM payload is encrypted — the one thing the two dialects
/// differ on in the send path. RAOP encrypts the payload in place (AES-128-CBC or
/// not at all); AirPlay 2 seals it with ChaCha20-Poly1305, the RTP header's
/// timestamp+SSRC as associated data, and the 8-byte nonce trailing the datagram.
pub enum PayloadCrypto {
    Raop(RaopCipher),
    Ap2(super::airplay2::Ap2AudioCipher),
}

/// Turns each packet's PCM into RTP audio bytes, advancing the sequence and
/// timestamp and encrypting the payload. Pure and deterministic.
pub struct Packetizer {
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
    pub fn new(
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
    pub fn packet(&mut self, frames: &[i16]) -> Vec<u8> {
        let mut payload = Vec::with_capacity(frames.len() * 2);
        for sample in frames {
            payload.extend_from_slice(&sample.to_le_bytes());
        }

        let packet = match &mut self.crypto {
            PayloadCrypto::Raop(cipher) => {
                cipher.encrypt_packet(&mut payload);
                rtp::audio_packet(
                    self.first,
                    self.sequence,
                    self.timestamp,
                    self.ssrc,
                    &payload,
                )
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
        self.timestamp = self
            .timestamp
            .wrapping_add(frames.len() as u32 / self.channels.max(1));
        packet
    }

    /// The next packet's RTP timestamp — what a sync packet advertises as "now".
    pub fn next_timestamp(&self) -> u32 {
        self.timestamp
    }

    /// The next packet's sequence number.
    pub fn next_sequence(&self) -> u16 {
        self.sequence
    }

    /// Re-anchor after a FLUSH: the next packet carries the marker bit again so
    /// the receiver treats the resumed stream as a fresh start. The RTP timestamp
    /// keeps advancing — the sync packet re-establishes the mapping.
    pub fn reanchor(&mut self) {
        self.first = true;
    }
}

/// Decides how many packets are due by a moment in the stream, so the sender runs
/// slightly ahead of real time to keep the receiver's buffer full. Timing only —
/// no I/O — so a test drives it with virtual elapsed durations.
pub struct Pacer {
    sample_rate: u32,
    latency_frames: u32,
    frames_sent: u64,
}

impl Pacer {
    /// `latency_frames` is how far ahead of the playback clock the sender stays
    /// (the receiver's reported audio latency, ~2 s).
    pub fn new(sample_rate: u32, latency_frames: u32) -> Self {
        Pacer {
            sample_rate,
            latency_frames,
            frames_sent: 0,
        }
    }

    /// Whole packets still owed at `elapsed` since the anchor: the target is the
    /// latency lead plus the frames the playback clock has reached, minus what's
    /// already been sent.
    pub fn packets_due(&self, elapsed: Duration) -> u64 {
        let played = (elapsed.as_secs_f64() * f64::from(self.sample_rate)) as u64;
        let target = u64::from(self.latency_frames) + played;
        target.saturating_sub(self.frames_sent) / u64::from(FRAMES_PER_PACKET)
    }

    /// Account for one packet just sent.
    pub fn record_packet(&mut self) {
        self.frames_sent += u64::from(FRAMES_PER_PACKET);
    }

    /// Total frames handed to the receiver so far — the basis for the position the
    /// UI shows (minus the receiver latency).
    pub fn frames_sent(&self) -> u64 {
        self.frames_sent
    }
}

/// A monotonic clock the pacing loop reads elapsed time from, and re-anchors on a
/// FLUSH. Injectable so tests don't sleep.
pub trait MonotonicClock: Send + Sync {
    /// Time since the clock was created or last [`reset`](MonotonicClock::reset).
    fn elapsed(&self) -> Duration;
    /// Restart the elapsed count from now (the re-anchor point after a FLUSH).
    fn reset(&self);
}

/// The real clock: elapsed since the last reset.
pub struct SystemClock {
    start: Mutex<Instant>,
}

impl SystemClock {
    pub fn new() -> Self {
        SystemClock {
            start: Mutex::new(Instant::now()),
        }
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
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

/// The UDP endpoints a SETUP response resolves to, plus the negotiated latency.
pub struct StreamEndpoints {
    pub receiver: IpAddr,
    /// Where audio datagrams go.
    pub audio_port: u16,
    /// Where sync datagrams go.
    pub control_port: u16,
    /// The receiver's audio latency in frames (its reported latency, else the
    /// RAOP default of ~2 s at the sample rate).
    pub latency_frames: u32,
}

/// The live push-audio stream shared by both dialects: the UDP audio flow, the
/// timing responder, and — for RAOP — the periodic sync packets. Dropping it
/// stops every thread. Created by a session after RECORD.
pub struct RaopStream {
    stop: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
}

#[derive(Clone)]
pub(super) struct RaopStreamControl {
    frames_sent: Arc<AtomicU64>,
    reanchor: Arc<AtomicU64>,
    failed: Arc<AtomicBool>,
}

impl RaopStreamControl {
    pub(super) fn new() -> Self {
        Self {
            frames_sent: Arc::new(AtomicU64::new(0)),
            reanchor: Arc::new(AtomicU64::new(0)),
            failed: Arc::new(AtomicBool::new(false)),
        }
    }

    pub(super) fn frames_sent(&self) -> u64 {
        self.frames_sent.load(Ordering::Relaxed)
    }

    pub(super) fn reanchor(&self) {
        self.reanchor.fetch_add(1, Ordering::Release);
    }

    pub(super) fn has_failed(&self) -> bool {
        self.failed.load(Ordering::Relaxed)
    }
}

impl RaopStream {
    /// Spawn the audio, (optional) sync, and timing threads over already-bound
    /// sockets. `timing_socket` and `control_socket` are the sockets whose ports
    /// the session announced in SETUP: the receiver sends timing requests to the
    /// former, and RAOP sync packets go out from the latter. AirPlay 2 anchors its
    /// timeline with SETRATEANCHORTIME instead, so it passes `send_sync = false`.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn spawn(
        source: Box<dyn PcmSource>,
        crypto: PayloadCrypto,
        endpoints: StreamEndpoints,
        ssrc: u32,
        sample_rate: u32,
        channels: u32,
        initial_timestamp: u32,
        timing_socket: UdpSocket,
        control_socket: UdpSocket,
        clock: Arc<dyn MonotonicClock>,
        send_sync: bool,
        control: RaopStreamControl,
    ) -> std::io::Result<Self> {
        let audio_socket = UdpSocket::bind((IpAddr::from([0, 0, 0, 0]), 0))?;
        let audio_dst = SocketAddr::new(endpoints.receiver, endpoints.audio_port);
        let control_dst = SocketAddr::new(endpoints.receiver, endpoints.control_port);

        let stop = Arc::new(AtomicBool::new(false));
        let mut threads = vec![
            spawn_audio_thread(AudioThread {
                source,
                packetizer: Packetizer::new(ssrc, crypto, channels, 0, initial_timestamp),
                pacer: Pacer::new(sample_rate, endpoints.latency_frames),
                sample_rate,
                latency_frames: endpoints.latency_frames,
                socket: audio_socket,
                dst: audio_dst,
                clock: clock.clone(),
                stop: stop.clone(),
                frames_sent: control.frames_sent.clone(),
                reanchor: control.reanchor.clone(),
                failed: control.failed.clone(),
            }),
            spawn_timing_thread(timing_socket, stop.clone()),
        ];
        if send_sync {
            threads.push(spawn_sync_thread(
                control_socket,
                control_dst,
                initial_timestamp,
                endpoints.latency_frames,
                control.frames_sent.clone(),
                sample_rate,
                stop.clone(),
            ));
        }

        Ok(RaopStream { stop, threads })
    }
}

impl Drop for RaopStream {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        for thread in self.threads.drain(..) {
            if thread.join().is_err() {
                warn!("airplay stream thread panicked during shutdown");
            }
        }
    }
}

/// The state one audio-sender thread owns.
struct AudioThread {
    source: Box<dyn PcmSource>,
    packetizer: Packetizer,
    pacer: Pacer,
    sample_rate: u32,
    latency_frames: u32,
    socket: UdpSocket,
    dst: SocketAddr,
    clock: Arc<dyn MonotonicClock>,
    stop: Arc<AtomicBool>,
    frames_sent: Arc<AtomicU64>,
    reanchor: Arc<AtomicU64>,
    failed: Arc<AtomicBool>,
}

/// Pull PCM one packet at a time, pace by the clock, and send. Persistent: a
/// source with nothing to give (paused, starved, between tracks) yields no
/// packet but the thread lives on — only [`RaopStream`] being dropped (the stop
/// flag) ends it, so pause and gapless track swaps don't tear the stream down. A
/// re-anchor resets the pacing lead and clock and re-marks the next packet.
fn spawn_audio_thread(mut t: AudioThread) -> JoinHandle<()> {
    let channels = 2usize; // L16 stereo; the session negotiates 44.1/16/2.
                           // Consecutive audio-send failures before the receiver is declared dead — a
                           // handful of packets (~50 ms) so a transient blip doesn't end the session.
    const FAILURE_THRESHOLD: u32 = 20;
    std::thread::spawn(move || {
        let mut buf = vec![0i16; FRAMES_PER_PACKET as usize * channels];
        let mut seen_reanchor = t.reanchor.load(Ordering::Acquire);
        let mut consecutive_failures = 0u32;
        while !t.stop.load(Ordering::Acquire) {
            let epoch = t.reanchor.load(Ordering::Acquire);
            if epoch != seen_reanchor {
                t.pacer = Pacer::new(t.sample_rate, t.latency_frames);
                t.clock.reset();
                t.packetizer.reanchor();
                seen_reanchor = epoch;
            }

            let due = t.pacer.packets_due(t.clock.elapsed());
            let mut sent_any = false;
            for _ in 0..due {
                if t.stop.load(Ordering::Acquire) {
                    break;
                }
                buf.iter_mut().for_each(|s| *s = 0);
                if t.source.next_frames(&mut buf) == 0 {
                    // Nothing to send this packet; wait rather than end.
                    break;
                }
                let packet = t.packetizer.packet(&buf);
                match t.socket.send_to(&packet, t.dst) {
                    Ok(_) => consecutive_failures = 0,
                    Err(e) => {
                        debug!("airplay audio send failed: {e}");
                        consecutive_failures += 1;
                        if consecutive_failures >= FAILURE_THRESHOLD {
                            // The receiver is unreachable — surface the death rather
                            // than erroring silently forever.
                            t.failed.store(true, Ordering::Release);
                        }
                    }
                }
                t.pacer.record_packet();
                t.frames_sent
                    .fetch_add(u64::from(FRAMES_PER_PACKET), Ordering::Relaxed);
                sent_any = true;
            }
            if due == 0 || !sent_any {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        debug!("airplay audio thread ended");
    })
}

/// Send a sync packet immediately (extension bit set) and then ~every second, so
/// the receiver keeps its clock anchored to ours.
fn spawn_sync_thread(
    socket: UdpSocket,
    dst: SocketAddr,
    initial_timestamp: u32,
    latency_frames: u32,
    frames_sent: Arc<std::sync::atomic::AtomicU64>,
    sample_rate: u32,
    stop: Arc<AtomicBool>,
) -> JoinHandle<()> {
    std::thread::spawn(move || {
        let mut first = true;
        let mut last_sent = Instant::now() - Duration::from_secs(1);
        while !stop.load(Ordering::Acquire) {
            if last_sent.elapsed() >= Duration::from_secs(1) {
                let now_ts = initial_timestamp
                    .wrapping_add(frames_sent.load(Ordering::Relaxed) as u32)
                    .wrapping_add(latency_frames);
                let played = now_ts.wrapping_sub(latency_frames);
                let packet = rtp::sync_packet(first, played, NtpTime::now(), now_ts);
                if let Err(e) = socket.send_to(&packet, dst) {
                    debug!("airplay sync send failed: {e}");
                }
                first = false;
                last_sent = Instant::now();
            }
            let _ = sample_rate;
            std::thread::sleep(Duration::from_millis(100));
        }
    })
}

/// Answer the receiver's timing requests until stopped.
fn spawn_timing_thread(socket: UdpSocket, stop: Arc<AtomicBool>) -> JoinHandle<()> {
    std::thread::spawn(move || {
        // A read timeout lets the thread notice the stop flag between requests.
        let _ = socket.set_read_timeout(Some(Duration::from_millis(200)));
        let mut buf = [0u8; 64];
        while !stop.load(Ordering::Acquire) {
            match socket.recv_from(&mut buf) {
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
                Err(ref e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut => {}
                Err(e) => {
                    debug!("airplay timing socket error: {e}");
                    break;
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The unencrypted packetizer emits the RTP header and raw little-endian PCM,
    /// advancing sequence and timestamp per packet — a golden fixture.
    #[test]
    fn packetizer_is_golden_unencrypted() {
        let mut p = Packetizer::new(
            0xDEAD_BEEF,
            PayloadCrypto::Raop(RaopCipher::none()),
            2,
            100,
            1000,
        );
        // One "packet" of two stereo frames: L,R,L,R.
        let pkt = p.packet(&[0x0102, 0x0304, 0x0506, 0x0708]);
        assert_eq!(
            pkt,
            vec![
                0x80, 0xE0, // first packet: marker set
                0x00, 0x64, // sequence 100
                0x00, 0x00, 0x03, 0xE8, // timestamp 1000
                0xDE, 0xAD, 0xBE, 0xEF, // ssrc
                0x02, 0x01, 0x04, 0x03, 0x06, 0x05, 0x08, 0x07, // LE PCM
            ]
        );
        // Two frames advanced the timestamp by 2; the next packet clears marker.
        assert_eq!(p.next_sequence(), 101);
        assert_eq!(p.next_timestamp(), 1002);
        let pkt2 = p.packet(&[0, 0, 0, 0]);
        assert_eq!(pkt2[1], 0x60, "later packets clear the marker");
        assert_eq!(&pkt2[2..4], &[0x00, 0x65], "sequence advanced to 101");
    }

    /// Encryption changes the payload but not the RTP header.
    #[test]
    fn packetizer_encrypts_payload_only() {
        use super::super::crypto::apple_public_key;
        let cipher = RaopCipher::from_key_iv(&apple_public_key(), [0x11; 16], [0x22; 16]).unwrap();
        let mut enc = Packetizer::new(1, PayloadCrypto::Raop(cipher), 2, 0, 0);
        let mut plain = Packetizer::new(1, PayloadCrypto::Raop(RaopCipher::none()), 2, 0, 0);
        // 16 stereo frames = 64 bytes payload = four whole AES blocks.
        let pcm: Vec<i16> = (0..32).collect();
        let e = enc.packet(&pcm);
        let p = plain.packet(&pcm);
        assert_eq!(e[..12], p[..12], "headers match");
        assert_ne!(e[12..], p[12..], "payload is encrypted");
    }

    /// The pacer runs ahead by the latency, then keeps pace with the clock, and
    /// never re-owes frames it already sent.
    #[test]
    fn pacer_leads_by_latency_then_tracks_the_clock() {
        // 44.1 kHz, ~2 s latency lead.
        let mut pacer = Pacer::new(44_100, 88_200);
        // At t=0, the whole latency lead is due: 88200 / 352 = 250 packets.
        assert_eq!(pacer.packets_due(Duration::ZERO), 250);
        for _ in 0..250 {
            pacer.record_packet();
        }
        // Nothing more is due until the playback clock advances.
        assert_eq!(pacer.packets_due(Duration::ZERO), 0);
        // After 1 s, 44100 more frames are due: 44100 / 352 = 125 packets.
        assert_eq!(pacer.packets_due(Duration::from_secs(1)), 125);
        assert_eq!(pacer.frames_sent(), 250 * u64::from(FRAMES_PER_PACKET));
    }

    /// A source that yields a fixed number of packets then reports nothing.
    struct FixedSource {
        packets_left: usize,
    }
    impl PcmSource for FixedSource {
        fn next_frames(&mut self, out: &mut [i16]) -> usize {
            if self.packets_left == 0 {
                return 0;
            }
            self.packets_left -= 1;
            out.fill(1);
            out.len() / 2
        }
    }

    /// A clock pinned far ahead so every packet is immediately due.
    struct FarAheadClock;
    impl MonotonicClock for FarAheadClock {
        fn elapsed(&self) -> Duration {
            Duration::from_secs(3600)
        }
        fn reset(&self) {}
    }

    fn audio_thread(
        source: Box<dyn PcmSource>,
        stop: Arc<AtomicBool>,
    ) -> (JoinHandle<()>, Arc<AtomicU64>, Arc<AtomicU64>) {
        let frames_sent = Arc::new(AtomicU64::new(0));
        let reanchor = Arc::new(AtomicU64::new(0));
        let handle = spawn_audio_thread(AudioThread {
            source,
            packetizer: Packetizer::new(1, PayloadCrypto::Raop(RaopCipher::none()), 2, 0, 0),
            pacer: Pacer::new(44_100, 88_200),
            sample_rate: 44_100,
            latency_frames: 88_200,
            socket: UdpSocket::bind(("127.0.0.1", 0)).unwrap(),
            dst: "127.0.0.1:9".parse().unwrap(),
            clock: Arc::new(FarAheadClock),
            stop,
            frames_sent: frames_sent.clone(),
            reanchor: reanchor.clone(),
            failed: Arc::new(AtomicBool::new(false)),
        });
        (handle, frames_sent, reanchor)
    }

    /// A source that drains does NOT end the thread — the stream is persistent, so
    /// it keeps running (idle) until the stop flag is set, and it sent exactly the
    /// frames the source produced.
    #[test]
    fn audio_thread_is_persistent_across_a_drained_source() {
        let stop = Arc::new(AtomicBool::new(false));
        let (handle, frames_sent, _reanchor) =
            audio_thread(Box::new(FixedSource { packets_left: 3 }), stop.clone());

        // Wait for the three packets to be sent; the thread stays alive after.
        let deadline = Instant::now() + Duration::from_secs(2);
        while frames_sent.load(Ordering::Relaxed) < 3 * u64::from(FRAMES_PER_PACKET) {
            assert!(Instant::now() < deadline, "packets were not sent in time");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            !handle.is_finished(),
            "the thread persists past a drained source"
        );

        stop.store(true, Ordering::Release);
        handle.join().unwrap();
        assert_eq!(
            frames_sent.load(Ordering::Relaxed),
            3 * u64::from(FRAMES_PER_PACKET)
        );
    }

    /// A re-anchor re-marks the stream: after it fires, a packet carries the RTP
    /// marker bit again.
    #[test]
    fn reanchor_re_marks_the_next_packet() {
        let mut p = Packetizer::new(1, PayloadCrypto::Raop(RaopCipher::none()), 2, 0, 0);
        let first = p.packet(&[0, 0, 0, 0]);
        assert_eq!(first[1], 0xE0, "first packet is marked");
        let second = p.packet(&[0, 0, 0, 0]);
        assert_eq!(second[1], 0x60, "later packets are unmarked");
        p.reanchor();
        let after = p.packet(&[0, 0, 0, 0]);
        assert_eq!(
            after[1], 0xE0,
            "the packet after a re-anchor is marked again"
        );
    }
}
