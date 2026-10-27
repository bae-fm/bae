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
        self.sync_status.snapshot().loop_running()
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

    /// The device's network may be back: the platform reported the network
    /// became available or changed, the app came to the foreground, or the
    /// device woke. coven ends every backoff that followed an attempt no
    /// provider answered — the connection's, operations', uploads' and
    /// downloads' — and runs a pass, so work blocked while the OS cut the
    /// network off resumes at once instead of sitting out a backoff of up to
    /// five minutes. Backoffs after a provider's answer and provider
    /// cooldowns hold. It requests a pass as [`Self::trigger_sync`] does.
    pub fn network_changed(&self) {
        self.database.network_changed();
    }

    /// The person asked to retry everything now, such as by pulling to
    /// refresh. coven ends every ordinary backoff, answered or not: the
    /// connection's, operations', uploads' and downloads', each keeping its
    /// failure count, and runs a pass. Provider cooldowns, failures that
    /// never retry, and waits for new credentials hold. Automatic triggers
    /// use [`Self::trigger_sync`], which leaves every backoff in place.
    ///
    /// The retry takes effect when called; the returned future resolves when
    /// the first pass after it ends, with what came of it.
    pub fn retry_now(
        &self,
    ) -> impl std::future::Future<Output = super::SyncRetryOutcome> + Send + 'static {
        let answer = self.database.retry_now();
        async move { answer.await.into() }
    }

    pub async fn save_s3_config(
        &self,
        data: S3ConfigData,
        device_name: &str,
    ) -> Result<coven::StorageSetup, LibraryError> {
        self.sync.save_s3_config(data, device_name).await
    }

    #[cfg(feature = "oauth-providers")]
    pub async fn sign_in(
        &self,
        provider: crate::config::CloudProvider,
    ) -> Result<(), LibraryError> {
        self.sync.sign_in(provider).await
    }

    #[cfg(feature = "oauth-providers")]
    pub async fn sign_in_again(
        &self,
        device_name: &str,
    ) -> Result<coven::StorageSetup, LibraryError> {
        self.sync.sign_in_again(device_name).await
    }

    #[cfg(feature = "oauth-providers")]
    pub async fn setup_oauth_storage(
        &self,
        storage: coven::StorageConfig,
        device_name: &str,
    ) -> Result<coven::StorageSetup, LibraryError> {
        self.sync.setup_oauth_storage(storage, device_name).await
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
    /// A release counts as pinned only when every one of its files is. Pin
    /// state belongs to coven: the count reads the pinned count local
    /// triggers keep per release from coven's `coven_pins` table.
    pub async fn cloud_only_release_count(&self) -> Result<u64, LibraryError> {
        Ok(self.database.cloud_only_release_count().await?)
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
    /// pending and abandoned subscriptions so the connection's first events
    /// have consumers.
    /// The task runs on the runtime retained by the app and is supervised like
    /// the manager's other background workers.
    pub fn start_sync_at_startup(&self) -> Result<(), LibraryError> {
        let manager = self.clone();
        self.spawn_supervised_task("startup sync attachment", async move {
            manager.attach_and_start_sync_at_startup().await;
        })
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
            self.set_sync_error(Some(&error));
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
        if !self.is_sync_ready() {
            if let Err(error) = self.attach_and_start_sync().await {
                warn!("sync reconnect failed: {error}");
                self.set_sync_error(Some(&error));
                return Err(error);
            }
        }
        self.set_sync_error(None);
        self.trigger_sync();
        Ok(())
    }

    /// Write the sync-status error the status line reads, in the failure's own
    /// category, as the sync loop's failures are: a locked keychain reads as
    /// locked, a refused sign-in as credentials. `None` clears it.
    fn set_sync_error(&self, error: Option<&LibraryError>) {
        self.sync_status
            .set_error(error.map(|error| crate::ui::UiError::diagnostic(error.category(), error)));
    }
}

impl LibraryManager {
    pub async fn detect_s3_settings(
        &self,
        storage: coven::StorageConfig,
        credentials: coven::S3Credentials,
    ) -> Result<coven::S3Detection, LibraryError> {
        Ok(self
            .database
            .detect_s3_settings(storage, credentials)
            .await?)
    }
    pub async fn storage_location(&self) -> Result<Option<coven::StorageConfig>, LibraryError> {
        Ok(self.database.storage_config().await?)
    }
}
