use super::*;

forward! { async this => {
    /// A library image's bytes, or `None` when no image is stored.
    fn fetch_library_image_bytes(image: crate::types::BridgeImageRef) -> Option<Vec<u8>> {
        this.services
            .read_image_bytes(&image.into_core())
            .await
            .map_err(BridgeError::from)
    }

    fn fetch_release_image_bytes(release_id: String, source: BridgeGallerySource) -> Vec<u8> {
        this.services
            .read_gallery_bytes(&release_id, &source.into_core())
            .await
            .map_err(BridgeError::from)
    }
} }

forward! {
    #[cfg(feature = "cloudkit")]
    async this => {
        fn use_cloudkit(setup: Option<std::sync::Arc<crate::BridgeLibrarySetup>>) -> crate::types::BridgeStorageSetup {
            let steps = crate::screens::SetupFeed::new(setup).steps();
            Ok(crate::types::BridgeStorageSetup::from_core(this.services.use_cloudkit(&|progress| steps.storage(progress)).await?))
        }
    }
}
forward! {
    #[cfg(feature = "oauth-providers")]
    async this => {
        fn sign_in(provider: crate::types::BridgeCloudProvider) -> () {
            Ok(this.services.sign_in(provider.into_core()).await?)
        }
        fn sign_in_again(setup: Option<std::sync::Arc<crate::BridgeLibrarySetup>>) -> crate::types::BridgeStorageSetup {
            let steps = crate::screens::SetupFeed::new(setup).steps();
            Ok(crate::types::BridgeStorageSetup::from_core(this.services.sign_in_again(&|progress| steps.storage(progress)).await?))
        }
        fn setup_oauth_storage(location: crate::types::BridgeStorageLocation, setup: Option<std::sync::Arc<crate::BridgeLibrarySetup>>) -> crate::types::BridgeStorageSetup {
            let steps = crate::screens::SetupFeed::new(setup).steps();
            Ok(crate::types::BridgeStorageSetup::from_core(this.services.setup_oauth_storage(location.into_core()?, &|progress| steps.storage(progress)).await?))
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
        fn create_dropbox_shared_folder(name: String) -> crate::types::BridgeStorageFolder {
            Ok(crate::types::BridgeStorageFolder::from_core(this.services.create_dropbox_shared_folder(&name).await?))
        }
    }
}
