//! Storage operations for [`LibraryManager`]: file-path helpers, the
//! is-unlocked signal, and the cloud outbox / delete plumbing.

use super::*;

impl LibraryManager {
    /// The path of the original coven records on this device for a file,
    /// whether or not it has been uploaded. `Ok(None)` means there is no
    /// original here or no such row; coven's cached copies expose no path.
    /// Database errors propagate.
    ///
    /// For the callers that need the user's actual file — the DiscID re-read of the
    /// rip's artifacts — rather than coven's locality-aware byte read.
    pub async fn file_local_path(&self, file_id: &str) -> Result<Option<PathBuf>, LibraryError> {
        Ok(self.database.external_file(file_id).await?.map(|ext| ext.path))
    }
}

impl LibraryManager {
    /// This device's id in the store.
    pub(crate) fn device_id(&self) -> coven::DeviceId {
        self.database.device_id()
    }

    /// Whether any of the release's files still waits to upload from this
    /// device.
    pub(crate) async fn has_pending_uploads_for_release(
        &self,
        release_id: &str,
    ) -> Result<bool, LibraryError> {
        Ok(self.database.has_pending_uploads_for_release(release_id).await?)
    }

    /// Test-only observability: how many upload entries a release's files still have
    /// queued (zero means every cloud copy is confirmed durable). Production paths
    /// gate on the boolean `has_pending_uploads_for_release` instead.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn count_pending_uploads_for_release(
        &self,
        release_id: &str,
    ) -> Result<i64, LibraryError> {
        Ok(self.database.count_pending_uploads_for_release(release_id).await?)
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
        match outcome {
            coven::DrainOutcome::Paused => Err(LibraryError::Storage(
                "cloud upload pipeline paused after the retry began".to_string(),
            )),
            coven::DrainOutcome::Drained { .. } | coven::DrainOutcome::QueueEmpty => Ok(()),
        }
    }

    /// Cancel the release's move to the cloud: its files stay on this device
    /// only, as they were, and no row changes.
    pub async fn cancel_release_upload(&self, release_id: &str) -> Result<(), LibraryError> {
        self.cancel_release_uploads(vec![release_id.to_string()]).await
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
        let before = self.database.uploads().await.map_err(|error| error.to_string())?;
        self.database.set_uploads_paused(false).await.map_err(|error| error.to_string())?;
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
        let after = self.database.uploads().await.map_err(|error| error.to_string())?;
        if before.files.is_empty() {
            Ok(coven::DrainOutcome::QueueEmpty)
        } else {
            let mut uploaded = 0;
            for queued in &before.files {
                if after.files.iter().any(|remaining| remaining.file == queued.file) {
                    continue;
                }
                let file =
                    self.database.file_ref(queued.file).await.map_err(|error| error.to_string())?;
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
