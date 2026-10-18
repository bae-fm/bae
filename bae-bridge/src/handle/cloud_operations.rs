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
        fn use_cloudkit(device_name: String) -> crate::types::BridgeStorageSetup {
            Ok(crate::types::BridgeStorageSetup::from_core(this.services.use_cloudkit(&device_name).await?))
        }
    }
}
forward! {
    #[cfg(feature = "oauth-providers")]
    async this => {
        fn sign_in(provider: crate::types::BridgeCloudProvider) -> () {
            Ok(this.services.sign_in(provider.into_core()).await?)
        }
        fn sign_in_again(device_name: String) -> crate::types::BridgeStorageSetup {
            Ok(crate::types::BridgeStorageSetup::from_core(this.services.sign_in_again(&device_name).await?))
        }
        fn setup_oauth_storage(location: crate::types::BridgeStorageLocation, device_name: String) -> crate::types::BridgeStorageSetup {
            Ok(crate::types::BridgeStorageSetup::from_core(this.services.setup_oauth_storage(location.into_core()?, &device_name).await?))
        }
        fn onedrive_root() -> crate::types::BridgeStorageFolder {
            Ok(crate::types::BridgeStorageFolder::from_core(this.services.onedrive_root().await?))
        }
        fn onedrive_folders(parent: crate::types::BridgeStorageLocation) -> Vec<crate::types::BridgeStorageFolder> {
            Ok(this.services.onedrive_folders(&parent.into_core()?).await?.into_iter().map(crate::types::BridgeStorageFolder::from_core).collect())
        }
        fn create_onedrive_folder(parent: crate::types::BridgeStorageLocation, name: String) -> crate::types::BridgeStorageFolder {
            Ok(crate::types::BridgeStorageFolder::from_core(this.services.create_onedrive_folder(&parent.into_core()?, &name).await?))
        }
        fn dropbox_shared_folders() -> Vec<crate::types::BridgeStorageFolder> {
            Ok(this.services.dropbox_shared_folders().await?.into_iter().map(crate::types::BridgeStorageFolder::from_core).collect())
        }
    }
}
