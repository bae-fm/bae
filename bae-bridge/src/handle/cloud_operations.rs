use super::*;

forward! { async this => {
    /// A library image's bytes, or `None` when no image is stored. A cover
    /// has a browse copy and a large copy: `pixels`, the longer edge of the
    /// slot that draws it, picks one, and with none the call returns the
    /// large copy. An artist's image has one copy.
    fn fetch_library_image_bytes(image: crate::types::BridgeImageRef, pixels: Option<u32>) -> Option<Vec<u8>> {
        this.services
            .read_image_bytes(&image.into_core(), pixels)
            .await
            .map_err(BridgeError::from)
    }

    /// One slot of a release's pictures. `pixels` is the longer edge of the
    /// slot that draws it: a cover slot returns the copy that covers it and
    /// an image file's slot its preview. A size above the largest copy
    /// returns the picture itself.
    fn fetch_release_image_bytes(release_id: String, source: BridgeGallerySource, pixels: u32) -> Vec<u8> {
        this.services
            .read_gallery_bytes(&release_id, &source.into_core(), pixels)
            .await
            .map_err(BridgeError::from)
    }

    /// The tiles the viewer zooms `source` from: the picture's levels, each
    /// with its size and how many tiles across and down. Core cuts the
    /// picture into tiles once, reading it in chunks, and keeps them in this
    /// device's tile cache, so a later call answers from the kept tiles.
    /// Dropping the call stops the cut when no other call wants the picture.
    fn picture_tiles(source: crate::types::BridgeCoverImageSource) -> crate::types::BridgePictureTiles {
        Ok(crate::types::BridgePictureTiles::from_core(
            this.services.picture_tiles(source.into_tiled()?).await?,
        ))
    }

    /// The bytes of one tile of `source`, a JPEG or, for a picture with
    /// transparency, a PNG: column `x` and row `y` of level `level`, 0 being
    /// the picture's own size, as `picture_tiles` lists the levels.
    fn picture_tile(source: crate::types::BridgeCoverImageSource, level: u32, x: u32, y: u32) -> Vec<u8> {
        Ok(this.services.picture_tile(source.into_tiled()?, level, x, y).await?)
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
