/// The owner whose metadata identities supply the artwork picker.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeCoverTarget {
    Release { release_id: String },
    Candidate { candidate_key: String },
}

mirror_enum! {
    BridgeCoverTarget = bae_core::import::cover_art::CoverTarget,
    into_core: pub(crate) fn,
    variants: { Release(release_id), Candidate(candidate_key) },
}

/// A missing external identity is distinct from a linked release with no art.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeRemoteCoverGallery {
    Unlinked,
    Linked {
        covers: Vec<super::BridgeRemoteCover>,
    },
}

mirror_enum! {
    BridgeRemoteCoverGallery = bae_core::import::cover_art::RemoteCoverGallery,
    from_core: pub(crate) fn,
    variants: {
        Unlinked,
        Linked(covers: (each super::BridgeRemoteCover)),
    },
}
