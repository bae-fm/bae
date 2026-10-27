//! An artist picker's live search of the library's artists: one query for as
//! long as the picker is open, moved in place to each text the person types
//! and to the windows of its list on screen.

use super::library_search::search_error;
use super::*;

/// One requested window of an artist search, with the artists that landed in
/// it.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeArtistSearchWindow {
    pub window: crate::types::BridgeLibraryPageWindow,
    pub rows: Vec<BridgeArtistSearchResult>,
}

/// Every requested window of an artist search as of one read, how many
/// artists its text found, and the revision of the `set_view` call it
/// answers.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeArtistSearchSnapshot {
    pub windows: Vec<BridgeArtistSearchWindow>,
    pub total_count: u64,
    pub request_revision: u64,
}

#[derive(uniffi::Object)]
pub struct ArtistSearchSubscription {
    inner: bae_core::library::ArtistSearchSubscription,
    runtime: tokio::runtime::Handle,
}

#[uniffi::export]
impl AppHandle {
    /// A live artist search with no text and no windows until `set_view`
    /// names them.
    pub fn subscribe_artist_search(&self) -> std::sync::Arc<ArtistSearchSubscription> {
        std::sync::Arc::new(ArtistSearchSubscription {
            inner: self.services.subscribe_artist_search(),
            runtime: self.runtime.clone(),
        })
    }
}

#[uniffi::export(async_runtime = "tokio", cancellable)]
impl ArtistSearchSubscription {
    /// Read `windows` of what `query` finds from now on; a blank one is no
    /// search. Returns the revision the answer will carry.
    pub fn set_view(
        &self,
        query: String,
        windows: Vec<crate::types::BridgeLibraryPageWindow>,
    ) -> Result<u64, BridgeError> {
        self.inner
            .set_view(
                &query,
                windows
                    .into_iter()
                    .map(crate::types::BridgeLibraryPageWindow::into_core)
                    .collect(),
            )
            .map_err(search_error)
    }

    pub async fn next(
        self: std::sync::Arc<Self>,
    ) -> Result<BridgeArtistSearchSnapshot, BridgeError> {
        let runtime = self.runtime.clone();
        crate::operation_runtime::run(runtime, move || async move {
            self.inner
                .next()
                .await
                .map(|snapshot| BridgeArtistSearchSnapshot {
                    windows: snapshot
                        .windows
                        .into_iter()
                        .map(|window| BridgeArtistSearchWindow {
                            window: crate::types::BridgeLibraryPageWindow::from_core(window.window),
                            rows: window
                                .rows
                                .into_iter()
                                .map(BridgeArtistSearchResult::from_core)
                                .collect(),
                        })
                        .collect(),
                    total_count: snapshot.total_count,
                    request_revision: snapshot.request_revision,
                })
                .map_err(search_error)
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

#[cfg(test)]
#[path = "artist_search_tests.rs"]
mod tests;
