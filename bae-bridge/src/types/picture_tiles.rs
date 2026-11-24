//! The tiles the viewer zooms a picture from, on every platform.

use super::*;

/// A picture's tiles, which the viewer zooms it from: its levels, the
/// picture's own size first and each one half the one before it, down to the
/// first that fits one tile. Each tile is `tile_edge` pixels square, except
/// at a level's right and bottom edges, where it is cut short.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgePictureTiles {
    pub tile_edge: u32,
    pub levels: Vec<BridgeTileLevel>,
}

/// One zoom level of a picture's tiles: its size in pixels, and how many
/// tiles across and down.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct BridgeTileLevel {
    pub width: u32,
    pub height: u32,
    pub columns: u32,
    pub rows: u32,
}

impl BridgePictureTiles {
    pub(crate) fn from_core(pyramid: bae_pictures::tiles::Pyramid) -> Self {
        Self {
            tile_edge: bae_pictures::tiles::TILE_EDGE,
            levels: pyramid
                .levels()
                .into_iter()
                .map(|level| BridgeTileLevel {
                    width: level.width,
                    height: level.height,
                    columns: level.columns,
                    rows: level.rows,
                })
                .collect(),
        }
    }
}

impl BridgeCoverImageSource {
    /// The picture this source names, as the viewer zooms it: a library
    /// release's cover or image file, or on a desktop a catalog's image or a
    /// picture of the import list.
    pub(crate) fn into_tiled(self) -> Result<bae_app::TiledPicture, BridgeError> {
        match self {
            Self::Library { image } => {
                if !matches!(image.image_type, BridgeLibraryImageType::Cover) {
                    return Err(BridgeError::internal(format!(
                        "the viewer zooms covers, and {} is an artist's image",
                        image.id
                    )));
                }
                let image = image.into_core();
                Ok(bae_app::TiledPicture::Gallery {
                    release_id: image.id.clone(),
                    source: bae_records::GallerySource::Cover(image),
                })
            }
            Self::ReleaseImage { release_id, source } => {
                Ok(bae_app::TiledPicture::Gallery { release_id, source: source.into_core() })
            }
            #[cfg(desktop)]
            Self::Remote { image } => Ok(bae_app::TiledPicture::Outside(
                bae_app::OutsidePicture::Catalog(image.into_core()),
            )),
            #[cfg(desktop)]
            import @ (Self::Local { .. } | Self::Embedded { .. }) => {
                Ok(bae_app::TiledPicture::Outside(bae_app::OutsidePicture::Import(
                    import.into_picture()?,
                )))
            }
            #[cfg(not(desktop))]
            Self::Remote { .. } | Self::Local { .. } | Self::Embedded { .. } => {
                Err(BridgeError::internal(
                    "only a desktop shows catalog and import pictures".to_string(),
                ))
            }
        }
    }
}
