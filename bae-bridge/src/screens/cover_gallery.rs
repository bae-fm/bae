//! A library release's or a candidate's cover picker as the Mac pulls it.

use crate::types::{
    BridgeCoverFilter, BridgeCoverGallery, BridgeCoverGalleryOwner, BridgeCoverKey, BridgeError,
};

impl crate::types::BridgeCoverGallerySnapshot {
    fn from_core(snapshot: bae_import::CoverGallerySnapshot) -> Self {
        Self {
            request: snapshot.request.map(|request| crate::types::BridgeCoverGalleryRequest {
                owner: BridgeCoverGalleryOwner::from_core(request.owner),
                filter: BridgeCoverFilter::from_core(request.filter),
                focus: request.focus.map(BridgeCoverKey::from_core),
            }),
            gallery: snapshot.gallery.map(BridgeCoverGallery::from_core),
        }
    }
}

pulled_read! {
    /// The covers a library release's or a candidate's picker shows under
    /// one source, and the cover it focuses. The subscription keeps one live
    /// query while the picker is open, and changes the query's request when
    /// the person picks another source or focuses another cover. The subscription delivers nothing until `set_request` names a
    /// release or a candidate.
    CoverGallerySubscription:
        bae_app::live_reads::LiveCoverGallery => crate::types::BridgeCoverGallerySnapshot,
    converted by crate::types::BridgeCoverGallerySnapshot::from_core,
    built by cover_gallery
}

#[uniffi::export]
impl CoverGallerySubscription {
    /// Read the picker of `owner` under `filter`, focused on the cover
    /// `focus` names while it shows it, from now on.
    pub fn set_request(
        &self,
        owner: BridgeCoverGalleryOwner,
        filter: BridgeCoverFilter,
        focus: Option<BridgeCoverKey>,
    ) -> Result<(), BridgeError> {
        self.inner
            .set(Some(bae_import::CoverGalleryRequest {
                owner: owner.into_core(),
                filter: filter.into_core(),
                focus: focus.map(BridgeCoverKey::into_core),
            }))
            .map(|_revision| ())
            .map_err(BridgeError::from)
    }
}
