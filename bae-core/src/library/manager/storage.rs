//! Storage operations for [`LibraryManager`]: file-path helpers, the
//! is-unlocked signal, the storage-page DB queries, and the cloud outbox /
//! delete plumbing.

use super::*;

impl LibraryManager {
    /// The original path registered on this device, whether the file is local
    /// or attached for upload. `Ok(None)` means there is no original here or no
    /// such row; coven's cached copies expose no path. Database errors propagate.
    ///
    /// For the callers that need the user's actual file — the DiscID re-read of the
    /// rip's artifacts — rather than coven's locality-aware byte read.
    pub async fn file_local_path(&self, file_id: &str) -> Result<Option<PathBuf>, LibraryError> {
        Ok(self
            .database
            .external_file(file_id)
            .await?
            .map(|ext| ext.path))
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

    /// Retry failed uploads now: clear coven's durable retry delay and run its
    /// upload drain immediately. Connection, queue-state, and provider failures
    /// are returned to the caller instead of reporting that a retry ran.
    pub async fn retry_outbox_now(&self) -> Result<(), LibraryError> {
        if self.is_sync_paused().await? {
            return Err(LibraryError::Storage(
                "upload retry cannot run while cloud sync is paused".to_string(),
            ));
        }
        let outcome = self.database.retry_uploads_now().await?;
        self.trigger_sync();
        self.emit_outbox_changed().await;
        match outcome {
            coven::DrainOutcome::Paused => Err(LibraryError::Storage(
                "cloud upload pipeline paused after the retry began".to_string(),
            )),
            coven::DrainOutcome::Drained { .. } | coven::DrainOutcome::QueueEmpty => Ok(()),
        }
    }

    /// Detach the release's audio and restore its original paths atomically.
    /// Coven retains uploads needed by shared history; the current rows become local.
    pub async fn cancel_release_upload(&self, release_id: &str) -> Result<(), LibraryError> {
        self.cancel_release_uploads(vec![release_id.to_string()])
            .await
    }

    pub async fn cancel_all_release_uploads(&self) -> Result<(), LibraryError> {
        let releases = self.database.uploading_releases().await?;
        self.cancel_release_uploads(releases).await
    }

    async fn cancel_release_uploads(&self, releases: Vec<String>) -> Result<(), LibraryError> {
        let manager = self.clone();
        self.run_file_operation("cancel release uploads", move |_closing| async move {
            let detached = manager.database.cancel_release_uploads(releases).await?;
            for release in detached {
                manager.cancel_download(&release);
            }
            manager.emit_outbox_changed().await;
            Ok(())
        })
        .await
    }

    /// Resume seeded uploads for one drain, then restore the fixture's pause
    /// setting. Coven's background uploader shares the drain lock, so count
    /// confirmed queue removals regardless of which caller drained them.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn drain_uploads_for_test(&self) -> Result<coven::DrainOutcome, String> {
        let before = self
            .database
            .uploads()
            .await
            .map_err(|error| error.to_string())?;
        self.database
            .set_uploads_paused(false)
            .await
            .map_err(|error| error.to_string())?;
        let drained = self.database.retry_uploads_now().await;
        let restored = self.database.set_uploads_paused(before.paused).await;
        restored.map_err(|error| {
            format!("restoring the upload pause setting failed: {error}; drain: {drained:?}")
        })?;
        match drained.map_err(|error| error.to_string())? {
            coven::DrainOutcome::Paused => {
                return Err("uploads paused during the fixture drain".into())
            }
            coven::DrainOutcome::Drained { .. } | coven::DrainOutcome::QueueEmpty => {}
        }
        let after = self
            .database
            .uploads()
            .await
            .map_err(|error| error.to_string())?;
        if before.files.is_empty() {
            Ok(coven::DrainOutcome::QueueEmpty)
        } else {
            let uploaded = before
                .files
                .iter()
                .filter(|queued| {
                    !after
                        .files
                        .iter()
                        .any(|remaining| remaining.file == queued.file)
                })
                .count();
            Ok(coven::DrainOutcome::Drained { uploaded })
        }
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

    /// Whether each row of a Storage Manager page is pinned, in row order, in
    /// one read. Both page paths — the direct query and the live-query
    /// projection — resolve their pin markers through here, so neither can drift
    /// into asking per row.
    async fn page_pin_states(
        &self,
        rows: &[crate::db::DbStorageRow],
    ) -> Result<Vec<bool>, LibraryError> {
        let any_file_ids: Vec<Option<&str>> = rows
            .iter()
            .map(|row| row.release.any_file_id.as_deref())
            .collect();
        self.releases_pinned(&any_file_ids).await
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
        let pin_states = self.page_pin_states(&raw_rows).await?;
        let originals = self
            .look_at_originals(crate::album_detail::recorded_originals(
                raw_rows
                    .iter()
                    .flat_map(|row| &row.release.unattached_audio),
            ))
            .await?;
        let mut rows = Vec::with_capacity(raw_rows.len());
        for (raw, pinned) in raw_rows.into_iter().zip(pin_states) {
            let transfer_action = self.current_transfer_action(&raw.release.id);
            rows.push(StorageRow::from_raw(
                raw,
                has_cloud_home,
                pinned,
                transfer_action,
                &originals,
                |rid| covers.get(rid).cloned(),
            ));
        }
        Ok(StoragePage { rows, total_count })
    }

    pub(crate) fn subscribe_storage_browse(
        &self,
        initial: crate::db::StorageBrowseRequest,
    ) -> coven::ReconfigurableLiveQuery<
        crate::db::StorageBrowseRequest,
        crate::db::StorageBrowseProjection,
    > {
        self.database.subscribe_storage_browse(initial)
    }

    /// Each row's representative file id, in window then row order: what the
    /// list's pin markers are watched by.
    pub(crate) fn storage_browse_pin_files(
        projection: &crate::db::StorageBrowseProjection,
    ) -> Vec<Option<String>> {
        projection
            .windows
            .iter()
            .flat_map(|window| &window.rows)
            .map(|row| row.release.any_file_id.clone())
            .collect()
    }

    /// The ids of the releases a Storage Manager delivery shows, one per row.
    pub(crate) fn storage_browse_release_ids(
        projection: &crate::db::StorageBrowseProjection,
    ) -> Vec<String> {
        projection
            .windows
            .iter()
            .flat_map(|window| &window.rows)
            .map(|row| row.release.id.clone())
            .collect()
    }

    /// The originals the library records this device holding for the
    /// releases a Storage Manager delivery shows: what the disk is asked about.
    pub(crate) fn storage_browse_originals(
        projection: &crate::db::StorageBrowseProjection,
    ) -> Vec<crate::db::RecordedOriginal> {
        crate::album_detail::recorded_originals(
            projection
                .windows
                .iter()
                .flat_map(|window| &window.rows)
                .flat_map(|row| &row.release.unattached_audio),
        )
    }

    /// Resolve a Storage Manager delivery with its rows' pin markers, one per
    /// row in the order [`Self::storage_browse_pin_files`] lists them, and what
    /// the disk has said of its originals (see
    /// [`Self::storage_browse_originals`]).
    pub(crate) fn resolve_storage_browse(
        &self,
        projection: crate::db::StorageBrowseProjection,
        pin_states: Vec<bool>,
        originals: &crate::album_detail::OriginalsSeen,
    ) -> crate::library::StorageBrowseSnapshot {
        let covers = image_refs(projection.cover_versions, LibraryImageType::Cover);
        let has_cloud_home = self.has_cloud_home();
        let mut pin_states = pin_states.into_iter();
        let windows = projection
            .windows
            .into_iter()
            .map(|window| crate::library::LibraryBrowseWindow {
                window: window.window,
                rows: window
                    .rows
                    .into_iter()
                    .map(|raw| {
                        let transfer_action = self.current_transfer_action(&raw.release.id);
                        StorageRow::from_raw(
                            raw,
                            has_cloud_home,
                            pin_states.next().expect("one pin state per row"),
                            transfer_action,
                            originals,
                            |release_id| covers.get(release_id).cloned(),
                        )
                    })
                    .collect(),
            })
            .collect();
        crate::library::StorageBrowseSnapshot {
            sort: projection.sort,
            filter: projection.filter,
            windows,
            total_count: projection.total_count,
            total_size: projection.total_size,
        }
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
