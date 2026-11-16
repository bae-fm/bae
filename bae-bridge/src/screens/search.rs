//! This module runs the searches that screens open: the library search, and
//! whether the releases an import pane offers are already in the library.
//! Each is one live query while its screen is open, and the screen changes
//! its text on that query as the person types.

use crate::types::BridgeError;

/// One value a live library search delivered.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeLibrarySearchSnapshot {
    /// The trimmed query these results answer; `None` when there is no search.
    pub query: Option<String>,
    pub results: crate::types::BridgeSearchResults,
    /// The revision of the `set_query` call these results answer.
    pub request_revision: u64,
}

impl BridgeLibrarySearchSnapshot {
    fn from_core(snapshot: bae_views::LibrarySearchSnapshot) -> Self {
        Self {
            query: snapshot.query.map(|query| query.as_str().to_string()),
            results: crate::types::BridgeSearchResults::from_core(snapshot.results),
            request_revision: snapshot.request_revision,
        }
    }
}

pulled_read! {
    /// A live library search whose query changes in place as the person
    /// types: one query for as long as the search is open, not one per
    /// keystroke. It starts with no query.
    LibrarySearchSubscription: bae_views::LiveLibrarySearch => BridgeLibrarySearchSnapshot,
    converted by BridgeLibrarySearchSnapshot::from_core,
    built by library_search
}

#[uniffi::export]
impl LibrarySearchSubscription {
    /// Search for `query`; `None`, or a blank one, is no search. Returns the
    /// revision the answer will carry.
    pub fn set_query(&self, query: Option<String>) -> Result<u64, BridgeError> {
        self.inner
            .set(query.as_deref().and_then(bae_views::LibrarySearchQuery::parse))
            .map_err(BridgeError::from)
    }
}
