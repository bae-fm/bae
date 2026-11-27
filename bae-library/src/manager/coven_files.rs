//! Coven file surface for [`LibraryManager`]: file reference construction, byte reads
//! for release files and library images, library-image stores, cover-ref
//! lookups, pins, and the per-namespace cache budgets. Moves bytes /
//! file references / pins through the [`CovenHandle`] and [`Database`], distinct
//! from the sync-provider control in `sync.rs`.

use super::release::cover_ref_for;
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
        self.library_image_file(id, image_type).await
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
    pub async fn cover_ref(&self, release_id: &str) -> Result<Option<ImageRef>, LibraryError> {
        cover_ref_for(&self.records, release_id).await
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
                self.read_library_image_bytes(&image.id, &image.image_type).await
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
        let Some(file) = self.records.cover_copy_file(release_id, copy).await? else {
            return Ok(None);
        };
        self.read_image_file(&format!("{copy:?} copy of {release_id}'s cover"), &file)
            .await
            .map(Some)
    }

    /// Read a library image's bytes by its subject id and kind; for a cover,
    /// its large copy. `None` when this device has no file for it.
    pub async fn read_library_image_bytes(
        &self,
        id: &str,
        image_type: &LibraryImageType,
    ) -> Result<Option<Vec<u8>>, LibraryError> {
        match image_type {
            LibraryImageType::Cover => self.read_cover_copy(id, CoverCopy::Large).await,
            LibraryImageType::Artist => {
                let Some(file) = self.library_image_file(id, image_type).await? else {
                    return Ok(None);
                };
                self.read_image_file(&format!("image {id}"), &file).await.map(Some)
            }
        }
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

    /// The coven file a library image's row references, as this device
    /// reads it now. `None` when there is no such row.
    async fn library_image_file(
        &self,
        id: &str,
        image_type: &LibraryImageType,
    ) -> Result<Option<coven::FileRef>, LibraryError> {
        Ok(self.records.library_image_file(id, image_type).await?)
    }

    /// Audio and companion files stay pinned together. Covers use their own
    /// cache budget and are never pinned.
    pub(super) async fn release_pinnable_files(
        &self,
        release_id: &str,
    ) -> Result<Vec<PinnableFile>, LibraryError> {
        let files = self.records.get_files_for_release(release_id).await?;
        let mut pinnable = Vec::with_capacity(files.len());
        for file in &files {
            pinnable.push(PinnableFile {
                row_id: file.id.clone(),
                file: self.release_file_ref(&file.id).await?,
                bytes: file.file_size,
            });
        }
        Ok(pinnable)
    }

    /// The Downloads pane's opening progress for a queued pin: zero of the same
    /// total [`Self::pin_release_files_with_progress`] counts down, so the bar
    /// never jumps when the pin starts reporting.
    pub(crate) async fn initial_download_progress(
        &self,
        release_id: &str,
    ) -> Result<bae_transfers::DownloadTransferProgress, LibraryError> {
        let pinnable = self.release_pinnable_files(release_id).await?;
        Ok(bae_transfers::DownloadTransferProgress::new(
            release_id,
            0,
            total_bytes(release_id, &pinnable)?,
        )?)
    }
}

/// One file of a release's pinned set, with the plaintext byte size its row
/// records — what a pin fetches, and what the Downloads pane counts.
pub(super) struct PinnableFile {
    /// The release file's row id.
    pub(super) row_id: String,
    pub(super) file: coven::FileRef,
    pub(super) bytes: u64,
}

pub(super) fn total_bytes(
    release_id: &str,
    pinnable: &[PinnableFile],
) -> Result<u64, LibraryError> {
    pinnable.iter().try_fold(0u64, |total, entry| {
        total.checked_add(entry.bytes).ok_or_else(|| {
            LibraryError::Storage(format!("pin release {release_id}: byte total overflow"))
        })
    })
}

#[cfg(test)]
#[path = "coven_files_tests.rs"]
mod tests;
