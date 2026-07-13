//! Coven blob surface for [`LibraryManager`]: blob-ref construction, byte reads
//! for release files and library images, library-image stores, cover-ref
//! lookups, pin state, and the per-namespace cache budgets. Moves bytes /
//! blob-refs / pin state through the [`CovenHandle`] and [`Database`], distinct
//! from the sync-provider control in `sync.rs`.

use super::release::cover_ref_for;
use super::*;

impl LibraryManager {
    /// The coven `BlobRef` for a remote release file's audio blob — its identity
    /// in coven's cache (and the cloud on a miss). `cloud_path` is the row's value
    /// RELATIVE to the `release_files` namespace coven prepends. A release file is
    /// a coven **user-provided** blob (the user's own imported file): Local = the
    /// file at the user's path (an external ref coven holds), Remote = uploaded and
    /// `CacheLazy` (fetched into the cache on first read). coven resolves which by
    /// where the bytes are — the same `BlobRef` addresses every locality.
    pub(crate) fn release_file_blob_ref(file: &DbFile) -> coven::BlobRef {
        coven::BlobRef {
            namespace: crate::sync::RELEASE_FILES_NAMESPACE.to_string(),
            id: file.id.clone(),
            scope: coven::BlobScope::Master,
            cloud_path: file.cloud_path.clone(),
            provenance: coven::Provenance::UserProvided,
            fill: coven::CacheFill::CacheLazy,
        }
    }

    /// Configure coven's per-namespace cache budgets for this device: the bulk for
    /// `release_files` (audio), a small reserved slice each for `covers` and
    /// `artist_images`, so each namespace evicts against its own budget and audio
    /// pressure never wipes the cover cache. Device-local; set once at startup.
    pub(crate) async fn configure_cache_budgets(&self) -> Result<(), LibraryError> {
        self.handle
            .set_cache_budget(
                crate::sync::RELEASE_FILES_NAMESPACE,
                crate::sync::RELEASE_FILES_CACHE_BUDGET,
            )
            .await?;
        self.handle
            .set_cache_budget(
                crate::sync::COVERS_NAMESPACE,
                crate::sync::COVERS_CACHE_BUDGET,
            )
            .await?;
        self.handle
            .set_cache_budget(
                crate::sync::ARTIST_IMAGES_NAMESPACE,
                crate::sync::ARTIST_IMAGES_CACHE_BUDGET,
            )
            .await?;
        Ok(())
    }

    /// Read a release file's whole plaintext through coven's locality-aware read:
    /// served from the user's file (Local user-provided), coven's local store
    /// (Local host-provided), `storage/pinned`/`storage/cache` on a Remote hit, or
    /// fetched from the cloud (into `cache/`) on a Remote miss. For the
    /// non-streaming readers (export, gallery images); playback streams ranges via
    /// `open_blob_stream` instead. A vanished/changed external file maps to a
    /// storage error so the caller surfaces a "files missing / moved" state.
    pub(crate) async fn read_release_blob(&self, file: &DbFile) -> Result<Vec<u8>, LibraryError> {
        let blob = Self::release_file_blob_ref(file);
        self.handle
            .read_blob(&blob)
            .await
            .map_err(|e| LibraryError::Storage(format!("read of {}: {e}", file.id)))
    }

    /// The coven `BlobRef` for a host-provided library image, addressed by the
    /// image row's `blob_id` (not its `id`, which names the release or artist). See
    /// [`crate::sync::image_blob_ref`].
    pub(crate) fn image_blob_ref(
        namespace: &str,
        blob_id: &str,
        cloud_path: Option<String>,
    ) -> coven::BlobRef {
        crate::sync::image_blob_ref(namespace, blob_id, cloud_path)
    }

    /// The cover [`ImageRef`] for one release — its image id paired with the
    /// `covers` row's `_updated_at` — or `None` when the release has no cover row.
    pub(crate) async fn cover_ref(
        &self,
        release_id: &str,
    ) -> Result<Option<ImageRef>, LibraryError> {
        cover_ref_for(&self.database, release_id).await
    }

    /// The cover [`ImageRef`] for each of `release_ids` that has a `covers` row,
    /// in one query. The batch source for the list/grid resolvers, which build a
    /// `Fn(&str) -> Option<ImageRef>` over the returned map.
    pub(super) async fn cover_refs(
        &self,
        release_ids: &[String],
    ) -> Result<HashMap<String, ImageRef>, LibraryError> {
        Ok(image_ref_map(
            self.database.cover_versions(release_ids).await?,
            LibraryImageType::Cover,
        ))
    }

    pub(super) async fn artist_image_refs(
        &self,
        artist_ids: &[String],
    ) -> Result<HashMap<String, ImageRef>, LibraryError> {
        Ok(image_ref_map(
            self.database.artist_image_versions(artist_ids).await?,
            LibraryImageType::Artist,
        ))
    }

    /// Read a host-provided library image's whole bytes through coven's
    /// locality-aware read. The [`ImageRef`] carries the image kind, so the read
    /// goes directly to the table/namespace that produced the ref.
    pub async fn read_image_blob(&self, image: &ImageRef) -> Result<Option<Vec<u8>>, LibraryError> {
        self.read_library_image_blob(&image.id, &image.image_type)
            .await
    }

    pub async fn read_cover_image_blob(
        &self,
        release_id: &str,
    ) -> Result<Option<Vec<u8>>, LibraryError> {
        self.read_library_image_blob(release_id, &LibraryImageType::Cover)
            .await
    }

    async fn read_library_image_blob(
        &self,
        id: &str,
        image_type: &LibraryImageType,
    ) -> Result<Option<Vec<u8>>, LibraryError> {
        let namespace = image_type.namespace();
        let Some(row) = self.database.find_library_image(id, image_type).await? else {
            return Ok(None);
        };
        let blob = Self::image_blob_ref(namespace, &row.blob_id, row.cloud_path.clone());
        let bytes = self
            .handle
            .read_blob(&blob)
            .await
            .map_err(|e| LibraryError::Storage(format!("read image {id}: {e}")))?;
        Ok(Some(bytes))
    }

    /// Whether coven holds this release pinned on this device — true iff its
    /// representative blob (any one of the release's files; pin/unpin act on all a
    /// release's blobs together) is kept in coven's `storage/pinned/`. `None` (a
    /// release with no files) reads as not pinned. Pinned-ness is coven cache
    /// state, never a bae column — answered through the handle, not by stat-ing
    /// coven's cache layout.
    pub(crate) async fn release_pinned(
        &self,
        any_file_id: Option<&str>,
    ) -> Result<bool, LibraryError> {
        let Some(file_id) = any_file_id else {
            return Ok(false);
        };
        match release_file_pin_state(&self.handle, file_id).await? {
            ReleasePinState::Pinned => Ok(true),
            ReleasePinState::NotPinned => Ok(false),
            ReleasePinState::RejectedBadId => {
                self.diagnostics().event(TelemetryEvent::Anomaly {
                    kind: crate::diagnostics::AnomalyKind::BlobIdInvalid,
                });
                Ok(false)
            }
        }
    }

    /// Pin a remote release's blobs for offline: coven fetches every blob into
    /// `storage/pinned/` (from the evictable cache if already there, else the
    /// cloud). Idempotent. Pinned-ness is coven cache state — there is no bae flag.
    /// The low-level cache op behind the "Pin" transition.
    ///
    /// Pinned one blob at a time so the Downloads pane advances as each file lands:
    /// coven's `pin` is a per-blob loop with no cross-blob state, so a file at a
    /// time is the same work in the same order as handing it the whole set — and it
    /// is the only granularity coven reports at, since a single `pin` call over the
    /// set returns nothing until every file is in.
    pub(crate) async fn pin_release_blobs_with_progress(
        &self,
        release_id: &str,
        mut on_progress: impl FnMut(crate::library::DownloadTransferProgress),
    ) -> Result<(), LibraryError> {
        let files = self.database.get_files_for_release(release_id).await?;
        let sizes = download_file_sizes(release_id, &files)?;
        let bytes_total = total_bytes(release_id, &sizes)?;
        let mut emit_progress = |bytes_done| {
            on_progress(crate::library::DownloadTransferProgress::new(
                release_id,
                bytes_done,
                bytes_total,
            )?);
            Ok::<(), LibraryError>(())
        };
        emit_progress(0)?;

        let mut bytes_done = 0u64;
        for (file, size) in files.iter().zip(&sizes) {
            let blob = Self::release_file_blob_ref(file);
            self.handle
                .pin(std::slice::from_ref(&blob))
                .await
                .map_err(|e| LibraryError::Storage(format!("pin release {release_id}: {e}")))?;
            // Never exceeds `bytes_total`: it is the sum of these same sizes.
            bytes_done += size;
            emit_progress(bytes_done)?;
        }

        Ok(())
    }

    pub(crate) async fn initial_download_progress(
        &self,
        release_id: &str,
    ) -> Result<crate::library::DownloadTransferProgress, LibraryError> {
        let files = self.database.get_files_for_release(release_id).await?;
        crate::library::DownloadTransferProgress::new(
            release_id,
            0,
            download_total_bytes(release_id, &files)?,
        )
    }

    /// Unpin a remote release's blobs: coven moves every blob from
    /// `storage/pinned/` to the evictable `storage/cache/` (still readable, now
    /// droppable). No cloud read, no bae flag. The low-level cache op behind the
    /// "Unpin" transition.
    pub(crate) async fn unpin_release_blobs(&self, release_id: &str) -> Result<(), LibraryError> {
        let files = self.database.get_files_for_release(release_id).await?;
        let blobs: Vec<_> = files.iter().map(Self::release_file_blob_ref).collect();
        self.handle
            .unpin(&blobs)
            .await
            .map_err(|e| LibraryError::Storage(format!("unpin release {release_id}: {e}")))
    }

    /// Store a bae-produced host-provided image and its row in one coven batch.
    pub async fn store_library_image_blob(
        &self,
        image: &DbLibraryImage,
        bytes: &[u8],
    ) -> Result<(), LibraryError> {
        self.database.write_library_image_blob(image, bytes).await?;
        Ok(())
    }
}

fn image_ref_map(
    versions: HashMap<String, String>,
    image_type: LibraryImageType,
) -> HashMap<String, ImageRef> {
    versions
        .into_iter()
        .map(|(id, version)| {
            (
                id.clone(),
                ImageRef {
                    id,
                    version,
                    image_type: image_type.clone(),
                },
            )
        })
        .collect()
}

fn download_file_sizes(release_id: &str, files: &[DbFile]) -> Result<Vec<u64>, LibraryError> {
    files
        .iter()
        .map(|file| {
            u64::try_from(file.file_size).map_err(|_| {
                LibraryError::Storage(format!(
                    "pin release {release_id}: file {} has negative size",
                    file.id
                ))
            })
        })
        .collect()
}

fn download_total_bytes(release_id: &str, files: &[DbFile]) -> Result<u64, LibraryError> {
    total_bytes(release_id, &download_file_sizes(release_id, files)?)
}

fn total_bytes(release_id: &str, sizes: &[u64]) -> Result<u64, LibraryError> {
    sizes.iter().try_fold(0u64, |total, file_size| {
        total.checked_add(*file_size).ok_or_else(|| {
            LibraryError::Storage(format!("pin release {release_id}: byte total overflow"))
        })
    })
}
