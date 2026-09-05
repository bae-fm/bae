/// The owner whose metadata identities supply the artwork picker.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeCoverTarget {
    Release { release_id: String },
    Candidate { candidate_key: String },
}

impl BridgeCoverTarget {
    pub(crate) fn into_core(self) -> bae_core::import::cover_art::CoverTarget {
        match self {
            Self::Release { release_id } => {
                bae_core::import::cover_art::CoverTarget::Release(release_id)
            }
            Self::Candidate { candidate_key } => {
                bae_core::import::cover_art::CoverTarget::Candidate(candidate_key)
            }
        }
    }
}
