use super::*;

/// One value an album selection delivered: the ids it read, and the summary
/// of each that is still in the library. A requested id with no summary names
/// an album that is gone.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeAlbumSelectionSnapshot {
    pub requested: Vec<String>,
    pub albums: Vec<crate::types::BridgeAlbum>,
}

/// The album grid's multi-selection read live: one query for as long as the
/// grid is open, whose ids move in place as the selection changes. It starts
/// with none.
#[derive(uniffi::Object)]
pub struct AlbumSelectionSubscription {
    inner: bae_core::library::AlbumSelectionSubscription,
    runtime: tokio::runtime::Handle,
}

#[uniffi::export]
impl AppHandle {
    pub fn subscribe_album_selection(&self) -> std::sync::Arc<AlbumSelectionSubscription> {
        std::sync::Arc::new(AlbumSelectionSubscription {
            inner: self.services.subscribe_album_selection(),
            runtime: self.runtime.clone(),
        })
    }
}

#[uniffi::export(async_runtime = "tokio", cancellable)]
impl AlbumSelectionSubscription {
    /// Read `album_ids` from now on.
    pub fn set_albums(&self, album_ids: Vec<String>) -> Result<(), BridgeError> {
        self.inner.set_albums(album_ids).map_err(selection_error)
    }

    pub async fn next(
        self: std::sync::Arc<Self>,
    ) -> Result<BridgeAlbumSelectionSnapshot, BridgeError> {
        let runtime = self.runtime.clone();
        crate::operation_runtime::run(runtime, move || async move {
            self.inner
                .next()
                .await
                .map(|snapshot| BridgeAlbumSelectionSnapshot {
                    requested: snapshot.requested.into_iter().collect(),
                    albums: snapshot
                        .albums
                        .into_iter()
                        .map(crate::types::BridgeAlbum::from_core)
                        .collect(),
                })
                .map_err(selection_error)
        })
        .await
    }

    pub async fn cancel(self: std::sync::Arc<Self>) -> Result<(), BridgeError> {
        let runtime = self.runtime.clone();
        crate::operation_runtime::run(runtime, move || async move {
            self.inner.cancel().await;
            Ok(())
        })
        .await
    }
}

fn selection_error(error: bae_core::library::AlbumSelectionSubscriptionError) -> BridgeError {
    match error {
        bae_core::library::AlbumSelectionSubscriptionError::Cancelled => BridgeError::Cancelled,
        bae_core::library::AlbumSelectionSubscriptionError::Query(error) => {
            BridgeError::database_query(error)
        }
    }
}
