use super::*;
use crate::test_support::txt;

#[test]
fn features_low_word_only() {
    assert_eq!(parse_features("0x12345678"), Some(0x1234_5678));
}

#[test]
fn features_two_words_high_is_more_significant() {
    // `0xLOW,0xHIGH` composes to `0xHIGH_LOW`.
    assert_eq!(parse_features("0x445F8A00,0x1C340"), Some(0x0001_C340_445F_8A00));
}

/// The AirPlay 2 markers as a two-word features string: bit 48 (CoreUtils
/// pairing/encryption) and bit 38 (unified media control).
const COREUTILS_BIT: u64 = feature_bits::SUPPORTS_COREUTILS_PAIRING_AND_ENCRYPTION;
const UNIFIED_BIT: u64 = feature_bits::SUPPORTS_UNIFIED_MEDIA_CONTROL;

fn features_str(bits: u64) -> String {
    format!("0x{:X},0x{:X}", bits & 0xFFFF_FFFF, bits >> 32)
}

/// A HomePod-class receiver advertises the CoreUtils pairing/encryption bit,
/// so it speaks AirPlay 2 and accepts transient pairing.
#[test]
fn homepod_speaks_airplay2_with_transient_pairing() {
    let features = COREUTILS_BIT | UNIFIED_BIT;
    let caps = AirPlayCapabilities::from_txt(
        RendererServiceType::AirPlay,
        &txt(&[("features", &features_str(features)), ("model", "AudioAccessory5,1")]),
    )
    .unwrap();
    assert_eq!(caps.dialect, Dialect::AirPlay2);
    assert!(caps.supports_transient_pairing);
    assert!(!caps.requires_pin);
    assert_eq!(caps.features, features);
}

/// A modern Apple TV advertises the unified-media-control bit; it too speaks
/// AirPlay 2 (even without the CoreUtils bit, though then not transient).
#[test]
fn apple_tv_speaks_airplay2() {
    let caps = AirPlayCapabilities::from_txt(
        RendererServiceType::AirPlay,
        &txt(&[("ft", &features_str(UNIFIED_BIT))]),
    )
    .unwrap();
    assert_eq!(caps.dialect, Dialect::AirPlay2);
}

/// An AirPort Express advertises `_raop._tcp` with RSA-AES encryption and
/// ALAC — the legacy dialect, no transient pairing.
#[test]
fn airport_express_speaks_raop_with_rsa_aes_alac() {
    let caps = AirPlayCapabilities::from_txt(
        RendererServiceType::Raop,
        &txt(&[
            ("cn", "0,1"),
            ("et", "0,1"),
            ("sr", "44100"),
            ("ss", "16"),
            ("ch", "2"),
            ("tp", "UDP"),
            ("vs", "105.1"),
        ]),
    )
    .unwrap();
    assert_eq!(
        caps.dialect,
        Dialect::Raop { encryption: vec![RaopEncryption::None, RaopEncryption::RsaAes] }
    );
    assert!(!caps.supports_transient_pairing);
}

/// A third-party AirPlay-2 speaker: the `_airplay._tcp` service carries an
/// AirPlay 2 bit, so it speaks AirPlay 2 even though it also advertises RAOP.
#[test]
fn third_party_airplay2_speaker() {
    let caps = AirPlayCapabilities::from_txt(
        RendererServiceType::AirPlay,
        &txt(&[("features", &features_str(COREUTILS_BIT)), ("model", "MyAmp,1")]),
    )
    .unwrap();
    assert_eq!(caps.dialect, Dialect::AirPlay2);
    assert!(caps.supports_transient_pairing);
}

/// A `_raop._tcp` receiver with no AirPlay 2 bit stays RAOP even when a
/// features value is present.
#[test]
fn raop_without_airplay2_bits_stays_raop() {
    let caps =
        AirPlayCapabilities::from_txt(RendererServiceType::Raop, &txt(&[("et", "1"), ("cn", "1")]))
            .unwrap();
    assert_eq!(caps.dialect, Dialect::Raop { encryption: vec![RaopEncryption::RsaAes] });
}

/// A password-protected or PIN-gated receiver is flagged so the sender can
/// refuse rather than attempt persistent pairing.
#[test]
fn pin_and_password_gate_the_receiver() {
    let pw = AirPlayCapabilities::from_txt(
        RendererServiceType::AirPlay,
        &txt(&[("pw", "true"), ("features", "0x39f7")]),
    )
    .unwrap();
    assert!(pw.requires_pin);

    let pin_flag = AirPlayCapabilities::from_txt(
        RendererServiceType::Raop,
        &txt(&[("sf", "0x208")]), // PIN_REQUIRED | LEGACY_PAIRING
    )
    .unwrap();
    assert!(pin_flag.requires_pin);
}

/// A features or flags value that is present but isn't hex is not zero:
/// the receiver can't be connected to, and the error names the key and the
/// text it sent.
#[test]
fn an_unparseable_features_or_flags_value_is_unreadable() {
    for (service_type, key, raw) in [
        (RendererServiceType::AirPlay, "features", "garbage"),
        (RendererServiceType::AirPlay, "features", "0x1FFFFFFFF,0x0"),
        (RendererServiceType::AirPlay, "features", "0x0,0xZZ"),
        (RendererServiceType::Raop, "ft", "0x"),
        (RendererServiceType::AirPlay, "flags", "on"),
        (RendererServiceType::Raop, "sf", "0x4g"),
    ] {
        let mut pairs = vec![(key, raw)];
        if key != "features" && service_type == RendererServiceType::AirPlay {
            pairs.push(("features", "0x0,0x10000"));
        }
        let unreadable = AirPlayCapabilities::from_txt(service_type, &txt(&pairs))
            .expect_err(&format!("{key}={raw} is unreadable"));
        assert_eq!(
            unreadable,
            UnreadableSetting { key: key.to_string(), raw: Some(raw.to_string()) }
        );
    }
}

/// The AirPlay service decides its dialect from its features value, and the
/// spec's AirPlay service always carries one; leaving it out is unreadable.
#[test]
fn an_airplay_service_without_features_is_unreadable() {
    let unreadable =
        AirPlayCapabilities::from_txt(RendererServiceType::AirPlay, &txt(&[("pw", "false")]))
            .expect_err("features is required on _airplay._tcp");
    assert_eq!(unreadable, UnreadableSetting { key: "features".to_string(), raw: None });
}

/// The fields the spec leaves out for older receivers take its meaning: a
/// RAOP service without `ft` advertises no feature bits (the legacy
/// dialect), and a receiver without `flags`/`sf` sets no status bits.
#[test]
fn optional_fields_take_the_meaning_of_their_absence() {
    let raop =
        AirPlayCapabilities::from_txt(RendererServiceType::Raop, &txt(&[("cn", "1")])).unwrap();
    assert!(matches!(raop.dialect, Dialect::Raop { .. }));
    assert_eq!(raop.features, 0);
    assert!(!raop.requires_pin);

    let airplay1 = AirPlayCapabilities::from_txt(
        RendererServiceType::AirPlay,
        &txt(&[("features", "0x39f7"), ("model", "AppleTV2,1")]),
    )
    .unwrap();
    assert_eq!(airplay1.features, 0x39f7);
    assert!(!airplay1.requires_pin);
}

/// A receiver that names no `et` is treated as accepting an unencrypted
/// stream.
#[test]
fn missing_encryption_defaults_to_none() {
    let caps =
        AirPlayCapabilities::from_txt(RendererServiceType::Raop, &txt(&[("cn", "1")])).unwrap();
    assert_eq!(caps.dialect, Dialect::Raop { encryption: vec![RaopEncryption::None] });
}

/// An `et` entry that isn't a number is not "no encryption offered": the
/// receiver can't be connected to, and the error names `et` and its text.
#[test]
fn an_unparseable_encryption_entry_is_unreadable() {
    for raw in ["0,x", "rsa", "1;0"] {
        let unreadable =
            AirPlayCapabilities::from_txt(RendererServiceType::Raop, &txt(&[("et", raw)]))
                .expect_err(&format!("et={raw} is unreadable"));
        assert_eq!(
            unreadable,
            UnreadableSetting { key: "et".to_string(), raw: Some(raw.to_string()) }
        );
    }
}

/// Encryption types the sender doesn't implement (FairPlay and MFiSAP) are
/// left out of what the receiver offers; a receiver offering only those
/// offers nothing the sender can use.
#[test]
fn encryption_the_sender_does_not_implement_is_left_out() {
    let caps = AirPlayCapabilities::from_txt(RendererServiceType::Raop, &txt(&[("et", "0,3,1,0")]))
        .unwrap();
    assert_eq!(
        caps.dialect,
        Dialect::Raop { encryption: vec![RaopEncryption::None, RaopEncryption::RsaAes] }
    );
    let fairplay_only =
        AirPlayCapabilities::from_txt(RendererServiceType::Raop, &txt(&[("et", "3,5")])).unwrap();
    assert_eq!(fairplay_only.dialect, Dialect::Raop { encryption: Vec::new() });
}
