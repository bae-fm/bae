//! A library release's cover picker as the Mac pulls it.

use crate::types::{BridgeCoverFilter, BridgeCoverGallery, BridgeError};

impl crate::types::BridgeCoverGallerySnapshot {
    fn from_core(snapshot: bae_import::CoverGallerySnapshot) -> Self {
        Self {
            request: snapshot.request.map(|request| crate::types::BridgeCoverGalleryRequest {
                release_id: request.release_id,
                filter: BridgeCoverFilter::from_core(request.filter),
            }),
            gallery: snapshot.gallery.map(BridgeCoverGallery::from_core),
        }
    }
}

pulled_read! {
    /// The covers a library release's picker shows under one source, read
    /// live: one read for as long as the picker shows, moved in place as its
    /// source changes. It shows nothing until `set_request` names a release.
    CoverGallerySubscription:
        bae_app::live_reads::LiveCoverGallery => crate::types::BridgeCoverGallerySnapshot,
    converted by crate::types::BridgeCoverGallerySnapshot::from_core,
    built by cover_gallery
}

#[uniffi::export]
impl CoverGallerySubscription {
    /// Read the picker of `release_id` under `filter` from now on.
    pub fn set_request(
        &self,
        release_id: String,
        filter: BridgeCoverFilter,
    ) -> Result<(), BridgeError> {
        self.inner
            .set(Some(bae_import::CoverGalleryRequest { release_id, filter: filter.into_core() }))
            .map(|_revision| ())
            .map_err(BridgeError::from)
    }
}
