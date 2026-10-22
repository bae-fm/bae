//! What an AirPlay receiver announces about itself, parsed from its mDNS TXT
//! records.
//!
//! A receiver advertises `_airplay._tcp` (AirPlay 2 and newer AirPlay 1 gear),
//! `_raop._tcp` (the legacy Remote Audio Output Protocol), or both. The TXT
//! records carry a `features`/`ft` bitmask and a set of RAOP audio parameters;
//! [`AirPlayCapabilities`] parses them into the two things the sender needs to
//! decide before it connects: which [`Dialect`] to speak, and — for that dialect
//! — how to pair and encrypt.
//!
//! The dialect is chosen from the announced bits, never probed on the wire: a
//! receiver that advertises the AirPlay 2 unified-media-control or
//! CoreUtils-pairing feature bit speaks AirPlay 2; anything else advertising RAOP
//! speaks the legacy dialect. This is the same rule the AirPlay reference
//! implementations use (established by pyatv's `get_protocol_version`, MIT).

use std::collections::HashMap;

use crate::renderer::discovery::RendererServiceType;

/// The AirPlay feature bits carried in the 64-bit `features`/`ft` value that a
/// sender acts on to choose a dialect. Bit indices are from the openairplay
/// spec's features table and pyatv's `AirPlayFlags` (MIT), which agree on these.
mod feature_bits {
    /// AirPlay 2 unified media control — one of the two AirPlay 2 markers.
    pub(super) const SUPPORTS_UNIFIED_MEDIA_CONTROL: u64 = 1 << 38;
    /// CoreUtils pairing and encryption — the other AirPlay 2 marker, and the bit
    /// that says the receiver accepts the password-less *transient* pair-setup the
    /// sender uses.
    pub(super) const SUPPORTS_COREUTILS_PAIRING_AND_ENCRYPTION: u64 = 1 << 48;
}

/// The status flags carried in the `flags`/`sf` value, and the RAOP `et`/`cn`
/// enumerations. These gate whether a receiver can be driven without a user PIN.
mod status_flags {
    /// A user PIN is required before pairing (`0x8`).
    pub(super) const PIN_REQUIRED: u64 = 0x8;
    /// The receiver is password-protected (`0x80`).
    pub(super) const PASSWORD: u64 = 0x80;
    /// Legacy (PIN) pairing is mandatory (`0x200`).
    pub(super) const LEGACY_PAIRING: u64 = 0x200;
}

/// Which AirPlay dialect a receiver speaks, chosen from its announced features.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    /// Legacy RAOP (`_raop._tcp`): RTSP + ALAC/PCM over RTP, RSA-AES or
    /// unencrypted per the receiver's `et`.
    Raop,
    /// AirPlay 2 (`_airplay._tcp` with an AirPlay 2 feature bit): HomeKit
    /// transient pairing, ChaCha20-Poly1305 packet encryption.
    AirPlay2,
}

/// The audio encryption a RAOP receiver requires, from its `et` (encryption
/// types) TXT value. A sender picks the strongest it implements that the receiver
/// offers; today that is [`RaopEncryption::None`] or [`RaopEncryption::RsaAes`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RaopEncryption {
    /// `et` includes `0`: the receiver accepts an unencrypted stream.
    None,
    /// `et` includes `1`: AES-128-CBC audio with the key RSA-encrypted to the
    /// receiver's public key (AirPort Express and compatibles).
    RsaAes,
}

/// A RAOP audio codec from the `cn` (codec) TXT value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RaopCodec {
    /// `cn` includes `0`: raw PCM.
    Pcm,
    /// `cn` includes `1`: Apple Lossless (ALAC).
    Alac,
    /// `cn` includes `2`: AAC.
    Aac,
}

/// The RAOP audio parameters a receiver announces, parsed from the `_raop._tcp`
/// TXT record. Sample rate/size/channels are the negotiated stream format; the
/// codec and encryption sets are what the receiver will accept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaopParams {
    /// Codecs the receiver decodes, from `cn`. Empty means the receiver named
    /// none (treated as PCM-only by callers).
    pub codecs: Vec<RaopCodec>,
    /// Encryption types the receiver offers, from `et`.
    pub encryption: Vec<RaopEncryption>,
    /// Sample rate in Hz, from `sr` (default 44100).
    pub sample_rate: u32,
    /// Sample size in bits, from `ss` (default 16).
    pub sample_size: u16,
    /// Channel count, from `ch` (default 2).
    pub channels: u16,
}

/// Everything a sender decides about a receiver before connecting: the dialect,
/// whether it can be driven without a user PIN, and the dialect-specific
/// parameters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AirPlayCapabilities {
    pub dialect: Dialect,
    /// The raw 64-bit features value, kept for the AirPlay 2 stream setup (which
    /// echoes specific bits back to the receiver).
    pub features: u64,
    /// True when the receiver demands a user PIN or password we do not implement
    /// (persistent pairing). The sender surfaces a clear error rather than
    /// attempting a flow it can't complete.
    pub requires_pin: bool,
    /// True when the receiver accepts the password-less transient pair-setup
    /// (AirPlay 2 only; always false for RAOP).
    pub supports_transient_pairing: bool,
    /// Present only for a RAOP receiver.
    pub raop: Option<RaopParams>,
}

/// A TXT value bae reads to decide how to connect that the receiver left out
/// where the spec requires it (`raw` is `None`), or sent as text bae can't
/// parse. A receiver advertising one can't be connected to: guessing the value
/// would pick the wrong dialect or skip a PIN it requires.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnreadableSetting {
    /// The TXT key, as the receiver spelled it (`features`, `ft`, `flags`, `sf`).
    pub key: String,
    /// The text it carried, or `None` when it was left out.
    pub raw: Option<String>,
}

impl std::fmt::Display for UnreadableSetting {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.raw {
            Some(raw) => write!(f, "{} is {raw:?}, which bae can't read", self.key),
            None => write!(f, "{} is missing", self.key),
        }
    }
}

impl AirPlayCapabilities {
    /// Parse a receiver's capabilities from the service type it was found on and
    /// its TXT records. `txt` maps TXT keys to their string values, lowercased
    /// keys as mDNS delivers them.
    ///
    /// Which fields may be left out follows the openairplay spec's service
    /// discovery tables (and pyatv's reading of them, MIT): the AirPlay
    /// service's `features` is required — its dialect is decided from it, and
    /// the spec's records always carry it. A RAOP service's `ft` is newer than
    /// the protocol (the spec's RAOP table has no such key), so without one the
    /// receiver advertises no feature bits. `flags`/`sf` is absent on older
    /// receivers on both services (the spec's Apple TV 2 record has none), which
    /// sets no status bits. Any of them present but not hex is unreadable.
    pub fn from_txt(
        service_type: RendererServiceType,
        txt: &HashMap<String, String>,
    ) -> Result<Self, UnreadableSetting> {
        let features = match txt_value(txt, "features", "ft") {
            Some((key, raw)) => parse_features(raw).ok_or_else(|| unreadable(key, raw))?,
            None if service_type == RendererServiceType::AirPlay => {
                return Err(UnreadableSetting {
                    key: "features".to_string(),
                    raw: None,
                });
            }
            None => 0,
        };
        let flags = match txt_value(txt, "flags", "sf") {
            Some((key, raw)) => parse_hex(raw).ok_or_else(|| unreadable(key, raw))?,
            None => 0,
        };

        let is_airplay2 = service_type == RendererServiceType::AirPlay
            && features
                & (feature_bits::SUPPORTS_UNIFIED_MEDIA_CONTROL
                    | feature_bits::SUPPORTS_COREUTILS_PAIRING_AND_ENCRYPTION)
                != 0;

        let password = txt
            .get("pw")
            .is_some_and(|v| v.eq_ignore_ascii_case("true"))
            || flags & status_flags::PASSWORD != 0;
        let requires_pin =
            password || flags & (status_flags::PIN_REQUIRED | status_flags::LEGACY_PAIRING) != 0;

        Ok(if is_airplay2 {
            AirPlayCapabilities {
                dialect: Dialect::AirPlay2,
                features,
                requires_pin,
                supports_transient_pairing: features
                    & feature_bits::SUPPORTS_COREUTILS_PAIRING_AND_ENCRYPTION
                    != 0,
                raop: None,
            }
        } else {
            AirPlayCapabilities {
                dialect: Dialect::Raop,
                features,
                requires_pin,
                supports_transient_pairing: false,
                raop: Some(RaopParams::from_txt(txt)),
            }
        })
    }
}

/// The value of the first of a field's two spellings that is present, with the
/// key it was found under.
fn txt_value<'a>(
    txt: &'a HashMap<String, String>,
    key: &'static str,
    other_key: &'static str,
) -> Option<(&'static str, &'a str)> {
    txt.get(key)
        .map(|raw| (key, raw.as_str()))
        .or_else(|| txt.get(other_key).map(|raw| (other_key, raw.as_str())))
}

fn unreadable(key: &str, raw: &str) -> UnreadableSetting {
    UnreadableSetting {
        key: key.to_string(),
        raw: Some(raw.to_string()),
    }
}

impl RaopParams {
    fn from_txt(txt: &HashMap<String, String>) -> Self {
        let codecs = txt
            .get("cn")
            .map(|s| parse_int_set(s, raop_codec))
            .unwrap_or_default();
        let encryption = txt
            .get("et")
            .map(|s| parse_int_set(s, raop_encryption))
            .unwrap_or_else(|| vec![RaopEncryption::None]);
        RaopParams {
            codecs,
            encryption,
            sample_rate: txt.get("sr").and_then(|s| s.parse().ok()).unwrap_or(44_100),
            sample_size: txt.get("ss").and_then(|s| s.parse().ok()).unwrap_or(16),
            channels: txt.get("ch").and_then(|s| s.parse().ok()).unwrap_or(2),
        }
    }
}

fn raop_codec(v: u32) -> Option<RaopCodec> {
    match v {
        0 => Some(RaopCodec::Pcm),
        1 => Some(RaopCodec::Alac),
        2 => Some(RaopCodec::Aac),
        _ => None,
    }
}

fn raop_encryption(v: u32) -> Option<RaopEncryption> {
    match v {
        0 => Some(RaopEncryption::None),
        1 => Some(RaopEncryption::RsaAes),
        // 3/4/5 are FairPlay/MFiSAP variants the sender does not implement.
        _ => None,
    }
}

/// Parse a comma-separated set of integers into the values `map` recognizes,
/// dropping unrecognized entries and de-duplicating while preserving order.
fn parse_int_set<T: PartialEq>(s: &str, map: impl Fn(u32) -> Option<T>) -> Vec<T> {
    let mut out = Vec::new();
    for part in s.split(',') {
        if let Some(value) = part.trim().parse().ok().and_then(&map) {
            if !out.contains(&value) {
                out.push(value);
            }
        }
    }
    out
}

/// Parse a features string into a 64-bit bitmask. Two forms, per the spec:
/// `0x12345678` (low 32 bits) or `0xLOW,0xHIGH` (the high word is the more
/// significant half, so `0xAAAA,0xBBBB` is `0xBBBB_AAAA`). `None` when either
/// word isn't hex or overflows 32 bits.
fn parse_features(s: &str) -> Option<u64> {
    match s.split_once(',') {
        Some((low, high)) => {
            let low = u32::try_from(parse_hex(low)?).ok()?;
            let high = u32::try_from(parse_hex(high)?).ok()?;
            Some((u64::from(high) << 32) | u64::from(low))
        }
        None => parse_hex(s),
    }
}

/// Parse a `0x`-prefixed (or bare) hex string into a u64.
fn parse_hex(s: &str) -> Option<u64> {
    let s = s.trim();
    let s = s
        .strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .unwrap_or(s);
    u64::from_str_radix(s, 16).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn txt(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn features_low_word_only() {
        assert_eq!(parse_features("0x12345678"), Some(0x1234_5678));
    }

    #[test]
    fn features_two_words_high_is_more_significant() {
        // `0xLOW,0xHIGH` composes to `0xHIGH_LOW`.
        assert_eq!(
            parse_features("0x445F8A00,0x1C340"),
            Some(0x0001_C340_445F_8A00)
        );
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
            &txt(&[
                ("features", &features_str(features)),
                ("model", "AudioAccessory5,1"),
            ]),
        )
        .unwrap();
        assert_eq!(caps.dialect, Dialect::AirPlay2);
        assert!(caps.supports_transient_pairing);
        assert!(!caps.requires_pin);
        assert_eq!(caps.features, features);
        assert!(caps.raop.is_none());
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
        assert_eq!(caps.dialect, Dialect::Raop);
        assert!(!caps.supports_transient_pairing);
        let raop = caps.raop.expect("a RAOP receiver carries RAOP params");
        assert_eq!(raop.codecs, vec![RaopCodec::Pcm, RaopCodec::Alac]);
        assert_eq!(
            raop.encryption,
            vec![RaopEncryption::None, RaopEncryption::RsaAes]
        );
        assert_eq!(raop.sample_rate, 44_100);
        assert_eq!(raop.sample_size, 16);
        assert_eq!(raop.channels, 2);
    }

    /// A third-party AirPlay-2 speaker: the `_airplay._tcp` service carries an
    /// AirPlay 2 bit, so it speaks AirPlay 2 even though it also advertises RAOP.
    #[test]
    fn third_party_airplay2_speaker() {
        let caps = AirPlayCapabilities::from_txt(
            RendererServiceType::AirPlay,
            &txt(&[
                ("features", &features_str(COREUTILS_BIT)),
                ("model", "MyAmp,1"),
            ]),
        )
        .unwrap();
        assert_eq!(caps.dialect, Dialect::AirPlay2);
        assert!(caps.supports_transient_pairing);
    }

    /// A `_raop._tcp` receiver with no AirPlay 2 bit stays RAOP even when a
    /// features value is present.
    #[test]
    fn raop_without_airplay2_bits_stays_raop() {
        let caps = AirPlayCapabilities::from_txt(
            RendererServiceType::Raop,
            &txt(&[("et", "1"), ("cn", "1")]),
        )
        .unwrap();
        assert_eq!(caps.dialect, Dialect::Raop);
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
                UnreadableSetting {
                    key: key.to_string(),
                    raw: Some(raw.to_string()),
                }
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
        assert_eq!(
            unreadable,
            UnreadableSetting {
                key: "features".to_string(),
                raw: None,
            }
        );
    }

    /// The fields the spec leaves out for older receivers take its meaning: a
    /// RAOP service without `ft` advertises no feature bits (the legacy
    /// dialect), and a receiver without `flags`/`sf` sets no status bits.
    #[test]
    fn optional_fields_take_the_meaning_of_their_absence() {
        let raop =
            AirPlayCapabilities::from_txt(RendererServiceType::Raop, &txt(&[("cn", "1")])).unwrap();
        assert_eq!(raop.dialect, Dialect::Raop);
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
        assert_eq!(caps.raop.unwrap().encryption, vec![RaopEncryption::None]);
    }
}
