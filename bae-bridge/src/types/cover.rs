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

/// A missing external identity is distinct from a linked release with no art.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeRemoteCoverGallery {
    Unlinked,
    Linked {
        covers: Vec<super::BridgeRemoteCover>,
    },
}

impl BridgeRemoteCoverGallery {
    pub(crate) fn from_core(gallery: bae_core::import::cover_art::RemoteCoverGallery) -> Self {
        match gallery {
            bae_core::import::cover_art::RemoteCoverGallery::Unlinked => Self::Unlinked,
            bae_core::import::cover_art::RemoteCoverGallery::Linked(covers) => Self::Linked {
                covers: covers
                    .into_iter()
                    .map(super::BridgeRemoteCover::from_core)
                    .collect(),
            },
        }
    }
}
