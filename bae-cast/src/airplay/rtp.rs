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
pub(crate) struct NtpTime {
    pub seconds: u32,
    pub fraction: u32,
}

impl NtpTime {
    /// Seconds between the NTP epoch (1900) and the Unix epoch (1970).
    const UNIX_TO_NTP: u64 = 2_208_988_800;

    /// The NTP time for a Unix instant given in nanoseconds. Injectable so the
    /// timing responder and sync sender are tested at known clocks.
    pub(crate) fn from_unix_nanos(unix_nanos: u128) -> Self {
        let unix_secs = (unix_nanos / 1_000_000_000) as u64;
        let sub_nanos = (unix_nanos % 1_000_000_000) as u64;
        NtpTime {
            seconds: (unix_secs + Self::UNIX_TO_NTP) as u32,
            fraction: ((sub_nanos << 32) / 1_000_000_000) as u32,
        }
    }

    /// Wall-clock now as an NTP timestamp.
    pub(crate) fn now() -> Self {
        let since_epoch =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
        Self::from_unix_nanos(since_epoch.as_nanos())
    }

    pub(crate) fn to_bytes(self) -> [u8; 8] {
        let mut out = [0u8; 8];
        out[0..4].copy_from_slice(&self.seconds.to_be_bytes());
        out[4..8].copy_from_slice(&self.fraction.to_be_bytes());
        out
    }

    #[cfg(test)]
    pub(crate) fn from_bytes(bytes: &[u8; 8]) -> Self {
        NtpTime {
            seconds: u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
            fraction: u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
        }
    }
}

/// RAOP RTP payload types (the low 7 bits of the second header byte).
pub(crate) mod payload_type {
    /// Timing request, receiver → sender.
    pub(crate) const TIMING_REQUEST: u8 = 0x52;
    /// Timing response, sender → receiver.
    pub(crate) const TIMING_RESPONSE: u8 = 0x53;
    /// Sync packet, sender → receiver on the control channel.
    pub(crate) const SYNC: u8 = 0x54;
    /// Realtime audio data, sender → receiver.
    pub(crate) const AUDIO: u8 = 0x60;
}

/// Serialize one RAOP audio data packet: a 12-byte RTP header (`0x80`, then
/// `0x60` with the marker bit on the first packet after RECORD/FLUSH, the 16-bit
/// sequence, the 32-bit RTP timestamp, and the 32-bit SSRC) followed by the
/// already-encrypted payload.
pub(crate) fn audio_packet(
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
pub(crate) fn sync_packet(first: bool, played_ts: u32, ntp: NtpTime, now_ts: u32) -> Vec<u8> {
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
pub(crate) fn timing_response(
    request: &[u8],
    received: NtpTime,
    sending: NtpTime,
) -> Option<Vec<u8>> {
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
#[path = "rtp_tests.rs"]
mod tests;
