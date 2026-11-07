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
        Ok(self.records.read_release_file(file_id).await?)
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

    /// Wait for every file of a release kept here as it moved to the cloud
    /// to be kept whole on this device. The write switching its files to
    /// the cloud asks coven to keep each, and coven fetches its copy from
    /// storage after that.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn wait_for_kept_release_for_test(&self, release_id: &str) {
        let mut kept = self.records.subscribe_release_files_kept(release_id);
        tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
            loop {
                let kept = kept.next().await.expect("the query stays open").unwrap();
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
        Ok(self.records.read_release_file(&file.id).await?)
    }

    /// The cover [`ImageRef`] for one release — its image id paired with the
    /// cover's version — or `None` when the release has no cover row.
    pub async fn cover_ref(&self, release_id: &str) -> Result<Option<ImageRef>, LibraryError> {
        cover_ref_for(&self.records, release_id).await
    }

    /// Read a host-provided library image's whole bytes through coven's
    /// locality-aware read. The [`ImageRef`] carries the image kind, so the read
    /// goes directly to the table/namespace that produced the ref.
    pub async fn read_image_bytes(
        &self,
        image: &ImageRef,
    ) -> Result<Option<Vec<u8>>, LibraryError> {
        self.read_library_image_bytes(&image.id, &image.image_type).await
    }

    pub async fn read_cover_image_bytes(
        &self,
        release_id: &str,
    ) -> Result<Option<Vec<u8>>, LibraryError> {
        self.read_library_image_bytes(release_id, &LibraryImageType::Cover).await
    }

    /// Read a library image's whole bytes by its subject id and kind; `None`
    /// when this device has no file for it.
    pub async fn read_library_image_bytes(
        &self,
        id: &str,
        image_type: &LibraryImageType,
    ) -> Result<Option<Vec<u8>>, LibraryError> {
        let Some(file) = self.library_image_file(id, image_type).await? else {
            return Ok(None);
        };
        let bytes = self
            .store
            .read_file(&file)
            .await
            .map_err(|e| LibraryError::file_read(format!("read image {id}"), e))?;
        Ok(Some(bytes))
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
                bytes: bae_store::query::to_u64(file.file_size, "release_files.file_size")?,
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

    /// Store a bae-produced host-provided image and its row in one coven batch.
    pub async fn store_library_image(
        &self,
        image: &DbLibraryImage,
        bytes: &[u8],
    ) -> Result<(), LibraryError> {
        self.records.write_library_image(image, bytes).await?;
        Ok(())
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
