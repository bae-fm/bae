//! Coven file surface for [`LibraryManager`]: file reference construction, byte reads
//! for release files and library images, library-image stores, cover-ref
//! lookups, pins, and the per-namespace cache budgets. Moves bytes /
//! file references / pins through the [`CovenHandle`] and [`Database`], distinct
//! from the sync-provider control in `sync.rs`.

use super::release::cover_ref_for;
use super::*;

impl LibraryManager {
    /// Each track of a release in play order: its title, and the MusicBrainz
    /// ids of the artists credited on it in a role and of the works it
    /// performs — what a test reads back from an import.
    #[cfg(test)]
    pub(crate) async fn track_sources_for_test(
        &self,
        release_id: &str,
    ) -> Result<Vec<(String, Vec<String>, Vec<String>)>, LibraryError> {
        Ok(self.database.track_sources_for_test(release_id).await?)
    }

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
        Ok(self.database.read_release_file(file_id).await?)
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

    #[cfg(any(test, feature = "test-utils"))]
    pub async fn evict_file_for_test(&self, file: &coven::FileRef) -> Result<(), LibraryError> {
        self.database
            .evict_file(file)
            .await
            .map_err(|error| LibraryError::file_read("evict", error))
    }

    /// Apply [`crate::sync::CACHE_BUDGETS`] to coven for this device.
    /// Device-local; set once at startup.
    pub(crate) async fn configure_cache_budgets(&self) -> Result<(), LibraryError> {
        for (namespace, budget) in crate::sync::CACHE_BUDGETS {
            self.database.set_cache_budget(namespace, budget).await?;
        }
        Ok(())
    }

    /// The exact-row file reference coven binds for a release file's audio file,
    /// captured from the live `release_files` row. coven's cache ops (read, pin,
    /// stream) key on this so a later row replacement cannot redirect a read; a
    /// missing row surfaces as an error.
    pub(crate) async fn release_file_ref(
        &self,
        file_id: &str,
    ) -> Result<coven::FileRef, LibraryError> {
        self.database
            .file_ref(crate::sync::RELEASE_FILES_NAMESPACE, file_id)
            .await
            .map_err(|e| {
                LibraryError::Storage(format!("file reference for release file {file_id}: {e}"))
            })
    }

    /// How many of these release files carry a file on this device in coven:
    /// this device's pending attachment, or the stored file the row names.
    pub(crate) async fn attached_release_file_count(
        &self,
        files: &[DbFile],
    ) -> Result<usize, LibraryError> {
        let mut attached = 0;
        for file in files {
            if self
                .database
                .current_file(crate::sync::RELEASE_FILES_NAMESPACE, &file.id)
                .await?
                .is_some()
            {
                attached += 1;
            }
        }
        Ok(attached)
    }

    /// Read through the same local-or-attached source used for playback.
    pub(crate) async fn read_release_file(&self, file: &DbFile) -> Result<Vec<u8>, LibraryError> {
        Ok(self.database.read_release_file(&file.id).await?)
    }

    /// The cover [`ImageRef`] for one release — its image id paired with the
    /// cover's version — or `None` when the release has no cover row.
    pub async fn cover_ref(&self, release_id: &str) -> Result<Option<ImageRef>, LibraryError> {
        cover_ref_for(&self.database, release_id).await
    }

    /// The cover [`ImageRef`] for each of `release_ids` that has a `covers` row,
    /// in one query. The batch source for the list/grid resolvers, which build a
    /// `Fn(&str) -> Option<ImageRef>` over the returned map.
    pub(super) async fn cover_refs(
        &self,
        release_ids: &[String],
    ) -> Result<HashMap<String, ImageRef>, LibraryError> {
        Ok(image_refs(
            self.database.cover_versions(release_ids).await?,
            LibraryImageType::Cover,
        ))
    }

    pub(super) async fn artist_image_refs(
        &self,
        artist_ids: &[String],
    ) -> Result<HashMap<String, ImageRef>, LibraryError> {
        Ok(image_refs(
            self.database.artist_image_versions(artist_ids).await?,
            LibraryImageType::Artist,
        ))
    }

    /// Read a host-provided library image's whole bytes through coven's
    /// locality-aware read. The [`ImageRef`] carries the image kind, so the read
    /// goes directly to the table/namespace that produced the ref.
    pub async fn read_image_bytes(
        &self,
        image: &ImageRef,
    ) -> Result<Option<Vec<u8>>, LibraryError> {
        self.read_library_image_bytes(&image.id, &image.image_type)
            .await
    }

    pub async fn read_cover_image_bytes(
        &self,
        release_id: &str,
    ) -> Result<Option<Vec<u8>>, LibraryError> {
        self.read_library_image_bytes(release_id, &LibraryImageType::Cover)
            .await
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
            .database
            .read_file(&file)
            .await
            .map_err(|e| LibraryError::file_read(format!("read image {id}"), e))?;
        Ok(Some(bytes))
    }

    /// A library image's file as this device sees it: its own pending
    /// attachment, or the stored file the row names. `None` when there is no
    /// such row, or another device hasn't finished uploading its file.
    async fn library_image_file(
        &self,
        id: &str,
        image_type: &LibraryImageType,
    ) -> Result<Option<coven::FileRef>, LibraryError> {
        Ok(self
            .database
            .current_file(image_type.namespace(), id)
            .await?)
    }

    /// Audio and companion files stay pinned together. Covers use their own
    /// cache budget and are never pinned.
    pub(super) async fn release_pinnable_files(
        &self,
        release_id: &str,
    ) -> Result<Vec<PinnableFile>, LibraryError> {
        let files = self.database.get_files_for_release(release_id).await?;
        let mut pinnable = Vec::with_capacity(files.len());
        for file in &files {
            pinnable.push(PinnableFile {
                file: self.release_file_ref(&file.id).await?,
                bytes: pinnable_bytes(release_id, &file.id, file.file_size)?,
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
    ) -> Result<crate::library::DownloadTransferProgress, LibraryError> {
        let pinnable = self.release_pinnable_files(release_id).await?;
        crate::library::DownloadTransferProgress::new(
            release_id,
            0,
            total_bytes(release_id, &pinnable)?,
        )
    }

    /// Unpin a remote release's files: coven moves every file from
    /// the pinned cache to the evictable cache (still readable, now
    /// droppable). No cloud read, no bae flag. The low-level cache op behind the
    /// "Unpin" transition.
    pub(crate) async fn unpin_release_files(&self, release_id: &str) -> Result<(), LibraryError> {
        let files: Vec<coven::FileRef> = self
            .release_pinnable_files(release_id)
            .await?
            .into_iter()
            .map(|entry| entry.file)
            .collect();
        self.database
            .unpin(&files)
            .await
            .map_err(|e| LibraryError::file_read(format!("unpin release {release_id}"), e))
    }

    /// Store a bae-produced host-provided image and its row in one coven batch.
    pub async fn store_library_image(
        &self,
        image: &DbLibraryImage,
        bytes: &[u8],
    ) -> Result<(), LibraryError> {
        self.database.write_library_image(image, bytes).await?;
        Ok(())
    }
}

/// One file of a release's pinned set, with the plaintext byte size its row
/// records — what a pin fetches, and what the Downloads pane counts.
pub(super) struct PinnableFile {
    pub(super) file: coven::FileRef,
    pub(super) bytes: u64,
}

fn pinnable_bytes(release_id: &str, row_id: &str, file_size: i64) -> Result<u64, LibraryError> {
    u64::try_from(file_size).map_err(|error| {
        LibraryError::Storage(format!(
            "pin release {release_id}: row {row_id} has negative size: {error}"
        ))
    })
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
