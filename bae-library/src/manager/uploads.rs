//! Moving a release's files to the cloud: the move's admission, a failed
//! move ended, and the upload queue the moves wait in.

use super::*;
use bae_diagnostics::{LocalId, TelemetryEvent};
use tracing::{error, info};

impl LibraryManager {
    /// Move Local releases to Cloud, each release of the selection in a write
    /// of its own, as one move of its own: no release shows a transition on
    /// the way. Empty and duplicate selections are invalid commands. A
    /// release that cannot move changes nothing, and the method returns it
    /// with the reason beside the durable receipt for the releases that
    /// moved. When coven refused the move, the reason is coven's. Once a move
    /// finishes, coven no
    /// longer reads the user's original; the source file itself remains
    /// untouched.
    pub async fn upload_releases(
        &self,
        release_ids: &[String],
    ) -> Result<UploadReleasesOutcome, LibraryError> {
        if release_ids.is_empty() {
            return Err(LibraryError::Validation("upload batch has no releases".to_string()));
        }

        let mut selected = std::collections::HashSet::with_capacity(release_ids.len());
        for release_id in release_ids {
            if !selected.insert(release_id) {
                return Err(LibraryError::Validation(format!(
                    "release {release_id} appears more than once in the storage batch"
                )));
            }
        }

        let mut failures = Vec::new();
        let mut admitted_release_ids = Vec::with_capacity(release_ids.len());
        for (release_id, moved) in self.move_releases(release_ids).await? {
            match moved {
                Ok(file_count) => {
                    info!(%release_id, "release move to the cloud queued");
                    self.record_telemetry(TelemetryEvent::StorageTransferCompleted {
                        action: ReleaseStorageAction::Upload,
                        release_id: LocalId(release_id.clone()),
                        file_count,
                    });
                    admitted_release_ids.push(release_id);
                }
                Err(error) => {
                    error!(%release_id, failure = %error, "release move to the cloud refused");
                    self.record_telemetry(TelemetryEvent::StorageTransferFailed {
                        action: ReleaseStorageAction::Upload,
                        release_id: LocalId(release_id.clone()),
                    });
                    failures.push((release_id, error));
                }
            }
        }

        let receipt = (!admitted_release_ids.is_empty())
            .then_some(UploadReceipt { release_ids: admitted_release_ids });
        if failures.is_empty() {
            return Ok(UploadReleasesOutcome::Complete {
                receipt: receipt.expect("a non-empty successful command has a receipt"),
            });
        }
        Ok(UploadReleasesOutcome::Partial {
            receipt,
            failure: UploadBatchFailure::from_errors(failures),
        })
    }

    /// Move each release of `release_ids` to the cloud in a write of its own,
    /// in the selection's order, and return each release with how many files
    /// it moved or why it did not move. The writes run to their end on a task
    /// of their own, so a dismissed view leaves no release of the selection
    /// unmoved. Once the library starts closing, the releases not yet moved
    /// fail as closed.
    async fn move_releases(
        &self,
        release_ids: &[String],
    ) -> Result<Vec<(String, Result<u32, LibraryError>)>, LibraryError> {
        const OPERATION: &str = "move releases to the cloud";
        let store = self.store.clone();
        let release_ids = release_ids.to_vec();
        self.run_file_operation(OPERATION, move |closing| async move {
            let mut moves = Vec::with_capacity(release_ids.len());
            for release_id in release_ids {
                let moved = if closing.is_cancelled() {
                    Err(LibraryError::Closed(OPERATION))
                } else {
                    bae_transfers::storage_moves::move_release_to_cloud(&store, release_id.clone())
                        .await
                        .map_err(LibraryError::from)
                };
                moves.push((release_id, moved));
            }
            Ok(moves)
        })
        .await
    }

    /// Move one release's files to the cloud, as a selection of one, for a
    /// test that sets one up.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn upload_release(&self, release_id: &str) -> Result<(), LibraryError> {
        let moves = self.move_releases(std::slice::from_ref(&release_id.to_string())).await?;
        moves.into_iter().try_for_each(|(_, moved)| moved.map(|_| ()))
    }

    /// End a release's failed move to the cloud, `id`, as its pending
    /// record offers: coven retires the move, and the release stays on this
    /// device only, playing from its originals.
    pub async fn cancel_failed_release_move(&self, id: coven::MoveId) -> Result<(), LibraryError> {
        Ok(self.records.cancel_failed_release_move(id).await?)
    }

    /// Test-only observability: how many upload entries a release's files still have
    /// queued (zero means every cloud copy is confirmed durable).
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn count_pending_uploads_for_release(
        &self,
        release_id: &str,
    ) -> Result<i64, LibraryError> {
        Ok(self.records.count_pending_uploads_for_release(release_id).await?)
    }

    /// Retry failed uploads now: clear coven's durable retry delay and run its
    /// upload drain immediately. Connection, queue-state, and provider failures
    /// are returned to the caller instead of reporting that a retry ran.
    pub async fn retry_outbox_now(&self) -> Result<(), LibraryError> {
        let result = match self.store.retry_uploads_now().await? {
            // coven refuses the retry while the person has paused uploads
            // and tries nothing, so no pass runs for it either.
            coven::DrainOutcome::Paused => {
                return Err(LibraryError::Storage(
                    "upload retry cannot run while cloud sync is paused".to_string(),
                ))
            }
            // coven holds trailing uploads on a metered network only while
            // the app asks it to move files on unmetered ones.
            coven::DrainOutcome::Metered => Err(LibraryError::Storage(
                "cloud uploads wait for an unmetered network".to_string(),
            )),
            // Stopped sync holds no storage connection, and coven tried
            // nothing: the retry needs connected storage, as coven says
            // when no storage is set up.
            coven::DrainOutcome::NotSyncing => Err(coven::SyncError::NoStorage.into()),
            coven::DrainOutcome::Drained { .. } | coven::DrainOutcome::QueueEmpty => Ok(()),
        };
        self.store.sync_now();
        result
    }

    /// Cancel the release's move to the cloud: its files stay on this device
    /// only, as they were, and no row changes.
    pub async fn cancel_release_upload(&self, release_id: &str) -> Result<(), LibraryError> {
        self.cancel_release_uploads(vec![release_id.to_string()]).await
    }

    pub async fn cancel_all_release_uploads(&self) -> Result<(), LibraryError> {
        let releases = self.records.uploading_releases().await?;
        self.cancel_release_uploads(releases).await
    }

    async fn cancel_release_uploads(&self, releases: Vec<String>) -> Result<(), LibraryError> {
        let records = self.records.clone();
        self.run_file_operation("cancel release uploads", move |_closing| async move {
            Ok(records.cancel_release_uploads(releases).await?)
        })
        .await
    }

    /// Resume seeded uploads for one drain, then restore the fixture's pause
    /// setting. Coven's background uploader shares the drain lock, so count
    /// the files that left the queue stored, whichever caller drained them.
    /// A file leaves it unstored too, when its move failed.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn drain_uploads_for_test(&self) -> Result<coven::DrainOutcome, String> {
        let before = self.store.uploads().await.map_err(|error| error.to_string())?;
        self.store.set_uploads_paused(false).await.map_err(|error| error.to_string())?;
        let drained = self.store.retry_uploads_now().await;
        let restored = self.store.set_uploads_paused(before.paused).await;
        restored.map_err(|error| {
            format!("restoring the upload pause setting failed: {error}; drain: {drained:?}")
        })?;
        match drained.map_err(|error| error.to_string())? {
            coven::DrainOutcome::Paused => {
                return Err("uploads paused during the fixture drain".into())
            }
            coven::DrainOutcome::NotSyncing => {
                return Err("sync stopped during the fixture drain".into())
            }
            coven::DrainOutcome::Metered => {
                return Err("uploads held for a metered network during the fixture drain".into())
            }
            coven::DrainOutcome::Drained { .. } | coven::DrainOutcome::QueueEmpty => {}
        }
        let after = self.store.uploads().await.map_err(|error| error.to_string())?;
        if before.files.is_empty() {
            Ok(coven::DrainOutcome::QueueEmpty)
        } else {
            let mut uploaded = 0;
            for queued in &before.files {
                if after.files.iter().any(|remaining| remaining.file == queued.file) {
                    continue;
                }
                let file =
                    self.store.file_ref(queued.file).await.map_err(|error| error.to_string())?;
                if matches!(file.place(), coven::FilePlace::Cloud { .. }) {
                    uploaded += 1;
                }
            }
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
            other => Err(format!("expected a drain that attempted queued uploads, got {other:?}")),
        }
    }
}

impl UploadBatchFailure {
    fn from_errors(failures: Vec<(String, LibraryError)>) -> Self {
        let mut failures = failures.into_iter();
        let (first_release_id, first_error) =
            failures.next().expect("a partial upload outcome has at least one failure");
        let first_category = first_error.category();
        let mut same_category = true;
        let mut release_ids = vec![first_release_id.clone()];
        let mut details = vec![format!("release {first_release_id}: {first_error}")];
        for (release_id, error) in failures {
            same_category &= error.category() == first_category;
            details.push(format!("release {release_id}: {error}"));
            release_ids.push(release_id);
        }
        Self {
            release_ids,
            error: bae_diagnostics::UiError::Diagnostic {
                category: if same_category {
                    first_category
                } else {
                    bae_diagnostics::UiErrorCategory::Internal
                },
                detail: details.join("\n"),
            },
        }
    }
}

#[cfg(test)]
#[path = "uploads_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "uploads_wait_tests.rs"]
mod wait_tests;
