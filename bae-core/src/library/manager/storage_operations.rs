use super::*;

/// The pin-state answer for a release file. A file id that names no
/// `release_files` row (e.g. a path-traversal token forged by a peer, or a since-
/// deleted file) is `RejectedBadId` rather than silently folded into `NotPinned`,
/// so the caller that holds the diagnostics sink can count it as an anomaly before
/// treating it as not pinned.
pub(super) enum ReleasePinState {
    Pinned,
    NotPinned,
    RejectedBadId,
}

/// Whether `file_id`'s release is pinned offline, answered through the handle's
/// cache-state query. coven binds the row blob reference from the live
/// `release_files` row; a `file_id` that names no such row can't be pinned and is
/// rejected (never trusted), while a real I/O failure on the pin check still
/// surfaces.
pub(super) async fn release_file_pin_state(
    database: &Database,
    file_id: &str,
) -> Result<ReleasePinState, LibraryError> {
    let blob = match database
        .row_blob_ref(crate::sync::RELEASE_FILES_NAMESPACE, file_id)
        .await
    {
        Ok(blob) => blob,
        Err(e) => {
            warn!("pin-state check: no release_files row for {file_id}: {e}");
            return Ok(ReleasePinState::RejectedBadId);
        }
    };
    // Pinning is a property of a *cloud* object: the pinned folder holds kept
    // copies of blobs that live remotely. A blob with no committed cloud object
    // — a Local release's file, or one whose upload has not landed yet — has
    // nothing to keep a copy of, so it is not pinned. Read that off coven's own
    // reference rather than asking it to resolve a cache path for an object that
    // does not exist.
    if blob.stored().is_none() {
        return Ok(ReleasePinState::NotPinned);
    }
    match database.is_pinned(&[blob]).await {
        Ok(true) => Ok(ReleasePinState::Pinned),
        Ok(false) => Ok(ReleasePinState::NotPinned),
        Err(e) => Err(LibraryError::Storage(format!(
            "pin-state for {file_id}: {e}"
        ))),
    }
}

impl LibraryManager {
    /// Build the database cleanup writes and cache blob refs for a release delete
    /// without mutating durable state. The caller commits `db_cleanup` in the same
    /// DB transaction as the row deletion, then asks coven to drop any leftover
    /// on-device copies.
    pub(super) async fn release_delete_plan(
        &self,
        release: &DbRelease,
    ) -> Result<ReleaseDeletePlan, LibraryError> {
        let release_id = &release.id;
        let files = self.get_files_for_release(release_id).await?;
        let mut evict_blobs = Vec::new();
        let mut blobs_to_tombstone = Vec::new();
        let mut external_refs_to_clear = Vec::new();
        // The transition outlasts its uploads — the queue empties at
        // publication-prepare while the intent lives until the Store write
        // activates — so this asks coven how far the make-remote got rather than
        // whether anything is still queued, which would read as done too early.
        let make_remote_in_flight = !release.remote
            && self
                .database
                .make_remote_progress("releases", release_id)
                .await
                .map_err(|e| {
                    LibraryError::Storage(format!("make-remote progress for {release_id}: {e}"))
                })?
                .is_some();

        if release.remote {
            for file in &files {
                let blob = self.release_file_row_blob_ref(&file.id).await?;
                // Whether there is a cloud object to remove is coven's locator,
                // not bae's gate column: a blob with no committed object has
                // nothing to tombstone and `enqueue_blob_delete` refuses it.
                if blob.stored().is_some() {
                    blobs_to_tombstone.push(blob.clone());
                }
                evict_blobs.push(blob);
            }
        } else if make_remote_in_flight {
            // A make-remote caught mid-flight is coven's to unwind: it clears the
            // intent, drops the still-queued uploads, and tombstones whatever
            // already landed. bae only evicts the on-device copies afterwards.
            for file in &files {
                evict_blobs.push(self.release_file_row_blob_ref(&file.id).await?);
            }
            external_refs_to_clear.extend(
                files
                    .iter()
                    .map(|f| ("release_files".to_string(), f.id.clone())),
            );
        } else {
            // Local: the files are the user's own files in place — never delete
            // them. Just drop coven's external registrations in the delete
            // transaction so no orphan ref outlives the release row.
            external_refs_to_clear.extend(
                files
                    .iter()
                    .map(|f| ("release_files".to_string(), f.id.clone())),
            );
        }

        let cover = self
            .database
            .find_library_image(release_id, &LibraryImageType::Cover)
            .await?;

        if cover.is_some() {
            let blob = self
                .database
                .row_blob_ref(crate::sync::COVERS_NAMESPACE, release_id)
                .await
                .map_err(|e| {
                    LibraryError::Storage(format!("blob ref for cover {release_id}: {e}"))
                })?;
            // Only a cover that reached the cloud has an object to remove; one
            // with no stored locator would be refused by `enqueue_blob_delete`.
            if blob.stored().is_some() {
                blobs_to_tombstone.push(blob.clone());
            }
            evict_blobs.push(blob);
        }

        Ok(ReleaseDeletePlan {
            db_cleanup: DeleteCleanupPlan {
                blobs_to_tombstone,
                external_refs_to_clear,
            },
            evict_blobs,
            cancel_make_remote: make_remote_in_flight,
        })
    }

    pub(super) async fn evict_delete_blobs(&self, blobs: Vec<coven::RowBlobRef>) {
        for blob in blobs {
            if let Err(e) = self.database.evict_blob(&blob).await {
                warn!(
                    "Failed to drop on-device copies during deletion for {}/{}: {e}",
                    blob.table(),
                    blob.row_id()
                );
            }
        }
    }
}
