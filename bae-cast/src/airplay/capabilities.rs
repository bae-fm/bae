//! What an AirPlay receiver announces about itself, parsed from its mDNS TXT
//! records.
//!
//! A receiver advertises `_airplay._tcp` (AirPlay 2 and newer AirPlay 1 gear),
//! `_raop._tcp` (the legacy Remote Audio Output Protocol), or both. The TXT
//! records carry a `features`/`ft` bitmask and, for RAOP, the encryption types
//! the receiver offers; [`AirPlayCapabilities`] parses them into the two things
//! the sender needs to decide before it connects: which [`Dialect`] to speak,
//! how to pair and encrypt for that dialect, and who the receiver plays from
//! ([`ReceiverAccess`]).
//!
//! The dialect is chosen from the announced bits, never probed on the wire: a
//! receiver that advertises the AirPlay 2 unified-media-control or
//! CoreUtils-pairing feature bit speaks AirPlay 2; anything else advertising RAOP
//! speaks the legacy dialect. This is the same rule the AirPlay reference
//! implementations use (established by pyatv's `get_protocol_version`, MIT).

use std::collections::HashMap;

use tracing::debug;

use crate::renderer::discovery::RendererServiceType;

/// The AirPlay feature bits carried in the 64-bit `features`/`ft` value that a
/// sender acts on to choose a dialect. Bit indices are from the openairplay
/// spec's features table and pyatv's `AirPlayFlags` (MIT), which agree on these.
mod feature_bits {
    /// AirPlay 2 unified media control — one of the two AirPlay 2 markers.
    pub(super) const SUPPORTS_UNIFIED_MEDIA_CONTROL: u64 = 1 << 38;
    /// CoreUtils pairing and encryption — the other AirPlay 2 marker.
    pub(super) const SUPPORTS_COREUTILS_PAIRING_AND_ENCRYPTION: u64 = 1 << 48;
}

/// The status flags carried in the `flags`/`sf` value, which say how a receiver
/// wants to be paired (names from OwnTone's table of the openairplay flags).
mod status_flags {
    /// A PIN is required on every connection (`0x8`).
    pub(super) const PIN_REQUIRED: u64 = 0x8;
    /// The receiver is password-protected (`0x80`).
    pub(super) const PASSWORD: u64 = 0x80;
    /// A one-time pairing is required before the receiver plays (`0x200`).
    pub(super) const ONE_TIME_PAIRING: u64 = 0x200;
}

/// Which AirPlay dialect a receiver speaks, chosen from its announced features,
/// with what that dialect has to do before audio flows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dialect {
    /// Legacy RAOP (`_raop._tcp`): RTSP + ALAC/PCM over RTP, RSA-AES or
    /// unencrypted. `encryption` is what the receiver's `et` offers that the
    /// sender implements, in the receiver's order.
    Raop { encryption: Vec<RaopEncryption>, auth: RaopAuth },
    /// AirPlay 2 (`_airplay._tcp` with an AirPlay 2 feature bit): HomeKit
    /// pairing as `pairing` says, ChaCha20-Poly1305 packet encryption.
    AirPlay2 { pairing: Ap2Pairing },
}

/// How an AirPlay 2 receiver wants to be paired, from its status flags and `pw`,
/// decided in OwnTone's order: one-time pairing first, then a PIN on every
/// connection, then a password, else the password-less transient pairing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ap2Pairing {
    /// Neither a PIN nor a password: transient pair-setup.
    Transient,
    /// `0x200`: a PIN pairing the first time, kept for the next casts.
    PinOnce,
    /// `0x8`: a PIN on every connection.
    PinEveryCast,
    /// `0x80` or `pw=true`: the receiver's password.
    Password,
}

/// What a RAOP receiver asks for before ANNOUNCE, from its status flags and
/// `pw`. The two are independent facts: a receiver can ask for both (pyatv
/// verifies the device, then sends the password on ANNOUNCE).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RaopAuth {
    /// `0x8` or `0x200`: the device must be paired with a PIN and verified at
    /// every connection (AirPlay 1 device verification).
    pub device_verification: bool,
    /// `pw=true` or `0x80`: ANNOUNCE needs the receiver's password.
    pub password: bool,
}

/// Who a receiver plays from, from the `acl` and `act` of its AirPlay
/// advertisement. Ordered from the loosest to the strictest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ReceiverAccess {
    /// Anyone on the network.
    Anyone,
    /// `acl=1`: only people in the receiver's home.
    HomeMembers,
    /// `acl=2`: only its home's admins.
    Admins,
    /// `act=2`: only the account the receiver is set up with.
    Owner,
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

/// Everything a sender decides about a receiver before connecting: the dialect
/// with its parameters, and who the receiver plays from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AirPlayCapabilities {
    pub dialect: Dialect,
    /// The raw 64-bit features value, kept for the AirPlay 2 stream setup (which
    /// echoes specific bits back to the receiver).
    pub features: u64,
    /// Who the receiver plays from.
    pub access: ReceiverAccess,
}

/// A TXT value bae reads to decide how to connect that the receiver left out
/// where the spec requires it (`raw` is `None`), or sent as text bae can't
/// parse. A receiver advertising one can't be connected to: guessing the value
/// would pick the wrong dialect or skip a PIN it requires.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnreadableSetting {
    /// The TXT key, as the receiver spelled it (`features`, `ft`, `flags`,
    /// `sf`, `et`, `acl`).
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
    /// sets no status bits. Any of them present but not hex is unreadable. A
    /// RAOP receiver's `et` lists the encryption types it offers as decimal
    /// numbers; one that isn't a number is unreadable, and without `et` the
    /// receiver offers an unencrypted stream.
    pub fn from_txt(
        service_type: RendererServiceType,
        txt: &HashMap<String, String>,
    ) -> Result<Self, UnreadableSetting> {
        let features = match txt_value(txt, "features", "ft") {
            Some((key, raw)) => parse_features(raw).ok_or_else(|| unreadable(key, raw))?,
            None if service_type == RendererServiceType::AirPlay => {
                return Err(UnreadableSetting { key: "features".to_string(), raw: None });
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

        let password = txt.get("pw").is_some_and(|v| v.eq_ignore_ascii_case("true"))
            || flags & status_flags::PASSWORD != 0;
        let access = if service_type == RendererServiceType::AirPlay {
            access_from_txt(txt)?
        } else {
            ReceiverAccess::Anyone
        };

        Ok(if is_airplay2 {
            let pairing = if flags & status_flags::ONE_TIME_PAIRING != 0 {
                Ap2Pairing::PinOnce
            } else if flags & status_flags::PIN_REQUIRED != 0 {
                Ap2Pairing::PinEveryCast
            } else if password {
                Ap2Pairing::Password
            } else {
                Ap2Pairing::Transient
            };
            AirPlayCapabilities { dialect: Dialect::AirPlay2 { pairing }, features, access }
        } else {
            let encryption = match txt.get("et") {
                Some(raw) => parse_encryption(raw).ok_or_else(|| unreadable("et", raw))?,
                None => vec![RaopEncryption::None],
            };
            let auth = RaopAuth {
                device_verification: flags
                    & (status_flags::PIN_REQUIRED | status_flags::ONE_TIME_PAIRING)
                    != 0,
                password,
            };
            AirPlayCapabilities { dialect: Dialect::Raop { encryption, auth }, features, access }
        })
    }
}

/// Who the AirPlay advertisement says the receiver plays from. `acl` absent or
/// `0` is anyone, `1` the home's members, `2` its admins (airplay2-receiver
/// `ap2-receiver.py`); another `acl` is unreadable, since how to connect is never
/// guessed. `act=2` is the owner's account (pyatv); no source names another
/// `act`, so another value limits nothing and is logged. The stricter of the two
/// holds.
fn access_from_txt(txt: &HashMap<String, String>) -> Result<ReceiverAccess, UnreadableSetting> {
    let by_acl = match txt.get("acl").map(String::as_str) {
        None | Some("0") => ReceiverAccess::Anyone,
        Some("1") => ReceiverAccess::HomeMembers,
        Some("2") => ReceiverAccess::Admins,
        Some(raw) => return Err(unreadable("acl", raw)),
    };
    let by_act = match txt.get("act").map(String::as_str) {
        Some("2") => ReceiverAccess::Owner,
        Some(raw) => {
            debug!("airplay: the receiver advertises act={raw}, which no source says limits who it plays from");
            ReceiverAccess::Anyone
        }
        None => ReceiverAccess::Anyone,
    };
    Ok(by_acl.max(by_act))
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
    UnreadableSetting { key: key.to_string(), raw: Some(raw.to_string()) }
}

fn raop_encryption(v: u32) -> Option<RaopEncryption> {
    match v {
        0 => Some(RaopEncryption::None),
        1 => Some(RaopEncryption::RsaAes),
        // 3/4/5 are FairPlay/MFiSAP variants the sender does not implement.
        _ => None,
    }
}

/// The encryption types a comma-separated `et` value offers that the sender
/// implements, de-duplicated in the receiver's order. Types the sender doesn't
/// implement are left out (logged). `None` when an entry isn't a number.
fn parse_encryption(raw: &str) -> Option<Vec<RaopEncryption>> {
    let mut offered = Vec::new();
    for part in raw.split(',') {
        let value: u32 = part.trim().parse().ok()?;
        match raop_encryption(value) {
            Some(encryption) if !offered.contains(&encryption) => offered.push(encryption),
            Some(_) => {}
            None => debug!(
                "airplay: the receiver offers encryption type {value}, which bae doesn't implement"
            ),
        }
    }
    Some(offered)
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
    let s = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
    u64::from_str_radix(s, 16).ok()
}

#[cfg(test)]
#[path = "capabilities_tests.rs"]
mod tests;
