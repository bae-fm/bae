//! Searches as their screens pull them: the library search, and whether the
//! releases an import pane offers are already in the library. Each is one
//! live query for as long as its screen is open, moved in place to each text
//! the person types.

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

/// One release an import pane offers, as the library-membership check reads
/// it: the catalog's own release id and, when the catalog surfaces one, its
/// group id.
#[cfg(desktop)]
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeLibraryCheck {
    pub source: crate::types::BridgeLookupCatalog,
    pub release_id: String,
    pub source_group_id: Option<String>,
}

/// The library membership of every checked release, by release id, as of
/// one revision of the checks.
#[cfg(desktop)]
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeLibraryStatusSnapshot {
    pub statuses: std::collections::HashMap<String, crate::types::BridgeLibraryStatus>,
    /// The revision of the `set_checks` call these statuses answer.
    pub request_revision: u64,
}

#[cfg(desktop)]
impl BridgeLibraryStatusSnapshot {
    fn from_core(snapshot: bae_library::LibraryStatusSnapshot) -> Self {
        Self {
            statuses: snapshot
                .statuses
                .into_iter()
                .map(|status| {
                    (
                        status.release_id.clone(),
                        crate::types::BridgeLibraryStatus::from_core(status),
                    )
                })
                .collect(),
            request_revision: snapshot.request_revision,
        }
    }
}

#[cfg(desktop)]
pulled_read! {
    /// Whether the releases an import pane offers are already in the
    /// library: one live query for as long as the pane is open, whose checks
    /// move in place as its offers change. It starts with no checks.
    LibraryStatusSubscription: bae_library::LiveLibraryStatus => BridgeLibraryStatusSnapshot,
    converted by BridgeLibraryStatusSnapshot::from_core,
    built by library_statuses
}

#[cfg(desktop)]
#[uniffi::export]
impl LibraryStatusSubscription {
    /// Check `checks` from now on. Returns the revision the answer will
    /// carry.
    pub fn set_checks(&self, checks: Vec<BridgeLibraryCheck>) -> Result<u64, BridgeError> {
        self.inner
            .set(
                checks
                    .into_iter()
                    .map(|check| bae_records::LibraryCheck {
                        release_id: check.release_id,
                        source: check.source.into_core().catalog(),
                        source_group_id: check.source_group_id,
                    })
                    .collect(),
            )
            .map_err(BridgeError::from)
    }
}
