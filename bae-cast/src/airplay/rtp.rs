//! RAOP RTP packet formats: the audio data packets bae sends, the sync packets
//! that anchor the receiver's clock, and the timing responder that answers the
//! receiver's NTP queries.
//!
//! Every packet is an RTP-shaped datagram (openairplay spec §7.2). bae, as the
//! sender, drives three UDP flows: audio and sync go out to the receiver; timing
//! comes *in* from the receiver as an NTP request that bae answers. All integers
//! are big-endian (network order).

/// A 64-bit NTP timestamp: seconds since 1900-01-01 in the high word, a binary
/// fraction of a second in the low word.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NtpTime {
    pub seconds: u32,
    pub fraction: u32,
}

impl NtpTime {
    /// Seconds between the NTP epoch (1900) and the Unix epoch (1970).
    const UNIX_TO_NTP: u64 = 2_208_988_800;

    /// The NTP time for a Unix instant given in nanoseconds. Injectable so the
    /// timing responder and sync sender are tested at known clocks.
    pub fn from_unix_nanos(unix_nanos: u128) -> Self {
        let unix_secs = (unix_nanos / 1_000_000_000) as u64;
        let sub_nanos = (unix_nanos % 1_000_000_000) as u64;
        NtpTime {
            seconds: (unix_secs + Self::UNIX_TO_NTP) as u32,
            fraction: ((sub_nanos << 32) / 1_000_000_000) as u32,
        }
    }

    /// Wall-clock now as an NTP timestamp.
    pub fn now() -> Self {
        let since_epoch = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        Self::from_unix_nanos(since_epoch.as_nanos())
    }

    pub fn to_bytes(self) -> [u8; 8] {
        let mut out = [0u8; 8];
        out[0..4].copy_from_slice(&self.seconds.to_be_bytes());
        out[4..8].copy_from_slice(&self.fraction.to_be_bytes());
        out
    }

    pub fn from_bytes(bytes: &[u8; 8]) -> Self {
        NtpTime {
            seconds: u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
            fraction: u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
        }
    }
}

/// RAOP RTP payload types (the low 7 bits of the second header byte).
pub mod payload_type {
    /// Timing request, receiver → sender.
    pub const TIMING_REQUEST: u8 = 0x52;
    /// Timing response, sender → receiver.
    pub const TIMING_RESPONSE: u8 = 0x53;
    /// Sync packet, sender → receiver on the control channel.
    pub const SYNC: u8 = 0x54;
    /// Realtime audio data, sender → receiver.
    pub const AUDIO: u8 = 0x60;
}

/// Serialize one RAOP audio data packet: a 12-byte RTP header (`0x80`, then
/// `0x60` with the marker bit on the first packet after RECORD/FLUSH, the 16-bit
/// sequence, the 32-bit RTP timestamp, and the 32-bit SSRC) followed by the
/// already-encrypted payload.
pub fn audio_packet(
    marker: bool,
    sequence: u16,
    timestamp: u32,
    ssrc: u32,
    payload: &[u8],
) -> Vec<u8> {
    let mut out = Vec::with_capacity(12 + payload.len());
    out.push(0x80);
    out.push(payload_type::AUDIO | if marker { 0x80 } else { 0x00 });
    out.extend_from_slice(&sequence.to_be_bytes());
    out.extend_from_slice(&timestamp.to_be_bytes());
    out.extend_from_slice(&ssrc.to_be_bytes());
    out.extend_from_slice(payload);
    out
}

/// Serialize one RAOP sync packet (20 bytes) for the control channel. `now_ts` is
/// the RTP timestamp of the next audio packet; `played_ts` is what the receiver
/// should be hearing now (`now_ts` minus the receiver latency). The extension bit
/// is set on the first sync after RECORD/FLUSH.
pub fn sync_packet(first: bool, played_ts: u32, ntp: NtpTime, now_ts: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(20);
    out.push(0x80 | if first { 0x10 } else { 0x00 });
    out.push(0x80 | payload_type::SYNC);
    out.extend_from_slice(&0x0007u16.to_be_bytes());
    out.extend_from_slice(&played_ts.to_be_bytes());
    out.extend_from_slice(&ntp.to_bytes());
    out.extend_from_slice(&now_ts.to_be_bytes());
    out
}

/// Answer a receiver's timing request. Given the 32-byte request datagram and the
/// NTP instants at which bae received it and is sending the reply, produce the
/// 32-byte response — echoing the request's transmit timestamp as the origin and
/// filling in receive/transmit, per NTP. Returns `None` for a datagram that isn't
/// a timing request.
pub fn timing_response(request: &[u8], received: NtpTime, sending: NtpTime) -> Option<Vec<u8>> {
    if request.len() < 32 || request[1] & 0x7F != payload_type::TIMING_REQUEST {
        return None;
    }
    // The client's transmit timestamp (T1) sits in the request's last field.
    let origin: [u8; 8] = request[24..32].try_into().ok()?;

    let mut out = Vec::with_capacity(32);
    out.push(0x80);
    out.push(0x80 | payload_type::TIMING_RESPONSE);
    // Echo the request's sequence bytes.
    out.extend_from_slice(&request[2..4]);
    out.extend_from_slice(&[0u8; 4]); // padding
    out.extend_from_slice(&origin); // reference/origin = client transmit
    out.extend_from_slice(&received.to_bytes());
    out.extend_from_slice(&sending.to_bytes());
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ntp_bytes_round_trip() {
        let t = NtpTime {
            seconds: 0x1234_5678,
            fraction: 0x9ABC_DEF0,
        };
        assert_eq!(NtpTime::from_bytes(&t.to_bytes()), t);
        assert_eq!(
            t.to_bytes(),
            [0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0]
        );
    }

    #[test]
    fn ntp_from_unix_epoch_is_the_offset() {
        // Exactly the Unix epoch → NTP seconds are the 1900→1970 offset, no frac.
        let t = NtpTime::from_unix_nanos(0);
        assert_eq!(t.seconds, 2_208_988_800);
        assert_eq!(t.fraction, 0);
        // Half a second of fraction is the high bit of the fraction word.
        let half = NtpTime::from_unix_nanos(500_000_000);
        assert_eq!(half.fraction, 0x8000_0000);
    }

    /// The audio header is the canonical 12 bytes; the marker bit rides the
    /// second byte only on the first packet.
    #[test]
    fn audio_packet_header_is_golden() {
        let pkt = audio_packet(true, 0x0102, 0x0A0B_0C0D, 0xDEAD_BEEF, &[0xAA, 0xBB]);
        assert_eq!(
            pkt,
            vec![
                0x80, 0xE0, // marker + PT 0x60
                0x01, 0x02, // sequence
                0x0A, 0x0B, 0x0C, 0x0D, // timestamp
                0xDE, 0xAD, 0xBE, 0xEF, // ssrc
                0xAA, 0xBB, // payload
            ]
        );
        // A non-first packet clears the marker bit.
        assert_eq!(audio_packet(false, 0, 0, 0, &[])[1], 0x60);
    }

    /// The sync packet is 20 bytes with the fixed 0x0007 sequence and the NTP
    /// time in the middle; the first sync sets the extension bit.
    #[test]
    fn sync_packet_is_golden() {
        let ntp = NtpTime {
            seconds: 0x1111_2222,
            fraction: 0x3333_4444,
        };
        let pkt = sync_packet(true, 0x5555_6666, ntp, 0x7777_8888);
        assert_eq!(
            pkt,
            vec![
                0x90, 0xD4, // extension + marker + PT 0x54
                0x00, 0x07, // fixed sequence
                0x55, 0x55, 0x66, 0x66, // played (now - latency)
                0x11, 0x11, 0x22, 0x22, 0x33, 0x33, 0x44, 0x44, // NTP
                0x77, 0x77, 0x88, 0x88, // now
            ]
        );
        // A later sync clears the extension bit.
        assert_eq!(sync_packet(false, 0, ntp, 0)[0], 0x80);
    }

    /// A canned timing request is answered by echoing its transmit timestamp as
    /// the response origin and stamping the receive/transmit instants.
    #[test]
    fn timing_response_echoes_origin_and_stamps_now() {
        let client_transmit = NtpTime {
            seconds: 0xAAAA_BBBB,
            fraction: 0xCCCC_DDDD,
        };
        let mut request = vec![0x80, 0xD2, 0x00, 0x09, 0, 0, 0, 0];
        request.extend_from_slice(&[0u8; 8]); // origin (unused by responder)
        request.extend_from_slice(&[0u8; 8]); // receive
        request.extend_from_slice(&client_transmit.to_bytes()); // transmit (T1)

        let received = NtpTime {
            seconds: 1,
            fraction: 2,
        };
        let sending = NtpTime {
            seconds: 3,
            fraction: 4,
        };
        let response = timing_response(&request, received, sending).unwrap();

        assert_eq!(response.len(), 32);
        assert_eq!(response[0], 0x80);
        assert_eq!(response[1], 0xD3, "marker + PT 0x53");
        assert_eq!(&response[2..4], &[0x00, 0x09], "sequence echoed");
        assert_eq!(&response[4..8], &[0, 0, 0, 0], "padding");
        assert_eq!(
            NtpTime::from_bytes(&response[8..16].try_into().unwrap()),
            client_transmit,
            "origin echoes the client's transmit timestamp"
        );
        assert_eq!(
            NtpTime::from_bytes(&response[16..24].try_into().unwrap()),
            received
        );
        assert_eq!(
            NtpTime::from_bytes(&response[24..32].try_into().unwrap()),
            sending
        );
    }

    /// A datagram that isn't a timing request is not answered.
    #[test]
    fn non_timing_request_is_ignored() {
        let sync = sync_packet(false, 0, NtpTime::default(), 0);
        assert!(timing_response(&sync, NtpTime::default(), NtpTime::default()).is_none());
        assert!(timing_response(&[0x80, 0xD2], NtpTime::default(), NtpTime::default()).is_none());
    }
}
