use super::*;

forward! { async this => {
    fn fetch_library_image_bytes(image: crate::types::BridgeImageRef) -> Option<Vec<u8>> {
        this.services
            .read_image_blob(&image.into_core())
            .await
            .map_err(BridgeError::database_query)
    }

    fn fetch_release_image_bytes(release_id: String, source: BridgeGallerySource) -> Vec<u8> {
        this.services
            .read_gallery_bytes(&release_id, &source.into_core())
            .await
            .map_err(BridgeError::database_query)
    }
} }

forward! {
    #[cfg(feature = "cloudkit")]
    async this => {
        fn use_cloudkit(storage: BridgeHomeStorage) -> () {
            this.services
                .use_cloudkit(crate::types::BridgeHomeStorage::into_core(storage))
                .await?;
            Ok(())
        }
    }
}

forward! {
    #[cfg(feature = "oauth-providers")]
    async this => {
        fn sign_in_cloud_provider(
            provider: BridgeCloudProvider,
            storage: BridgeHomeStorage,
        ) -> () {
            this.services
                .sign_in_cloud_provider(
                    crate::types::BridgeCloudProvider::into_core(provider),
                    crate::types::BridgeHomeStorage::into_core(storage),
                )
                .await?;
            Ok(())
        }
    }
}
