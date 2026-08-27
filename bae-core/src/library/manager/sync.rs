//! Cloud sync surface for [`LibraryManager`]: provider connection, the upload
//! pipeline, and pause state.

use super::*;
impl LibraryManager {
    /// Whether a cloud provider is connected. Reads config, not manager presence:
    /// the connected provider lives in config and is known synchronously from the
    /// first read, whereas the `SyncManager` (and its cloud client) is built lazily
    /// once connected and still absent on mobile when the first listing query runs.
    pub fn is_sync_configured(&self) -> bool {
        self.config_handle.config().cloud_home.provider.is_some()
    }

    pub fn has_cloud_home(&self) -> bool {
        self.database.is_connected()
    }

    /// Pause or resume the cloud-upload pipeline. New enqueues still land in
    /// the outbox; coven suspends active preparation and provider request
    /// bodies while retaining their open upload sessions. Re-emits the outbox
    /// snapshot so the UI's paused indicator and bottom-panel summary update.
    pub async fn set_sync_paused(&self, paused: bool) {
        self.sync.set_sync_paused(paused).await
    }

    /// Current paused state of the upload pipeline. The snapshot builder
    /// reads this so the UI can render its paused indicator.
    pub fn is_sync_paused(&self) -> bool {
        self.sync.is_sync_paused()
    }

    /// Whether the background sync loop is running and draining uploads.
    /// Make-Remote does not require this: it durably queues against a connected
    /// cloud home, then remains queued until a loop can drain and publish it.
    pub fn is_sync_ready(&self) -> bool {
        self.database.is_syncing()
    }

    pub fn get_sync_status(&self) -> SyncStatusSnapshot {
        let state = self.sync_status.lock().unwrap().clone();
        SyncStatusSnapshot {
            error: state.error.map(crate::ui::UiError::internal),
            last_sync_time: state.last_sync_time,
            syncing: state.syncing,
            sync_ready: self.is_sync_ready(),
        }
    }

    pub fn trigger_sync(&self) {
        self.database.sync_now();
    }

    pub fn subscribe_eager_cache_fill_status(
        &self,
    ) -> tokio::sync::watch::Receiver<crate::library::EagerCacheFillStatus> {
        self.database.subscribe_eager_cache_fill_status()
    }

    pub fn cancel_eager_cache_fill(&self) {
        self.database.cancel_eager_cache_fill();
    }

    pub async fn save_s3_config(&self, data: S3ConfigData) -> Result<(), LibraryError> {
        self.sync.save_s3_config(data).await?;
        Ok(())
    }

    #[cfg(feature = "oauth-providers")]
    pub async fn sign_in_cloud_provider(
        &self,
        provider: CloudProvider,
        storage: crate::config::HomeStorage,
    ) -> Result<(), LibraryError> {
        self.sync.sign_in_cloud_provider(provider, storage).await?;
        Ok(())
    }

    pub async fn use_cloudkit(
        &self,
        storage: crate::config::HomeStorage,
    ) -> Result<(), LibraryError> {
        self.sync.use_cloudkit(storage).await?;
        Ok(())
    }

    pub async fn disconnect_cloud_provider(&self) -> Result<(), LibraryError> {
        self.sync.disconnect_cloud_provider().await
    }

    pub async fn unlock_cloud_home(&self, serialized_master_key: &str) -> Result<(), LibraryError> {
        self.sync.unlock_cloud_home(serialized_master_key).await?;
        self.config_handle.config().save_active_library()?;
        Ok(())
    }

    /// How many releases are reachable only through cloud sync — remote and not
    /// pinned in coven's cache — and would become unplayable if this device
    /// disconnected. `0` means nothing is at risk.
    ///
    /// The count is the domain fact; the sentence is not. Each surface renders it
    /// with its own locale's plural rules from the `core.sync.cloud_only_releases`
    /// catalog key, the same way the release-group card renders
    /// `core.import.pressings`.
    ///
    /// Asks coven's cache per remote release (a representative blob in
    /// `storage/pinned/`); pinned-ness is coven cache state, never a bae column.
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
    /// device's keyring already holds. Shares the sync controller's outbox
    /// in-flight set and event channel with the sync loop's upload observer.
    /// Call before [`Self::start`].
    pub async fn attach_and_start_sync(&self) -> Result<(), LibraryError> {
        self.sync.attach_and_start_sync().await
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
        if let Err(error) = self.attach_and_start_sync().await {
            warn!("startup sync connect failed; opening library not connected: {error}");
            self.set_sync_error(Some(error.to_string()));
        }
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
        if !self.is_sync_configured() {
            return Err(coven::SyncError::NotConfigured.into());
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
        {
            let mut state = self.sync_status.lock().unwrap();
            state.error = error;
        }
        self.sync_status_values.send_replace(self.get_sync_status());
    }
}
