//! Coven blob surface for [`LibraryManager`]: blob-ref construction, byte reads
//! for release files and library images, library-image stores, cover-ref
//! lookups, pin state, and the per-namespace cache budgets. Moves bytes /
//! blob-refs / pin state through the [`CovenHandle`] and [`Database`], distinct
//! from the sync-provider control in `sync.rs`.

use super::resolve::*;
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

    /// The coven `BlobRef` for a host-provided library image (a cover or an artist
    /// image) — its identity in coven's local store while Local and its cache while
    /// Remote. `namespace` is `covers` or `artist_images`; `id` is the release id
    /// (a cover) or artist id (an artist image). A host-provided `CacheEager` blob:
    /// the bytes are produced by bae and kept by coven, fetched into the cache on
    /// pull so a grid renders from local bytes. `cloud_path` is the row's readable
    /// path on a browsable home (`None` on an opaque one).
    pub(crate) fn image_blob_ref(
        namespace: &str,
        id: &str,
        cloud_path: Option<String>,
    ) -> coven::BlobRef {
        coven::BlobRef {
            namespace: namespace.to_string(),
            id: id.to_string(),
            scope: coven::BlobScope::Master,
            cloud_path,
            provenance: coven::Provenance::HostProvided,
            fill: coven::CacheFill::CacheEager,
        }
    }

    /// The cover [`ImageRef`] for one release — its image id paired with the
    /// `covers` row's `_updated_at` — or `None` when the release has no cover row.
    pub(super) async fn cover_ref(
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
        Ok(self
            .database
            .cover_versions(release_ids)
            .await?
            .into_iter()
            .map(|(id, version)| (id.clone(), ImageRef { id, version }))
            .collect())
    }

    /// Read a host-provided library image's whole bytes through coven's
    /// locality-aware read: coven's local store while Local, the pinned/evictable
    /// cache or the cloud while Remote. `id` is a release id (a cover) or an artist
    /// id (an artist image); the `covers` row is probed first (the common grid
    /// case), then `artist_images`. `None` when no such image row exists (no cover
    /// produced); a read error surfaces rather than being masked.
    pub async fn read_image_blob(&self, id: &str) -> Result<Option<Vec<u8>>, LibraryError> {
        for (namespace, image_type) in [
            (crate::sync::COVERS_NAMESPACE, LibraryImageType::Cover),
            (
                crate::sync::ARTIST_IMAGES_NAMESPACE,
                LibraryImageType::Artist,
            ),
        ] {
            let Some(row) = self.database.find_library_image(id, &image_type).await? else {
                continue;
            };
            let blob = Self::image_blob_ref(namespace, id, row.cloud_path.clone());
            let bytes = self
                .handle
                .read_blob(&blob)
                .await
                .map_err(|e| LibraryError::Storage(format!("read image {id}: {e}")))?;
            return Ok(Some(bytes));
        }
        Ok(None)
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
        match any_file_id {
            Some(file_id) => release_file_pinned(&self.handle, file_id).await,
            None => Ok(false),
        }
    }

    /// Pin a remote release's blobs for offline: coven fetches every blob into
    /// `storage/pinned/` (from the evictable cache if already there, else the
    /// cloud). Idempotent. Pinned-ness is coven cache state — there is no bae flag.
    /// The low-level cache op behind the "Pin" transition.
    pub(crate) async fn pin_release_blobs(&self, release_id: &str) -> Result<(), LibraryError> {
        let files = self.database.get_files_for_release(release_id).await?;
        let blobs: Vec<_> = files.iter().map(Self::release_file_blob_ref).collect();
        self.handle
            .pin(&blobs)
            .await
            .map_err(|e| LibraryError::Storage(format!("pin release {release_id}: {e}")))
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
