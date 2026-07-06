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
        self.handle.is_connected()
    }

    /// The one coven data handle. The playback reader clones it to stream blob
    /// ranges; callers route every blob read/write and the sync lifecycle through
    /// it rather than reaching into coven's internals.
    pub(crate) fn handle(&self) -> &CovenHandle {
        &self.handle
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn database_for_test(&self) -> Database {
        self.database.clone()
    }

    /// Pause or resume the cloud-upload pipeline. Paused means new enqueues
    /// still land in the outbox but the sync cycle won't drain them; in-flight
    /// uploads finish (coven's `drain_uploads` checks the flag between
    /// entries, not mid-write). Re-emits the outbox snapshot so the UI's
    /// paused indicator and the bottom-panel summary update.
    pub async fn set_sync_paused(&self, paused: bool) {
        self.sync.set_sync_paused(paused).await
    }

    /// Current paused state of the upload pipeline. The snapshot builder
    /// reads this so the UI can render its paused indicator.
    pub fn is_sync_paused(&self) -> bool {
        self.sync.is_sync_paused()
    }

    // Sync provider configuration

    /// Whether the background sync loop is running and draining uploads. The
    /// manage gate requires this: managing has no inline remote flip — the
    /// release only becomes remote once the upload observer (which fires from
    /// inside the running loop) confirms the last upload landed.
    pub fn is_sync_ready(&self) -> bool {
        self.handle.is_syncing()
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
        self.handle.sync_now();
    }

    pub async fn save_s3_config(&self, data: S3ConfigData) -> Result<(), String> {
        self.sync.save_s3_config(data).await?;
        // A cloud home now exists, so every release gains its storage actions.
        self.emit_all_albums_updated().await;
        Ok(())
    }

    #[cfg(feature = "oauth-providers")]
    pub async fn sign_in_cloud_provider(
        &self,
        provider: CloudProvider,
        storage: crate::config::HomeStorage,
    ) -> Result<(), String> {
        self.sync
            .sign_in_cloud_provider(provider, storage, &self.clock)
            .await?;
        // A cloud home now exists, so every release gains its storage actions.
        self.emit_all_albums_updated().await;
        Ok(())
    }

    pub async fn use_cloudkit(&self, storage: crate::config::HomeStorage) -> Result<(), String> {
        self.sync.use_cloudkit(storage).await?;
        // A cloud home now exists, so every release gains its storage actions.
        self.emit_all_albums_updated().await;
        Ok(())
    }

    pub fn disconnect_cloud_provider(&self) -> Result<(), String> {
        self.sync.disconnect_cloud_provider()?;

        // The cloud home is gone, so releases lose their storage actions; re-emit
        // every album so cached UI details drop the now-invalid actions. Spawned
        // because this fn is sync and the re-emit re-resolves each album (async).
        let manager = self.clone();
        self.runtime_handle.spawn(async move {
            manager.emit_all_albums_updated().await;
        });
        Ok(())
    }

    /// Warning text to append to the disconnect-sync confirmation when the
    /// library has releases reachable only through cloud sync — remote and not
    /// pinned in coven's cache. Returns `None` when no releases are at risk, so the
    /// dialog just shows its base message. Asks coven's cache per remote release
    /// (a representative blob in `storage/pinned/`); pinned-ness is coven cache
    /// state, never a bae column.
    pub async fn disconnect_warning_message(&self) -> Result<Option<String>, String> {
        let remote_file_ids = self
            .database
            .get_remote_release_file_ids()
            .await
            .map_err(|e| format!("list remote releases: {e}"))?;
        let mut count: u64 = 0;
        for any_file_id in &remote_file_ids {
            if !self
                .release_pinned(any_file_id.as_deref())
                .await
                .map_err(|e| format!("pin-state check: {e}"))?
            {
                count += 1;
            }
        }
        Ok(match count {
            0 => None,
            1 => Some(
                "1 release is only stored in the cloud — it will become unplayable \
                 until you reconnect."
                    .to_string(),
            ),
            n => Some(format!(
                "{n} releases are only stored in the cloud — they will become \
                 unplayable until you reconnect."
            )),
        })
    }

    /// Build, start, and attach a sync manager. Used once at startup for a
    /// returning user with a configured cloud home: an unlocked key for an opaque
    /// home (`Some`), or no key for a browsable home (`None`). Shares the sync
    /// controller's outbox in-flight set and event channel with the sync loop's
    /// upload observer. Call before [`Self::start`].
    pub async fn attach_and_start_sync(
        &self,
        encryption_service: Option<EncryptionService>,
    ) -> Result<(), String> {
        self.sync.attach_and_start_sync(encryption_service).await
    }
}
