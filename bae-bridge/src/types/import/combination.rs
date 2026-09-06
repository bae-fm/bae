use super::super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeCombinationAction {
    Combine,
    Separate,
}

#[cfg(feature = "desktop")]
impl BridgeCombinationAction {
    pub(crate) fn from_core(action: bae_core::import::combination::CombinationAction) -> Self {
        match action {
            bae_core::import::combination::CombinationAction::Combine => Self::Combine,
            bae_core::import::combination::CombinationAction::Separate => Self::Separate,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeCombinationTrackOrder {
    SeparateDiscs,
    Continuous,
}

#[cfg(feature = "desktop")]
impl BridgeCombinationTrackOrder {
    pub(crate) fn into_core(self) -> bae_core::import::combination::CombinationTrackOrder {
        use bae_core::import::combination::CombinationTrackOrder as Order;
        match self {
            Self::SeparateDiscs => Order::SeparateDiscs,
            Self::Continuous => Order::Continuous,
        }
    }
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

#[cfg(feature = "desktop")]
impl BridgeCombinationPart {
    pub(crate) fn from_core(part: bae_core::import::combination::CombinationPart) -> Self {
        let bae_core::import::combination::CombinationPart {
            candidate_key,
            folder_name,
            file_prefix,
            first_disc,
            disc_count,
            track_count,
        } = part;
        Self {
            candidate_key,
            folder_name,
            file_prefix,
            first_disc,
            disc_count,
            track_count,
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeCombinationPreview {
    pub parts: Vec<BridgeCombinationPart>,
    pub tracks: Vec<BridgeTrackUserEdit>,
}

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
