//! Image domain operations for [`LibraryManager`].

use super::*;

impl LibraryManager {
    /// Bytes of one gallery slot, dispatching the read on its [`GallerySource`]
    /// so no caller picks the byte source itself. With a size, a cover slot
    /// reads the copy that covers it and an image file's slot reads its
    /// preview; with none, a slot reads the picture itself, which only the
    /// viewer asks for: a cover's kept original, or the image file as the
    /// release holds it.
    /// The gallery carries a cover slot only when a cover exists, so a `Cover`
    /// with no bytes here is exceptional and surfaces rather than being masked.
    pub async fn read_gallery_bytes(
        &self,
        release_id: &str,
        source: &GallerySource,
        pixels: Option<u32>,
    ) -> Result<Vec<u8>, LibraryError> {
        match (source, pixels) {
            (GallerySource::Cover(image), Some(_)) => {
                self.read_image_bytes(image, pixels).await?.ok_or_else(|| {
                    LibraryError::Storage(format!("gallery cover image {} has no bytes", image.id))
                })
            }
            (GallerySource::Cover(image), None) => {
                self.read_cover_original(&image.id).await?.ok_or_else(|| {
                    LibraryError::Storage(format!("gallery cover {} has no original", image.id))
                })
            }
            (GallerySource::ReleaseFile { file_id }, Some(_)) => {
                self.read_release_preview(file_id).await
            }
            (GallerySource::ReleaseFile { file_id }, None) => {
                self.load_gallery_image(release_id, file_id).await
            }
        }
    }

    /// The preview of one of a release's image files, which the device that
    /// imported the release made from the original. A file with no preview is
    /// an error: every image file is written with its preview.
    async fn read_release_preview(&self, file_id: &str) -> Result<Vec<u8>, LibraryError> {
        let file = self.records.release_preview_file(file_id).await?.ok_or_else(|| {
            LibraryError::Storage(format!("release file {file_id} has no preview"))
        })?;
        self.read_image_file(&format!("preview of release file {file_id}"), &file).await
    }

    /// Bytes of one of a release's image files, read from the local copy when it
    /// exists here and otherwise downloaded from the release's storage (and
    /// decrypted). The `ReleaseFile` arm of [`read_gallery_bytes`](Self::read_gallery_bytes).
    async fn load_gallery_image(
        &self,
        release_id: &str,
        file_id: &str,
    ) -> Result<Vec<u8>, LibraryError> {
        let file = self.records.find_release_file(release_id, file_id).await?.ok_or_else(|| {
            LibraryError::FileMissing {
                release_id: release_id.to_string(),
                file_id: file_id.to_string(),
            }
        })?;
        self.read_release_file(&file).await
    }

    /// Change the cover art for an album's release. A `ReleaseImage` selection
    /// reads an image file already in the library by file id and makes the cover
    /// from that file; a `RemoteCover` one downloads the catalog's image from a
    /// URL and keeps it as the cover's original. Either way the cover's browse
    /// and large copies are made from the picture.
    #[cfg(desktop)]
    pub async fn change_cover(
        &self,
        release_id: &str,
        selection: CoverSelection,
    ) -> Result<(), LibraryError> {
        let (picture, bytes) = match selection {
            CoverSelection::ReleaseImage { file_id } => {
                let file = self.records.find_file_by_id(&file_id).await?.ok_or_else(|| {
                    LibraryError::FileMissing {
                        release_id: release_id.to_string(),
                        file_id: file_id.clone(),
                    }
                })?;

                // Read the chosen release image file through coven's locality-aware
                // read (the user's own file when Local, the cache/cloud when Remote).
                let bytes = self.read_release_file(&file).await?;
                (NewCoverPicture::ReleaseFile { file_id }, bytes)
            }
            CoverSelection::RemoteCover { url } => {
                let bytes = self
                    .remote_images
                    .fetch_required(&url)
                    .await
                    .map_err(bae_import::ImportError::from)?
                    .bytes;
                (NewCoverPicture::Original { bytes: bytes.clone() }, bytes)
            }
        };

        // Decoding and shrinking the picture is CPU work.
        let (browse, large) = self
            .diagnostics
            .blocking("make a cover's copies", move || {
                let copy =
                    |bytes: &Vec<u8>, step| bae_pictures::make_copy_of_bytes(bytes.clone(), step);
                Ok::<_, bae_pictures::PicturesError>((
                    copy(&bytes, bae_pictures::PictureStep::S768)?,
                    copy(&bytes, bae_pictures::PictureStep::S1536)?,
                ))
            })
            .await?
            .map_err(|error| bae_import::ImportError::CoverArt { detail: error.to_string() })?;
        let content_type = match (browse.format, large.format) {
            (bae_pictures::CopyFormat::Jpeg, bae_pictures::CopyFormat::Jpeg) => ContentType::Jpeg,
            (bae_pictures::CopyFormat::Png, bae_pictures::CopyFormat::Png) => ContentType::Png,
            _ => {
                return Err(LibraryError::Storage(format!(
                    "the copies of {release_id}'s cover are in different formats"
                )))
            }
        };
        self.records
            .write_cover(NewCover {
                release_id: release_id.to_string(),
                picture,
                browse: browse.bytes,
                large: large.bytes,
                content_type,
            })
            .await?;

        Ok(())
    }
}

#[cfg(test)]
#[path = "image_tests.rs"]
mod tests;
