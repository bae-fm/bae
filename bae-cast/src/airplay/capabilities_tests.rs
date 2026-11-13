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

/// What a RAOP receiver with no pairing or password asks for.
const NO_AUTH: RaopAuth = RaopAuth { device_verification: false, password: false };

/// An AirPlay 2 receiver that asks for nothing pairs with the transient pairing.
const TRANSIENT: Dialect = Dialect::AirPlay2 { pairing: Ap2Pairing::Transient };

/// A HomePod-class receiver advertises the CoreUtils pairing/encryption bit,
/// so it speaks AirPlay 2 and pairs transiently.
#[test]
fn homepod_speaks_airplay2_with_transient_pairing() {
    let features = COREUTILS_BIT | UNIFIED_BIT;
    let caps = AirPlayCapabilities::from_txt(
        RendererServiceType::AirPlay,
        &txt(&[("features", &features_str(features)), ("model", "AudioAccessory5,1")]),
    )
    .unwrap();
    assert_eq!(caps.dialect, TRANSIENT);
    assert_eq!(caps.access, ReceiverAccess::Anyone);
    assert_eq!(caps.features, features);
}

/// A modern Apple TV advertises the unified-media-control bit; it too speaks
/// AirPlay 2, and without status flags pairs transiently whatever its other
/// feature bits.
#[test]
fn apple_tv_speaks_airplay2() {
    let caps = AirPlayCapabilities::from_txt(
        RendererServiceType::AirPlay,
        &txt(&[("ft", &features_str(UNIFIED_BIT))]),
    )
    .unwrap();
    assert_eq!(caps.dialect, TRANSIENT);
}

/// An AirPort Express advertises `_raop._tcp` with RSA-AES encryption and
/// ALAC — the legacy dialect, asking for nothing.
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
        Dialect::Raop {
            encryption: vec![RaopEncryption::None, RaopEncryption::RsaAes],
            auth: NO_AUTH
        }
    );
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
    assert_eq!(caps.dialect, TRANSIENT);
}

/// A `_raop._tcp` receiver with no AirPlay 2 bit stays RAOP even when a
/// features value is present.
#[test]
fn raop_without_airplay2_bits_stays_raop() {
    let caps =
        AirPlayCapabilities::from_txt(RendererServiceType::Raop, &txt(&[("et", "1"), ("cn", "1")]))
            .unwrap();
    assert_eq!(
        caps.dialect,
        Dialect::Raop { encryption: vec![RaopEncryption::RsaAes], auth: NO_AUTH }
    );
}

fn airplay2_caps(extra: &[(&str, &str)]) -> Result<AirPlayCapabilities, UnreadableSetting> {
    let features = features_str(COREUTILS_BIT);
    let mut pairs = vec![("features", features.as_str())];
    pairs.extend_from_slice(extra);
    AirPlayCapabilities::from_txt(RendererServiceType::AirPlay, &txt(&pairs))
}

fn raop_caps(extra: &[(&str, &str)]) -> AirPlayCapabilities {
    AirPlayCapabilities::from_txt(RendererServiceType::Raop, &txt(extra)).unwrap()
}

/// What an AirPlay 2 receiver asks for follows OwnTone's order: the one-time
/// pairing flag first, then a PIN on every connection, then a password; none of
/// them is the transient pairing. The rows are pyatv's `test_utils` flags.
#[test]
fn an_airplay2_receivers_pairing_follows_its_status_flags() {
    for (extra, pairing) in [
        (vec![("sf", "0x1")], Ap2Pairing::Transient),
        (vec![("flags", "0x4")], Ap2Pairing::Transient),
        (vec![("sf", "0x200")], Ap2Pairing::PinOnce),
        (vec![("sf", "0x208")], Ap2Pairing::PinOnce),
        (vec![("sf", "0x280")], Ap2Pairing::PinOnce),
        (vec![("sf", "0x8")], Ap2Pairing::PinEveryCast),
        (vec![("sf", "0x88")], Ap2Pairing::PinEveryCast),
        (vec![("sf", "0x80")], Ap2Pairing::Password),
        (vec![("pw", "true")], Ap2Pairing::Password),
        (vec![("pw", "TRUE"), ("sf", "0x1")], Ap2Pairing::Password),
        (vec![("pw", "false")], Ap2Pairing::Transient),
    ] {
        let caps = airplay2_caps(&extra).unwrap();
        assert_eq!(caps.dialect, Dialect::AirPlay2 { pairing }, "{extra:?}");
    }
}

/// A RAOP receiver's device verification and password are two facts, each from
/// its own flags, and a receiver can ask for both.
#[test]
fn a_raop_receivers_auth_follows_its_status_flags() {
    for (extra, device_verification, password) in [
        (vec![("sf", "0x1")], false, false),
        (vec![("sf", "0x200")], true, false),
        (vec![("sf", "0x8")], true, false),
        (vec![("sf", "0x208")], true, false),
        (vec![("sf", "0x80")], false, true),
        (vec![("pw", "true")], false, true),
        (vec![("sf", "0x280")], true, true),
        (vec![("sf", "0x200"), ("pw", "true")], true, true),
    ] {
        let caps = raop_caps(&extra);
        let Dialect::Raop { auth, .. } = caps.dialect else {
            panic!("{extra:?} reads as RAOP");
        };
        assert_eq!(auth, RaopAuth { device_verification, password }, "{extra:?}");
    }
}

/// `acl` and `act` on the AirPlay advertisement say who the receiver plays
/// from, apart from how it pairs: `acl=1` with the one-time pairing flag is
/// still a home-only receiver (pyatv `test_airplay.py`).
#[test]
fn acl_and_act_name_who_the_receiver_plays_from() {
    for (extra, access) in [
        (vec![], ReceiverAccess::Anyone),
        (vec![("acl", "0")], ReceiverAccess::Anyone),
        (vec![("acl", "1")], ReceiverAccess::HomeMembers),
        (vec![("acl", "2")], ReceiverAccess::Admins),
        (vec![("acl", "1"), ("sf", "0x200")], ReceiverAccess::HomeMembers),
        (vec![("act", "2")], ReceiverAccess::Owner),
        // The stricter of the two holds.
        (vec![("acl", "1"), ("act", "2")], ReceiverAccess::Owner),
        (vec![("acl", "2"), ("act", "2")], ReceiverAccess::Owner),
        // No source names another `act` value, so it limits nothing.
        (vec![("act", "1")], ReceiverAccess::Anyone),
        (vec![("act", "7")], ReceiverAccess::Anyone),
    ] {
        assert_eq!(airplay2_caps(&extra).unwrap().access, access, "{extra:?}");
    }
    let with_pairing = airplay2_caps(&[("acl", "1"), ("sf", "0x200")]).unwrap();
    assert_eq!(with_pairing.dialect, Dialect::AirPlay2 { pairing: Ap2Pairing::PinOnce });
}

/// An `acl` that isn't one of the values that name who the receiver plays from
/// isn't "everyone": the receiver can't be connected to, and the error names
/// the key and its text.
#[test]
fn an_unreadable_acl_is_unreadable() {
    for raw in ["7", "-1", "home", ""] {
        let unreadable = airplay2_caps(&[("acl", raw)]).expect_err(&format!("acl={raw}"));
        assert_eq!(
            unreadable,
            UnreadableSetting { key: "acl".to_string(), raw: Some(raw.to_string()) }
        );
    }
}

/// `acl` and `act` live on the AirPlay advertisement; the RAOP one carries
/// neither, so a RAOP service states no restriction of its own.
#[test]
fn a_raop_advertisement_states_no_access_of_its_own() {
    let caps = raop_caps(&[("acl", "1"), ("act", "2")]);
    assert_eq!(caps.access, ReceiverAccess::Anyone);
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
    assert_eq!(
        raop.dialect,
        Dialect::Raop { encryption: vec![RaopEncryption::None], auth: NO_AUTH }
    );
    assert_eq!(raop.features, 0);

    let airplay1 = AirPlayCapabilities::from_txt(
        RendererServiceType::AirPlay,
        &txt(&[("features", "0x39f7"), ("model", "AppleTV2,1")]),
    )
    .unwrap();
    assert_eq!(airplay1.features, 0x39f7);
    assert_eq!(
        airplay1.dialect,
        Dialect::Raop { encryption: vec![RaopEncryption::None], auth: NO_AUTH }
    );
    assert_eq!(airplay1.access, ReceiverAccess::Anyone);
}

/// A receiver that names no `et` is treated as accepting an unencrypted
/// stream.
#[test]
fn missing_encryption_defaults_to_none() {
    let caps =
        AirPlayCapabilities::from_txt(RendererServiceType::Raop, &txt(&[("cn", "1")])).unwrap();
    assert_eq!(
        caps.dialect,
        Dialect::Raop { encryption: vec![RaopEncryption::None], auth: NO_AUTH }
    );
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
        Dialect::Raop {
            encryption: vec![RaopEncryption::None, RaopEncryption::RsaAes],
            auth: NO_AUTH
        }
    );
    let fairplay_only =
        AirPlayCapabilities::from_txt(RendererServiceType::Raop, &txt(&[("et", "3,5")])).unwrap();
    assert_eq!(fairplay_only.dialect, Dialect::Raop { encryption: Vec::new(), auth: NO_AUTH });
}
