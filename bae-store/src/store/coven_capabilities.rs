use crate::store::Store;
use crate::DbError;
use coven::SqlReadContext;
use std::sync::Arc;

/// Whether the person paused uploads, as coven's one row says.
pub const UPLOADS_PAUSED: &str = "SELECT paused FROM coven_upload_pause";

/// Whether the person has files move only on unmetered networks, as coven's
/// network row says.
pub const UNMETERED_ONLY: &str = "SELECT unmetered_only FROM coven_network";

/// Whether storage is set up on this device, as coven's sync status says:
/// what a release's storage choices and markers are read against, beside the
/// library's own rows.
pub fn syncs_on(sql: &SqlReadContext<'_>) -> Result<bool, DbError> {
    Ok(sql.query_row("SELECT storage = 'connected' FROM coven_sync_status", [], |row| row.get(0))?)
}

/// Where a storage setup reports each step coven runs, as coven reports it.
pub type StorageSetupSteps<'a> =
    &'a (dyn Fn(coven::Progress<coven::StorageSetupStep>) + Send + Sync);

impl Store {
    pub fn store_id(&self) -> coven::StoreId {
        self.inner.handle.store_id()
    }

    pub async fn store_name(&self) -> Result<String, DbError> {
        self.reported(self.inner.handle.store_name().await).map_err(DbError::from)
    }

    pub fn subscribe_store_name(&self) -> coven::LiveQuery<String, coven::CovenError> {
        self.inner.handle.subscribe_store_name()
    }

    /// The person's name the invite carried, which coven keeps on this
    /// device with a library a join published until a write acknowledges it
    /// (`CovenWrite::acknowledge_joined_invitee`); none for any other
    /// library, and once acknowledged.
    pub async fn joined_invitee(&self) -> Result<Option<String>, DbError> {
        self.reported(self.inner.handle.joined_invitee().await).map_err(DbError::from)
    }

    /// Delete the store, discarding what this device hasn't uploaded; the
    /// person confirmed that loss from [`Self::unsent_before_removal`].
    pub async fn delete_store(&self) -> Result<(), coven::StoreDeletionError> {
        self.reported(self.inner.handle.delete_store(coven::IfUnsent::Discard).await)
    }

    /// What deleting the store from this device loses: the writes and files
    /// still waiting to upload to its storage. A store with no storage set up
    /// has nowhere it uploads to and loses its whole catalog instead, so
    /// nothing counts as unsent.
    pub async fn unsent_before_removal(&self) -> Result<coven::UnsentWork, DbError> {
        let storage = self
            .reported(self.inner.handle.storage_config().await)
            .map_err(|error| DbError::from(coven::CovenError::Sync(error)))?;
        if storage.is_none() {
            return Ok(coven::UnsentWork { writes: 0, files: 0 });
        }
        Ok(self.reported(self.inner.handle.unsent_work().await)?)
    }

    pub async fn storage_config(&self) -> Result<Option<coven::StorageConfig>, coven::SyncError> {
        self.reported(self.inner.handle.storage_config().await)
    }

    pub async fn pending(&self) -> Result<Vec<coven::PendingRecord>, DbError> {
        self.reported(self.inner.handle.pending().await).map_err(DbError::from)
    }

    pub fn set_transfer_limits(&self, limits: coven::TransferLimits) {
        self.inner.handle.set_transfer_limits(limits)
    }

    pub fn transfer_limits(&self) -> coven::TransferLimits {
        self.inner.handle.transfer_limits()
    }

    /// Give back the pages SQLite keeps cached that no statement is using.
    pub async fn release_memory(&self) -> Result<(), DbError> {
        Ok(self.reported(self.inner.handle.release_memory().await)?)
    }

    /// coven's sync state followed beside the read `rows` of the library's
    /// tables, each pair made into one value by `make`, the rows read after
    /// the state they are put with.
    pub fn with_sync_state<F, R, T>(
        &self,
        rows: F,
        make: fn(coven::SyncStatus, R) -> T,
    ) -> crate::WithSyncState<R, T>
    where
        F: Fn(SqlReadContext<'_>) -> Result<R, DbError> + Send + Sync + 'static,
        R: Clone + PartialEq + Send + 'static,
        T: Clone + PartialEq + Send + 'static,
    {
        let rows: crate::with_sync_state::RowsRead<R> = Arc::new(rows);
        let changes = {
            let rows = Arc::clone(&rows);
            self.inner.handle.subscribe(move |sql| rows(sql))
        };
        crate::WithSyncState::new(
            self.inner.handle.clone(),
            self.inner.handle.subscribe_sync_status(),
            changes,
            rows,
            make,
        )
    }

    /// One read of the store, failing with the closure's own error.
    pub async fn read_coven<R, E>(
        &self,
        read: impl FnOnce(SqlReadContext<'_>) -> Result<R, E> + Send + 'static,
    ) -> Result<R, E>
    where
        R: Send + 'static,
        E: From<coven::DbError> + coven::task::CarriesBug + Send + 'static,
    {
        self.reported(self.inner.handle.read(read).await)
    }

    /// coven's sync state now beside one read `rows` of the library's
    /// tables made after it, made into one value by `make`, as
    /// [`Self::with_sync_state`] makes each.
    pub async fn read_with_sync_state<F, R, T>(
        &self,
        rows: F,
        make: fn(coven::SyncStatus, R) -> T,
    ) -> Result<T, DbError>
    where
        F: FnOnce(SqlReadContext<'_>) -> Result<R, DbError> + Send + 'static,
        R: Send + 'static,
    {
        let state = self.reported(self.inner.handle.sync_status().await)?;
        let rows = self.reported(self.inner.handle.read(rows).await)?;
        Ok(make(state, rows))
    }

    /// Stop the sync loop: returns once the running pass and transfers have
    /// finished and the provider client is released. A failed release is
    /// returned, and coven records it as the last pass's failure.
    pub async fn stop_sync(&self) -> Result<(), coven::SyncError> {
        self.reported(self.inner.handle.stop_sync().await)
    }

    pub async fn close(&self) -> Result<(), coven::SyncError> {
        self.reported(self.inner.handle.close().await)
    }

    pub fn sync_now(&self) {
        self.inner.handle.sync_now();
    }

    /// Retry now: the retry is in place once this returns, with its answer
    /// to await.
    pub async fn retry_now(&self) -> Result<coven::RetryAnswer, coven::SyncError> {
        self.reported(self.inner.handle.retry_now().await)
    }

    /// Idle passes at most `interval` apart until the guard drops.
    pub fn sync_often(&self, interval: std::time::Duration) -> coven::SyncOftenGuard {
        self.inner.handle.sync_often(interval)
    }

    pub async fn retry_pending_operation(
        &self,
        operation: coven::OperationId,
    ) -> Result<(), coven::OperationError> {
        self.reported(self.inner.handle.retry_pending_operation(operation).await)
    }

    pub async fn start_sync(&self) -> Result<(), coven::SyncError> {
        self.reported(self.inner.handle.start_sync().await)
    }

    pub async fn setup_s3_storage(
        &self,
        storage: coven::StorageConfig,
        device_name: &str,
        access_key: String,
        secret_key: coven::SecretText,
        steps: StorageSetupSteps<'_>,
    ) -> Result<coven::StorageSetup, coven::StorageSetupError> {
        self.reported(
            self.inner
                .handle
                .setup_s3_storage(storage, device_name, access_key, secret_key, steps)
                .await,
        )
    }

    /// Cancel the storage setup running on this store: before its point of
    /// no return it is undone and its call fails as cancelled; past it, it
    /// finishes.
    pub fn cancel_storage_setup(&self) -> coven::SetupCancellation {
        self.inner.handle.cancel_storage_setup()
    }

    pub async fn setup_cloudkit_storage(
        &self,
        device_name: &str,
        steps: StorageSetupSteps<'_>,
    ) -> Result<coven::StorageSetup, coven::StorageSetupError> {
        self.reported(
            self.inner
                .handle
                .setup_cloudkit_storage("iCloud.fm.bae.desktop", device_name, steps)
                .await,
        )
    }

    #[cfg(feature = "oauth-providers")]
    pub async fn authenticate(
        &self,
        access: coven::OAuthAccess,
    ) -> Result<(), coven::StorageSetupError> {
        self.reported(self.inner.handle.authenticate(access).await)
    }

    #[cfg(feature = "oauth-providers")]
    pub async fn setup_oauth_storage(
        &self,
        storage: coven::StorageConfig,
        device_name: &str,
        steps: StorageSetupSteps<'_>,
    ) -> Result<coven::StorageSetup, coven::StorageSetupError> {
        self.reported(self.inner.handle.setup_oauth_storage(storage, device_name, steps).await)
    }

    #[cfg(feature = "oauth-providers")]
    pub async fn onedrive_root(&self) -> Result<coven::StorageFolder, coven::StorageSetupError> {
        self.reported(self.inner.handle.onedrive_root().await)
    }

    #[cfg(feature = "oauth-providers")]
    pub async fn onedrive_folders(
        &self,
        parent: &coven::StorageConfig,
    ) -> Result<Vec<coven::StorageFolder>, coven::StorageSetupError> {
        self.reported(self.inner.handle.onedrive_folders(parent).await)
    }

    #[cfg(feature = "oauth-providers")]
    pub async fn create_onedrive_folder(
        &self,
        parent: &coven::StorageConfig,
        name: &str,
    ) -> Result<coven::StorageFolder, coven::StorageSetupError> {
        self.reported(self.inner.handle.create_onedrive_folder(parent, name).await)
    }

    #[cfg(feature = "oauth-providers")]
    pub async fn dropbox_shared_folders(
        &self,
    ) -> Result<Vec<coven::StorageFolder>, coven::StorageSetupError> {
        self.reported(self.inner.handle.dropbox_shared_folders().await)
    }

    #[cfg(feature = "oauth-providers")]
    pub async fn create_dropbox_shared_folder(
        &self,
        name: &str,
    ) -> Result<coven::StorageFolder, coven::StorageSetupError> {
        self.reported(self.inner.handle.create_dropbox_shared_folder(name).await)
    }

    pub fn store_key_state(&self) -> Result<coven::StoreKeyState, coven::KeyError> {
        self.reported(self.inner.handle.store_key_state())
    }

    pub async fn unlock_store_key(
        &self,
    ) -> Result<coven::ConnectedStorage, coven::StoreKeyUnlockError> {
        self.reported(self.inner.handle.unlock_store_key().await)
    }

    pub async fn forget_store_keys(&self) -> Result<(), coven::KeyError> {
        self.reported(self.inner.handle.forget_store_keys().await)
    }

    pub fn host_secret(&self, name: &str) -> Result<Option<String>, coven::KeyError> {
        self.reported(self.inner.handle.host_secret(name))
    }

    pub fn set_host_secret(&self, name: &str, value: &str) -> Result<(), coven::KeyError> {
        self.reported(self.inner.handle.set_host_secret(name, value))
    }

    #[cfg(desktop)]
    pub fn delete_host_secret(&self, name: &str) -> Result<(), coven::KeyError> {
        self.reported(self.inner.handle.delete_host_secret(name))
    }

    /// This device's id in the store, which a file on one device only names
    /// when it is this device's.
    pub fn device_id(&self) -> coven::DeviceId {
        self.inner.handle.device_id()
    }

    /// coven's record of the file `file`, as this device reads it now.
    pub async fn file_ref(&self, file: coven::FileId) -> Result<coven::FileRef, DbError> {
        Ok(self.reported(self.inner.handle.file_ref(file).await)?)
    }

    /// The original coven records on this device for `file`: the person's
    /// file it was imported from, or the copy making a release local again
    /// wrote.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn user_file(&self, file: coven::FileId) -> Result<Option<coven::UserFile>, DbError> {
        Ok(self.reported(self.inner.handle.user_file(file).await)?)
    }

    pub async fn read_file(&self, file: &coven::FileRef) -> Result<Vec<u8>, coven::FileReadError> {
        self.reported(self.inner.handle.read_file(file).await)
    }

    pub async fn open_file_stream(
        &self,
        file: &coven::FileRef,
    ) -> Result<crate::FileStream, coven::FileReadError> {
        let stream = self.reported(self.inner.handle.open_file_stream(file).await)?;
        Ok(crate::FileStream::new(stream, self.inner.bugs.clone()))
    }

    /// Download these cloud files to their paths under `folder`, for a write
    /// to switch them all to this device only with coven's
    /// `make_device_only`, telling `on_progress` how far each file
    /// and the whole call have come. A failure or a cancel removes every copy
    /// and folder the call made.
    pub async fn prepare_device_only(
        &self,
        folder: &std::path::Path,
        files: Vec<(coven::FileId, coven::RelativePath)>,
        cancel: &tokio::sync::watch::Receiver<bool>,
        on_progress: impl Fn(coven::Progress<coven::DeviceOnlyStep>),
    ) -> Result<coven::PreparedDeviceOnly, coven::DeviceOnlyError> {
        self.reported(
            self.inner.handle.prepare_device_only(folder, files, cancel, on_progress).await,
        )
    }

    /// Download what each of `files` lacks into a whole copy, pinning nothing:
    /// a write commits the result with `CovenWrite::pin`, beside whatever
    /// else it must commit with the pin.
    pub async fn prepare_pin(
        &self,
        files: &[coven::FileRef],
    ) -> Result<coven::PreparedPin, coven::FileReadError> {
        self.reported(self.inner.handle.prepare_pin(files).await)
    }

    /// Pin `files` in a write of their own, for a test that has no queue
    /// entry to end with them.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn pin(&self, files: &[coven::FileRef]) -> Result<(), coven::FileReadError> {
        self.reported(self.inner.handle.pin(files).await)
    }

    /// How far each file this process is downloading to keep whole has got,
    /// by file, from coven's download watch.
    pub fn subscribe_download_file_transfers(&self) -> coven::TransferWatch {
        self.inner.handle.subscribe_download_transfers()
    }

    pub async fn unpin(&self, files: &[coven::FileId]) -> Result<(), DbError> {
        self.reported(self.inner.handle.unpin(files).await).map_err(DbError::from)
    }

    /// What this device keeps for the store, in bytes: each namespace's cache
    /// and pinned copies, partial downloads, the store's own files and the
    /// archives resets kept.
    pub async fn local_bytes(&self) -> Result<coven::LocalBytes, DbError> {
        self.reported(self.inner.handle.local_bytes().await).map_err(DbError::from)
    }

    pub async fn files_pinned(
        &self,
        files: &[coven::FileId],
    ) -> Result<Vec<Option<bool>>, DbError> {
        self.reported(self.inner.handle.files_pinned(files).await).map_err(DbError::from)
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub async fn evict_file(&self, file: coven::FileId) -> Result<(), DbError> {
        self.reported(self.inner.handle.evict_file(file).await).map_err(DbError::from)
    }

    /// coven's whole upload queue, for a test's drain to count what left it.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn uploads(&self) -> Result<coven::UploadQueue, DbError> {
        self.reported(self.inner.handle.uploads().await).map_err(DbError::from)
    }

    /// Whether the person paused uploads now.
    pub async fn uploads_paused(&self) -> Result<bool, DbError> {
        self.read(|sql| sql.query_row(UPLOADS_PAUSED, [], |row| row.get(0)).map_err(DbError::from))
            .await
    }

    /// Whether files move only on unmetered networks now.
    pub async fn unmetered_only(&self) -> Result<bool, DbError> {
        self.read(|sql| sql.query_row(UNMETERED_ONLY, [], |row| row.get(0)).map_err(DbError::from))
            .await
    }

    /// How far each file this process is sending has got, by file, from
    /// coven's upload watch.
    pub fn subscribe_upload_transfers(&self) -> coven::TransferWatch {
        self.inner.handle.subscribe_upload_transfers()
    }

    /// Whether storage is set up on this device now.
    pub async fn syncs(&self) -> Result<bool, DbError> {
        self.read(|sql| syncs_on(&sql)).await
    }

    pub async fn set_uploads_paused(&self, paused: bool) -> Result<(), DbError> {
        self.reported(self.inner.handle.set_uploads_paused(paused).await).map_err(DbError::from)
    }

    /// Move files only on unmetered networks, or on any.
    pub async fn set_unmetered_only(&self, unmetered_only: bool) -> Result<(), DbError> {
        self.reported(self.inner.handle.set_unmetered_only(unmetered_only).await)
            .map_err(DbError::from)
    }

    pub async fn retry_uploads_now(&self) -> Result<coven::DrainOutcome, coven::SyncError> {
        self.reported(self.inner.handle.retry_uploads_now().await)
    }

    pub async fn generate_restore_code(&self) -> Result<String, coven::SyncError> {
        self.reported(self.inner.handle.restore_code().await)
    }

    pub async fn restore_code_for_transfer(&self) -> Result<String, coven::SyncError> {
        self.reported(self.inner.handle.restore_code_for_transfer().await)
    }

    /// Invite a new person: the write journals the invite, offline too,
    /// and a pass grants its access and fixes its code (§5.6). Re-inviting
    /// creates a fresh identity; the removed person's rows retain their
    /// owner. The invite carries no name for the person: they type theirs
    /// when they join (§5.4). `inviter` is the inviting admin's name, which
    /// the joining person's screens show.
    pub async fn invite_person(
        &self,
        role: coven::MemberRole,
        access: coven::InviteAccess,
        lifetime: coven::InviteLifetime,
        inviter: String,
    ) -> Result<coven::InviteId, coven::SyncError> {
        let (invite, _member, _operation) = self.reported(
            self.inner
                .handle
                .create_invite(coven::InviteTarget::NewMember { role }, access, lifetime, inviter)
                .await,
        )?;
        Ok(invite)
    }

    pub async fn decline_join_request(
        &self,
        request: &coven::JoinRequest,
    ) -> Result<(), coven::SyncError> {
        self.reported(self.inner.handle.decline_join_request(request).await)
    }

    pub async fn cancel_invite(&self, invite: &coven::InviteId) -> Result<(), coven::SyncError> {
        self.reported(self.inner.handle.cancel_invite(invite).await)
    }

    /// Tell this device that the person did the provider step for a removed
    /// device of theirs, so nothing it sent can land any more: this device
    /// takes back the accounts that device's shares named which nothing
    /// intends now (coven §14.2, §20.9). Fails with
    /// `DbError::ProviderRequestRunning` while a request to one of those
    /// accounts runs, changing nothing.
    /// Whether this device's member reaches storage through the account that
    /// holds the store (coven §20.9).
    pub async fn holds_store(&self) -> Result<bool, coven::SyncError> {
        self.reported(self.inner.handle.holds_store().await)
    }

    pub async fn confirm_device_cut_off(
        &self,
        device: coven::DeviceId,
    ) -> Result<(), coven::SyncError> {
        self.reported(self.inner.handle.confirm_device_cut_off(device).await)
    }
}
