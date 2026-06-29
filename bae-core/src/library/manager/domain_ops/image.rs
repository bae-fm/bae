//! Image domain operations for [`LibraryManager`].

use super::*;

impl LibraryManager {
    /// Bytes of one gallery slot, dispatching the read on its [`GallerySource`]
    /// so no caller picks the byte source itself: a `Cover` is read by image id
    /// (`read_image_blob`), a `ReleaseFile` by file id (`load_gallery_image`).
    /// The gallery carries a cover slot only when a cover exists, so a `Cover`
    /// with no bytes here is exceptional and surfaces rather than being masked.
    pub async fn read_gallery_bytes(
        &self,
        release_id: &str,
        source: &GallerySource,
    ) -> Result<Vec<u8>, LibraryError> {
        match source {
            GallerySource::Cover(image) => {
                self.read_image_blob(&image.id).await?.ok_or_else(|| {
                    LibraryError::Storage(format!("gallery cover image {} has no bytes", image.id))
                })
            }
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
        crate::storage::local::transfer::read_release_file_bytes(&file, self)
            .await
            .map_err(|e| LibraryError::Import(e.to_string()))
    }

    /// Upsert a library image record
    pub async fn upsert_library_image(&self, image: &DbLibraryImage) -> Result<(), LibraryError> {
        self.database.upsert_library_image(image).await?;
        Ok(())
    }

    /// The readable `cloud_path` for an artist image under the current home:
    /// `None` (hashed-by-id) on an opaque home, `Some({artist}/artist.{ext})`
    /// on a browsable one. The manager owns config, so it reads the storage mode.
    pub fn artist_image_cloud_path(
        &self,
        artist_id: &str,
        content_type: &crate::util::content_type::ContentType,
    ) -> Option<String> {
        let storage = self.config_handle.config().cloud_home.storage;
        self.database
            .artist_image_cloud_path_for_storage(storage, artist_id, content_type)
    }

    /// Get a library image by ID and type
    pub async fn get_library_image(
        &self,
        id: &str,
        image_type: &LibraryImageType,
    ) -> Result<Option<DbLibraryImage>, LibraryError> {
        Ok(self.database.find_library_image(id, image_type).await?)
    }

    /// Delete a library image by ID and type
    pub async fn delete_library_image(
        &self,
        id: &str,
        image_type: &LibraryImageType,
    ) -> Result<(), LibraryError> {
        self.database.delete_library_image(id, image_type).await?;
        Ok(())
    }

    /// Change the cover art for an album's release.
    ///
    /// `ReleaseImage`: reads an image file already in the library (by file ID),
    /// copies it to the images dir, and records it as the cover.
    /// `RemoteCover`: downloads cover art from a URL, writes it, records it.
    pub async fn change_cover(
        &self,
        album_id: &str,
        release_id: &str,
        selection: CoverSelection,
    ) -> Result<(), LibraryError> {
        let (bytes, content_type, source, source_url) = match selection {
            CoverSelection::ReleaseImage { file_id } => {
                let file = self
                    .get_file_by_id(&file_id)
                    .await?
                    .ok_or_else(|| LibraryError::Import(format!("File '{file_id}' not found")))?;

                // Read the chosen release image file through coven's locality-aware
                // read (the user's own file when Local, the cache/cloud when Remote).
                let bytes = self.read_release_blob(&file).await?;
                let source_url = format!("release://{}", file.original_filename);
                (
                    bytes,
                    file.content_type.clone(),
                    "local".to_string(),
                    Some(source_url),
                )
            }
            CoverSelection::RemoteCover { url, source } => {
                let (bytes, content_type) =
                    crate::import::cover_art::download_cover_art_bytes(&url)
                        .await
                        .map_err(|e| {
                            LibraryError::Import(format!("Failed to download cover: {e}"))
                        })?;

                (bytes, content_type, source.as_str().to_string(), Some(url))
            }
        };

        // Record the cover blob and row in one coven write. The cover's `id` IS
        // the release id. Under a
        // browsable home the cover blob lands at a readable
        // `{artist}/{album}/cover.{ext}` key, computed + stored here; an opaque
        // home leaves `cloud_path` NULL (hashed-by-id).
        let now = self.clock.now();
        let storage = self.config_handle.config().cloud_home.storage;
        let cloud_path = self
            .database
            .cover_cloud_path_for_storage(storage, release_id, &content_type)
            .await?;
        let library_image = DbLibraryImage {
            id: release_id.to_string(),
            image_type: LibraryImageType::Cover,
            content_type,
            file_size: bytes.len() as i64,
            width: None,
            height: None,
            source,
            source_url,
            cloud_path,
            created_at: now,
        };
        self.store_library_image_blob(&library_image, &bytes)
            .await?;

        // Don't touch primary_release_id here — "change cover" updates
        // the image on this release; "set primary release" is a separate
        // user action. Let the event emit so UIs refresh.
        self.emit_album_updated(album_id).await;

        Ok(())
    }
}
