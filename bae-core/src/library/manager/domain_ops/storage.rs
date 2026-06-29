//! Storage domain operations for [`LibraryManager`].

use super::*;

impl LibraryManager {
    /// Count outbox upload entries still pending for a release's files.
    /// Zero means the cloud copy is confirmed durable. Used by the unpin
    /// guard in `make_release_local` to refuse a transition mid-upload — the
    /// UI side of "no actions mid-upload" reads the `OutboxSnapshot.per_release`
    /// map instead.
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
    }

    /// One page of the Storage Manager list. Rows are returned pre-sorted
    /// and pre-filtered; `total_count` in the returned `StoragePage`
    /// reflects the filtered subset (not the full library).
    pub async fn get_storage_page(
        &self,
        sort: &StorageSort,
        filter: StorageFilter,
        offset: u64,
        limit: u64,
    ) -> Result<StoragePage, LibraryError> {
        let db_sort = to_db_storage_sort(sort);
        let db_filter = to_db_storage_filter(filter);

        let raw_rows = self
            .database
            .get_storage_page(&db_sort, db_filter, offset, limit)
            .await?;
        let total_count = self.database.get_storage_count(db_filter).await?;

        let has_cloud_home = self.has_cloud_home();
        // The cover resolver serves both halves of each row — the release's own id
        // and the album's primary release id — so gather both for the batch lookup.
        let cover_ids: Vec<String> = raw_rows
            .iter()
            .flat_map(|r| {
                [r.release.id.clone()]
                    .into_iter()
                    .chain(r.album.primary_release_id.clone())
                    .chain(r.album.release_ids.iter().cloned())
            })
            .collect();
        let covers = self.cover_refs(&cover_ids).await?;
        let mut rows = Vec::with_capacity(raw_rows.len());
        for raw in raw_rows {
            let pinned = self
                .release_pinned(raw.release.any_file_id.as_deref())
                .await?;
            rows.push(StorageRow::from_raw(raw, has_cloud_home, pinned, |rid| {
                covers.get(rid).cloned()
            }));
        }
        Ok(StoragePage { rows, total_count })
    }

    /// Count storage rows matching `filter`. Matches `get_storage_page`'s
    /// `total_count` for the same filter.
    pub async fn get_storage_count(&self, filter: StorageFilter) -> Result<u64, LibraryError> {
        let db_filter = to_db_storage_filter(filter);
        Ok(self.database.get_storage_count(db_filter).await?)
    }

    /// Tombstone every file's cloud blob (cancelling any pending upload first) and
    /// drop coven's local cache copies, for a Remote release that is being deleted.
    ///
    /// SAFETY: the cloud copies are the only ones, so this is only safe when the
    /// release is genuinely being removed. Its sole caller is
    /// `queue_release_files_for_deletion` (the delete path); make-Local
    /// tombstoning is coven's (it enqueues the deletes inside `make_local`'s
    /// atomic commit). `files` are precomputed by the caller, so the cloud keys are
    /// correct.
    pub(crate) async fn queue_storage_deletions(&self, files: &[DbFile]) {
        // Queue cloud outbox deletes and cancel pending uploads. The delete key
        // must match the key the blob was uploaded under, derived through coven
        // for the home's scheme (the row's readable `cloud_path` on a browsable
        // home, the hashed-by-id default on an opaque one).
        for file in files {
            let cloud_key = match self.release_file_cloud_key(file) {
                Ok(key) => key,
                Err(e) => {
                    warn!("Failed to derive delete key for {}: {e}", file.id);
                    continue;
                }
            };

            // Cancel any pending upload for this file
            if let Err(e) = self
                .database
                .remove_cloud_outbox_uploads_for_key(&cloud_key)
                .await
            {
                warn!("Failed to cancel outbox upload for {}: {e}", cloud_key);
            }

            // Queue cloud delete
            if let Err(e) = self.database.add_cloud_outbox_delete(&cloud_key).await {
                warn!("Failed to add outbox delete for {}: {e}", cloud_key);
            }
        }

        // Drop coven's local cache copies (both pinned and evictable folders) so a
        // deleted release leaks nothing on disk. The release is Remote here, so its
        // blobs are cache copies, not external refs. Dropping the on-device cache
        // for a deleted blob is bae's delete-path responsibility. Best-effort: each
        // drop logs and continues so a cleanup hiccup never aborts the delete.
        for file in files {
            if let Err(e) = self
                .handle
                .evict_blob(&Self::release_file_blob_ref(file))
                .await
            {
                warn!(
                    "Failed to drop on-device copies of {} during deletion: {e}",
                    file.id
                );
            }
        }

        self.emit_outbox_changed().await;
    }
}
