//! Moving a release's files to the cloud: the move's admission, a failed
//! move ended, and the upload queue the moves wait in.

use super::*;

impl LibraryManager {
    /// Move every file of a release on this device only to the cloud, as one
    /// move; `pin` keeps each file whole on this device too once stored. The
    /// release shows the move as a transition from the write that admits it
    /// until the write that queues its files, which ends it; a failure ends
    /// it in a write of its own. Refused while the release is in another.
    pub async fn upload_release(&self, release_id: &str, pin: bool) -> Result<(), LibraryError> {
        let manager = self.clone();
        let release_id = release_id.to_string();
        // The admission runs to its end on a task of its own, so a dismissed
        // view leaves neither the move half made nor its transition behind;
        // coven keeps the files here once stored when `pin` asks it to.
        self.run_file_operation("attach release", move |_closing| async move {
            manager
                .run_storage_action(&release_id, bae_transfers::StoredTransition::Upload, async {
                    Ok(bae_transfers::release_storage_action::attach_release_files(
                        &manager.store,
                        &release_id,
                        pin,
                    )
                    .await?)
                })
                .await
        })
        .await
    }

    /// Move Local releases to Cloud. Each release is admitted independently so
    /// coven can start draining one release while later releases are still being
    /// checked and admitted. Empty and duplicate selections are invalid commands;
    /// release-specific refusals are returned beside the durable receipt for the
    /// releases that were admitted. The stored import storage choice says
    /// whether coven keeps the files whole here once stored (offline) or only
    /// in the evictable cache. Once the move finishes, coven no longer reads
    /// the user's original; the source file itself remains untouched.
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

        let pin = self.preferences().await?.import_storage.pinned;
        let mut admitted_release_ids = Vec::with_capacity(release_ids.len());
        let mut failures = Vec::new();
        for release_id in release_ids {
            // This release's transfer, from its precondition check to whichever
            // terminal state it reaches. coven's outbox reports its progress, so
            // there is no channel to announce a start on.
            let transfer = crate::storage::transfer::ReleaseTransfer::new(
                self.clone(),
                release_id.clone(),
                ReleaseStorageAction::Upload,
            );
            let admission = async {
                let file_count = transfer.start().await?;
                self.upload_release(release_id, pin).await?;
                Ok::<_, LibraryError>(file_count)
            }
            .await;

            match admission {
                Ok(file_count) => {
                    transfer.complete(file_count);
                    admitted_release_ids.push(release_id.clone());
                }
                Err(error) => {
                    transfer.fail(&error);
                    failures.push((release_id.clone(), error));
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

    /// End a release's failed move to the cloud, `id`, as its pending
    /// record offers: coven retires the move, and the release stays on this
    /// device only, playing from its originals.
    pub async fn cancel_failed_release_move(&self, id: coven::MoveId) -> Result<(), LibraryError> {
        Ok(self.records.cancel_failed_release_move(id).await?)
    }

    /// Whether any of the release's files still waits to upload from this
    /// device.
    pub(crate) async fn has_pending_uploads_for_release(
        &self,
        release_id: &str,
    ) -> Result<bool, LibraryError> {
        Ok(self.records.has_pending_uploads_for_release(release_id).await?)
    }

    /// Test-only observability: how many upload entries a release's files still have
    /// queued (zero means every cloud copy is confirmed durable). Production paths
    /// gate on the boolean `has_pending_uploads_for_release` instead.
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
        if self.is_sync_paused().await? {
            return Err(LibraryError::Storage(
                "upload retry cannot run while cloud sync is paused".to_string(),
            ));
        }
        let outcome = self.store.retry_uploads_now().await?;
        self.store.sync_now();
        match outcome {
            coven::DrainOutcome::Paused => Err(LibraryError::Storage(
                "cloud upload pipeline paused after the retry began".to_string(),
            )),
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
        }
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
        let manager = self.clone();
        self.run_file_operation("cancel release uploads", move |_closing| async move {
            let detached = manager.records.cancel_release_uploads(releases).await?;
            for release in detached {
                manager.cancel_download(&release).await?;
            }
            Ok(())
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
