use super::*;

#[uniffi::export(async_runtime = "tokio", cancellable)]
impl AppHandle {
    pub async fn fetch_library_image_bytes(
        self: std::sync::Arc<Self>,
        image: crate::types::BridgeImageRef,
    ) -> Result<Option<Vec<u8>>, BridgeError> {
        self.run_exported(move |this| async move {
            this.services
                .read_image_blob(&image.into_core())
                .await
                .map_err(BridgeError::database_query)
        })
        .await
    }

    pub async fn fetch_release_image_bytes(
        self: std::sync::Arc<Self>,
        release_id: String,
        source: BridgeGallerySource,
    ) -> Result<Vec<u8>, BridgeError> {
        self.run_exported(move |this| async move {
            this.services
                .read_gallery_bytes(&release_id, &source.into_core())
                .await
                .map_err(BridgeError::database_query)
        })
        .await
    }
}

#[cfg(feature = "cloudkit")]
#[uniffi::export(async_runtime = "tokio", cancellable)]
impl AppHandle {
    pub async fn use_cloudkit(
        self: std::sync::Arc<Self>,
        storage: BridgeHomeStorage,
    ) -> Result<(), BridgeError> {
        self.run_exported(move |this| async move {
            this.services
                .use_cloudkit(crate::types::BridgeHomeStorage::into_core(storage))
                .await?;
            Ok(())
        })
        .await
    }
}

#[cfg(feature = "oauth-providers")]
#[uniffi::export(async_runtime = "tokio", cancellable)]
impl AppHandle {
    pub async fn sign_in_cloud_provider(
        self: std::sync::Arc<Self>,
        provider: BridgeCloudProvider,
        storage: BridgeHomeStorage,
    ) -> Result<(), BridgeError> {
        self.run_exported(move |this| async move {
            this.services
                .sign_in_cloud_provider(
                    crate::types::BridgeCloudProvider::into_core(provider),
                    crate::types::BridgeHomeStorage::into_core(storage),
                )
                .await?;
            Ok(())
        })
        .await
    }
}
