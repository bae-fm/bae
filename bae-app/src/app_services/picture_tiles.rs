//! The tiles the viewer zooms a picture from, for every kind of picture it
//! shows: a library release's cover or image file, and on a desktop a
//! catalog's image or a picture of the import list. The library's tile cache
//! keeps them all; each kind opens its own original for a cut.

#[cfg(desktop)]
use bae_pictures::tile_cache::OpenError;
use bae_pictures::tiles::Pyramid;
#[cfg(desktop)]
use bae_pictures::PictureOrigin;

use super::*;

/// A picture the viewer zooms.
#[derive(Debug, Clone)]
pub enum TiledPicture {
    /// A slot of a library release's gallery: its cover or one of its image
    /// files.
    Gallery { release_id: String, source: bae_records::GallerySource },
    /// A picture from outside the library.
    #[cfg(desktop)]
    Outside(OutsidePicture),
}

/// A picture from outside the library that a desktop's viewer zooms.
#[cfg(desktop)]
#[derive(Debug, Clone)]
pub enum OutsidePicture {
    /// A catalog's image, cut from the original it serves.
    Catalog(bae_catalogs::remote_image::RemoteImageSet),
    /// A picture of the import list: an image file of a candidate's folder,
    /// or art in the tags of one of its audio files.
    Import(bae_import::ImportPicture),
}

/// What opens a picture's original for a cut.
#[cfg(desktop)]
type Opener = Box<
    dyn FnOnce() -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<Box<dyn PictureOrigin>, OpenError>> + Send>,
        > + Send,
>;

impl AppServices {
    /// The tiles of `picture`: its levels, each with its size and how many
    /// tiles across and down. The first request cuts them, and every later
    /// one reads the kept tiles.
    pub async fn picture_tiles(
        &self,
        picture: TiledPicture,
    ) -> Result<Pyramid, bae_library::LibraryError> {
        let manager = &self.inner.manager;
        match picture {
            TiledPicture::Gallery { release_id, source } => {
                manager.gallery_tiles(&release_id, &source).await
            }
            #[cfg(desktop)]
            TiledPicture::Outside(picture) => {
                let (identity, open) = self.outside_picture(picture);
                Ok(self.inner.tiles.pyramid(&identity, open).await?)
            }
        }
    }

    /// One tile of `picture`: column `x` and row `y` of level `level`, 0
    /// being the picture's own size.
    pub async fn picture_tile(
        &self,
        picture: TiledPicture,
        level: u32,
        x: u32,
        y: u32,
    ) -> Result<Vec<u8>, bae_library::LibraryError> {
        let manager = &self.inner.manager;
        match picture {
            TiledPicture::Gallery { release_id, source } => {
                manager.gallery_tile(&release_id, &source, level, x, y).await
            }
            #[cfg(desktop)]
            TiledPicture::Outside(picture) => {
                let (identity, open) = self.outside_picture(picture);
                Ok(self.inner.tiles.tile(&identity, level, x, y, open).await?)
            }
        }
    }

    /// What names a picture from outside the library, and what opens its
    /// original: the catalog's image downloaded through the catalog image
    /// cache, or the import list's picture as the import reads it.
    #[cfg(desktop)]
    fn outside_picture(&self, picture: OutsidePicture) -> (String, Opener) {
        let import = self.inner.import.clone();
        match picture {
            OutsidePicture::Catalog(image) => (
                format!("catalog image {}", image.url),
                Box::new(move || {
                    Box::pin(async move {
                        let url = image.url.clone();
                        let original = import
                            .fetch_remote_image_bytes(image, None)
                            .await
                            .map_err(|error| OpenError::new(error.category(), error))?
                            .ok_or_else(|| {
                                OpenError::new(
                                    bae_diagnostics::UiErrorCategory::Import,
                                    std::io::Error::other(format!("{url} serves no image")),
                                )
                            })?;
                        Ok(Box::new(std::io::Cursor::new(original.bytes)) as Box<dyn PictureOrigin>)
                    })
                }),
            ),
            OutsidePicture::Import(picture) => (
                picture.identity(),
                Box::new(move || {
                    Box::pin(async move {
                        import
                            .open_picture_original(picture)
                            .await
                            .map_err(|error| OpenError::new(error.category(), error))
                    })
                }),
            ),
        }
    }
}
