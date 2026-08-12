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
    ) -> Result<Vec<u8>, LibraryError> {
        match source {
            GallerySource::Cover(image) => self.read_image_blob(image).await?.ok_or_else(|| {
                LibraryError::Storage(format!("gallery cover image {} has no bytes", image.id))
            }),
            GallerySource::ReleaseFile { file_id } => {
                self.load_gallery_image(release_id, file_id).await
            }
        }
    }

    /// Bytes of one of a release's image files, read from the local copy when it
    /// exists here and otherwise downloaded from the release's cloud home (and
    /// decrypted). The `ReleaseFile` arm of [`read_gallery_bytes`](Self::read_gallery_bytes).
    pub async fn load_gallery_image(
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
        crate::storage::transfer::read_release_file_bytes(&file, self)
            .await
            .map_err(|e| LibraryError::Import(e.to_string()))
    }

    /// Test-only: seed a bare image row. Production writes cover/artist images
    /// through `change_cover` / `store_library_image_blob`, which pair the row with
    /// its blob.
    #[cfg(test)]
    pub async fn upsert_library_image(&self, image: &DbLibraryImage) -> Result<(), LibraryError> {
        self.database.upsert_library_image(image).await?;
        Ok(())
    }

    /// The readable `cloud_path` for an artist image under the current home: `None`
    /// (hashed-by-id) on an opaque home, `Some({artist_id}/artist-{blob_id}.{ext})`
    /// on a browsable one. The manager owns config, so it reads the storage mode.
    pub fn artist_image_cloud_path(
        &self,
        artist_id: &str,
        blob_id: &str,
        content_type: &crate::util::content_type::ContentType,
    ) -> Option<String> {
        let storage = self.config_handle.config().cloud_home.storage;
        self.database
            .artist_image_cloud_path_for_storage(storage, artist_id, blob_id, content_type)
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
    /// release's cover blob.
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
                    let bytes = self.read_release_blob(&file).await?;
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
        let bytes = crate::util::cover::resize_cover(&bytes)
            .map_err(|e| LibraryError::Import(format!("Failed to resize cover: {e}")))?;
        let mut library_image = DbLibraryImage::cover(
            release_id,
            &self.ids.new_id(),
            &source,
            source_url,
            &bytes,
            self.clock.now(),
        );

        // Record the cover blob and row in one coven write; the cover's `id` IS the
        // release id, and its `blob_id` is fresh: a coven blob id names one
        // immutable byte-string, so a changed cover is a NEW blob — the write
        // repoints the row at it and deletes the one it replaced. On a browsable
        // home the blob lands at a readable `{album_id}/{release_id}/cover.{ext}`
        // key, computed and stored here; an opaque home leaves `cloud_path` NULL
        // (hashed-by-id).
        let storage = self.config_handle.config().cloud_home.storage;
        library_image.cloud_path = self
            .database
            .cover_cloud_path_for_storage(
                storage,
                release_id,
                &library_image.blob_id,
                &library_image.content_type,
            )
            .await?;
        self.store_library_image_blob(&library_image, &bytes)
            .await?;

        Ok(())
    }
}
