use super::*;

impl Database {
    #[cfg(test)]
    pub(crate) async fn rename_artist_images_table_for_test(&self) -> Result<(), coven::DbError> {
        self.call(|sql| {
            sql.execute(
                "ALTER TABLE artist_images RENAME TO artist_images_renamed",
                [],
            )?;
            Ok(())
        })
        .await
    }

    #[cfg(test)]
    pub(crate) async fn artist_and_image_counts_for_test(
        &self,
    ) -> Result<(i64, i64), coven::DbError> {
        self.read(|sql| {
            let artists = sql.query_row("SELECT COUNT(*) FROM artists", [], |row| row.get(0))?;
            let images =
                sql.query_row("SELECT COUNT(*) FROM artist_images", [], |row| row.get(0))?;
            Ok((artists, images))
        })
        .await
    }

    pub(crate) fn subscribe_sync_status(
        &self,
    ) -> tokio::sync::watch::Receiver<coven::SyncLoopStatus> {
        self.inner.handle.subscribe_sync_status()
    }

    pub(crate) fn is_syncing(&self) -> bool {
        self.inner.handle.is_syncing()
    }

    pub(crate) fn is_connected(&self) -> bool {
        self.inner.handle.is_connected()
    }

    pub(crate) fn sync_now(&self) {
        self.inner.handle.sync_now();
    }

    pub(crate) async fn connect_sync(&self) -> Result<(), coven::SyncError> {
        self.inner.handle.connect_sync().await
    }

    pub(crate) async fn connect_sync_with_cloudkit(
        &self,
        cloudkit_ops: Arc<dyn coven::CloudKitOps>,
    ) -> Result<(), coven::SyncError> {
        self.inner
            .handle
            .connect_sync_with_cloudkit(cloudkit_ops)
            .await
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) async fn connect_sync_with_test_home(
        &self,
        home: Arc<dyn coven::ExactCloudHome>,
        cipher: coven::CloudCipher,
    ) -> Result<(), coven::CloudHomeSetupError> {
        self.inner
            .handle
            .connect_sync_with_test_home(home, cipher)
            .await
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) async fn connect_sync_with_test_home_caller_driven(
        &self,
        home: Arc<dyn coven::ExactCloudHome>,
        cipher: coven::CloudCipher,
    ) -> Result<(), coven::CloudHomeSetupError> {
        self.inner
            .handle
            .connect_sync_with_test_home_caller_driven(home, cipher)
            .await
    }

    pub(crate) async fn setup_s3_cloud_home(
        &self,
        cloud_home: coven::CloudHomeConfig,
        access_key: String,
        secret_key: String,
    ) -> Result<coven::ConnectedCloudHome, coven::CloudHomeSetupError> {
        self.inner
            .handle
            .setup_s3_cloud_home(cloud_home, access_key, secret_key)
            .await
    }

    pub(crate) async fn setup_cloudkit_cloud_home(
        &self,
        cloud_home: coven::CloudHomeConfig,
        cloudkit_ops: Arc<dyn coven::CloudKitOps>,
    ) -> Result<coven::ConnectedCloudHome, coven::CloudHomeSetupError> {
        self.inner
            .handle
            .setup_cloudkit_cloud_home(cloud_home, cloudkit_ops)
            .await
    }

    #[cfg(feature = "oauth-providers")]
    pub(crate) async fn setup_oauth_cloud_home(
        &self,
        cloud_home: coven::CloudHomeConfig,
        cancel: tokio::sync::watch::Receiver<bool>,
    ) -> Result<coven::ConnectedCloudHome, coven::CloudHomeSetupError> {
        self.inner
            .handle
            .setup_oauth_cloud_home(cloud_home, cancel)
            .await
    }

    pub(crate) fn cloud_home_key_state(
        &self,
        storage: coven::HomeStorage,
    ) -> Result<coven::CloudHomeKeyState, coven::KeyError> {
        self.inner.handle.cloud_home_key_state(storage)
    }

    pub(crate) async fn unlock_cloud_home(
        &self,
        serialized_master_key: &str,
    ) -> Result<coven::ConnectedCloudHome, coven::CloudHomeUnlockError> {
        self.inner
            .handle
            .unlock_cloud_home(serialized_master_key)
            .await
    }

    pub(crate) async fn disconnect_cloud_home(&self) -> Result<(), coven::SyncError> {
        self.inner.handle.disconnect_cloud_home().await
    }

    pub(crate) async fn forget_master_key(&self) -> Result<(), coven::SyncError> {
        self.inner.handle.forget_master_key().await
    }

    pub(crate) fn host_secret(&self, name: &str) -> Result<Option<String>, coven::KeyError> {
        self.inner.handle.host_secret(name)
    }

    pub(crate) fn set_host_secret(&self, name: &str, value: &str) -> Result<(), coven::KeyError> {
        self.inner.handle.set_host_secret(name, value)
    }

    pub(crate) fn delete_host_secret(&self, name: &str) -> Result<(), coven::KeyError> {
        self.inner.handle.delete_host_secret(name)
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) fn establish_test_identity(&self) -> Result<(), coven::IdentityError> {
        match self.inner.handle.initialize_identity() {
            Ok(_) | Err(coven::IdentityError::AlreadyEstablished) => Ok(()),
            Err(error) => Err(error),
        }
    }

    pub(crate) async fn row_blob_ref(
        &self,
        table: &str,
        row_id: &str,
    ) -> Result<coven::RowBlobRef, coven::DbError> {
        self.inner.handle.row_blob_ref(table, row_id).await
    }

    pub(crate) async fn read_blob(
        &self,
        blob: &coven::RowBlobRef,
    ) -> Result<Vec<u8>, coven::BlobCacheError> {
        self.inner.handle.read_blob(blob).await
    }

    pub(crate) async fn open_blob_stream(
        &self,
        blob: &coven::RowBlobRef,
    ) -> Result<coven::BlobStream, coven::BlobCacheError> {
        self.inner.handle.open_blob_stream(blob).await
    }

    pub(crate) async fn pin(
        &self,
        blobs: &[coven::RowBlobRef],
    ) -> Result<(), coven::BlobCacheError> {
        self.inner.handle.pin(blobs).await
    }

    pub(crate) async fn unpin(
        &self,
        blobs: &[coven::RowBlobRef],
    ) -> Result<(), coven::BlobCacheError> {
        self.inner.handle.unpin(blobs).await
    }

    pub(crate) async fn is_pinned(
        &self,
        blobs: &[coven::RowBlobRef],
    ) -> Result<bool, coven::BlobCacheError> {
        self.inner.handle.is_pinned(blobs).await
    }

    pub(crate) async fn evict_blob(
        &self,
        blob: &coven::RowBlobRef,
    ) -> Result<(), coven::BlobCacheError> {
        self.inner.handle.evict_blob(blob).await
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) fn blob_cloud_key(
        &self,
        blob: &coven::BlobRef,
    ) -> Result<String, coven::StorageError> {
        self.inner.handle.blob_cloud_key(blob)
    }

    pub(crate) async fn make_remote(
        &self,
        root_table: &str,
        root_id: &str,
        pin: bool,
    ) -> Result<(), coven::MakeRemoteError> {
        self.inner
            .handle
            .make_remote(root_table, root_id, pin)
            .await
    }

    pub(crate) async fn cancel_make_remote(
        &self,
        root_table: &str,
        root_id: &str,
    ) -> Result<(), coven::MakeRemoteError> {
        self.inner
            .handle
            .cancel_make_remote(root_table, root_id)
            .await
    }

    pub(crate) async fn make_local(
        &self,
        root_table: &str,
        root_id: &str,
        dest: &HashMap<String, std::path::PathBuf>,
        cancel: &tokio::sync::watch::Receiver<bool>,
    ) -> Result<(), coven::MakeLocalError> {
        self.inner
            .handle
            .make_local(root_table, root_id, dest, cancel)
            .await
    }

    pub(crate) async fn drain_uploads(&self) -> Result<coven::DrainOutcome, coven::SyncError> {
        self.inner.handle.drain_uploads().await
    }

    pub(crate) async fn retry_uploads_now(&self) -> Result<coven::DrainOutcome, coven::SyncError> {
        self.inner.handle.retry_uploads_now().await
    }

    pub(crate) async fn set_cache_budget(
        &self,
        namespace: &str,
        max_bytes: u64,
    ) -> Result<(), coven::DbError> {
        self.inner
            .handle
            .set_cache_budget(namespace, max_bytes)
            .await
    }

    pub(crate) async fn make_remote_progress(
        &self,
        root_table: &str,
        root_id: &str,
    ) -> Result<Option<coven::MakeRemoteProgress>, coven::DbError> {
        self.inner
            .handle
            .make_remote_progress(root_table, root_id)
            .await
    }

    pub(crate) async fn generate_restore_code(&self) -> Result<String, coven::SyncError> {
        self.inner.handle.generate_restore_code().await
    }

    pub(crate) async fn get_members(&self) -> Result<Vec<coven::MemberInfo>, coven::SyncError> {
        self.inner.handle.get_members().await
    }

    pub(crate) async fn begin_device_invite(
        &self,
        join_request_code: &str,
        role: coven::MemberRole,
    ) -> Result<coven::DeviceJoinInvite, coven::BeginDeviceInviteError> {
        self.inner
            .handle
            .begin_device_invite(join_request_code, role)
            .await
    }

    pub(crate) async fn drive_device_join(
        &self,
        invite: &coven::DeviceJoinInvite,
        policy: coven::DeviceJoinApprovalPolicy<'_>,
        timing: coven::DeviceJoinTransportTiming,
    ) -> Result<coven::DeviceJoinDriveOutcome, coven::SyncError> {
        self.inner
            .handle
            .drive_device_join(invite, policy, None, timing)
            .await
    }

    pub(crate) async fn cancel_device_invite(
        &self,
        invite: &coven::DeviceJoinInvite,
        timing: coven::DeviceJoinTransportTiming,
    ) -> Result<coven::DeviceJoinCleanupActivation, coven::SyncError> {
        self.inner.handle.cancel_device_invite(invite, timing).await
    }

    pub(crate) async fn remove_member(&self, public_key_hex: &str) -> Result<(), coven::SyncError> {
        self.inner.handle.remove_member(public_key_hex).await
    }
}
