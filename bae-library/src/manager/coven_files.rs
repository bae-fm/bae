//! Coven file surface for [`LibraryManager`]: file reference construction, byte reads
//! for release files and library images, library-image stores, cover-ref
//! lookups, pins, and the per-namespace cache budgets. Moves bytes /
//! file references / pins through the [`CovenHandle`] and [`Database`], distinct
//! from the sync-provider control in `sync.rs`.

use super::*;

impl LibraryManager {
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn release_file_ref_for_test(
        &self,
        file_id: &str,
    ) -> Result<coven::FileRef, LibraryError> {
        self.release_file_ref(file_id).await
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub async fn materialize_release_file_for_test(
        &self,
        file_id: &str,
    ) -> Result<Vec<u8>, LibraryError> {
        let file = self.release_file_ref(file_id).await?;
        let lease = bae_store::lease_one(&self.files).await;
        Ok(self.records.read_release_file(file.id(), &lease).await?)
    }

    /// A library image's file as this device sees it, for tests that check
    /// what it holds.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn library_image_file_for_test(
        &self,
        id: &str,
        image_type: &LibraryImageType,
    ) -> Result<Option<coven::FileRef>, LibraryError> {
        Ok(self.records.library_image_file(id, image_type).await?)
    }

    /// Write `cover` as the cover of its release, for tests that need one
    /// without the import that makes it.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn write_test_cover(&self, cover: bae_records::NewCover) -> Result<(), LibraryError> {
        self.records.write_cover(cover).await?;
        Ok(())
    }

    /// Write `bytes` as the image of an artist, for tests that need one
    /// without the import that makes it.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn write_test_artist_image(
        &self,
        image: &bae_records::DbArtistImage,
        bytes: &[u8],
    ) -> Result<(), LibraryError> {
        self.records.write_artist_image(image, bytes).await?;
        Ok(())
    }

    /// Wait for every file of a release being pinned to be kept whole on
    /// this device.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn wait_for_kept_release_for_test(&self, release_id: &str) {
        let mut kept = self.records.subscribe_release_files_kept(release_id);
        tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
            loop {
                let kept = kept.next_inline().await.expect("the query stays open").unwrap();
                if kept.values().all(|kept| *kept) {
                    break;
                }
            }
        })
        .await
        .expect("every kept file is whole here");
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub async fn evict_file_for_test(&self, file: coven::FileId) -> Result<(), LibraryError> {
        Ok(self.store.evict_file(file).await?)
    }

    /// coven's record of the file a release file's row references, as this
    /// device reads it now: what coven's reads, pins and streams take. A
    /// row that is gone is an error.
    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) async fn release_file_ref(
        &self,
        file_id: &str,
    ) -> Result<coven::FileRef, LibraryError> {
        let file = self
            .records
            .find_file_by_id(file_id)
            .await?
            .map(|row| row.file)
            .ok_or_else(|| LibraryError::Storage(format!("no release file {file_id}")))?;
        self.store.file_ref(file).await.map_err(|e| {
            LibraryError::Storage(format!("file reference for release file {file_id}: {e}"))
        })
    }

    /// Read through the same local-or-attached source used for playback.
    pub(crate) async fn read_release_file(&self, file: &DbFile) -> Result<Vec<u8>, LibraryError> {
        let lease = bae_store::lease_one(&self.files).await;
        Ok(self.records.read_release_file(file.file, &lease).await?)
    }

    /// The cover [`ImageRef`] for one release — its image id paired with the
    /// cover's version — or `None` when the release has no cover row.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn cover_ref(&self, release_id: &str) -> Result<Option<ImageRef>, LibraryError> {
        Ok(bae_records::cover_ref(release_id, self.records.cover_version(release_id).await?))
    }

    /// Read a library image's bytes through coven's locality-aware read. The
    /// [`ImageRef`] carries the image kind, so the read goes directly to the
    /// table/namespace that produced the ref. A cover has two copies: `pixels`,
    /// the longer edge of the slot that draws it, picks the browse copy or the
    /// large one, and a read with no size takes the large copy. An artist's
    /// image has one copy.
    pub async fn read_image_bytes(
        &self,
        image: &ImageRef,
        pixels: Option<u32>,
    ) -> Result<Option<Vec<u8>>, LibraryError> {
        match image.image_type {
            LibraryImageType::Cover => {
                let copy = pixels.map_or(CoverCopy::Large, CoverCopy::covering);
                self.read_cover_copy(&image.id, copy).await
            }
            LibraryImageType::Artist => {
                let artist = self.read_library_image(&image.id, &image.image_type).await?;
                Ok(artist.map(|image| image.bytes))
            }
        }
    }

    /// The large copy of the release's cover, the most bae keeps of it
    /// besides its original; `None` when the release has no cover.
    pub async fn read_cover_image_bytes(
        &self,
        release_id: &str,
    ) -> Result<Option<Vec<u8>>, LibraryError> {
        self.read_cover_copy(release_id, CoverCopy::Large).await
    }

    /// One copy of the release's cover; `None` when the release has no cover.
    pub async fn read_cover_copy(
        &self,
        release_id: &str,
        copy: CoverCopy,
    ) -> Result<Option<Vec<u8>>, LibraryError> {
        let Some(picture) = self.records.cover_copy(release_id, copy).await? else {
            return Ok(None);
        };
        self.read_image_file(&format!("{copy:?} copy of {release_id}'s cover"), &picture.file)
            .await
            .map(Some)
    }

    /// A library image by its subject id and kind, its format with its bytes,
    /// both from one read of its row; for a cover, its large copy. `None` when
    /// the library holds no such image.
    pub async fn read_library_image(
        &self,
        id: &str,
        image_type: &LibraryImageType,
    ) -> Result<Option<LibraryImage>, LibraryError> {
        let (picture, what) = match image_type {
            LibraryImageType::Cover => (
                self.records.cover_copy(id, CoverCopy::Large).await?,
                format!("large copy of {id}'s cover"),
            ),
            LibraryImageType::Artist => {
                (self.records.artist_picture(id).await?, format!("image {id}"))
            }
        };
        let Some(picture) = picture else {
            return Ok(None);
        };
        let bytes = self.read_image_file(&what, &picture.file).await?;
        Ok(Some(LibraryImage { content_type: picture.content_type, bytes }))
    }

    pub(super) async fn read_image_file(
        &self,
        what: &str,
        file: &coven::FileRef,
    ) -> Result<Vec<u8>, LibraryError> {
        let lease = bae_store::lease_one(&self.files).await;
        self.store
            .read_file(file, &lease)
            .await
            .map_err(|e| LibraryError::file_read(format!("read {what}"), e))
    }

    /// The files a pin of the release `release_id` keeps; `None` when the
    /// library holds no such release.
    pub(crate) async fn find_files_to_pin(
        &self,
        release_id: &str,
    ) -> Result<Option<Vec<DbFile>>, LibraryError> {
        let release = self.records.find_release_with_files(release_id).await?;
        Ok(release.map(|release| release.files))
    }

    /// coven's record of each of a release's `files`, which a pin keeps
    /// together: audio and companion files alike. Covers use their own cache
    /// budget and are never pinned.
    pub(super) async fn release_pinnable_files(
        &self,
        files: &[DbFile],
    ) -> Result<Vec<PinnableFile>, LibraryError> {
        let mut pinnable = Vec::with_capacity(files.len());
        for file in files {
            let reference = self.store.file_ref(file.file).await.map_err(|e| {
                LibraryError::Storage(format!("file reference for release file {}: {e}", file.id))
            })?;
            pinnable.push(PinnableFile {
                row_id: file.id.clone(),
                file: reference,
                bytes: file.file_size,
            });
        }
        Ok(pinnable)
    }
}

/// A library image as a client outside the app takes it.
pub struct LibraryImage {
    /// The format the bytes are in.
    pub content_type: bae_util::content_type::ContentType,
    pub bytes: Vec<u8>,
}

/// One file of a release's pinned set, with the plaintext byte size its row
/// records — what a pin fetches, and what the Downloads pane counts.
pub(super) struct PinnableFile {
    /// The release file's row id.
    pub(super) row_id: String,
    pub(super) file: coven::FileRef,
    pub(super) bytes: u64,
}

/// The bytes of a release's `files`, as their rows record their sizes: what a
/// pin or a move to this device counts down.
pub(crate) fn total_bytes(release_id: &str, files: &[DbFile]) -> Result<u64, LibraryError> {
    files.iter().try_fold(0u64, |total, file| {
        total.checked_add(file.file_size).ok_or_else(|| {
            LibraryError::Storage(format!("release {release_id}: byte total overflow"))
        })
    })
}

// Each release's cover's browse copy, and the hash and place coven records
// for it.
#[cfg(test)]
coven::read!(
    writer CoverBrowseCopies,
    r#"
    SELECT cover.release_id, cover.browse_file, stored.hash, stored.coordinates
    FROM @bae_records::records::files::CoverFiles AS cover
    LEFT JOIN coven_files AS stored ON stored.id = cover.browse_file
"#
);

// Each release's cover's browse copy.
#[cfg(test)]
coven::read!(
    writer CoverBrowseFiles,
    r#"
    SELECT cover.release_id, cover.browse_file
    FROM @bae_records::records::files::CoverFiles AS cover
"#
);

#[cfg(test)]
#[path = "coven_files_tests.rs"]
mod tests;
