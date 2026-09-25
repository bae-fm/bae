use super::*;

/// The album, release, artist, composer, work, and import candidate detail
/// views read live through one subscription each, whose id moves in place as
/// the view shows another item. A macro rather than a generic: uniffi exports concrete
/// objects and records, so each detail needs its own named types.
macro_rules! detail_subscription {
    (
        object: $object:ident,
        subscribe: $subscribe:ident,
        snapshot: $snapshot:ident,
        value: $bridge_value:ty,
    ) => {
        /// One value a detail read delivered: the id it was read for, and that
        /// item's detail, or none once no such item exists.
        #[derive(Debug, Clone, uniffi::Record)]
        pub struct $snapshot {
            pub id: Option<String>,
            pub value: Option<$bridge_value>,
        }

        #[derive(uniffi::Object)]
        pub struct $object {
            inner: bae_core::library::DetailSubscription<<$bridge_value as DetailValue>::Core>,
            runtime: tokio::runtime::Handle,
        }

        #[uniffi::export]
        impl AppHandle {
            /// A read that shows nothing until `set_id` names an item.
            pub fn $subscribe(&self) -> std::sync::Arc<$object> {
                std::sync::Arc::new($object {
                    inner: self.services.$subscribe(&self.runtime),
                    runtime: self.runtime.clone(),
                })
            }
        }

        #[uniffi::export(async_runtime = "tokio", cancellable)]
        impl $object {
            /// Show `id` from now on; `None` shows nothing.
            pub fn set_id(&self, id: Option<String>) -> Result<(), BridgeError> {
                self.inner.set(id).map_err(live_read_error)
            }

            pub async fn next(self: std::sync::Arc<Self>) -> Result<$snapshot, BridgeError> {
                let runtime = self.runtime.clone();
                crate::operation_runtime::run(runtime, move || async move {
                    self.inner
                        .next()
                        .await
                        .map(|snapshot| $snapshot {
                            id: snapshot.id,
                            value: snapshot.value.map(<$bridge_value>::from_core),
                        })
                        .map_err(live_read_error)
                })
                .await
            }

            pub async fn cancel(self: std::sync::Arc<Self>) -> Result<(), BridgeError> {
                let runtime = self.runtime.clone();
                crate::operation_runtime::run(runtime, move || async move {
                    self.inner.cancel();
                    Ok(())
                })
                .await
            }
        }
    };
}

/// The core detail each bridge detail mirrors.
trait DetailValue {
    type Core;
}

impl DetailValue for BridgeAlbumDetail {
    type Core = bae_core::album_detail::AlbumDetail;
}

impl DetailValue for BridgeRelease {
    type Core = bae_core::album_detail::ReleaseDetail;
}

impl DetailValue for BridgeArtistDetail {
    type Core = bae_core::album_detail::ArtistDetail;
}

impl DetailValue for BridgeComposerDetail {
    type Core = bae_core::album_detail::ComposerDetail;
}

impl DetailValue for BridgeWorkDetail {
    type Core = bae_core::album_detail::WorkDetail;
}

#[cfg(feature = "desktop")]
impl DetailValue for crate::types::BridgeImportCandidateDetail {
    type Core = bae_core::import::ImportCandidateDetail;
}

#[cfg(feature = "desktop")]
detail_subscription! {
    object: ImportCandidateSubscription,
    subscribe: subscribe_import_candidate,
    snapshot: BridgeImportCandidateSnapshot,
    value: crate::types::BridgeImportCandidateDetail,
}

detail_subscription! {
    object: AlbumDetailSubscription,
    subscribe: subscribe_album_detail,
    snapshot: BridgeAlbumDetailSnapshot,
    value: BridgeAlbumDetail,
}

detail_subscription! {
    object: ReleaseDetailSubscription,
    subscribe: subscribe_release_detail,
    snapshot: BridgeReleaseDetailSnapshot,
    value: BridgeRelease,
}

detail_subscription! {
    object: ArtistDetailSubscription,
    subscribe: subscribe_artist_detail,
    snapshot: BridgeArtistDetailSnapshot,
    value: BridgeArtistDetail,
}

detail_subscription! {
    object: ComposerDetailSubscription,
    subscribe: subscribe_composer_detail,
    snapshot: BridgeComposerDetailSnapshot,
    value: BridgeComposerDetail,
}

detail_subscription! {
    object: WorkDetailSubscription,
    subscribe: subscribe_work_detail,
    snapshot: BridgeWorkDetailSnapshot,
    value: BridgeWorkDetail,
}

/// A task-driven live read's failure as the bridge reports it.
pub(super) fn live_read_error(error: bae_core::library::LiveReadError) -> BridgeError {
    match error {
        bae_core::library::LiveReadError::Cancelled => BridgeError::Cancelled,
        bae_core::library::LiveReadError::Query(error) => BridgeError::database_query(error),
    }
}
