//! The tiles a viewer zooms a library release's pictures from, through the
//! library's tile cache: its cover or one of its image files.

use std::future::Future;

use bae_pictures::tile_cache::OpenError;
use bae_pictures::tiles::Pyramid;
use bae_pictures::PictureOrigin;

use super::*;

/// The open of a picture for a cut, as it runs.
type OpeningPicture =
    std::pin::Pin<Box<dyn Future<Output = Result<Box<dyn PictureOrigin>, OpenError>> + Send>>;

impl LibraryManager {
    /// The tiles of the gallery slot `source` of release `release_id`, cut
    /// from the slot's picture at full quality: the cover's picture, or the
    /// image file as the release holds it.
    pub async fn gallery_tiles(
        &self,
        release_id: &str,
        source: &GallerySource,
    ) -> Result<Pyramid, LibraryError> {
        let open = self.gallery_opener(release_id, source);
        Ok(self.tiles.pyramid(&gallery_identity(source), open).await?)
    }

    /// One tile of the gallery slot `source` of release `release_id`: column
    /// `x` and row `y` of level `level`, 0 being the picture's own size.
    pub async fn gallery_tile(
        &self,
        release_id: &str,
        source: &GallerySource,
        level: u32,
        x: u32,
        y: u32,
    ) -> Result<Vec<u8>, LibraryError> {
        let open = self.gallery_opener(release_id, source);
        Ok(self.tiles.tile(&gallery_identity(source), level, x, y, open).await?)
    }

    /// What opens the picture of a gallery slot for a cut.
    fn gallery_opener(
        &self,
        release_id: &str,
        source: &GallerySource,
    ) -> impl FnOnce() -> OpeningPicture + Send + 'static {
        let (manager, release_id, source) = (self.clone(), release_id.to_string(), source.clone());
        move || {
            Box::pin(async move {
                manager
                    .open_gallery_picture(&release_id, &source)
                    .await
                    .map_err(|error| OpenError::new(error.category(), error))
            })
        }
    }

    /// The picture of a gallery slot at full quality, read in ranges as a
    /// cut asks for them: the image file as the release holds it, or the
    /// cover's picture.
    async fn open_gallery_picture(
        &self,
        release_id: &str,
        source: &GallerySource,
    ) -> Result<Box<dyn PictureOrigin>, LibraryError> {
        match source {
            GallerySource::ReleaseFile { file_id } => {
                let file = self.release_file(release_id, file_id).await?;
                let file = self.store.file_ref(file.file).await?;
                Ok(Box::new(self.open_stored(&format!("release file {file_id}"), &file).await?))
            }
            GallerySource::Cover(image) => {
                self.open_cover_picture(&image.id).await?.ok_or_else(|| {
                    LibraryError::Storage(format!("gallery cover {} has no row", image.id))
                })
            }
        }
    }
}

/// What names the bytes of a gallery slot's picture in the tile cache: a
/// release file's id, which never names other bytes, or the cover's version,
/// which moves whenever the cover does.
fn gallery_identity(source: &GallerySource) -> String {
    match source {
        GallerySource::ReleaseFile { file_id } => format!("release file {file_id}"),
        GallerySource::Cover(image) => format!("cover {} {}", image.id, image.version),
    }
}

#[cfg(test)]
#[path = "picture_tiles_tests.rs"]
mod tests;
