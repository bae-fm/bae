use super::*;
use crate::airplay::fake_audio_intake::AudioIntake;

/// The unencrypted packetizer emits the RTP header and raw little-endian PCM,
/// advancing sequence and timestamp per packet — a golden fixture.
#[test]
fn packetizer_is_golden_unencrypted() {
    let mut p = Packetizer::new(
        0xDEAD_BEEF,
        PayloadCrypto::Raop(RaopCipher::none()),
        2,
        RtpPosition { sequence: 100, timestamp: 1000 },
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
    let mut enc = Packetizer::new(1, PayloadCrypto::Raop(cipher), 2, RtpPosition::START);
    let mut plain =
        Packetizer::new(1, PayloadCrypto::Raop(RaopCipher::none()), 2, RtpPosition::START);
    // 16 stereo frames = 64 bytes payload = four whole AES blocks.
    let pcm: Vec<i16> = (0..32).collect();
    let e = enc.packet(&pcm);
    let p = plain.packet(&pcm);
    assert_eq!(e[..12], p[..12], "headers match");
    assert_ne!(e[12..], p[12..], "payload is encrypted");
}

/// The pacer sends its burst at the anchor, then keeps pace with the clock,
/// and never re-owes frames it already sent.
#[test]
fn pacer_sends_its_burst_then_tracks_the_clock() {
    // 44.1 kHz, ~2 s latency: the burst is shorter than the latency.
    let mut pacer = Pacer::new(88_200);
    assert_eq!(pacer.step(Duration::ZERO), Step { silent_frames: 0, packets: BURST_PACKETS });
    for _ in 0..BURST_PACKETS {
        pacer.record_packet();
    }
    // Nothing more is due until the playback clock advances.
    assert_eq!(pacer.step(Duration::ZERO).packets, 0);
    // After 0.25 s, 11025 more frames are due: 11025 / 352 = 31 packets.
    assert_eq!(pacer.step(Duration::from_millis(250)), Step { silent_frames: 0, packets: 31 });
    assert_eq!(pacer.passed(), BURST_PACKETS * u64::from(FRAMES_PER_PACKET));
}

/// A receiver whose latency is shorter than the burst gets only its latency
/// at the anchor. Sabotage: send the whole burst, and the sync packets start
/// the receiver partway into it.
#[test]
fn pacer_sends_no_more_than_the_latency_at_the_anchor() {
    let mut pacer = Pacer::new(11_025);
    // 11025 / 352 = 31 whole packets.
    assert_eq!(pacer.step(Duration::ZERO).packets, 31);
}

/// The packets a source has at each moment of a test: none in `dry`, and
/// every packet asked for otherwise.
struct DrySpell {
    dry: std::ops::Range<Duration>,
}

impl DrySpell {
    fn has_frames_at(&self, at: Duration) -> bool {
        !self.dry.contains(&at)
    }
}

/// Drive `pacer` as the audio sender does, waking every 5 ms until `until`,
/// with a source that is dry in `spell.dry`. A sender that finds the source
/// dry waits until it has frames again, and each packet reaches `intake` as
/// it is sent. Each packet must land on the timeline ahead of the frame the
/// receiver plays as it arrives, or the receiver would never play it.
fn pace_through(
    pacer: &mut Pacer,
    intake: &mut AudioIntake,
    spell: &DrySpell,
    until: Duration,
) -> u16 {
    let mut sequence = 0u16;
    let mut waiting = false;
    let mut at = Duration::ZERO;
    while at <= until {
        if !(waiting && !spell.has_frames_at(at)) {
            waiting = false;
            for _ in 0..pacer.step(at).packets {
                if !spell.has_frames_at(at) {
                    waiting = true;
                    break;
                }
                let playing = pacer.playing(at).max(0) as u64;
                assert!(
                    pacer.passed() >= playing,
                    "a packet sent at {at:?} lands {} frames behind the frame playing",
                    playing - pacer.passed()
                );
                intake.arrive(at, sequence);
                sequence = sequence.wrapping_add(1);
                pacer.record_packet();
            }
        }
        at += Duration::from_millis(5);
    }
    sequence
}

/// A stream's start sends a Mac's AirPlay Receiver no more at once than it
/// keeps, so the receiver drops none of it, and the sender keeps pace with
/// the clock after. Sabotage: send the whole 2 s latency lead at the start,
/// and the receiver drops every packet past its 90th.
#[test]
fn a_stream_start_sends_no_more_than_a_mac_receiver_keeps() {
    let mut intake = AudioIntake::mac();
    let mut pacer = Pacer::new(88_200);
    let never_dry = DrySpell { dry: Duration::ZERO..Duration::ZERO };

    // 3 s of stream.
    pace_through(&mut pacer, &mut intake, &never_dry, Duration::from_secs(3));

    assert_eq!(intake.dropped(), &[] as &[u16]);
    // The burst, then 3 s of audio: 132300 / 352 = 375 whole packets.
    assert_eq!(pacer.passed(), (BURST_PACKETS + 375) * u64::from(FRAMES_PER_PACKET));
}

/// A source that runs dry partway and then has frames again leaves the
/// frames the timeline ran past silent: the sender sends no more at once than
/// a Mac's AirPlay Receiver keeps, and lands the audio that came back where
/// the receiver has not played yet. Sabotage: drop the cap on what the pacer
/// owes, and it sends the 3 s it fell behind at once, so the receiver drops
/// all but 90 of them and the first lands long after its frame played.
#[test]
fn a_source_that_runs_dry_and_returns_sends_no_more_than_a_mac_receiver_keeps() {
    let mut intake = AudioIntake::mac();
    let mut pacer = Pacer::new(88_200);
    let spell = DrySpell { dry: Duration::from_secs(1)..Duration::from_secs(4) };

    let sent = pace_through(&mut pacer, &mut intake, &spell, Duration::from_secs(6));

    assert_eq!(intake.dropped(), &[] as &[u16]);
    // The start's burst and 1 s of audio, then the burst after the source
    // came back and 2 s of audio: everything but the 3 s left silent.
    let packets = |frames: u64| frames / u64::from(FRAMES_PER_PACKET);
    assert!(
        u64::from(sent).abs_diff(2 * BURST_PACKETS + packets(3 * 44_100)) <= 2,
        "sent {sent} packets"
    );
}

/// Once the sender has sent more than the latency since the stream started,
/// the receiver has played all but the latency of it. Sabotage: count every
/// frame sent as not played, and the position falls further behind with
/// each packet.
#[test]
fn the_audio_not_played_stops_at_the_latency() {
    let mut intake = AudioIntake::mac();
    let mut pacer = Pacer::new(88_200);
    let never_dry = DrySpell { dry: Duration::ZERO..Duration::ZERO };
    // A moment the clock reaches a whole packet: 375 packets in.
    let whole = bae_util::sample_time::frame_start(375 * u64::from(FRAMES_PER_PACKET), 44_100);

    pace_through(&mut pacer, &mut intake, &never_dry, whole);
    for _ in 0..pacer.step(whole).packets {
        pacer.record_packet();
    }

    assert_eq!(pacer.unplayed(whole), 88_200);
}

/// After the source comes back from running dry, the audio not played is the
/// audio sent before the source ran dry that the receiver has not reached,
/// and the burst sent after: the frames left silent between them don't count.
/// Sabotage: count the silent frames as audio, and the position shown falls
/// behind by them.
#[test]
fn the_audio_not_played_leaves_out_the_silence_while_the_source_was_dry() {
    let mut intake = AudioIntake::mac();
    let mut pacer = Pacer::new(88_200);
    let back = Duration::from_millis(3800);
    let spell = DrySpell { dry: Duration::from_secs(3)..back };

    pace_through(&mut pacer, &mut intake, &spell, back - Duration::from_millis(5));
    let sent_before = pacer.passed();
    let step = pacer.step(back);
    assert!(step.silent_frames > 0, "the sender fell more than a burst behind");
    for _ in 0..step.packets {
        pacer.record_packet();
    }

    let playing = pacer.playing(back) as u64;
    let burst = BURST_PACKETS * u64::from(FRAMES_PER_PACKET);
    assert_eq!(step.packets, BURST_PACKETS, "the sender sends one burst");
    assert_eq!(pacer.unplayed(back), sent_before - playing + burst);
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

/// A clock that stands at the anchor, so the burst is due at once and
/// nothing after it.
struct StandingClock;
impl MonotonicClock for StandingClock {
    fn elapsed(&self) -> Duration {
        Duration::ZERO
    }
    fn reset(&self) {}
}

/// A clock that stands a fixed time past the anchor.
struct FixedClock(Duration);
impl MonotonicClock for FixedClock {
    fn elapsed(&self) -> Duration {
        self.0
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
    audio_thread_pacing_by(diagnostics, source, stop, wake, failed, Arc::new(StandingClock))
}

fn audio_thread_pacing_by(
    diagnostics: &Diagnostics,
    source: Box<dyn PcmSource>,
    stop: Arc<AtomicBool>,
    wake: PcmWake,
    failed: AirPlayFailed,
    clock: Arc<dyn MonotonicClock>,
) -> (ReportedThread, RaopStreamControl) {
    let control = RaopStreamControl::new(88_200, clock, StreamFailure::new(failed));
    // No sync sender listens.
    let (anchored, _) = std::sync::mpsc::channel();
    let handle = spawn_audio_thread(
        diagnostics,
        AudioThread {
            source,
            wake,
            packetizer: Packetizer::new(
                1,
                PayloadCrypto::Raop(RaopCipher::none()),
                2,
                RtpPosition::START,
            ),
            socket: UdpSocket::bind(("127.0.0.1", 0)).unwrap(),
            dst: "127.0.0.1:9".parse().unwrap(),
            stop,
            control: control.clone(),
            anchored,
        },
    )
    .expect("spawn the audio sender");
    (handle, control)
}

/// A clock that a session started before its setup: it reads the seconds a
/// receiver held SETUP until it is reset, and stands still after. Each read
/// is told to the test.
struct StartedBeforeSetupClock {
    reset: AtomicBool,
    reads: std::sync::mpsc::Sender<Duration>,
}
impl MonotonicClock for StartedBeforeSetupClock {
    fn elapsed(&self) -> Duration {
        let elapsed = if self.reset.load(Ordering::Acquire) {
            Duration::ZERO
        } else {
            Duration::from_secs(10)
        };
        self.reads.send(elapsed).ok();
        elapsed
    }
    fn reset(&self) {
        self.reset.store(true, Ordering::Release);
    }
}

/// The sender paces from when the audio starts, so the seconds a receiver
/// held SETUP are owed no audio: at the start it sends its burst from the
/// first packet, which RECORD named, and nothing more. Sabotage: drop the
/// clock's reset at the thread's start, and the sender leaves the setup's
/// ten seconds silent and sends its burst after them.
#[test]
fn the_sender_owes_no_audio_for_the_time_setup_took() {
    let stop = Arc::new(AtomicBool::new(false));
    let wake = PcmWake::unattached();
    let (source, _packets, _drained) = FixedSource::holding(2000);
    let (reads_tx, reads) = std::sync::mpsc::channel();
    let clock =
        Arc::new(StartedBeforeSetupClock { reset: AtomicBool::new(false), reads: reads_tx });
    let (handle, control) = audio_thread_pacing_by(
        &Diagnostics::noop(),
        Box::new(source),
        stop.clone(),
        wake.clone(),
        AirPlayFailed::ignored(),
        clock,
    );

    // The sender sends every packet due at one read before it reads the
    // clock again, so after the second read the first read's packets are out.
    for _ in 0..2 {
        reads
            .recv_timeout(bae_util::test_channels::HANG_GUARD)
            .expect("the sender reads its clock");
    }
    stop.store(true, Ordering::Release);
    wake.wake();
    handle.join();
    assert_eq!(
        control.next_packet(),
        RtpPosition {
            sequence: BURST_PACKETS as u16,
            timestamp: BURST_PACKETS as u32 * FRAMES_PER_PACKET
        }
    );
}

/// Read sync packets from `receiver` until one carries the extension bit,
/// which marks the first after the stream starts or resumes, and give its
/// playing and next frames.
fn first_sync_packet(receiver: &UdpSocket) -> SyncPoint {
    loop {
        let mut packet = [0u8; 64];
        let (len, _) = receiver.recv_from(&mut packet).expect("the sender sends a sync packet");
        assert_eq!(len, 20);
        assert_eq!(packet[1], 0xD4, "a sync packet");
        if packet[0] == 0x90 {
            return SyncPoint {
                playing: u32::from_be_bytes(packet[4..8].try_into().unwrap()),
                next: u32::from_be_bytes(packet[16..20].try_into().unwrap()),
            };
        }
    }
}

/// A sync packet tells the receiver the frame due on the pacer's timeline,
/// which the clock runs, and the frame it should be playing now, a latency
/// behind: so the timeline moves on while the source has sent nothing.
/// Sabotage: count from the frames sent, and the timeline stands at the
/// start; count the latency onto the next frame, and the receiver plays each
/// frame as it arrives and runs out.
#[test]
fn a_sync_packet_puts_the_frame_due_a_latency_ahead_of_the_one_playing() {
    let receiver = UdpSocket::bind(("127.0.0.1", 0)).unwrap();
    receiver.set_read_timeout(Some(bae_util::test_channels::HANG_GUARD)).unwrap();
    // 3 s past the start, with nothing sent.
    let control = RaopStreamControl::new(
        88_200,
        Arc::new(FixedClock(Duration::from_secs(3))),
        StreamFailure::new(AirPlayFailed::ignored()),
    );
    let (anchored, anchors) = std::sync::mpsc::channel();
    let sender = spawn_sync_thread(
        &Diagnostics::noop(),
        UdpSocket::bind(("127.0.0.1", 0)).unwrap(),
        receiver.local_addr().unwrap(),
        control,
        anchors,
    )
    .expect("spawn the sync sender");

    anchored.send(()).unwrap();
    let sync = first_sync_packet(&receiver);
    drop(anchored);
    sender.join();

    let due = BURST_PACKETS as u32 * FRAMES_PER_PACKET + 3 * 44_100;
    assert_eq!(sync.next, due, "the frame due 3 s past the start");
    assert_eq!(sync.playing, due - 88_200, "the frame playing now, a latency before it");
}

/// A resume sends a sync packet right after its burst, marked as the first
/// after a FLUSH, so the receiver knows where the resumed audio plays before
/// it plays it: the clock stands at the resume, and the packet puts the
/// resumed audio's first frame ahead of the frame playing. Sabotage: send
/// sync packets only each second, and none after the resume carries the
/// extension bit.
#[test]
fn a_resume_sends_a_sync_packet_before_its_audio_plays() {
    let receiver = UdpSocket::bind(("127.0.0.1", 0)).unwrap();
    receiver.set_read_timeout(Some(bae_util::test_channels::HANG_GUARD)).unwrap();
    let (source, _packets, _drained) = FixedSource::holding(2000);
    let failure = StreamFailure::new(AirPlayFailed::ignored());
    let control = RaopStreamControl::new(88_200, Arc::new(StandingClock), failure.clone());
    let stream = RaopStream::spawn(
        &Diagnostics::noop(),
        PcmFeed::new(Box::new(source), PcmWake::unattached()),
        PayloadCrypto::Raop(RaopCipher::none()),
        StreamEndpoints {
            receiver: IpAddr::from([127, 0, 0, 1]),
            audio_port: 9,
            control_port: receiver.local_addr().unwrap().port(),
        },
        RtpAudio { ssrc: 1, channels: 2 },
        StreamSockets::open(&Diagnostics::noop(), &failure).unwrap(),
        control.clone(),
    )
    .expect("the stream starts");
    let burst = BURST_PACKETS as u32 * FRAMES_PER_PACKET;

    let start = first_sync_packet(&receiver);
    assert_eq!(start.next, burst, "the start's sync packet follows its burst");

    control.reanchor();
    let resume = first_sync_packet(&receiver);
    let resumed_at = control.next_packet().timestamp - burst;
    drop(stream);

    assert_eq!(resumed_at, burst, "the resume starts where the start's timeline stood");
    assert_eq!(resume.next, resumed_at + burst, "the sync packet follows the resume's burst");
    assert_eq!(
        resumed_at.wrapping_sub(resume.playing),
        88_200 - burst,
        "the resumed audio plays the latency less its burst after the resume"
    );
}

/// After each packet the packetizer's next packet has the next sequence
/// number, wrapping at 65536, and a timestamp the packet's frames later; its
/// `RTP-Info` names both.
#[test]
fn the_next_packet_follows_the_packet_made() {
    let start = RtpPosition { sequence: 65_535, timestamp: 1000 };
    let mut p = Packetizer::new(1, PayloadCrypto::Raop(RaopCipher::none()), 2, start);
    assert_eq!(p.next(), start);
    assert_eq!(p.next().rtp_info(), "seq=65535;rtptime=1000");

    // Two stereo frames.
    p.packet(&[0, 0, 0, 0]);
    assert_eq!(p.next(), RtpPosition { sequence: 0, timestamp: 1002 });
    assert_eq!(p.next().rtp_info(), "seq=0;rtptime=1002");
}

/// What the audio sender did to a [`StillClock`].
#[derive(Debug, PartialEq)]
enum ClockUse {
    Read,
    Reset,
}

/// A clock that stays at the anchor, so the sender owes only its burst, and
/// tells the test each time the sender reads or resets it.
struct StillClock {
    uses: std::sync::mpsc::Sender<ClockUse>,
}
impl MonotonicClock for StillClock {
    fn elapsed(&self) -> Duration {
        self.uses.send(ClockUse::Read).ok();
        Duration::ZERO
    }
    fn reset(&self) {
        self.uses.send(ClockUse::Reset).ok();
    }
}

/// Wait until the sender has read its clock twice from here. The sender
/// sends every packet due at one read before it reads the clock again, so
/// then the first read's packets are out.
fn wait_for_two_reads(uses: &std::sync::mpsc::Receiver<ClockUse>) {
    let mut reads = 0;
    while reads < 2 {
        let used = uses
            .recv_timeout(bae_util::test_channels::HANG_GUARD)
            .expect("the sender reads its clock");
        if used == ClockUse::Read {
            reads += 1;
        }
    }
}

/// A re-anchor restarts the pacing with a fresh burst, no longer than the
/// stream's start sent, so a resume fits what the receiver keeps too. The
/// receiver plays none of a burst until it has the latency's worth, so after
/// each burst the audio it has not played is the burst, not what came before.
/// Sabotage: re-anchor with the latency as the burst, and the resume sends
/// 250 packets at once; drop the anchor the re-anchor records, and the audio
/// not played counts the start's burst too.
#[test]
fn a_reanchor_sends_one_burst_again() {
    let stop = Arc::new(AtomicBool::new(false));
    let wake = PcmWake::unattached();
    let (source, _packets, _drained) = FixedSource::holding(2000);
    let (uses_tx, uses) = std::sync::mpsc::channel();
    let (handle, control) = audio_thread_pacing_by(
        &Diagnostics::noop(),
        Box::new(source),
        stop.clone(),
        wake.clone(),
        AirPlayFailed::ignored(),
        Arc::new(StillClock { uses: uses_tx }),
    );
    let burst = BURST_PACKETS as u16;
    let burst_time =
        bae_util::sample_time::frame_start(BURST_PACKETS * u64::from(FRAMES_PER_PACKET), 44_100);

    wait_for_two_reads(&uses);
    assert_eq!(control.next_packet().sequence, burst, "the start sends one burst");
    assert_eq!(control.unheard(), burst_time, "the receiver has played none of it");

    control.reanchor();
    loop {
        let used =
            uses.recv_timeout(bae_util::test_channels::HANG_GUARD).expect("the sender re-anchors");
        if used == ClockUse::Reset {
            break;
        }
    }
    wait_for_two_reads(&uses);
    stop.store(true, Ordering::Release);
    wake.wake();
    handle.join();

    assert_eq!(control.next_packet().sequence, 2 * burst, "the re-anchor sends one burst more");
    assert_eq!(control.unheard(), burst_time, "the receiver plays nothing sent before the resume");
}

/// A source whose pull panics: a bug in the samples' way to the sender.
struct PanickingSource;
impl PcmSource for PanickingSource {
    fn next_frames(&mut self, _out: &mut [i16]) -> usize {
        bae_diagnostics::test_panic::panics("ring index out of range")
    }
}

/// An audio sender that panics fails the stream, which ends the session
/// rather than leaving the receiver silent while the session says it
/// streams: playback is told, and the sender doesn't report the panic again
/// (the panic hook reported it). Sabotage: drop the `on_panic` in
/// `spawn_audio_thread`, and the stream reads as healthy; drop the report in
/// `mark_panicked`, and playback is never told.
#[test]
fn a_sender_that_panics_fails_the_stream_and_tells_playback() {
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
    assert_eq!(bugs.reports(), Vec::new());
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
    assert_eq!(
        control.next_packet(),
        RtpPosition { sequence: 3, timestamp: 3 * FRAMES_PER_PACKET }
    );
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
    assert_eq!(
        control.next_packet(),
        RtpPosition { sequence: 2, timestamp: 2 * FRAMES_PER_PACKET }
    );
    stop.store(true, Ordering::Release);
    wake.wake();
    handle.join();
}

/// A re-anchor re-marks the stream: after it fires, a packet carries the RTP
/// marker bit again.
#[test]
fn reanchor_re_marks_the_next_packet() {
    let mut p = Packetizer::new(1, PayloadCrypto::Raop(RaopCipher::none()), 2, RtpPosition::START);
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
/// keep a sender of the sync sender's channel outside the audio sender, and
/// the drop waits forever.
#[test]
fn dropping_the_stream_stops_every_thread() {
    let (source, _packets, _drained) = FixedSource::holding(0);
    let failure = StreamFailure::new(AirPlayFailed::ignored());
    let stream = RaopStream::spawn(
        &Diagnostics::noop(),
        PcmFeed::new(Box::new(source), PcmWake::unattached()),
        PayloadCrypto::Raop(RaopCipher::none()),
        StreamEndpoints { receiver: IpAddr::from([127, 0, 0, 1]), audio_port: 9, control_port: 9 },
        RtpAudio { ssrc: 1, channels: 2 },
        StreamSockets::open(&Diagnostics::noop(), &failure).unwrap(),
        RaopStreamControl::new(88_200, Arc::new(StandingClock), failure),
    )
    .expect("the stream starts");

    let (dropped_tx, dropped) = std::sync::mpsc::channel();
    bae_util::test_spawn::thread(move || {
        drop(stream);
        dropped_tx.send(()).unwrap();
    });

    dropped
        .recv_timeout(bae_util::test_channels::HANG_GUARD)
        .expect("dropping the stream never finished stopping its threads");
}
