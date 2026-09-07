use super::super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeCombinationAction {
    Combine,
    Separate,
}

mirror_enum! {
    #[cfg(feature = "desktop")]
    BridgeCombinationAction = bae_core::import::combination::CombinationAction,
    from_core: pub(crate) fn,
    variants: { Combine, Separate },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeCombinationTrackOrder {
    SeparateDiscs,
    Continuous,
}

mirror_enum! {
    #[cfg(feature = "desktop")]
    BridgeCombinationTrackOrder = bae_core::import::combination::CombinationTrackOrder,
    into_core: pub(crate) fn,
    variants: { SeparateDiscs, Continuous },
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeCombinationPart {
    pub candidate_key: String,
    pub folder_name: String,
    pub file_prefix: String,
    pub first_disc: u32,
    pub disc_count: u32,
    pub track_count: u32,
}

mirror_struct! {
    #[cfg(feature = "desktop")]
    BridgeCombinationPart = bae_core::import::combination::CombinationPart,
    from_core: pub(crate) fn,
    fields: {
        candidate_key,
        folder_name,
        file_prefix,
        first_disc,
        disc_count,
        track_count,
    },
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeCombinationPreview {
    pub parts: Vec<BridgeCombinationPart>,
    pub tracks: Vec<BridgeTrackUserEdit>,
}

/// Not a `mirror_struct`: `CandidateCombination` keeps fields of its own that
/// are nobody else's to read, so it cannot be destructured here.
#[cfg(feature = "desktop")]
impl BridgeCombinationPreview {
    pub(crate) fn from_core(
        combination: bae_core::import::combination::CandidateCombination,
    ) -> Self {
        Self {
            parts: combination
                .parts
                .into_iter()
                .map(BridgeCombinationPart::from_core)
                .collect(),
            tracks: combination
                .tracks
                .into_iter()
                .map(BridgeTrackUserEdit::from_core)
                .collect(),
        }
    }
}
