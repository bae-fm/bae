//! What a gallery slot or a catalog's cover is called, as typed pieces each
//! surface words from the catalog's messages: core sends no prose about an
//! image.

/// What a catalog offers a cover as.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeCoverLabel {
    /// The catalog's lead image of one entity, or (`release_group`) the front
    /// image of the release group the album belongs to.
    Lead { release_group: bool },
    /// One image of the catalog's own gallery of an entity, numbered from 1.
    Gallery {
        /// How the catalog refers to the entity (a Discogs release `r123`).
        reference: Option<String>,
        position: u32,
        types: Vec<BridgeCoverImageType>,
        comment: String,
    },
}

mirror_enum! {
    #[cfg(desktop)]
    BridgeCoverLabel = bae_catalogs::remote_image::CoverLabel,
    from_core: pub(crate) fn,
    variants: {
        Lead { release_group },
        Gallery { reference, position, types: (each BridgeCoverImageType), comment },
    },
}

/// What an image of a release shows, by the Cover Art Archive's vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeCoverImageType {
    Front,
    Back,
    Booklet,
    Medium,
    Obi,
    Spine,
    Track,
    Liner,
    Sticker,
    Poster,
    Watermark,
    Raw,
    Matrix,
    Top,
    Bottom,
    Tray,
    Other,
}

mirror_enum! {
    #[cfg(desktop)]
    BridgeCoverImageType = bae_catalogs::remote_image::CoverImageType,
    from_core: pub(crate) fn,
    variants: {
        Front, Back, Booklet, Medium, Obi, Spine, Track, Liner, Sticker, Poster, Watermark, Raw,
        Matrix, Top, Bottom, Tray, Other,
    },
}

/// Localization key for the word an image type is called.
#[uniffi::export]
pub fn bridge_cover_image_type_key(image_type: BridgeCoverImageType) -> String {
    let word = match image_type {
        BridgeCoverImageType::Front => "front",
        BridgeCoverImageType::Back => "back",
        BridgeCoverImageType::Booklet => "booklet",
        BridgeCoverImageType::Medium => "medium",
        BridgeCoverImageType::Obi => "obi",
        BridgeCoverImageType::Spine => "spine",
        BridgeCoverImageType::Track => "track",
        BridgeCoverImageType::Liner => "liner",
        BridgeCoverImageType::Sticker => "sticker",
        BridgeCoverImageType::Poster => "poster",
        BridgeCoverImageType::Watermark => "watermark",
        BridgeCoverImageType::Raw => "raw",
        BridgeCoverImageType::Matrix => "matrix",
        BridgeCoverImageType::Top => "top",
        BridgeCoverImageType::Bottom => "bottom",
        BridgeCoverImageType::Tray => "tray",
        BridgeCoverImageType::Other => "other",
    };
    format!("core.cover.image_type.{word}")
}

/// What a release's gallery slot is called: the surface words the cover; a
/// file is called by its original filename.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeGalleryLabel {
    Cover,
    File { name: String },
}

mirror_enum! {
    BridgeGalleryLabel = bae_views::album_detail::GalleryLabel,
    from_core: pub(crate) fn,
    variants: { Cover, File { name } },
}
