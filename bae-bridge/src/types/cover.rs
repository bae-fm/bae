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
    /// The release or candidate names no catalog release, so there is
    /// nothing to offer.
    Unlinked,
    /// The release or candidate names a catalog release, and the last fetch
    /// stored the covers its catalogs offered, which can be none.
    Linked,
}

mirror_enum! {
    BridgeCoverFetch = bae_import::CoverFetch,
    from_core: pub(crate) fn,
    variants: { NotFetched, Unlinked, Linked },
}

/// What names a cover in the picker, which the app words, and what saving it
/// selects. Saving the cover the release has now selects nothing.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeCoverItemLabel {
    /// The cover the release has now.
    Current,
    /// An image file of the release or the folder, by its name there.
    File { name: String, selection: super::BridgeCoverSelection },
    /// A cover a catalog offers: the service that serves it, and what it shows.
    Remote {
        source_name: String,
        label: super::BridgeCoverLabel,
        selection: super::BridgeCoverSelection,
    },
}

/// What names one cover of a picker: the saved cover, a catalog's image by
/// its address, or an image file by its id.
#[derive(Debug, Clone, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum BridgeCoverKey {
    Saved,
    Remote { url: String },
    File { file_id: String },
}

mirror_enum! {
    BridgeCoverKey = bae_import::CoverKey,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    variants: { Saved, Remote(url), File(file_id) },
}

/// One cover the picker shows: its key, where its image comes from, and its
/// label, which holds what saving it selects.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeGalleryCover {
    pub key: BridgeCoverKey,
    pub image: super::BridgeCoverImageSource,
    pub label: BridgeCoverItemLabel,
}

/// The covers of one source, and how many the filter leaves in it.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeCoverSection {
    pub items: Vec<BridgeGalleryCover>,
    pub count: u32,
}

mirror_struct! {
    BridgeCoverSection = bae_import::CoverSection,
    from_core: fn,
    fields: { items: (each BridgeGalleryCover), count },
}

/// The covers before and after the one a picker focuses, wrapping around
/// the ends.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeCoverNeighbors {
    pub previous: BridgeCoverKey,
    pub next: BridgeCoverKey,
}

mirror_struct! {
    BridgeCoverNeighbors = bae_import::Neighbors<bae_import::CoverKey>,
    from_core: fn,
    fields: { previous: (BridgeCoverKey), next: (BridgeCoverKey) },
}

/// The cover a picker focuses: the cover, where it sits among the covers in
/// the order the picker walks them, and the covers before and after it,
/// which a picker of one cover lacks.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeGalleryFocus {
    pub cover: BridgeGalleryCover,
    pub position: u32,
    pub neighbors: Option<BridgeCoverNeighbors>,
}

/// A cover picker under one filter: a library release's saved cover, the
/// catalogs' covers and the image files, each section only while the filter
/// shows it, the number of covers in all of them, and the cover it focuses,
/// none while it shows no cover.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeCoverGallery {
    pub current: Option<BridgeGalleryCover>,
    pub fetch: BridgeCoverFetch,
    pub remote: Option<BridgeCoverSection>,
    pub files: Option<BridgeCoverSection>,
    pub total: u32,
    pub focus: Option<BridgeGalleryFocus>,
}

impl BridgeCoverGallery {
    pub(crate) fn from_core(gallery: bae_import::CoverGallery) -> Self {
        Self {
            current: gallery.current.map(BridgeGalleryCover::from_core),
            fetch: BridgeCoverFetch::from_core(gallery.fetch),
            remote: gallery.remote.map(BridgeCoverSection::from_core),
            files: gallery.files.map(BridgeCoverSection::from_core),
            total: gallery.total,
            focus: gallery.focus.map(|focus| BridgeGalleryFocus {
                cover: BridgeGalleryCover::from_core(focus.cover),
                position: focus.position,
                neighbors: focus.neighbors.map(BridgeCoverNeighbors::from_core),
            }),
        }
    }
}

impl BridgeGalleryCover {
    fn from_core(cover: bae_import::GalleryCover) -> Self {
        Self {
            key: BridgeCoverKey::from_core(cover.key),
            image: super::BridgeCoverImageSource::from_core(cover.image),
            label: match cover.label {
                bae_import::GalleryLabel::Current => BridgeCoverItemLabel::Current,
                bae_import::GalleryLabel::File { name, selection } => BridgeCoverItemLabel::File {
                    name,
                    selection: super::BridgeCoverSelection::from_core(selection),
                },
                bae_import::GalleryLabel::Remote { label, source, selection } => {
                    BridgeCoverItemLabel::Remote {
                        source_name: source.cover_source_name().to_string(),
                        label: super::BridgeCoverLabel::from_core(label),
                        selection: super::BridgeCoverSelection::from_core(selection),
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
    /// The release the scan stores at this key, before bae imports it.
    Candidate { candidate_key: Vec<u8> },
}

mirror_enum! {
    BridgeCoverGalleryOwner = bae_import::CoverGalleryOwner,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    variants: { Release(release_id), Candidate(candidate_key: (into)) },
}

/// Whose picker to read, the source it shows, and the cover the person
/// focused last.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeCoverGalleryRequest {
    pub owner: BridgeCoverGalleryOwner,
    pub filter: BridgeCoverFilter,
    pub focus: Option<BridgeCoverKey>,
}

/// One value the read delivered: the request it answered, and the picker's
/// covers.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeCoverGallerySnapshot {
    pub request: BridgeCoverGalleryRequest,
    pub gallery: BridgeCoverGallery,
}
