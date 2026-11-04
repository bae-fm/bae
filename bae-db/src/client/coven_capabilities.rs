use super::*;

/// Where a storage setup reports each step coven runs, as coven reports it.
pub type StorageSetupSteps<'a> =
    &'a (dyn Fn(coven::Progress<coven::StorageSetupStep>) + Send + Sync);

/// Where each of the release `?1`'s files stands for its pin: whether this
/// device keeps it whole, as `coven_pins` says, and while this process
/// downloads it to be kept, the file coven downloads, as `coven_downloading`
/// lists it.
const RELEASE_FILE_PINS: &str = "SELECT f.id, pin.file IS NOT NULL, downloading.file \
     FROM release_files AS f \
     LEFT JOIN coven_pins AS pin ON pin.file = f.file \
     LEFT JOIN coven_downloading AS downloading ON downloading.file = f.file \
     WHERE f.release_id = ?1";

/// [`RELEASE_FILE_PINS`] for `release_id`, by release file id.
fn release_file_pins_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<std::collections::BTreeMap<String, DbFilePin>, DbError> {
    Ok(sql
        .query(RELEASE_FILE_PINS, [release_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                DbFilePin { kept: row.get(1)?, downloading: row.get(2)? },
            ))
        })?
        .into_iter()
        .collect())
}

impl Database {
    pub fn store_id(&self) -> coven::StoreId {
        self.inner.handle.store_id()
    }

    pub async fn store_name(&self) -> Result<String, DbError> {
        self.inner.handle.store_name().await.map_err(DbError::from)
    }

    pub fn subscribe_store_name(&self) -> coven::LiveQuery<String> {
        self.inner.handle.subscribe_store_name()
    }

    /// Rename the store as an admin: the write journals the rename, which
    /// a pass claims, and records it to show (§5.6).
    pub async fn rename_store(&self, name: &str) -> Result<(), coven::SyncError> {
        let name = name.to_owned();
        let renamed = name.clone();
        self.admin_change(super::admin_operations::AdminTarget::LibraryName(renamed), move |sql| {
            Ok((sql.rename_store(&name)?, ()))
        })
        .await
    }

    /// Delete the store, discarding what this device hasn't uploaded; the
    /// person confirmed that loss from [`Self::unsent_before_removal`].
    pub async fn delete_store(&self) -> Result<(), coven::StoreDeletionError> {
        self.inner.handle.delete_store(coven::IfUnsent::Discard).await
    }

    /// What deleting the store from this device loses: the writes and files
    /// still waiting to upload to its storage. A store with no storage set up
    /// has nowhere it uploads to and loses its whole catalog instead, so
    /// nothing counts as unsent.
    pub async fn unsent_before_removal(&self) -> Result<coven::UnsentWork, coven::SyncError> {
        if self.inner.handle.storage_config().await?.is_none() {
            return Ok(coven::UnsentWork { writes: 0, files: 0 });
        }
        Ok(self.inner.handle.unsent_work().await?)
    }

    pub async fn storage_config(&self) -> Result<Option<coven::StorageConfig>, coven::SyncError> {
        self.inner.handle.storage_config().await
    }

    pub async fn pending(&self) -> Result<Vec<coven::PendingRecord>, DbError> {
        self.inner.handle.pending().await.map_err(DbError::from)
    }

    pub fn set_transfer_limits(&self, limits: coven::TransferLimits) {
        self.inner.handle.set_transfer_limits(limits)
    }

    pub fn transfer_limits(&self) -> coven::TransferLimits {
        self.inner.handle.transfer_limits()
    }

    /// Give back the pages SQLite keeps cached that no statement is using.
    pub async fn release_memory(&self) -> Result<(), DbError> {
        Ok(self.inner.handle.release_memory().await?)
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
        F: Fn(SqlReadContext<'_>) -> Result<R, CovenError> + Send + Sync + 'static,
        R: Clone + PartialEq + Send + 'static,
        T: Clone + PartialEq + Send + 'static,
    {
        let rows: crate::with_sync_state::RowsRead<R> = Arc::new(rows);
        let changes = {
            let rows = Arc::clone(&rows);
            self.inner.handle.subscribe(move |sql| rows(sql))
        };
        crate::WithSyncState::new(
            self.clone(),
            self.inner.handle.subscribe_sync_status(),
            changes,
            rows,
            make,
        )
    }

    /// One read of the store, its failure as coven reports it.
    pub(crate) async fn read_coven<R>(
        &self,
        read: impl FnOnce(SqlReadContext<'_>) -> Result<R, CovenError> + Send + 'static,
    ) -> Result<R, CovenError>
    where
        R: Send + 'static,
    {
        self.inner.handle.read(read).await
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
        F: FnOnce(SqlReadContext<'_>) -> Result<R, CovenError> + Send + 'static,
        R: Send + 'static,
    {
        let state = self.inner.handle.sync_status().await?;
        let rows = self.inner.handle.read(rows).await?;
        Ok(make(state, rows))
    }

    /// Stop the sync loop: returns once the running pass and transfers have
    /// finished and the provider client is released. A failed release is
    /// returned, and coven records it as the last pass's failure.
    pub async fn stop_sync(&self) -> Result<(), coven::SyncError> {
        self.inner.handle.stop_sync().await
    }

    pub async fn close(&self) -> Result<(), coven::SyncError> {
        self.inner.handle.close().await
    }

    pub fn sync_now(&self) {
        self.inner.handle.sync_now();
    }

    /// Retry now: the retry is in place once this returns, with its answer
    /// to await.
    pub async fn retry_now(&self) -> Result<coven::RetryAnswer, coven::SyncError> {
        self.inner.handle.retry_now().await
    }

    /// Idle passes at most `interval` apart until the guard drops.
    pub fn sync_often(&self, interval: std::time::Duration) -> coven::SyncOftenGuard {
        self.inner.handle.sync_often(interval)
    }

    pub async fn retry_pending_operation(
        &self,
        operation: coven::OperationId,
    ) -> Result<(), coven::OperationError> {
        self.inner.handle.retry_pending_operation(operation).await
    }

    pub async fn start_sync(&self) -> Result<(), coven::SyncError> {
        self.inner.handle.start_sync().await
    }

    pub async fn setup_s3_storage(
        &self,
        storage: coven::StorageConfig,
        device_name: &str,
        access_key: String,
        secret_key: coven::SecretText,
        steps: StorageSetupSteps<'_>,
    ) -> Result<coven::StorageSetup, coven::StorageSetupError> {
        self.inner
            .handle
            .setup_s3_storage(storage, device_name, access_key, secret_key, steps)
            .await
    }

    pub async fn setup_cloudkit_storage(
        &self,
        device_name: &str,
        steps: StorageSetupSteps<'_>,
    ) -> Result<coven::StorageSetup, coven::StorageSetupError> {
        self.inner.handle.setup_cloudkit_storage("iCloud.fm.bae.desktop", device_name, steps).await
    }

    #[cfg(feature = "oauth-providers")]
    pub async fn authenticate(
        &self,
        access: coven::OAuthAccess,
    ) -> Result<(), coven::StorageSetupError> {
        self.inner.handle.authenticate(access).await
    }

    #[cfg(feature = "oauth-providers")]
    pub async fn setup_oauth_storage(
        &self,
        storage: coven::StorageConfig,
        device_name: &str,
        steps: StorageSetupSteps<'_>,
    ) -> Result<coven::StorageSetup, coven::StorageSetupError> {
        self.inner.handle.setup_oauth_storage(storage, device_name, steps).await
    }

    #[cfg(feature = "oauth-providers")]
    pub async fn onedrive_root(&self) -> Result<coven::StorageFolder, coven::StorageSetupError> {
        self.inner.handle.onedrive_root().await
    }

    #[cfg(feature = "oauth-providers")]
    pub async fn onedrive_folders(
        &self,
        parent: &coven::StorageConfig,
    ) -> Result<Vec<coven::StorageFolder>, coven::StorageSetupError> {
        self.inner.handle.onedrive_folders(parent).await
    }

    #[cfg(feature = "oauth-providers")]
    pub async fn create_onedrive_folder(
        &self,
        parent: &coven::StorageConfig,
        name: &str,
    ) -> Result<coven::StorageFolder, coven::StorageSetupError> {
        self.inner.handle.create_onedrive_folder(parent, name).await
    }

    #[cfg(feature = "oauth-providers")]
    pub async fn dropbox_shared_folders(
        &self,
    ) -> Result<Vec<coven::StorageFolder>, coven::StorageSetupError> {
        self.inner.handle.dropbox_shared_folders().await
    }

    pub fn store_key_state(&self) -> Result<coven::StoreKeyState, coven::KeyError> {
        self.inner.handle.store_key_state()
    }

    pub async fn unlock_store_key(
        &self,
    ) -> Result<coven::ConnectedStorage, coven::StoreKeyUnlockError> {
        self.inner.handle.unlock_store_key().await
    }

    pub async fn forget_store_keys(&self) -> Result<(), coven::KeyError> {
        self.inner.handle.forget_store_keys().await
    }

    pub fn host_secret(&self, name: &str) -> Result<Option<String>, coven::KeyError> {
        self.inner.handle.host_secret(name)
    }

    pub fn set_host_secret(&self, name: &str, value: &str) -> Result<(), coven::KeyError> {
        self.inner.handle.set_host_secret(name, value)
    }

    #[cfg(desktop)]
    pub fn delete_host_secret(&self, name: &str) -> Result<(), coven::KeyError> {
        self.inner.handle.delete_host_secret(name)
    }

    /// This device's id in the store, which a file on one device only names
    /// when it is this device's.
    pub fn device_id(&self) -> coven::DeviceId {
        self.inner.handle.device_id()
    }

    pub async fn file_ref(&self, file: coven::FileId) -> Result<coven::FileRef, coven::DbError> {
        self.inner.handle.file_ref(file).await
    }

    pub async fn read_file(&self, file: &coven::FileRef) -> Result<Vec<u8>, coven::FileReadError> {
        self.inner.handle.read_file(file).await
    }

    pub(crate) async fn open_file_stream(
        &self,
        file: &coven::FileRef,
    ) -> Result<coven::FileStream, coven::FileReadError> {
        self.inner.handle.open_file_stream(file).await
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
        self.inner.handle.prepare_device_only(folder, files, cancel, on_progress).await
    }

    pub async fn pin(&self, files: &[coven::FileRef]) -> Result<(), coven::FileReadError> {
        self.inner.handle.pin(files).await
    }

    /// Where each of the release's files stands for its pin now, by release
    /// file id.
    pub async fn release_file_pins(
        &self,
        release_id: &str,
    ) -> Result<std::collections::BTreeMap<String, DbFilePin>, DbError> {
        let release_id = release_id.to_string();
        self.read(move |sql| release_file_pins_on(&sql, &release_id)).await
    }

    /// Where each of the release's files stands for its pin, live: it reruns
    /// as a file is kept and as a download to keep one starts or ends.
    pub fn subscribe_release_file_pins(
        &self,
        release_id: &str,
    ) -> coven::LiveQuery<std::collections::BTreeMap<String, DbFilePin>> {
        let release_id = release_id.to_string();
        self.inner.handle.subscribe(move |sql| Ok(release_file_pins_on(&sql, &release_id)?))
    }

    /// How far each file this process is downloading to keep whole has got,
    /// by file, from coven's download watch.
    pub fn subscribe_download_file_transfers(&self) -> coven::TransferWatch {
        self.inner.handle.subscribe_download_transfers()
    }

    pub async fn unpin(&self, files: &[coven::FileId]) -> Result<(), DbError> {
        self.inner.handle.unpin(files).await.map_err(DbError::from)
    }

    pub async fn files_pinned(
        &self,
        files: &[coven::FileId],
    ) -> Result<Vec<Option<bool>>, DbError> {
        self.inner.handle.files_pinned(files).await.map_err(DbError::from)
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub async fn evict_file(&self, file: coven::FileId) -> Result<(), DbError> {
        self.inner.handle.evict_file(file).await.map_err(DbError::from)
    }

    /// coven's whole upload queue, for a test's drain to count what left it.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn uploads(&self) -> Result<coven::UploadQueue, DbError> {
        self.inner.handle.uploads().await.map_err(DbError::from)
    }

    pub async fn set_uploads_paused(&self, paused: bool) -> Result<(), DbError> {
        self.inner.handle.set_uploads_paused(paused).await.map_err(DbError::from)
    }

    pub async fn retry_uploads_now(&self) -> Result<coven::DrainOutcome, coven::SyncError> {
        self.inner.handle.retry_uploads_now().await
    }

    pub async fn generate_restore_code(&self) -> Result<String, coven::SyncError> {
        self.inner.handle.restore_code().await
    }

    pub async fn restore_code_for_transfer(&self) -> Result<String, coven::SyncError> {
        self.inner.handle.restore_code_for_transfer().await
    }

    /// Remove a member: the write journals the removal, which a pass
    /// claims, and records it to show (§5.6).
    pub async fn remove_member(&self, member: &coven::MemberId) -> Result<(), coven::SyncError> {
        let member = *member;
        self.admin_change(super::admin_operations::AdminTarget::Person(member), move |sql| {
            Ok((sql.remove_member(&member)?, ()))
        })
        .await
    }

    /// Remove a device, journaled and recorded as a removal is, and say
    /// what is left to do at the provider.
    pub async fn remove_device(
        &self,
        device: coven::DeviceId,
    ) -> Result<Option<coven::ProviderSignOut>, coven::SyncError> {
        self.admin_change(super::admin_operations::AdminTarget::Device(device), move |sql| {
            sql.remove_device(device)
        })
        .await
    }

    /// Change a member's role, journaled and recorded as a removal is.
    pub async fn set_member_role(
        &self,
        member: &coven::MemberId,
        role: coven::MemberRole,
    ) -> Result<(), coven::SyncError> {
        let member = *member;
        self.admin_change(super::admin_operations::AdminTarget::Role(member, role), move |sql| {
            Ok((sql.set_member_role(&member, role)?, ()))
        })
        .await
    }

    /// Invite a new person: the write journals the invite, offline too,
    /// and a pass grants its access and fixes its code (§5.6).
    pub async fn invite_person(
        &self,
        role: coven::MemberRole,
        name: String,
        access: coven::InviteAccess,
        delivery: coven::InviteDelivery,
    ) -> Result<coven::InviteId, coven::SyncError> {
        let (invite, _member, _operation) = self
            .inner
            .handle
            .create_invite(coven::InviteTarget::NewMember { role, name }, access, delivery)
            .await?;
        Ok(invite)
    }

    /// Approve a sent invite's request: the write journals the approval,
    /// which a pass binds, and records it to show by the name the invite
    /// gave its person (§5.6).
    pub async fn approve_join_request(
        &self,
        request: &coven::JoinRequest,
    ) -> Result<(), coven::SyncError> {
        let invite = request.invite;
        let invitee = self
            .inner
            .handle
            .read(coven::Invite::list)
            .await?
            .into_iter()
            .find(|open| open.id == invite)
            .and_then(|open| open.invitee);
        let request = request.clone();
        self.admin_change(
            super::admin_operations::AdminTarget::Approval(request.member, invitee),
            move |sql| Ok((Some(sql.approve_join_request(&request)?), ())),
        )
        .await
    }

    pub async fn decline_join_request(
        &self,
        request: &coven::JoinRequest,
    ) -> Result<(), coven::SyncError> {
        self.inner.handle.decline_join_request(request).await
    }

    pub async fn cancel_invite(&self, invite: &coven::InviteId) -> Result<(), coven::SyncError> {
        self.inner.handle.cancel_invite(invite).await
    }
}

impl Database {
    pub async fn detect_s3_settings(
        &self,
        storage: coven::StorageConfig,
        credentials: coven::S3Credentials,
    ) -> Result<coven::S3Detection, coven::StorageSetupError> {
        self.inner
            .handle
            .detect_s3_settings(storage, credentials.access_key_id, credentials.secret_access_key)
            .await
    }
}
