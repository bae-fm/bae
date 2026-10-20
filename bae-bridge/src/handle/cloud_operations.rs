use super::*;

forward! { async this => {
    /// A library image to show, or `None` when no image is stored.
    fn fetch_library_image(image: crate::types::BridgeImageRef) -> Option<crate::types::BridgeImageRead> {
        Ok(this.services
            .read_image(&image.into_core())
            .await
            .map_err(BridgeError::database_query)?
            .map(crate::types::BridgeImageRead::from_core))
    }

    fn fetch_release_image(release_id: String, source: BridgeGallerySource) -> crate::types::BridgeImageRead {
        Ok(crate::types::BridgeImageRead::from_core(this.services
            .read_gallery_bytes(&release_id, &source.into_core())
            .await
            .map_err(BridgeError::database_query)?))
    }

    /// Resolves once `file` no longer arrives, as when coven fetched it after
    /// its uploader uploaded it; the image that named it reads again then.
    /// Dropping the call stops the wait.
    fn wait_for_arrival(file: std::sync::Arc<crate::types::BridgeArrivingFile>) -> () {
        this.services
            .wait_for_arrival(file.0)
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
