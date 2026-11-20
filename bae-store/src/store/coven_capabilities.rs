use crate::store::Store;
use crate::DbError;
use coven::SqlReadContext;

/// Whether the person paused uploads, as coven's one row says.
pub const UPLOADS_PAUSED: &str = "SELECT paused FROM coven_upload_pause";

/// Whether the person has files move only on unmetered networks, as coven's
/// network row says.
pub const UNMETERED_ONLY: &str = "SELECT unmetered_only FROM coven_network";

/// The location recorded on this device with its account and folder labels,
/// as coven's sync status row holds it, none while no storage is set up; then
/// this person's access as the library records it, their member row's
/// `account` and `access_key_id`. The location reads as
/// `Option<coven::StorageConfig>`, and the two columns after it as
/// `coven::MemberAccess::from_columns` reads them.
pub const RECORDED_STORAGE: &str = "SELECT s.location, m.account, m.access_key_id \
     FROM coven_sync_status AS s LEFT JOIN coven_members AS m ON m.is_self = 1";

/// What this device holds that the store's storage, `storage`, lacks, from
/// `queued`, the writes and files coven's queues hold: all of them while
/// storage is set up. A store with no storage set up has nowhere it uploads
/// to, so nothing counts as unsent: deleting it loses its whole catalog
/// instead, and its standing is its sync state. coven queues every write
/// whether or not storage is set up, such as the creator's profile, which the
/// store's creation writes.
pub fn unsent_to_storage(
    storage: Option<&coven::StorageConfig>,
    queued: coven::UnsentWork,
) -> coven::UnsentWork {
    match storage {
        Some(_) => queued,
        None => coven::UnsentWork { writes: 0, files: 0 },
    }
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

    /// Delete the store, discarding what this device hasn't uploaded; the
    /// person confirmed that loss from [`Self::unsent_before_removal`].
    pub async fn delete_store(&self) -> Result<(), coven::StoreDeletionError> {
        self.reported(self.inner.handle.delete_store(coven::IfUnsent::Discard).await)
    }

    /// What deleting the store from this device loses: the writes and files
    /// still waiting to upload to its storage, as [`unsent_to_storage`]
    /// counts them.
    pub async fn unsent_before_removal(&self) -> Result<coven::UnsentWork, DbError> {
        let storage = self
            .reported(self.inner.handle.storage_config().await)
            .map_err(|error| DbError::from(coven::CovenError::Sync(error)))?;
        let queued = self.reported(self.inner.handle.unsent_work().await)?;
        Ok(unsent_to_storage(storage.as_ref(), queued))
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

    /// coven's sync state now, then again after each change to it: a live
    /// query of coven's view `coven_sync_state`.
    pub fn subscribe_coven_sync_status(
        &self,
    ) -> coven::LiveQuery<coven::SyncStatus, coven::CovenError> {
        self.inner.handle.subscribe(|sql| Ok(coven::SyncStatus::read(sql)?))
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

    pub async fn start_sync(&self) -> Result<(), coven::SyncError> {
        self.reported(self.inner.handle.start_sync().await)
    }

    pub async fn setup_s3_storage(
        &self,
        storage: coven::StorageConfig,
        access_key: String,
        secret_key: coven::SecretText,
        steps: StorageSetupSteps<'_>,
    ) -> Result<coven::StorageSetup, coven::StorageSetupError> {
        self.reported(
            self.inner.handle.setup_s3_storage(storage, access_key, secret_key, steps).await,
        )
    }

    /// Check the S3 key `access_key_id` with the library's S3 storage and,
    /// once the check passes, use it in place of the one this person's
    /// access records. `cancel` stops the check; once it passed, the call
    /// finishes.
    pub async fn use_access_key(
        &self,
        access_key_id: String,
        secret_access_key: coven::SecretText,
        cancel: &tokio::sync::watch::Receiver<bool>,
    ) -> Result<coven::AccessKeyUsed, coven::UseAccessKeyError> {
        self.reported(
            self.inner.handle.use_access_key(access_key_id, secret_access_key, cancel).await,
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
        steps: StorageSetupSteps<'_>,
    ) -> Result<coven::StorageSetup, coven::StorageSetupError> {
        self.reported(
            self.inner.handle.setup_cloudkit_storage("iCloud.fm.bae.desktop", steps).await,
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
        steps: StorageSetupSteps<'_>,
    ) -> Result<coven::StorageSetup, coven::StorageSetupError> {
        self.reported(self.inner.handle.setup_oauth_storage(storage, steps).await)
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

    #[cfg(any(test, feature = "test-utils"))]
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
    pub async fn user_file(&self, file: coven::FileId) -> Result<Option<coven::UserFile>, DbError> {
        Ok(self.reported(self.inner.handle.user_file(file).await)?)
    }

    /// Read a whole file, in one place of `lease`.
    pub async fn read_file(
        &self,
        file: &coven::FileRef,
        lease: &coven::FileLease,
    ) -> Result<Vec<u8>, coven::FileReadError> {
        self.reported(self.inner.handle.read_file(file, lease).await)
    }

    /// Open a file for reading ranges. The stream holds one place of `lease`
    /// until it drops.
    pub async fn open_file_stream(
        &self,
        file: &coven::FileRef,
        lease: &coven::FileLease,
    ) -> Result<crate::FileStream, coven::FileReadError> {
        let stream = self.reported(self.inner.handle.open_file_stream(file, lease).await)?;
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

    /// [`Self::local_bytes`] now and after each write that changes it, for a
    /// test that waits on what a pin leaves on this device.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn subscribe_local_bytes(&self) -> coven::LiveQuery<coven::LocalBytes, coven::CovenError> {
        self.inner.handle.subscribe_local_bytes()
    }

    /// Follow how many reads of files wait for the storage connection that
    /// is coming, for a test that goes on once a read waits.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn reads_awaiting_connection(&self) -> tokio::sync::watch::Receiver<usize> {
        self.inner.handle.test_reads_awaiting_connection()
    }

    /// Delete the installations resets kept on this device.
    pub async fn delete_discarded_devices(&self) -> Result<(), DbError> {
        self.reported(self.inner.handle.delete_discarded_devices().await).map_err(DbError::from)
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
        let (invite, _member, _operation) =
            self.reported(self.inner.handle.create_invite(role, access, lifetime, inviter).await)?;
        Ok(invite)
    }

    /// Start a pairing to add another of this person's devices: its id and
    /// the codes to show. On S3 the QR code carries the key this device
    /// connects with and the shared code carries none (coven §13.2).
    pub async fn add_device(&self) -> Result<coven::AddDevice, coven::SyncError> {
        self.reported(self.inner.handle.add_device().await)
    }

    /// Where the pairing is, live: waiting, with the checked request once
    /// one has come, approved, or ended; none once it is forgotten.
    pub fn follow_add_device(
        &self,
        pairing: coven::AddDeviceId,
    ) -> coven::LiveQuery<Option<coven::AddDeviceStatus>, coven::CovenError> {
        self.inner.handle.follow_add_device(pairing)
    }

    pub async fn approve_add_device(
        &self,
        request: &coven::AddDeviceRequest,
    ) -> Result<(), coven::SyncError> {
        self.reported(self.inner.handle.approve_add_device(request).await)
    }

    pub async fn decline_add_device(
        &self,
        request: &coven::AddDeviceRequest,
    ) -> Result<(), coven::SyncError> {
        self.reported(self.inner.handle.decline_add_device(request).await)
    }

    /// The screen showing the pairing's code closed: end the pairing and
    /// forget it.
    pub async fn end_add_device(
        &self,
        pairing: coven::AddDeviceId,
    ) -> Result<(), coven::SyncError> {
        self.reported(self.inner.handle.end_add_device(pairing).await)
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

    /// Whether this device's member reaches storage through the account that
    /// holds the store (coven §20.9).
    pub async fn holds_store(&self) -> Result<bool, coven::SyncError> {
        self.reported(self.inner.handle.holds_store().await)
    }

    /// Tell this device that the person did the provider step for a removed
    /// device of theirs, so nothing it sent can land any more: this device
    /// takes back the accounts that device's shares named which nothing
    /// intends now (coven §14.2, §20.9). Fails with
    /// `DbError::ProviderRequestRunning` while a request to one of those
    /// accounts runs, changing nothing.
    pub async fn confirm_device_cut_off(
        &self,
        device: coven::DeviceId,
    ) -> Result<(), coven::SyncError> {
        self.reported(self.inner.handle.confirm_device_cut_off(device).await)
    }
}
