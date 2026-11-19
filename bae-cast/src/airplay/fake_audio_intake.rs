//! What a test receiver keeps of the audio packets that reach it.
//!
//! A Mac's AirPlay Receiver, measured, kept about 90 packets of a 250-packet
//! burst and dropped the rest. This intake holds packets until the receiver
//! takes them out, and the receiver takes out one packet for each packet of
//! audio the clock plays: packets that come faster than that pile up in the
//! intake, and those past what it holds are lost. A real receiver takes packets
//! out faster than it plays them, so a sender this intake drops nothing of
//! drops nothing at a Mac either.

use std::time::Duration;

use bae_playback::airplay_output::PCM_SAMPLE_RATE;

use super::stream::FRAMES_PER_PACKET;

/// The packets a Mac's AirPlay Receiver holds of a burst.
const MAC_INTAKE_PACKETS: u64 = 90;

/// The intake of one receiver, from the start of a stream.
pub(crate) struct AudioIntake {
    capacity: u64,
    /// The packets in the intake as of `taken_to`.
    held: u64,
    /// The clock time up to which the receiver has taken packets out.
    taken_to: Duration,
    /// The sequence numbers of the packets it dropped, in order.
    dropped: Vec<u16>,
}

impl AudioIntake {
    /// The intake of a Mac's AirPlay Receiver, empty at the stream's start.
    pub(crate) fn mac() -> Self {
        AudioIntake {
            capacity: MAC_INTAKE_PACKETS,
            held: 0,
            taken_to: Duration::ZERO,
            dropped: Vec::new(),
        }
    }

    /// Take the packet numbered `sequence`, which reached the receiver at
    /// `at` on the test's clock: the receiver keeps it if the intake has
    /// room, and drops it if not.
    pub(crate) fn arrive(&mut self, at: Duration, sequence: u16) {
        self.take_out_to(at);
        if self.held < self.capacity {
            self.held += 1;
        } else {
            self.dropped.push(sequence);
        }
    }

    /// The sequence numbers of the packets the receiver dropped.
    pub(crate) fn dropped(&self) -> &[u16] {
        &self.dropped
    }

    /// Take out of the intake the packets the clock played from `taken_to`
    /// to `at`.
    fn take_out_to(&mut self, at: Duration) {
        assert!(at >= self.taken_to, "packets reach the receiver in clock order");
        let played = |time| {
            bae_util::sample_time::frame_at(time, PCM_SAMPLE_RATE) / u64::from(FRAMES_PER_PACKET)
        };
        self.held = self.held.saturating_sub(played(at) - played(self.taken_to));
        self.taken_to = at;
    }
}
