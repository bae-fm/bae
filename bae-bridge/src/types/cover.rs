/// Which of the cover picker's sources a read shows.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeCoverFilter {
    All,
    ReleaseFiles,
    /// The covers one catalog offered.
    Catalog {
        catalog: super::BridgeCatalog,
    },
}

mirror_enum! {
    BridgeCoverFilter = bae_import::CoverFilter,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    variants: { All, ReleaseFiles, Catalog(catalog: (super::BridgeCatalog)) },
}

/// What this device knows of the catalog covers of the release or candidate
/// a picker shows.
#[derive(Debug, Clone, Copy, uniffi::Enum)]
pub enum BridgeCoverFetch {
    /// No fetch has stored an answer yet.
    NotFetched,
    /// It names no catalog release, so there is nothing to offer.
    Unlinked,
    /// It names a catalog release, and the last fetch stored the covers its
    /// catalogs offered, which can be none.
    Linked,
}

mirror_enum! {
    BridgeCoverFetch = bae_import::CoverFetch,
    from_core: pub(crate) fn,
    variants: { NotFetched, Unlinked, Linked },
}

/// Which part of the cover picker a section is.
#[derive(Debug, Clone, Copy, uniffi::Enum)]
pub enum BridgeCoverSectionSource {
    Remote,
    ReleaseFiles,
}

mirror_enum! {
    BridgeCoverSectionSource = bae_import::CoverSectionSource,
    from_core: pub(crate) fn,
    variants: { Remote, ReleaseFiles },
}

/// What names a cover in the picker; the app words it.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeCoverItemLabel {
    /// The cover the release has now.
    Current,
    /// An image file of the release or the folder, by its name there.
    File { name: String },
    /// A cover a catalog offers: the service that serves it, and what it shows.
    Remote { source_name: String, label: super::BridgeCoverLabel },
}

/// One cover the picker shows: what saving it selects (none for the saved
/// cover), where its image comes from, and its label.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeGalleryCover {
    pub selection: Option<super::BridgeCoverSelection>,
    pub image: super::BridgeCoverImageSource,
    pub label: BridgeCoverItemLabel,
}

/// The covers of one source, and how many the filter leaves in it.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeCoverSection {
    pub source: BridgeCoverSectionSource,
    pub items: Vec<BridgeGalleryCover>,
    pub count: u32,
}

/// A cover picker under one filter: a library release's saved cover, the
/// sections the filter shows, and the number of covers in all of them.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeCoverGallery {
    pub current: Option<BridgeGalleryCover>,
    pub fetch: BridgeCoverFetch,
    pub sections: Vec<BridgeCoverSection>,
    pub total: u32,
}

impl BridgeCoverGallery {
    pub(crate) fn from_core(gallery: bae_import::CoverGallery) -> Self {
        Self {
            current: gallery.current.map(BridgeGalleryCover::from_core),
            fetch: BridgeCoverFetch::from_core(gallery.fetch),
            sections: gallery
                .sections
                .into_iter()
                .map(|section| BridgeCoverSection {
                    source: BridgeCoverSectionSource::from_core(section.source),
                    items: section.items.into_iter().map(BridgeGalleryCover::from_core).collect(),
                    count: section.count,
                })
                .collect(),
            total: gallery.total,
        }
    }
}

impl BridgeGalleryCover {
    fn from_core(cover: bae_import::GalleryCover) -> Self {
        Self {
            selection: cover.selection.map(super::BridgeCoverSelection::from_core),
            image: super::BridgeCoverImageSource::from_core(cover.image),
            label: match cover.label {
                bae_import::GalleryLabel::Current => BridgeCoverItemLabel::Current,
                bae_import::GalleryLabel::File(name) => BridgeCoverItemLabel::File { name },
                bae_import::GalleryLabel::Remote { label, source } => {
                    BridgeCoverItemLabel::Remote {
                        source_name: source.cover_source_name().to_string(),
                        label: super::BridgeCoverLabel::from_core(label),
                    }
                }
            },
        }
    }
}

/// Whose covers a picker shows.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeCoverGalleryOwner {
    /// A library release.
    Release { release_id: String },
    /// The release the scan stores at this key, before it is imported.
    Candidate { candidate_key: Vec<u8> },
}

impl BridgeCoverGalleryOwner {
    pub(crate) fn from_core(owner: bae_import::CoverGalleryOwner) -> Self {
        match owner {
            bae_import::CoverGalleryOwner::Release(release_id) => Self::Release { release_id },
            bae_import::CoverGalleryOwner::Candidate(key) => {
                Self::Candidate { candidate_key: key.into() }
            }
        }
    }

    pub(crate) fn into_core(self) -> bae_import::CoverGalleryOwner {
        match self {
            Self::Release { release_id } => bae_import::CoverGalleryOwner::Release(release_id),
            Self::Candidate { candidate_key } => {
                bae_import::CoverGalleryOwner::Candidate(candidate_key.into())
            }
        }
    }
}

/// Whose picker to read, and the source it shows.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeCoverGalleryRequest {
    pub owner: BridgeCoverGalleryOwner,
    pub filter: BridgeCoverFilter,
}

/// One value the read delivered: the request it answered, and the picker's
/// covers.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeCoverGallerySnapshot {
    pub request: Option<BridgeCoverGalleryRequest>,
    pub gallery: Option<BridgeCoverGallery>,
}
