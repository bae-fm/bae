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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
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
    /// frames and return how many *frames* were produced. A short read (fewer
    /// than `out.len() / channels`) means the source has drained; the stream
    /// zero-pads the remainder. `0` means fully drained.
    fn next_frames(&mut self, out: &mut [i16]) -> usize;
}

/// Turns each packet's PCM into RTP audio bytes, advancing the sequence and
/// timestamp and encrypting the payload. Pure and deterministic.
pub struct Packetizer {
    ssrc: u32,
    cipher: RaopCipher,
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
        cipher: RaopCipher,
        channels: u32,
        initial_sequence: u16,
        initial_timestamp: u32,
    ) -> Self {
        Packetizer {
            ssrc,
            cipher,
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
        self.cipher.encrypt_packet(&mut payload);

        let packet = rtp::audio_packet(
            self.first,
            self.sequence,
            self.timestamp,
            self.ssrc,
            &payload,
        );

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

/// A monotonic clock the pacing loop reads elapsed time from. Injectable so tests
/// don't sleep.
pub trait MonotonicClock: Send + Sync {
    fn elapsed(&self) -> Duration;
}

/// The real clock: elapsed since construction.
pub struct SystemClock {
    start: Instant,
}

impl SystemClock {
    pub fn new() -> Self {
        SystemClock {
            start: Instant::now(),
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
        self.start.elapsed()
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

/// The live RAOP audio stream: the three UDP flows and their threads. Dropping it
/// stops every thread. Created by the session after RECORD.
pub struct RaopStream {
    stop: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
    frames_sent: Arc<std::sync::atomic::AtomicU64>,
}

impl RaopStream {
    /// Spawn the audio, sync, and timing threads over already-bound sockets.
    /// `timing_socket` and `control_socket` are the sockets whose ports the
    /// session announced in SETUP: the receiver sends timing requests to the
    /// former, and sync packets go out from the latter.
    #[allow(clippy::too_many_arguments)]
    pub fn spawn(
        source: Box<dyn PcmSource>,
        cipher: RaopCipher,
        endpoints: StreamEndpoints,
        ssrc: u32,
        sample_rate: u32,
        channels: u32,
        initial_timestamp: u32,
        timing_socket: UdpSocket,
        control_socket: UdpSocket,
        clock: Arc<dyn MonotonicClock>,
    ) -> std::io::Result<Self> {
        let audio_socket = UdpSocket::bind((IpAddr::from([0, 0, 0, 0]), 0))?;
        let audio_dst = SocketAddr::new(endpoints.receiver, endpoints.audio_port);
        let control_dst = SocketAddr::new(endpoints.receiver, endpoints.control_port);

        let stop = Arc::new(AtomicBool::new(false));
        let frames_sent = Arc::new(std::sync::atomic::AtomicU64::new(0));

        let threads = vec![
            spawn_audio_thread(
                source,
                Packetizer::new(ssrc, cipher, channels, 0, initial_timestamp),
                Pacer::new(sample_rate, endpoints.latency_frames),
                audio_socket,
                audio_dst,
                clock.clone(),
                stop.clone(),
                frames_sent.clone(),
            ),
            spawn_sync_thread(
                control_socket,
                control_dst,
                initial_timestamp,
                endpoints.latency_frames,
                frames_sent.clone(),
                sample_rate,
                stop.clone(),
            ),
            spawn_timing_thread(timing_socket, stop.clone()),
        ];

        Ok(RaopStream {
            stop,
            threads,
            frames_sent,
        })
    }

    /// Frames handed to the receiver so far — the basis for the audible position.
    pub fn frames_sent(&self) -> u64 {
        self.frames_sent.load(Ordering::Relaxed)
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

/// Pull PCM one packet at a time, pace by the clock, and send. Ends when the
/// source drains or the stop flag is set.
#[allow(clippy::too_many_arguments)]
fn spawn_audio_thread(
    mut source: Box<dyn PcmSource>,
    mut packetizer: Packetizer,
    mut pacer: Pacer,
    socket: UdpSocket,
    dst: SocketAddr,
    clock: Arc<dyn MonotonicClock>,
    stop: Arc<AtomicBool>,
    frames_sent: Arc<std::sync::atomic::AtomicU64>,
) -> JoinHandle<()> {
    let channels = 2usize; // L16 stereo; the session negotiates 44.1/16/2.
    std::thread::spawn(move || {
        let packet_len = FRAMES_PER_PACKET as usize * channels;
        let mut buf = vec![0i16; packet_len];
        let mut drained = false;
        while !stop.load(Ordering::Acquire) && !drained {
            let due = pacer.packets_due(clock.elapsed());
            if due == 0 {
                std::thread::sleep(Duration::from_millis(5));
                continue;
            }
            for _ in 0..due {
                if stop.load(Ordering::Acquire) {
                    break;
                }
                buf.iter_mut().for_each(|s| *s = 0);
                let frames = source.next_frames(&mut buf);
                if frames == 0 {
                    drained = true;
                    break;
                }
                let packet = packetizer.packet(&buf);
                if let Err(e) = socket.send_to(&packet, dst) {
                    debug!("airplay audio send failed: {e}");
                }
                pacer.record_packet();
                frames_sent.store(pacer.frames_sent(), Ordering::Relaxed);
            }
        }
        debug!("airplay audio thread ended (drained={drained})");
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
        let mut p = Packetizer::new(0xDEAD_BEEF, RaopCipher::none(), 2, 100, 1000);
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
        let mut enc = Packetizer::new(1, cipher, 2, 0, 0);
        let mut plain = Packetizer::new(1, RaopCipher::none(), 2, 0, 0);
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

    /// A drained source stops the audio thread promptly. Uses a fixed-frame source
    /// and a clock pinned far ahead so all packets are immediately due.
    #[test]
    fn audio_thread_ends_when_source_drains() {
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
        struct FarAheadClock;
        impl MonotonicClock for FarAheadClock {
            fn elapsed(&self) -> Duration {
                Duration::from_secs(3600)
            }
        }

        let socket = UdpSocket::bind(("127.0.0.1", 0)).unwrap();
        let frames_sent = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let handle = spawn_audio_thread(
            Box::new(FixedSource { packets_left: 3 }),
            Packetizer::new(1, RaopCipher::none(), 2, 0, 0),
            Pacer::new(44_100, 88_200),
            socket,
            "127.0.0.1:9".parse().unwrap(),
            Arc::new(FarAheadClock),
            stop,
            frames_sent.clone(),
        );
        handle.join().unwrap();
        assert_eq!(
            frames_sent.load(Ordering::Relaxed),
            3 * u64::from(FRAMES_PER_PACKET),
            "exactly the three packets' worth of frames were sent before draining"
        );
    }
}
