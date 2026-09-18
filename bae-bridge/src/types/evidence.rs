use super::{BridgeMarkKind, BridgeSignalOrigin};
use bae_mirror::mirror_enum;

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeEvidenceSubject {
    Candidate { key: String },
    Release { id: String },
}

mirror_enum! {
    BridgeEvidenceSubject = bae_core::library::EvidenceSubject,
    into_core: pub(crate) fn,
    variants: { Candidate { key }, Release { id } },
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeEvidenceSelection {
    Mark {
        kind: BridgeMarkKind,
        value: String,
        origin: BridgeSignalOrigin,
    },
    Verification,
}

mirror_enum! {
    BridgeEvidenceSelection = bae_core::library::EvidenceSelection,
    into_core: pub(crate) fn,
    variants: { Mark { kind: (BridgeMarkKind), value, origin: (BridgeSignalOrigin) }, Verification },
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeEvidenceContent {
    Document { name: String, text: String },
    Image { name: String, bytes: Vec<u8> },
    Reveal { path: String },
}

mirror_enum! {
    BridgeEvidenceContent = bae_core::library::EvidenceContent,
    from_core: pub(crate) fn,
    variants: { Document { name, text }, Image { name, bytes }, Reveal { path } },
}
