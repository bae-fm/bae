use super::*;

/// The unencrypted packetizer emits the RTP header and raw little-endian PCM,
/// advancing sequence and timestamp per packet — a golden fixture.
#[test]
fn packetizer_is_golden_unencrypted() {
    let mut p = Packetizer::new(0xDEAD_BEEF, PayloadCrypto::Raop(RaopCipher::none()), 2, 100, 1000);
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
    let pkt2 = p.packet(&[0, 0, 0, 0]);
    assert_eq!(pkt2[1], 0x60, "later packets clear the marker");
    assert_eq!(&pkt2[2..4], &[0x00, 0x65], "sequence advanced to 101");
    assert_eq!(
        &pkt2[4..8],
        &[0x00, 0x00, 0x03, 0xEA],
        "two stereo frames advanced the timestamp to 1002"
    );
}

/// Encryption changes the payload but not the RTP header.
#[test]
fn packetizer_encrypts_payload_only() {
    use crate::airplay::crypto::apple_public_key;
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

/// A source that yields a fixed number of packets then reports nothing, and
/// tells a test each time it is asked after that: the sender has sent every
/// packet the source made by then.
struct FixedSource {
    packets_left: usize,
    drained: std::sync::mpsc::Sender<()>,
}
impl PcmSource for FixedSource {
    fn next_frames(&mut self, out: &mut [i16]) -> usize {
        if self.packets_left == 0 {
            // A test that stopped listening needs no more.
            self.drained.send(()).ok();
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
    diagnostics: &Diagnostics,
    source: Box<dyn PcmSource>,
    stop: Arc<AtomicBool>,
) -> (ReportedThread, RaopStreamControl) {
    let control = RaopStreamControl::new(88_200);
    let handle = spawn_audio_thread(
        diagnostics,
        AudioThread {
            source,
            packetizer: Packetizer::new(1, PayloadCrypto::Raop(RaopCipher::none()), 2, 0, 0),
            pacer: Pacer::new(44_100, 88_200),
            sample_rate: 44_100,
            socket: UdpSocket::bind(("127.0.0.1", 0)).unwrap(),
            dst: "127.0.0.1:9".parse().unwrap(),
            clock: Arc::new(FarAheadClock),
            stop,
            control: control.clone(),
        },
    )
    .expect("spawn the audio sender");
    (handle, control)
}

/// A source whose pull panics: a bug in the samples' way to the sender.
struct PanickingSource;
impl PcmSource for PanickingSource {
    fn next_frames(&mut self, _out: &mut [i16]) -> usize {
        panic!("ring index out of range")
    }
}

/// An audio sender that panics fails the stream, which ends the session
/// rather than leaving the receiver silent while the session says it
/// streams, and the panic is reported. Sabotage: drop the `on_panic` in
/// `spawn_audio_thread`, and the stream reads as healthy.
#[test]
fn a_sender_that_panics_fails_the_stream_and_is_reported() {
    let bugs = Arc::new(bae_diagnostics::RecordingBugReporter::default());
    let diagnostics = Diagnostics::noop().reporting_bugs_to(bugs.clone());
    let (handle, control) =
        audio_thread(&diagnostics, Box::new(PanickingSource), Arc::new(AtomicBool::new(false)));

    handle.join();

    assert_eq!(
        control.failure().as_deref(),
        Some("airplay audio sender panicked: ring index out of range")
    );
    assert_eq!(
        bugs.reports(),
        vec![(
            "airplay audio sender".to_string(),
            "airplay audio sender panicked: ring index out of range".to_string()
        )]
    );
}

/// A source that drains does NOT end the thread — the stream is persistent, so
/// it keeps running (idle) until the stop flag is set, and it sent exactly the
/// frames the source produced.
#[test]
fn audio_thread_is_persistent_across_a_drained_source() {
    let stop = Arc::new(AtomicBool::new(false));
    let (drained, drained_rx) = std::sync::mpsc::channel();
    let (handle, control) = audio_thread(
        &Diagnostics::noop(),
        Box::new(FixedSource { packets_left: 3, drained }),
        stop.clone(),
    );

    // The source is asked again once the three packets are sent; the thread
    // stays alive after.
    drained_rx
        .recv_timeout(bae_util::test_channels::HANG_GUARD)
        .expect("the sender asks the drained source again");
    assert!(!handle.is_finished(), "the thread persists past a drained source");

    stop.store(true, Ordering::Release);
    handle.join();
    assert_eq!(control.frames_sent(), 3 * u64::from(FRAMES_PER_PACKET));
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
    assert_eq!(after[1], 0xE0, "the packet after a re-anchor is marked again");
}
