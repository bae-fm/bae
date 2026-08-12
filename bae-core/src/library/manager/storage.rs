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

    /// Retry failed uploads now: run coven's upload drain immediately rather
    /// than waiting for the next sync cycle's own pass.
    ///
    /// The per-entry backoff is coven's bookkeeping — a drain it is asked for
    /// explicitly is the retry, so bae no longer reaches into the queue to clear
    /// timestamps. A drain with no provider connected is not an error the user
    /// needs: there is simply nothing to send yet.
    pub async fn retry_outbox_now(&self) -> Result<(), LibraryError> {
        if let Err(e) = self.database.drain_uploads().await {
            warn!("retrying uploads now: {e}");
        }
        self.trigger_sync();
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
        self.sync.clear_upload_session(release_id);
        self.emit_outbox_changed().await;
        Ok(())
    }

    /// Drive coven's upload drain once through the handle's connected sync
    /// manager, for tests that connected an injected cloud home via
    /// [`connect_test_cloud_home`](Self::connect_test_cloud_home). Production
    /// drains from the running sync loop, so this stays out of release builds.
    ///
    /// The [`DrainOutcome`](coven::DrainOutcome) says what the pass found. Only
    /// `Drained` carries a count; an empty queue, one held entirely in retry
    /// backoff, and a paused one are each their own answer, so a caller that
    /// planted work and expects it moved wants
    /// [`drain_uploads_expecting_work`](Self::drain_uploads_expecting_work).
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn drain_uploads_for_test(&self) -> Result<coven::DrainOutcome, String> {
        self.database
            .drain_uploads()
            .await
            .map_err(|error| error.to_string())
    }

    /// Drive one drain that is expected to attempt queued work, and report the
    /// cloud objects it wrote.
    ///
    /// A pass that attempted nothing is a failure here rather than a zero count:
    /// the test planted uploads, so an empty queue means something else consumed
    /// them and "nothing to do" must not read as "the work is done".
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn drain_uploads_expecting_work(&self) -> Result<usize, String> {
        match self.drain_uploads_for_test().await? {
            coven::DrainOutcome::Drained { uploaded, .. } => Ok(uploaded),
            other => Err(format!(
                "expected a drain that attempted queued uploads, got {other:?}"
            )),
        }
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

    pub(crate) fn subscribe_storage_page(
        &self,
        sort: &crate::db::StorageSortCriterion,
        filter: crate::db::StorageFilter,
        uploading_release_ids: Vec<String>,
        offset: u64,
        limit: u64,
    ) -> coven::LiveQuery<crate::db::StoragePageProjection> {
        self.database
            .subscribe_storage_page(sort, filter, uploading_release_ids, offset, limit)
    }

    pub(crate) async fn resolve_storage_page_projection(
        &self,
        projection: crate::db::StoragePageProjection,
    ) -> Result<(StoragePage, u64), LibraryError> {
        let covers = projection
            .cover_versions
            .into_iter()
            .map(|(id, version)| {
                (
                    id.clone(),
                    ImageRef {
                        id,
                        version,
                        image_type: crate::db::LibraryImageType::Cover,
                    },
                )
            })
            .collect::<HashMap<_, _>>();
        let has_cloud_home = self.has_cloud_home();
        let sync_ready = self.is_sync_ready();
        let mut rows = Vec::with_capacity(projection.rows.len());
        for raw in projection.rows {
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
                |release_id| covers.get(release_id).cloned(),
            ));
        }
        Ok((
            StoragePage {
                rows,
                total_count: projection.total_count,
            },
            projection.total_size,
        ))
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
