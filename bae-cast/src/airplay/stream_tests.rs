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

/// A source that yields the packets a test gives it, then reports nothing,
/// and tells the test each time it is asked with none left: the sender has
/// sent every packet the source had by then.
struct FixedSource {
    packets_left: Arc<std::sync::atomic::AtomicUsize>,
    drained: std::sync::mpsc::Sender<()>,
}
impl FixedSource {
    fn holding(
        packets: usize,
    ) -> (Self, Arc<std::sync::atomic::AtomicUsize>, std::sync::mpsc::Receiver<()>) {
        let packets_left = Arc::new(std::sync::atomic::AtomicUsize::new(packets));
        let (drained, drained_rx) = std::sync::mpsc::channel();
        (Self { packets_left: packets_left.clone(), drained }, packets_left, drained_rx)
    }
}
impl PcmSource for FixedSource {
    fn next_frames(&mut self, out: &mut [i16]) -> usize {
        if self.packets_left.load(Ordering::Acquire) == 0 {
            // A test that stopped listening needs no more.
            self.drained.send(()).ok();
            return 0;
        }
        self.packets_left.fetch_sub(1, Ordering::AcqRel);
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
    wake: PcmWake,
) -> (ReportedThread, RaopStreamControl) {
    audio_thread_reporting(diagnostics, source, stop, wake, AirPlayFailed::ignored())
}

fn audio_thread_reporting(
    diagnostics: &Diagnostics,
    source: Box<dyn PcmSource>,
    stop: Arc<AtomicBool>,
    wake: PcmWake,
    failed: AirPlayFailed,
) -> (ReportedThread, RaopStreamControl) {
    let control = RaopStreamControl::new(88_200, failed);
    let handle = spawn_audio_thread(
        diagnostics,
        AudioThread {
            source,
            wake,
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
/// streams: playback is told, and the panic is reported. Sabotage: drop the
/// `on_panic` in `spawn_audio_thread`, and the stream reads as healthy; drop
/// the report in `mark_panicked`, and playback is never told.
#[test]
fn a_sender_that_panics_fails_the_stream_and_is_reported() {
    let bugs = Arc::new(bae_diagnostics::RecordingBugReporter::default());
    let diagnostics = Diagnostics::noop().reporting_bugs_to(bugs.clone());
    let (reported_tx, reported) = std::sync::mpsc::channel();
    let failed = AirPlayFailed::reporting_to(move || reported_tx.send(()).unwrap());
    let (handle, control) = audio_thread_reporting(
        &diagnostics,
        Box::new(PanickingSource),
        Arc::new(AtomicBool::new(false)),
        PcmWake::unattached(),
        failed,
    );

    handle.join();
    reported.try_recv().expect("the failure is reported to playback");

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
/// it keeps running (waiting) until the stop flag is set and the wake rung,
/// and it sent exactly the frames the source produced.
#[test]
fn audio_thread_is_persistent_across_a_drained_source() {
    let stop = Arc::new(AtomicBool::new(false));
    let wake = PcmWake::unattached();
    let (source, _packets, drained_rx) = FixedSource::holding(3);
    let (handle, control) =
        audio_thread(&Diagnostics::noop(), Box::new(source), stop.clone(), wake.clone());

    // The source is asked again once the three packets are sent; the thread
    // waits for more after.
    drained_rx
        .recv_timeout(bae_util::test_channels::HANG_GUARD)
        .expect("the sender asks the drained source again");
    wake.wait_until_waiting();
    assert!(!handle.is_finished(), "the thread persists past a drained source");

    stop.store(true, Ordering::Release);
    wake.wake();
    handle.join();
    assert_eq!(control.frames_sent(), 3 * u64::from(FRAMES_PER_PACKET));
}

/// A sender waiting on a drained source sends what the source has once the
/// wake says it may have frames. Sabotage: give the source packets without
/// waking, and the sender waits forever.
#[test]
fn a_waiting_sender_sends_once_woken() {
    let stop = Arc::new(AtomicBool::new(false));
    let wake = PcmWake::unattached();
    let (source, packets, drained_rx) = FixedSource::holding(0);
    let (handle, control) =
        audio_thread(&Diagnostics::noop(), Box::new(source), stop.clone(), wake.clone());
    drained_rx
        .recv_timeout(bae_util::test_channels::HANG_GUARD)
        .expect("the sender asks the empty source");
    wake.wait_until_waiting();

    packets.store(2, Ordering::Release);
    wake.wake();

    drained_rx
        .recv_timeout(bae_util::test_channels::HANG_GUARD)
        .expect("the woken sender sends the packets and asks again");
    assert_eq!(control.frames_sent(), 2 * u64::from(FRAMES_PER_PACKET));
    stop.store(true, Ordering::Release);
    wake.wake();
    handle.join();
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

/// Dropping a running stream stops all three threads: the sender waiting on
/// an empty source, the sync sender between packets and the timing responder
/// in its read. Sabotage: leave out the datagram to the timing socket, or
/// keep the sync channel open, and the drop waits forever.
#[test]
fn dropping_the_stream_stops_every_thread() {
    let (source, _packets, _drained) = FixedSource::holding(0);
    let stream = RaopStream::spawn(
        &Diagnostics::noop(),
        PcmFeed::new(Box::new(source), PcmWake::unattached()),
        PayloadCrypto::Raop(RaopCipher::none()),
        StreamEndpoints { receiver: IpAddr::from([127, 0, 0, 1]), audio_port: 9, control_port: 9 },
        RtpAudio { ssrc: 1, sample_rate: 44_100, channels: 2, initial_timestamp: 0 },
        StreamSockets::bind().unwrap(),
        Arc::new(FarAheadClock),
        true,
        RaopStreamControl::new(88_200, AirPlayFailed::ignored()),
    )
    .expect("the stream starts");

    let (dropped_tx, dropped) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        drop(stream);
        dropped_tx.send(()).unwrap();
    });

    dropped
        .recv_timeout(bae_util::test_channels::HANG_GUARD)
        .expect("dropping the stream never finished stopping its threads");
}
