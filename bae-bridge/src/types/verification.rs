//! How many other copies of a release's audio agree with this one.

use bae_mirror::{mirror_enum, mirror_struct};

/// Where the counts came from. Mirrors
/// `bae_core::import::VerificationSource`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeVerificationSource {
    Log,
}

mirror_enum! {
    BridgeVerificationSource = bae_core::import::VerificationSource,
    from_core: pub(crate) fn,
    variants: { Log },
}

/// One track's agreement count from each rip database, and the CRC of the
/// audio those counts are about. Mirrors
/// `bae_core::import::TrackVerification`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct BridgeTrackVerification {
    pub number: u32,
    pub accuraterip_confidence: Option<u32>,
    pub ctdb_confidence: Option<u32>,
    pub crc: Option<u32>,
}

mirror_struct! {
    BridgeTrackVerification = bae_core::import::TrackVerification,
    from_core: pub(crate) fn,
    fields: {
        number,
        accuraterip_confidence,
        ctdb_confidence,
        crc,
    },
}

/// What the rip databases said about a release's audio.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeVerification {
    pub source: BridgeVerificationSource,
    /// How many other rips of this release match, as the one number a surface
    /// draws: the weakest track's best database. `None` when a track no
    /// database confirmed leaves the release unverified — core decides that,
    /// so no surface counts the tracks itself.
    pub matched_copies: Option<u32>,
    pub tracks: Vec<BridgeTrackVerification>,
}

impl BridgeVerification {
    pub(crate) fn from_core(value: bae_core::import::Verification) -> Self {
        let matched_copies = value.matched_copies();
        let bae_core::import::Verification { source, tracks } = value;
        BridgeVerification {
            source: BridgeVerificationSource::from_core(source),
            matched_copies,
            tracks: tracks
                .into_iter()
                .map(BridgeTrackVerification::from_core)
                .collect(),
        }
    }
}
