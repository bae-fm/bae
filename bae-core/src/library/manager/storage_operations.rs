use super::*;

/// The pin-state answer for a release file. A file id that names no
/// `release_files` row (e.g. a path-traversal token forged by a peer, or a since-
/// deleted file) is `RejectedBadId` rather than silently folded into `NotPinned`,
/// so the caller that holds the diagnostics sink can count it as an anomaly before
/// treating it as not pinned.
#[derive(Clone, Copy)]
pub(super) enum ReleasePinState {
    Pinned,
    NotPinned,
    RejectedBadId,
}

/// Whether each release is pinned offline, one answer per entry in the order
/// given, through the handle's set-based cache-state query. Each entry is a
/// release's representative file id, or `None` for a release with no files —
/// which is not pinned and asks coven nothing.
///
/// coven resolves each live `release_files` row and answers the pin question for
/// the whole set in one read, so a list that shows a pin marker per row costs one
/// call rather than one per row. A file id that names no such row can't be
/// pinned and is rejected (never trusted); a file with no committed cloud object
/// — a Local release's, or one whose upload has not landed — has nothing to keep
/// a copy of and is not pinned; a real I/O failure on the pin check still
/// surfaces.
pub(super) async fn release_file_pin_states(
    database: &Database,
    any_file_ids: &[Option<&str>],
) -> Result<Vec<ReleasePinState>, LibraryError> {
    let named: Vec<String> = any_file_ids
        .iter()
        .flatten()
        .map(|file_id| (*file_id).to_string())
        .collect();
    if named.is_empty() {
        return Ok(vec![ReleasePinState::NotPinned; any_file_ids.len()]);
    }
    let pinned = database
        .rows_pinned(crate::sync::RELEASE_FILES_NAMESPACE, named.clone())
        .await
        .map_err(|e| LibraryError::Storage(format!("pin-state for {named:?}: {e}")))?;
    // coven answers one entry per named id, in order, so stepping those answers
    // through the named slots puts each back beside the release it came from.
    let mut answers = named.iter().zip(pinned);
    Ok(any_file_ids
        .iter()
        .map(
            |any_file_id| match any_file_id.and_then(|_| answers.next()) {
                Some((_, Some(true))) => ReleasePinState::Pinned,
                Some((_, Some(false))) | None => ReleasePinState::NotPinned,
                Some((file_id, None)) => {
                    warn!("pin-state check: no release_files row for {file_id}");
                    ReleasePinState::RejectedBadId
                }
            },
        )
        .collect())
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

        // A file coven holds a copy of — a Remote one in its cache, or one an
        // in-flight make-remote already wrote there — has an on-device copy to
        // drop. The cloud object is not bae's to remove: the deleted row stops
        // naming it, and coven's accepted reclaim retires the orphan once the
        // accepted snapshot shows nothing owns the bytes. A mid-flight
        // make-remote is coven's to unwind besides — it clears the intent, drops
        // the still-queued uploads, and takes back out whatever already landed.
        if release.remote || make_remote_in_flight {
            for file in &files {
                evict_blobs.push(self.release_file_row_blob_ref(&file.id).await?);
            }
        }
        // While the release is not Remote its files are the user's own files in
        // place — never deleted. Drop coven's external registrations in the
        // delete transaction so no orphan ref outlives the release row.
        if !release.remote {
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
            evict_blobs.push(
                self.database
                    .row_blob_ref(crate::sync::COVERS_NAMESPACE, release_id)
                    .await
                    .map_err(|e| {
                        LibraryError::Storage(format!("blob ref for cover {release_id}: {e}"))
                    })?,
            );
        }

        Ok(ReleaseDeletePlan {
            db_cleanup: DeleteCleanupPlan {
                external_refs_to_clear,
            },
            evict_blobs,
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
