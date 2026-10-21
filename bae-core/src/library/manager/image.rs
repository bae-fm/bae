//! Image domain operations for [`LibraryManager`].

use super::*;

impl LibraryManager {
    /// Bytes of one gallery slot, dispatching the read on its [`GallerySource`]
    /// so no caller picks the byte source itself: a `Cover` is read by its image
    /// ref, a `ReleaseFile` by file id (`load_gallery_image`).
    /// The gallery carries a cover slot only when a cover exists, so a `Cover`
    /// with no bytes here is exceptional and surfaces rather than being masked.
    pub async fn read_gallery_bytes(
        &self,
        release_id: &str,
        source: &GallerySource,
    ) -> Result<ImageRead, LibraryError> {
        match source {
            GallerySource::Cover(image) => self.read_image(image).await?.ok_or_else(|| {
                LibraryError::Storage(format!("gallery cover image {} has no bytes", image.id))
            }),
            GallerySource::ReleaseFile { file_id } => {
                match self.load_gallery_image(release_id, file_id).await {
                    Ok(bytes) => Ok(ImageRead::Bytes(bytes)),
                    Err(LibraryError::Database(crate::db::DbError::FileRead(
                        coven::FileReadError::Uploading { .. },
                    ))) => {
                        let file = self.release_file_ref(file_id).await?;
                        Ok(ImageRead::Arriving(super::coven_files::arriving(
                            file_id, &file,
                        )))
                    }
                    Err(error) => Err(error),
                }
            }
        }
    }

    /// Wait until `file` no longer arrives: coven fetched it once its uploader
    /// uploaded it, or its wait turned into another condition. The image that
    /// named it reads again then. A file coven fetches by itself, as every
    /// cover, artist image and avatar, ends its wait on its own; any file
    /// ends it on the person's retry.
    pub async fn wait_for_arrival(&self, file: ArrivingFile) -> Result<(), LibraryError> {
        let mut pending = self.database.subscribe_pending();
        loop {
            let records = pending.next().await?;
            if !records
                .iter()
                .any(|record| ArrivingFile::of(record) == Some(file))
            {
                return Ok(());
            }
        }
    }

    /// Bytes of one of a release's image files, read from the local copy when it
    /// exists here and otherwise downloaded from the release's cloud home (and
    /// decrypted). The `ReleaseFile` arm of [`read_gallery_bytes`](Self::read_gallery_bytes).
    async fn load_gallery_image(
        &self,
        release_id: &str,
        file_id: &str,
    ) -> Result<Vec<u8>, LibraryError> {
        let file = self
            .get_files_for_release(release_id)
            .await?
            .into_iter()
            .find(|f| f.id == file_id)
            .ok_or_else(|| {
                LibraryError::Import(format!(
                    "Image file {file_id} is not part of release {release_id}"
                ))
            })?;
        crate::storage::transfer::read_release_file_bytes(&file, self).await
    }

    pub async fn get_library_image(
        &self,
        id: &str,
        image_type: &LibraryImageType,
    ) -> Result<Option<DbLibraryImage>, LibraryError> {
        Ok(self.database.find_library_image(id, image_type).await?)
    }

    /// Change the cover art for an album's release. A `ReleaseImage` selection
    /// reads an image file already in the library by file id; a `RemoteCover` one
    /// downloads it from a URL. Either way the bytes are resized and stored as the
    /// release's cover file.
    pub async fn change_cover(
        &self,
        release_id: &str,
        selection: CoverSelection,
    ) -> Result<(), LibraryError> {
        // The source content type is discarded: every stored cover is resized to
        // JPEG below, so only the bytes, provenance, and URL carry through.
        let (bytes, source, source_url) =
            match selection {
                CoverSelection::ReleaseImage { file_id } => {
                    let file = self.get_file_by_id(&file_id).await?.ok_or_else(|| {
                        LibraryError::Import(format!("File '{file_id}' not found"))
                    })?;

                    // Read the chosen release image file through coven's locality-aware
                    // read (the user's own file when Local, the cache/cloud when Remote).
                    let bytes = self.read_release_file(&file).await?;
                    let source_url = format!("release://{}", file.original_filename);
                    (bytes, "local".to_string(), Some(source_url))
                }
                CoverSelection::RemoteCover { url, source } => {
                    let image = self.remote_images.fetch_required(&url).await.map_err(|e| {
                        LibraryError::Import(format!("Failed to download cover: {e}"))
                    })?;

                    (image.bytes, source.as_str().to_string(), Some(url))
                }
            };

        // Resize to a ≤600px JPEG thumbnail, then build the row from that output:
        // the stored bytes, their format, size and hash all describe the thumbnail,
        // not the source.
        // Decoding and re-encoding the image is CPU work.
        let bytes = tokio::task::spawn_blocking(move || crate::util::cover::resize_cover(&bytes))
            .await
            .map_err(|e| LibraryError::Import(format!("cover resize task failed: {e}")))?
            .map_err(|e| LibraryError::Import(format!("Failed to resize cover: {e}")))?;
        let library_image = DbLibraryImage::cover(
            release_id,
            &source,
            source_url,
            &bytes,
            chrono::DateTime::<chrono::Utc>::from(self.clock.now()),
        );

        self.store_library_image(&library_image, &bytes).await?;

        Ok(())
    }
}
