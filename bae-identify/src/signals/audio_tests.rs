use super::*;

fn format(sample_rate_hz: i64, channels: i64) -> AudioFormat {
    AudioFormat {
        codec: "FLAC".to_string(),
        sample_rate_hz,
        bits_per_sample: Some(24),
        bitrate_kbps: None,
        channels,
    }
}

/// Audio is mono when every file carries one channel, and its rate rules
/// a CD out by the rip rule.
#[test]
fn the_facts_are_read_off_every_format() {
    let mono = format(96_000, 1);
    let stereo = format(96_000, 2);
    let facts = AudioFacts::of(2, Vec::new(), [&mono, &mono]);
    assert!(facts.mono);
    assert_eq!(facts.rate_ruling_out_cd, Some(96_000));
    assert_eq!(facts.track_count, 2);
    assert!(!AudioFacts::of(2, Vec::new(), [&mono, &stereo]).mono);
    assert!(!AudioFacts::of(0, Vec::new(), []).mono);
    assert_eq!(AudioFacts::of(1, Vec::new(), [&format(44_100, 1)]).rate_ruling_out_cd, None);
}
