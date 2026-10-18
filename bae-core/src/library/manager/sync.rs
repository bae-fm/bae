//! Cloud sync surface for [`LibraryManager`]: provider connection, the upload
//! pipeline, and pause state.

use super::*;
impl LibraryManager {
    pub async fn is_sync_configured(&self) -> Result<bool, LibraryError> {
        Ok(self.database.storage_config().await?.is_some())
    }

    pub fn has_cloud_home(&self) -> bool {
        self.sync_status.snapshot().connected()
    }

    pub(crate) fn subscribe_cloud_home(&self) -> tokio::sync::watch::Receiver<bool> {
        self.sync_status.subscribe_cloud_home()
    }

    pub async fn set_sync_paused(&self, paused: bool) -> Result<(), LibraryError> {
        self.sync.set_sync_paused(paused).await
    }

    pub async fn is_sync_paused(&self) -> Result<bool, LibraryError> {
        self.sync.is_sync_paused().await
    }

    pub fn is_sync_ready(&self) -> bool {
        self.sync_status.snapshot().running()
    }

    pub fn get_sync_status(&self) -> SyncStatusSnapshot {
        self.sync_status.snapshot()
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

    pub fn stop_sync(&self) {
        self.database.stop_sync();
    }

    pub fn trigger_sync(&self) {
        self.database.sync_now();
    }

    pub fn subscribe_eager_cache_fill_status(
        &self,
    ) -> coven::LiveQuery<crate::library::EagerCacheFillStatus> {
        self.database.subscribe_eager_cache_fill_status()
    }

    pub async fn cancel_eager_cache_fill(&self) -> Result<(), LibraryError> {
        Ok(self.database.cancel_eager_cache_fill().await?)
    }

    pub async fn save_s3_config(
        &self,
        data: S3ConfigData,
        device_name: &str,
    ) -> Result<coven::StorageSetup, LibraryError> {
        self.sync.save_s3_config(data, device_name).await
    }

    #[cfg(feature = "oauth-providers")]
    pub async fn authenticate(&self, access: coven::OAuthAccess) -> Result<(), LibraryError> {
        self.sync.authenticate(access).await
    }

    #[cfg(feature = "oauth-providers")]
    pub async fn setup_oauth_storage(
        &self,
        storage: coven::StorageConfig,
        device_name: &str,
    ) -> Result<coven::StorageSetup, LibraryError> {
        self.sync.setup_oauth_storage(storage, device_name).await
    }

    pub async fn use_cloudkit(
        &self,
        device_name: &str,
    ) -> Result<coven::StorageSetup, LibraryError> {
        self.sync.use_cloudkit(device_name).await
    }

    pub async fn disconnect_cloud_provider(&self) -> Result<(), LibraryError> {
        self.sync.disconnect_cloud_provider().await
    }

    pub async fn unlock_store_key(&self) -> Result<(), LibraryError> {
        self.sync.unlock_store_key().await
    }

    /// How many releases are reachable only through cloud sync — remote and not
    /// pinned in coven's cache — and would become unplayable if this device
    /// disconnected. `0` means nothing is at risk.
    ///
    /// The count is the domain fact; the sentence is not. Each surface renders it
    /// with its own locale's plural rules from the `core.sync.cloud_only_releases`
    /// catalog key.
    ///
    /// A representative file identifies each release, then every file must be
    /// pinned. Pin state belongs to coven, never a bae column.
    pub async fn cloud_only_release_count(&self) -> Result<u64, LibraryError> {
        let remote_file_ids = self.database.get_remote_release_file_ids().await?;
        let mut count: u64 = 0;
        for any_file_id in &remote_file_ids {
            if !self.release_pinned(any_file_id.as_deref()).await? {
                count += 1;
            }
        }
        Ok(count)
    }

    /// Build, start, and attach a sync manager. Used once at startup for a
    /// returning user with a configured cloud home: coven resolves the at-rest
    /// cipher from the master-key custody itself, keyed off whatever this
    /// device's keyring already holds.
    pub async fn attach_and_start_sync(&self) -> Result<(), LibraryError> {
        self.sync.attach_and_start_sync().await
    }

    /// Start the returning-user attachment without withholding local library
    /// services. [`Self::start`] must already have installed the status and
    /// outbox subscriptions so the connection's first events have consumers.
    /// The task runs on the runtime retained by the app and is supervised like
    /// the manager's other background workers.
    pub fn start_sync_at_startup(&self) {
        let manager = self.clone();
        self.spawn_supervised_task("startup sync attachment", async move {
            manager.attach_and_start_sync_at_startup().await;
        });
    }

    /// Attach the sync manager at startup, tolerating a connect failure.
    ///
    /// A returning user who launches offline must still open their library — local
    /// browse and pinned playback need no network, and the connect can be retried on
    /// the next launch (idempotently). coven's connect leaves the handle home-less on
    /// failure, so nothing is half-attached: the failure is recorded as the
    /// sync-status error (identical to a later cycle failing) so the UI shows its
    /// reconnect banner, and `has_cloud_home` / `is_sync_ready` report not connected.
    pub async fn attach_and_start_sync_at_startup(&self) {
        let started = std::time::Instant::now();
        tracing::info!("startup sync attachment started");
        if let Err(error) = self.attach_and_start_sync().await {
            warn!("startup sync connect failed; opening library not connected: {error}");
            self.set_sync_error(Some(error.to_string()));
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
    /// Two failures land here and each needs a different move. A startup connect
    /// that failed left no connection at all, and only a fresh connect builds
    /// one; a connected loop whose cycle failed is already backing off, and a
    /// wake runs its next cycle now instead of up to five minutes from now.
    /// `is_sync_ready` is what separates them: it is true only for an installed
    /// connection with a live loop behind it.
    ///
    /// A connect that fails becomes this retry's recorded sync-status error, so
    /// every surface that already renders a sync failure shows why the retry
    /// didn't take; it is cleared the moment a retry is under way. A library
    /// with no provider is not failing sync, it has none, so that refusal goes
    /// back to the caller alone rather than posing as a sync failure.
    pub async fn reconnect_sync(&self) -> Result<(), LibraryError> {
        if !self.is_sync_configured().await? {
            return Err(coven::SyncError::NoStorage.into());
        }
        if !self.is_sync_ready() {
            if let Err(error) = self.attach_and_start_sync().await {
                warn!("sync reconnect failed: {error}");
                self.set_sync_error(Some(error.to_string()));
                return Err(error);
            }
        }
        self.set_sync_error(None);
        self.trigger_sync();
        Ok(())
    }

    /// Write the sync-status error the UI's failure banner reads, matching the
    /// sync loop's own failure path. `None` clears it.
    fn set_sync_error(&self, error: Option<String>) {
        self.sync_status
            .set_error(error.map(crate::ui::UiError::internal));
    }
}
