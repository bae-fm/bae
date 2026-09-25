use super::*;

/// One value a live library search delivered.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeLibrarySearchSnapshot {
    /// The trimmed query these results answer; empty when there is no search.
    pub query: String,
    pub results: BridgeSearchResults,
    /// The revision of the `set_query` call these results answer.
    pub request_revision: u64,
}

/// A live library search whose query changes in place as the person types:
/// one query for as long as the search is open, not one per keystroke. It
/// starts with no query.
#[derive(uniffi::Object)]
pub struct LibrarySearchSubscription {
    inner: bae_core::library::LibrarySearchSubscription,
    runtime: tokio::runtime::Handle,
}

#[uniffi::export]
impl AppHandle {
    pub fn subscribe_library_search(&self) -> std::sync::Arc<LibrarySearchSubscription> {
        std::sync::Arc::new(LibrarySearchSubscription {
            inner: self.services.subscribe_library_search(),
            runtime: self.runtime.handle().clone(),
        })
    }
}

#[uniffi::export(async_runtime = "tokio", cancellable)]
impl LibrarySearchSubscription {
    /// Search for `query`; a blank one is no search. Returns the revision the
    /// answer will carry.
    pub fn set_query(&self, query: String) -> Result<u64, BridgeError> {
        self.inner.set_query(&query).map_err(search_error)
    }

    pub async fn next(
        self: std::sync::Arc<Self>,
    ) -> Result<BridgeLibrarySearchSnapshot, BridgeError> {
        let runtime = self.runtime.clone();
        crate::operation_runtime::run(runtime, move || async move {
            self.inner
                .next()
                .await
                .map(|snapshot| BridgeLibrarySearchSnapshot {
                    query: snapshot
                        .query
                        .map(|query| query.as_str().to_string())
                        .unwrap_or_default(),
                    results: BridgeSearchResults::from_core(snapshot.results),
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

fn search_error(error: bae_core::library::LibrarySearchSubscriptionError) -> BridgeError {
    match error {
        bae_core::library::LibrarySearchSubscriptionError::Cancelled => BridgeError::Cancelled,
        bae_core::library::LibrarySearchSubscriptionError::Query(error) => {
            BridgeError::database_query(error)
        }
    }
}

#[cfg(test)]
#[path = "library_search_tests.rs"]
mod tests;
