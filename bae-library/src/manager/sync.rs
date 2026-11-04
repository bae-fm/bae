//! Cloud sync surface for [`LibraryManager`]: provider connection, the upload
//! pipeline, and pause state.

use super::*;
impl LibraryManager {
    pub async fn is_sync_configured(&self) -> Result<bool, LibraryError> {
        Ok(self.database.storage_config().await?.is_some())
    }

    /// Whether storage is set up on this device, as coven's sync status
    /// says now.
    pub async fn has_cloud_home(&self) -> Result<bool, LibraryError> {
        Ok(self.database.cloud_home().await?)
    }

    pub async fn set_sync_paused(&self, paused: bool) -> Result<(), LibraryError> {
        self.sync.set_sync_paused(paused).await
    }

    pub async fn is_sync_paused(&self) -> Result<bool, LibraryError> {
        self.sync.is_sync_paused().await
    }

    /// Whether coven's sync loop is running now: storage is set up and
    /// syncing isn't stopped.
    pub async fn is_sync_ready(&self) -> Result<bool, LibraryError> {
        Ok(self.sync_status().await?.running())
    }

    /// The sync status now, as [`Self::subscribe_sync_status`] makes each.
    pub async fn sync_status(&self) -> Result<bae_db::SyncStatusSnapshot, LibraryError> {
        Ok(self
            .database
            .read_with_sync_state(bae_db::sync_status_rows_on, bae_db::SyncStatusSnapshot::read)
            .await?)
    }

    pub async fn pending(&self) -> Result<Vec<coven::PendingRecord>, LibraryError> {
        Ok(self.database.pending().await?)
    }

    pub async fn retry_pending_operation(
        &self,
        id: coven::OperationId,
    ) -> Result<(), LibraryError> {
        self.database.retry_pending_operation(id).await?;
        Ok(())
    }

    /// Stop the sync loop: returns once no pass runs.
    pub async fn stop_sync(&self) -> Result<(), LibraryError> {
        Ok(self.database.stop_sync().await?)
    }

    pub fn trigger_sync(&self) {
        self.database.sync_now();
    }

    /// The person asked to retry everything now, such as by pulling to
    /// refresh. coven ends every ordinary backoff, answered or not: the
    /// connection's, operations', uploads' and downloads', each keeping its
    /// failure count, and runs a pass. Provider cooldowns, failures that
    /// never retry, and waits for new credentials hold. Automatic triggers
    /// use [`Self::trigger_sync`], which leaves every backoff in place.
    ///
    /// The retry is in place once this returns, with its answer to await:
    /// the first pass after it ends, with what came of it. Deciding the
    /// answer writes coven's retry state; a failed write is returned.
    pub async fn retry_now(
        &self,
    ) -> Result<
        impl std::future::Future<Output = bae_db::SyncRetryOutcome> + Send + 'static,
        LibraryError,
    > {
        let answer = self.database.retry_now().await?;
        Ok(async move { answer.await.into() })
    }

    /// Set S3 up as the library's storage, telling `steps` each step coven
    /// runs.
    pub async fn save_s3_config(
        &self,
        data: S3ConfigData,
        device_name: &str,
        steps: crate::StorageSetupSteps<'_>,
    ) -> Result<coven::StorageSetup, LibraryError> {
        self.sync.save_s3_config(data, device_name, steps).await
    }

    #[cfg(feature = "oauth-providers")]
    pub async fn sign_in(&self, provider: coven::CloudProvider) -> Result<(), LibraryError> {
        self.sync.sign_in(provider).await
    }

    #[cfg(feature = "oauth-providers")]
    pub async fn sign_in_again(
        &self,
        device_name: &str,
        steps: crate::StorageSetupSteps<'_>,
    ) -> Result<coven::StorageSetup, LibraryError> {
        self.sync.sign_in_again(device_name, steps).await
    }

    #[cfg(feature = "oauth-providers")]
    pub async fn setup_oauth_storage(
        &self,
        storage: coven::StorageConfig,
        device_name: &str,
        steps: crate::StorageSetupSteps<'_>,
    ) -> Result<coven::StorageSetup, LibraryError> {
        self.sync.setup_oauth_storage(storage, device_name, steps).await
    }

    #[cfg(feature = "oauth-providers")]
    pub async fn onedrive_root(&self) -> Result<coven::StorageFolder, LibraryError> {
        Ok(self.database.onedrive_root().await?)
    }

    #[cfg(feature = "oauth-providers")]
    pub async fn onedrive_folders(
        &self,
        parent: &coven::StorageConfig,
    ) -> Result<Vec<coven::StorageFolder>, LibraryError> {
        Ok(self.database.onedrive_folders(parent).await?)
    }

    #[cfg(feature = "oauth-providers")]
    pub async fn create_onedrive_folder(
        &self,
        parent: &coven::StorageConfig,
        name: &str,
    ) -> Result<coven::StorageFolder, LibraryError> {
        Ok(self.database.create_onedrive_folder(parent, name).await?)
    }

    #[cfg(feature = "oauth-providers")]
    pub async fn dropbox_shared_folders(&self) -> Result<Vec<coven::StorageFolder>, LibraryError> {
        Ok(self.database.dropbox_shared_folders().await?)
    }

    pub async fn use_cloudkit(
        &self,
        device_name: &str,
        steps: crate::StorageSetupSteps<'_>,
    ) -> Result<coven::StorageSetup, LibraryError> {
        self.sync.use_cloudkit(device_name, steps).await
    }

    pub async fn unlock_store_key(&self) -> Result<(), LibraryError> {
        self.sync.unlock_store_key().await
    }

    /// Start coven's sync loop with the store keys this session unlocked. Its
    /// first pass connects, and a pass that can't reach storage backs off and
    /// retries like any other; starting fails only when the session lacks a
    /// store key or custody can't be read.
    pub async fn attach_and_start_sync(&self) -> Result<(), LibraryError> {
        self.sync.attach_and_start_sync().await
    }

    /// Start sync for a returning user without withholding local library
    /// services. [`Self::start`] must already have installed the status,
    /// pending subscriptions so the connection's first events
    /// have consumers.
    /// The task runs on the runtime retained by the app and is supervised like
    /// the manager's other background workers.
    pub fn start_sync_at_startup(&self) -> Result<(), LibraryError> {
        let manager = self.clone();
        let failed_start = self.clone();
        self.spawn_supervised_task(
            "startup sync attachment",
            async move {
                manager.attach_and_start_sync_at_startup().await;
            },
            // A start that panicked started no loop, like one that failed:
            // the status line says why and offers Reconnect.
            move |failed| {
                let manager = failed_start.clone();
                let error = LibraryError::from(failed);
                let recorded = failed_start.spawn_supervised_task(
                    "record the failed sync start",
                    async move { manager.set_sync_error(Some(&error)).await },
                    |_| {},
                );
                if let Err(error) = recorded {
                    tracing::warn!(%error, "could not record the failed sync start");
                }
            },
        )
    }

    /// Start sync at startup, tolerating a failure.
    ///
    /// A returning user who launches offline still opens their library — local
    /// browse and pinned playback need no network: the loop starts, the status
    /// line reads offline, and the loop retries on its own. A start that fails
    /// outright, such as a keychain locked for the session, starts no loop and
    /// is recorded as the sync-status error in its own category, so the status
    /// line says why and offers Reconnect.
    pub async fn attach_and_start_sync_at_startup(&self) {
        let started = std::time::Instant::now();
        tracing::info!("startup sync attachment started");
        if let Err(error) = self.attach_and_start_sync().await {
            warn!("startup sync connect failed; opening library not connected: {error}");
            self.set_sync_error(Some(&error)).await;
        }
        tracing::info!(
            attachment_ms = started.elapsed().as_millis(),
            "startup sync attachment returned"
        );
    }

    /// Retry sync against the provider this library already has configured, so a
    /// failure a user has since fixed — an offline launch, an unreachable
    /// endpoint, a bad object the cloud side no longer serves — clears without
    /// relaunching the app or re-entering the provider settings.
    ///
    /// Two failures land here and each needs a different move. A start that
    /// failed left no loop running, and only starting it again builds one; a
    /// running loop whose pass failed is backing off, and a wake runs its next
    /// pass now instead of when its backoff ends. `is_sync_ready` is what
    /// separates them: it is true only while a loop is running.
    ///
    /// A start that fails becomes this retry's recorded sync-status error, so
    /// the status line shows why the retry didn't take; it is cleared the moment a retry is under way. A library
    /// with no provider is not failing sync, it has none, so that refusal goes
    /// back to the caller alone rather than posing as a sync failure.
    pub async fn reconnect_sync(&self) -> Result<(), LibraryError> {
        if !self.is_sync_configured().await? {
            return Err(coven::SyncError::NoStorage.into());
        }
        if !self.is_sync_ready().await? {
            if let Err(error) = self.attach_and_start_sync().await {
                warn!("sync reconnect failed: {error}");
                self.set_sync_error(Some(&error)).await;
                return Err(error);
            }
        }
        self.database.record_sync_start_failure(None).await?;
        self.trigger_sync();
        Ok(())
    }

    /// Record the failed start the status line reads, in the failure's own
    /// category, as the sync loop's failures are: a locked keychain reads as
    /// locked, a refused sign-in as credentials. `None` clears it. A failure
    /// to record it is logged and reported as a bug: the status line can't
    /// say why sync didn't start.
    async fn set_sync_error(&self, error: Option<&LibraryError>) {
        let failure =
            error.map(|error| bae_diagnostics::UiError::diagnostic(error.category(), error));
        if let Err(error) = self.database.record_sync_start_failure(failure).await {
            self.diagnostics.report_bug("record why sync failed to start", &error);
        }
    }
}

impl LibraryManager {
    pub async fn detect_s3_settings(
        &self,
        storage: coven::StorageConfig,
        credentials: coven::S3Credentials,
    ) -> Result<coven::S3Detection, LibraryError> {
        Ok(self.database.detect_s3_settings(storage, credentials).await?)
    }
    pub async fn storage_location(&self) -> Result<Option<coven::StorageConfig>, LibraryError> {
        Ok(self.database.storage_config().await?)
    }
}

#[cfg(test)]
#[path = "sync_tests.rs"]
mod tests;
