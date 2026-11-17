//! A candidate's folder images as the Mac's lightbox pulls them.

use crate::types::{BridgeError, BridgeFolderImagesRequest};

pulled_read! {
    /// The images of a candidate's folder, the image the lightbox shows and
    /// the images before and after it. The subscription keeps one live query
    /// while the lightbox is open, and changes the query's request when the
    /// person moves to another image. The subscription delivers nothing until
    /// `set_request` names a candidate.
    FolderImagesSubscription:
        bae_app::live_reads::LiveFolderImages => crate::types::BridgeFolderImagesSnapshot,
    converted by crate::types::BridgeFolderImagesSnapshot::from_core,
    built by folder_images
}

#[uniffi::export]
impl FolderImagesSubscription {
    /// Read `request` from now on; `None` reads nothing.
    pub fn set_request(
        &self,
        request: Option<BridgeFolderImagesRequest>,
    ) -> Result<(), BridgeError> {
        self.inner
            .set(request.map(BridgeFolderImagesRequest::into_core))
            .map(|_revision| ())
            .map_err(BridgeError::from)
    }
}
