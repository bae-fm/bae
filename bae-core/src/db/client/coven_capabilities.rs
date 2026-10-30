use super::*;

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
                DbFilePin {
                    kept: row.get(1)?,
                    downloading: row.get(2)?,
                },
            ))
        })?
        .into_iter()
        .collect())
}

impl Database {
    pub(crate) fn store_id(&self) -> coven::StoreId {
        self.inner.handle.store_id()
    }

    pub(crate) async fn store_name(&self) -> Result<String, DbError> {
        self.inner.handle.store_name().await.map_err(DbError::from)
    }

    pub(crate) fn subscribe_store_name(&self) -> coven::LiveQuery<String> {
        self.inner.handle.subscribe_store_name()
    }

    pub(crate) async fn rename_store(&self, name: &str) -> Result<(), coven::SyncError> {
        self.inner.handle.rename_store(name).await
    }

    /// Delete the store, discarding what this device hasn't uploaded; the
    /// person confirmed that loss from [`Self::unsent_before_removal`].
    pub(crate) async fn delete_store(&self) -> Result<(), coven::StoreDeletionError> {
        self.inner
            .handle
            .delete_store(coven::IfUnsent::Discard)
            .await
    }

    /// What deleting the store from this device loses: the writes and files
    /// still waiting to upload to its storage. A store with no storage set up
    /// has nowhere it uploads to and loses its whole catalog instead, so
    /// nothing counts as unsent.
    pub(crate) async fn unsent_before_removal(
        &self,
    ) -> Result<coven::UnsentWork, coven::SyncError> {
        if self.inner.handle.storage_config().await?.is_none() {
            return Ok(coven::UnsentWork {
                writes: 0,
                files: 0,
            });
        }
        Ok(self.inner.handle.unsent_work().await?)
    }

    pub(crate) async fn storage_config(
        &self,
    ) -> Result<Option<coven::StorageConfig>, coven::SyncError> {
        self.inner.handle.storage_config().await
    }

    pub(crate) fn subscribe_sync_status(&self) -> tokio::sync::watch::Receiver<coven::SyncStatus> {
        self.inner.handle.subscribe_sync_status()
    }

    /// What `retry_now` would do if called now, decided as coven answers it.
    pub(crate) fn subscribe_retry_effect(
        &self,
    ) -> tokio::sync::watch::Receiver<coven::RetryEffect> {
        self.inner.handle.subscribe_retry_effect()
    }

    pub(crate) async fn pending(&self) -> Result<Vec<coven::PendingRecord>, DbError> {
        self.inner.handle.pending().await.map_err(DbError::from)
    }

    pub(crate) fn set_transfer_limits(&self, limits: coven::TransferLimits) {
        self.inner.handle.set_transfer_limits(limits)
    }

    pub(crate) fn transfer_limits(&self) -> coven::TransferLimits {
        self.inner.handle.transfer_limits()
    }

    pub(crate) fn stop_sync(&self) {
        self.inner.handle.stop_sync();
    }

    pub(crate) async fn close(&self) -> Result<(), coven::SyncError> {
        self.inner.handle.close().await
    }

    pub(crate) fn sync_now(&self) {
        self.inner.handle.sync_now();
    }

    pub(crate) fn network_changed(&self) {
        self.inner.handle.network_changed();
    }

    pub(crate) fn retry_now(&self) -> coven::RetryAnswer {
        self.inner.handle.retry_now()
    }

    /// Idle passes at most `interval` apart until the guard drops.
    pub(crate) fn sync_often(&self, interval: std::time::Duration) -> coven::SyncOftenGuard {
        self.inner.handle.sync_often(interval)
    }

    pub(crate) async fn retry_pending_operation(
        &self,
        operation: coven::OperationId,
    ) -> Result<(), coven::OperationError> {
        self.inner.handle.retry_pending_operation(operation).await
    }

    pub(crate) async fn start_sync(&self) -> Result<(), coven::SyncError> {
        self.inner.handle.start_sync().await
    }

    pub(crate) async fn setup_s3_storage(
        &self,
        storage: coven::StorageConfig,
        device_name: &str,
        access_key: String,
        secret_key: coven::SecretText,
    ) -> Result<coven::StorageSetup, coven::StorageSetupError> {
        self.inner
            .handle
            .setup_s3_storage(storage, device_name, access_key, secret_key)
            .await
    }

    pub(crate) async fn setup_cloudkit_storage(
        &self,
        device_name: &str,
    ) -> Result<coven::StorageSetup, coven::StorageSetupError> {
        self.inner
            .handle
            .setup_cloudkit_storage("iCloud.fm.bae.desktop", device_name)
            .await
    }

    #[cfg(feature = "oauth-providers")]
    pub(crate) async fn authenticate(
        &self,
        access: coven::OAuthAccess,
    ) -> Result<(), coven::StorageSetupError> {
        self.inner.handle.authenticate(access).await
    }

    #[cfg(feature = "oauth-providers")]
    pub(crate) async fn setup_oauth_storage(
        &self,
        storage: coven::StorageConfig,
        device_name: &str,
    ) -> Result<coven::StorageSetup, coven::StorageSetupError> {
        self.inner
            .handle
            .setup_oauth_storage(storage, device_name)
            .await
    }

    #[cfg(feature = "oauth-providers")]
    pub(crate) async fn onedrive_root(
        &self,
    ) -> Result<coven::StorageFolder, coven::StorageSetupError> {
        self.inner.handle.onedrive_root().await
    }

    #[cfg(feature = "oauth-providers")]
    pub(crate) async fn onedrive_folders(
        &self,
        parent: &coven::StorageConfig,
    ) -> Result<Vec<coven::StorageFolder>, coven::StorageSetupError> {
        self.inner.handle.onedrive_folders(parent).await
    }

    #[cfg(feature = "oauth-providers")]
    pub(crate) async fn create_onedrive_folder(
        &self,
        parent: &coven::StorageConfig,
        name: &str,
    ) -> Result<coven::StorageFolder, coven::StorageSetupError> {
        self.inner.handle.create_onedrive_folder(parent, name).await
    }

    #[cfg(feature = "oauth-providers")]
    pub(crate) async fn dropbox_shared_folders(
        &self,
    ) -> Result<Vec<coven::StorageFolder>, coven::StorageSetupError> {
        self.inner.handle.dropbox_shared_folders().await
    }

    pub(crate) fn store_key_state(&self) -> Result<coven::StoreKeyState, coven::KeyError> {
        self.inner.handle.store_key_state()
    }

    pub(crate) async fn unlock_store_key(
        &self,
    ) -> Result<coven::ConnectedStorage, coven::StoreKeyUnlockError> {
        self.inner.handle.unlock_store_key().await
    }

    pub(crate) async fn disconnect_storage(&self) -> Result<(), coven::SyncError> {
        self.inner.handle.disconnect_storage().await
    }

    pub(crate) async fn forget_store_keys(&self) -> Result<(), coven::KeyError> {
        self.inner.handle.forget_store_keys().await
    }

    pub(crate) fn host_secret(&self, name: &str) -> Result<Option<String>, coven::KeyError> {
        self.inner.handle.host_secret(name)
    }

    pub(crate) fn set_host_secret(&self, name: &str, value: &str) -> Result<(), coven::KeyError> {
        self.inner.handle.set_host_secret(name, value)
    }

    #[cfg(desktop)]
    pub(crate) fn delete_host_secret(&self, name: &str) -> Result<(), coven::KeyError> {
        self.inner.handle.delete_host_secret(name)
    }

    pub(crate) async fn file_ref(
        &self,
        file: coven::FileId,
    ) -> Result<coven::FileRef, coven::DbError> {
        self.inner.handle.file_ref(file).await
    }

    pub(crate) async fn read_file(
        &self,
        file: &coven::FileRef,
    ) -> Result<Vec<u8>, coven::FileReadError> {
        self.inner.handle.read_file(file).await
    }

    pub(crate) async fn open_file_stream(
        &self,
        file: &coven::FileRef,
    ) -> Result<coven::FileStream, coven::FileReadError> {
        self.inner.handle.open_file_stream(file).await
    }

    pub(crate) async fn pin(&self, files: &[coven::FileRef]) -> Result<(), coven::FileReadError> {
        self.inner.handle.pin(files).await
    }

    /// Where each of the release's files stands for its pin now, by release
    /// file id.
    pub(crate) async fn release_file_pins(
        &self,
        release_id: &str,
    ) -> Result<std::collections::BTreeMap<String, DbFilePin>, DbError> {
        let release_id = release_id.to_string();
        self.read(move |sql| release_file_pins_on(&sql, &release_id))
            .await
    }

    /// Where each of the release's files stands for its pin, live: it reruns
    /// as a file is kept and as a download to keep one starts or ends.
    pub(crate) fn subscribe_release_file_pins(
        &self,
        release_id: &str,
    ) -> coven::LiveQuery<std::collections::BTreeMap<String, DbFilePin>> {
        let release_id = release_id.to_string();
        self.inner
            .handle
            .subscribe(move |sql| Ok(release_file_pins_on(&sql, &release_id)?))
    }

    /// How far each file this process is downloading to keep whole has got,
    /// by file, from coven's download watch.
    pub(crate) fn subscribe_download_file_transfers(&self) -> coven::TransferWatch {
        self.inner.handle.subscribe_download_transfers()
    }

    pub(crate) async fn unpin(&self, files: &[coven::FileRef]) -> Result<(), coven::FileReadError> {
        self.inner.handle.unpin(files).await
    }

    pub(crate) async fn files_pinned(
        &self,
        files: &[coven::FileId],
    ) -> Result<Vec<Option<bool>>, DbError> {
        self.inner
            .handle
            .files_pinned(files)
            .await
            .map_err(DbError::from)
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) async fn evict_file(
        &self,
        file: &coven::FileRef,
    ) -> Result<(), coven::FileReadError> {
        self.inner.handle.evict_file(file).await
    }

    /// coven's whole upload queue, for a test's drain to count what left it.
    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) async fn uploads(&self) -> Result<coven::UploadQueue, DbError> {
        self.inner.handle.uploads().await.map_err(DbError::from)
    }

    pub(crate) async fn set_uploads_paused(&self, paused: bool) -> Result<(), DbError> {
        self.inner
            .handle
            .set_uploads_paused(paused)
            .await
            .map_err(DbError::from)
    }

    pub(crate) async fn retry_uploads_now(&self) -> Result<coven::DrainOutcome, coven::SyncError> {
        self.inner.handle.retry_uploads_now().await
    }

    pub(crate) async fn generate_restore_code(&self) -> Result<String, coven::SyncError> {
        self.inner.handle.restore_code().await
    }

    pub(crate) async fn restore_code_for_transfer(&self) -> Result<String, coven::SyncError> {
        self.inner.handle.restore_code_for_transfer().await
    }

    pub(crate) async fn remove_member(
        &self,
        member: &coven::MemberId,
    ) -> Result<(), coven::SyncError> {
        self.inner.handle.remove_member(member).await
    }

    pub(crate) async fn remove_device(
        &self,
        device: coven::DeviceId,
    ) -> Result<Option<coven::ProviderSignOut>, coven::SyncError> {
        self.inner.handle.remove_device(device).await
    }

    pub(crate) async fn set_member_role(
        &self,
        member: &coven::MemberId,
        role: coven::MemberRole,
    ) -> Result<(), coven::SyncError> {
        self.inner.handle.set_member_role(member, role).await
    }

    pub(crate) async fn invite_person(
        &self,
        role: coven::MemberRole,
        name: String,
        access: coven::InviteAccess,
        delivery: coven::InviteDelivery,
    ) -> Result<coven::Invite, coven::SyncError> {
        self.inner
            .handle
            .create_invite(
                coven::InviteTarget::NewMember { role, name },
                access,
                delivery,
            )
            .await
    }

    pub(crate) fn subscribe_invites(&self) -> tokio::sync::watch::Receiver<Vec<coven::Invite>> {
        self.inner.handle.subscribe_invites()
    }

    pub(crate) async fn approve_join_request(
        &self,
        request: &coven::JoinRequest,
    ) -> Result<(), coven::SyncError> {
        self.inner.handle.approve_join_request(request).await
    }

    pub(crate) async fn decline_join_request(
        &self,
        request: &coven::JoinRequest,
    ) -> Result<(), coven::SyncError> {
        self.inner.handle.decline_join_request(request).await
    }

    pub(crate) async fn cancel_invite(
        &self,
        invite: &coven::InviteId,
    ) -> Result<(), coven::SyncError> {
        self.inner.handle.cancel_invite(invite).await
    }
}

impl Database {
    pub(crate) async fn detect_s3_settings(
        &self,
        storage: coven::StorageConfig,
        credentials: coven::S3Credentials,
    ) -> Result<coven::S3Detection, coven::StorageSetupError> {
        self.inner
            .handle
            .detect_s3_settings(
                storage,
                credentials.access_key_id,
                credentials.secret_access_key,
            )
            .await
    }
}
