/// A missing external identity is distinct from a linked release with no art.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeRemoteCoverGallery {
    Unlinked,
    Linked { covers: Vec<super::BridgeRemoteCover> },
}

mirror_enum! {
    BridgeRemoteCoverGallery = bae_import::RemoteCoverGallery,
    from_core: pub(crate) fn,
    variants: {
        Unlinked,
        Linked(covers: (each super::BridgeRemoteCover)),
    },
}

/// Which of the library cover picker's sources a read shows.
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

/// What this device knows of a library release's catalog covers.
#[derive(Debug, Clone, Copy, uniffi::Enum)]
pub enum BridgeCoverFetch {
    /// No fetch has stored an answer yet.
    NotFetched,
    /// The release names no catalog release, so there is nothing to offer.
    Unlinked,
    /// The release is linked; the covers stored are what its catalogs offered,
    /// possibly none.
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
    /// An image file of the release, by its name in the release.
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

/// A library release's cover picker under one filter: its saved cover, the
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

/// The library release whose picker to read, and the source it shows.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeCoverGalleryRequest {
    pub release_id: String,
    pub filter: BridgeCoverFilter,
}

/// One value the read delivered: the request it answered, and the picker's
/// covers.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeCoverGallerySnapshot {
    pub request: Option<BridgeCoverGalleryRequest>,
    pub gallery: Option<BridgeCoverGallery>,
}
