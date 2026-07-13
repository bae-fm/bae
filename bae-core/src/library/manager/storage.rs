//! Storage operations for [`LibraryManager`]: file-path helpers, the
//! is-unlocked signal, the storage-page DB queries, and the cloud outbox /
//! delete plumbing.

use super::*;

impl LibraryManager {
    /// The on-disk path of a release file that sits at the user's own path — the
    /// external ref of a coven **user-provided Local** blob. `Ok(None)` for a Remote
    /// file (its bytes are in coven's cache, keyed by id, with no stable bae path)
    /// or an unknown one; DB errors propagate, so a caller can tell "no in-place
    /// file" from "library broken".
    ///
    /// For the callers that need the user's actual file — the DiscID re-read of the
    /// rip's artifacts — rather than coven's locality-aware byte read.
    pub async fn file_local_path(&self, file_id: &str) -> Result<Option<PathBuf>, LibraryError> {
        Ok(self
            .database
            .external_blob(file_id)
            .await?
            .map(|ext| ext.path))
    }

    // =========================================================================
    // Encryption
    // =========================================================================

    /// Whether this store's master key is established in this device's
    /// keyring — "unlocked". `bootstrap` treats the opposite of this as
    /// "locked" and defers attaching sync until an explicit unlock.
    pub fn has_encryption(&self) -> bool {
        self.sync.has_encryption()
    }
}

impl LibraryManager {
    /// Test-only observability: how many upload entries a release's files still have
    /// queued (zero means every cloud copy is confirmed durable). Production paths
    /// gate on the boolean `has_pending_uploads_for_release` instead.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn count_pending_uploads_for_release(
        &self,
        release_id: &str,
    ) -> Result<i64, LibraryError> {
        Ok(self
            .database
            .count_pending_uploads_for_release(release_id)
            .await?)
    }

    /// Seed an upload outbox row + refresh the snapshot. coven owns enqueueing in
    /// `make_remote`, so this is only a test helper for exercising the
    /// outbox-snapshot / drain machinery directly.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn add_cloud_outbox_upload(
        &self,
        file_id: &str,
        cloud_key: &str,
        source_path: Option<&str>,
        retain_pinned: bool,
    ) -> Result<(), LibraryError> {
        self.database
            .add_cloud_outbox_upload(file_id, cloud_key, source_path, retain_pinned)
            .await?;
        self.emit_outbox_changed().await;
        Ok(())
    }

    /// Retry failed uploads now: clear their backoff so the next cycle picks
    /// them up immediately, then kick the sync loop.
    pub async fn retry_outbox_now(&self) -> Result<(), LibraryError> {
        self.database.reset_cloud_outbox_backoff().await?;
        self.trigger_sync();
        self.emit_outbox_changed().await;
        Ok(())
    }

    /// Cancel one queued outbox entry by id. Removes the queue row only; the
    /// local file is untouched, so the release just stops syncing this entry.
    pub async fn cancel_outbox_item(&self, id: i64) -> Result<(), LibraryError> {
        self.database.remove_cloud_outbox_entry(id).await?;
        self.emit_outbox_changed().await;
        Ok(())
    }

    /// Stop uploading a release that's mid-make-Remote and keep it Local.
    ///
    /// coven owns the cancel: it clears the durable make-Remote intent and the
    /// release's pending upload rows, and tombstones any blob that already reached
    /// the cloud, in one transaction. The gate never flips, so the release stays
    /// Local — its files are still the external refs coven holds, untouched.
    pub async fn cancel_release_upload(&self, release_id: &str) -> Result<(), LibraryError> {
        self.coven_cancel_make_remote(release_id).await?;
        // The cancel tombstones the blobs that already reached the cloud, so
        // the release's completed-upload tally would now report done work that
        // isn't — drop it before re-deriving the snapshot.
        self.sync.upload_sessions().clear_group(Some(release_id));
        self.emit_outbox_changed().await;
        // Refresh the release row (it no longer reads as "uploading"). A
        // best-effort UI nudge — the cancel itself already succeeded above.
        match self.get_release_by_id(release_id).await {
            Ok(Some(release)) => {
                self.emit_release_updated(&release.album_id, release_id)
                    .await
            }
            Ok(None) => {
                warn!("cancel_release_upload: release {release_id} missing; skipped UI refresh")
            }
            Err(e) => {
                warn!("cancel_release_upload: loading release {release_id} for refresh failed: {e}")
            }
        }
        Ok(())
    }

    /// Drive coven's upload drain once through the handle's connected sync
    /// manager, for tests that connected an injected cloud home via
    /// [`connect_test_cloud_home`](Self::connect_test_cloud_home). Returns the
    /// number of blobs uploaded. Production drains from the running sync loop, so
    /// this stays out of release builds.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn drain_uploads_for_test(&self) -> Result<usize, String> {
        self.handle
            .drain_uploads()
            .await
            .map(|outcome| outcome.uploaded)
            .map_err(|error| error.to_string())
    }

    /// One page of the Storage Manager list, pre-sorted and pre-filtered.
    /// `total_count` counts the filtered subset, not the whole library.
    pub async fn get_storage_page(
        &self,
        sort: &crate::db::StorageSortCriterion,
        filter: crate::db::StorageFilter,
        offset: u64,
        limit: u64,
    ) -> Result<StoragePage, LibraryError> {
        let raw_rows = self
            .database
            .get_storage_page(sort, filter, offset, limit)
            .await?;
        let total_count = self.database.get_storage_count(filter).await?;

        let has_cloud_home = self.has_cloud_home();
        let sync_ready = self.is_sync_ready();
        // The cover resolver serves both halves of each row — the release's own id
        // and the album's resolved primary release id — so gather both for the
        // batch lookup.
        let cover_ids: Vec<String> = raw_rows
            .iter()
            .flat_map(|r| {
                [r.release.id.clone()]
                    .into_iter()
                    .chain(crate::db::resolve_primary_release_id(
                        r.album.primary_release_id.as_deref(),
                        r.album.release_ids.iter().map(String::as_str),
                    ))
            })
            .collect();
        let covers = self.cover_refs(&cover_ids).await?;
        let mut rows = Vec::with_capacity(raw_rows.len());
        for raw in raw_rows {
            let pinned = self
                .release_pinned(raw.release.any_file_id.as_deref())
                .await?;
            let transfer_action = self.current_transfer_action(&raw.release.id);
            rows.push(StorageRow::from_raw(
                raw,
                has_cloud_home,
                sync_ready,
                pinned,
                transfer_action,
                |rid| covers.get(rid).cloned(),
            ));
        }
        Ok(StoragePage { rows, total_count })
    }

    /// Count storage rows matching `filter`. Matches `get_storage_page`'s
    /// `total_count` for the same filter.
    pub async fn get_storage_count(
        &self,
        filter: crate::db::StorageFilter,
    ) -> Result<u64, LibraryError> {
        Ok(self.database.get_storage_count(filter).await?)
    }

    /// Sum of `total_size` over every storage row matching `filter` — the
    /// storage-manager footer's "Total:", independent of how many pages are loaded.
    pub async fn get_storage_total_size(
        &self,
        filter: crate::db::StorageFilter,
    ) -> Result<u64, LibraryError> {
        Ok(self.database.get_storage_total_size(filter).await?)
    }
}
