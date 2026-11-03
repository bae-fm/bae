use super::*;

#[test]
fn ntp_bytes_round_trip() {
    let t = NtpTime { seconds: 0x1234_5678, fraction: 0x9ABC_DEF0 };
    assert_eq!(NtpTime::from_bytes(&t.to_bytes()), t);
    assert_eq!(t.to_bytes(), [0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0]);
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
    let ntp = NtpTime { seconds: 0x1111_2222, fraction: 0x3333_4444 };
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
    let client_transmit = NtpTime { seconds: 0xAAAA_BBBB, fraction: 0xCCCC_DDDD };
    let mut request = vec![0x80, 0xD2, 0x00, 0x09, 0, 0, 0, 0];
    request.extend_from_slice(&[0u8; 8]); // origin (unused by responder)
    request.extend_from_slice(&[0u8; 8]); // receive
    request.extend_from_slice(&client_transmit.to_bytes()); // transmit (T1)

    let received = NtpTime { seconds: 1, fraction: 2 };
    let sending = NtpTime { seconds: 3, fraction: 4 };
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
    assert_eq!(NtpTime::from_bytes(&response[16..24].try_into().unwrap()), received);
    assert_eq!(NtpTime::from_bytes(&response[24..32].try_into().unwrap()), sending);
}

/// A datagram that isn't a timing request is not answered.
#[test]
fn non_timing_request_is_ignored() {
    let sync = sync_packet(false, 0, NtpTime::default(), 0);
    assert!(timing_response(&sync, NtpTime::default(), NtpTime::default()).is_none());
    assert!(timing_response(&[0x80, 0xD2], NtpTime::default(), NtpTime::default()).is_none());
}
