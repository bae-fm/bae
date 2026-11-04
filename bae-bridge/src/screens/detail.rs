//! The album, release, artist, composer, work, import candidate, invalid
//! import folder, and storage inspector detail views, each read live through
//! one subscription whose id moves in place as the view shows another item.

use crate::types::BridgeError;

/// A macro rather than a generic: uniffi exports concrete objects and records,
/// so each detail needs its own named types.
macro_rules! detail_subscription {
    (
        object: $object:ident,
        built by $make:ident,
        snapshot: $snapshot:ident,
        value: $bridge_value:ty,
    ) => {
        detail_subscription! {
            object: $object,
            built by $make,
            snapshot: $snapshot,
            value: $bridge_value,
            id: String = String,
        }
    };
    (
        object: $object:ident,
        built by $make:ident,
        snapshot: $snapshot:ident,
        value: $bridge_value:ty,
        id: $id:ty = $core_id:ty,
    ) => {
        /// One value a detail read delivered: the id it was read for, and that
        /// item's detail, or none once no such item exists.
        #[derive(Debug, Clone, uniffi::Record)]
        pub struct $snapshot {
            pub id: Option<$id>,
            pub value: Option<$bridge_value>,
        }

        pulled_read! {
            /// A read that shows nothing until `set_id` names an item.
            $object: bae_library::LiveDetail<<$bridge_value as DetailValue>::Core, $core_id>
                => $snapshot,
            converted by |snapshot: bae_library::DetailSnapshot<
                <$bridge_value as DetailValue>::Core,
                $core_id,
            >| $snapshot {
                id: snapshot.id.map(<$id>::from),
                value: snapshot.value.map(<$bridge_value>::from_core),
            },
            built by $make
        }

        #[uniffi::export]
        impl $object {
            /// Show `id` from now on. A view that stops showing anything
            /// cancels the read rather than pointing it at nothing.
            pub fn set_id(&self, id: $id) -> Result<(), BridgeError> {
                self.inner.set(Some(<$core_id>::from(id))).map_err(live_read_error)
            }
        }
    };
}

/// The core detail each bridge detail mirrors.
pub(crate) trait DetailValue {
    type Core;
}

impl DetailValue for crate::types::BridgeAlbumDetail {
    type Core = bae_views::album_detail::AlbumDetail;
}

impl DetailValue for crate::types::BridgeRelease {
    type Core = bae_views::album_detail::ReleaseDetail;
}

impl DetailValue for crate::types::BridgeArtistDetail {
    type Core = bae_views::album_detail::ArtistDetail;
}

impl DetailValue for crate::types::BridgeComposerDetail {
    type Core = bae_views::album_detail::ComposerDetail;
}

impl DetailValue for crate::types::BridgeWorkDetail {
    type Core = bae_views::album_detail::WorkDetail;
}

#[cfg(feature = "desktop")]
impl DetailValue for crate::types::BridgeImportCandidateDetail {
    type Core = bae_import::ImportCandidateDetail;
}

#[cfg(feature = "desktop")]
impl DetailValue for crate::types::BridgeInvalidCandidateDetail {
    type Core = bae_import::InvalidCandidateDetail;
}

// Gated on the target, as the export queue its transfers include is.
#[cfg(desktop)]
impl DetailValue for crate::types::BridgeStorageInspector {
    type Core = bae_views::storage_inspector::StorageInspector;
}

#[cfg(desktop)]
detail_subscription! {
    object: StorageInspectorSubscription,
    built by storage_inspector,
    snapshot: BridgeStorageInspectorSnapshot,
    value: crate::types::BridgeStorageInspector,
}

#[cfg(feature = "desktop")]
detail_subscription! {
    object: InvalidCandidateSubscription,
    built by invalid_candidate,
    snapshot: BridgeInvalidCandidateSnapshot,
    value: crate::types::BridgeInvalidCandidateDetail,
    id: Vec<u8> = bae_import::CandidateKey,
}

#[cfg(feature = "desktop")]
detail_subscription! {
    object: ImportCandidateSubscription,
    built by import_candidate,
    snapshot: BridgeImportCandidateSnapshot,
    value: crate::types::BridgeImportCandidateDetail,
    id: Vec<u8> = bae_import::CandidateKey,
}

detail_subscription! {
    object: AlbumDetailSubscription,
    built by album_detail,
    snapshot: BridgeAlbumDetailSnapshot,
    value: crate::types::BridgeAlbumDetail,
}

detail_subscription! {
    object: ReleaseDetailSubscription,
    built by release_detail,
    snapshot: BridgeReleaseDetailSnapshot,
    value: crate::types::BridgeRelease,
}

detail_subscription! {
    object: ArtistDetailSubscription,
    built by artist_detail,
    snapshot: BridgeArtistDetailSnapshot,
    value: crate::types::BridgeArtistDetail,
}

detail_subscription! {
    object: ComposerDetailSubscription,
    built by composer_detail,
    snapshot: BridgeComposerDetailSnapshot,
    value: crate::types::BridgeComposerDetail,
}

detail_subscription! {
    object: WorkDetailSubscription,
    built by work_detail,
    snapshot: BridgeWorkDetailSnapshot,
    value: crate::types::BridgeWorkDetail,
}

/// A task-driven live read's failure as the bridge reports it.
pub(super) fn live_read_error(error: bae_library::LiveReadError) -> BridgeError {
    match error {
        bae_library::LiveReadError::Cancelled => BridgeError::Cancelled,
        bae_library::LiveReadError::Query(error) => BridgeError::database_query(error),
        bae_library::LiveReadError::Failed(error) => BridgeError::from(error),
    }
}
