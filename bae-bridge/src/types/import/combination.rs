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

/// Which source folders a combined candidate is made of. Its files and track
/// rows reach the receiver as the candidate's own files and draft rows, so they
/// are not repeated here.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeCombination {
    pub parts: Vec<BridgeCombinationPart>,
}

/// Not a `mirror_struct`: `CandidateCombination` keeps fields of its own that
/// are nobody else's to read, so it cannot be destructured here.
#[cfg(feature = "desktop")]
impl BridgeCombination {
    pub(crate) fn from_core(
        combination: bae_core::import::combination::CandidateCombination,
    ) -> Self {
        Self {
            parts: combination
                .parts
                .into_iter()
                .map(BridgeCombinationPart::from_core)
                .collect(),
        }
    }
}
