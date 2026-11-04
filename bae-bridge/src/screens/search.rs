//! Searches as their screens pull them: the library search, an artist
//! picker's search, and whether the releases an import pane offers are
//! already in the library. Each is one live query for as long as its screen
//! is open, moved in place to each text the person types.

use crate::types::BridgeError;

/// One value a live library search delivered.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeLibrarySearchSnapshot {
    /// The trimmed query these results answer; empty when there is no search.
    pub query: String,
    pub results: crate::types::BridgeSearchResults,
    /// The revision of the `set_query` call these results answer.
    pub request_revision: u64,
}

impl BridgeLibrarySearchSnapshot {
    fn from_core(snapshot: bae_views::LibrarySearchSnapshot) -> Self {
        Self {
            query: snapshot.query.map(|query| query.as_str().to_string()).unwrap_or_default(),
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
    /// Search for `query`; a blank one is no search. Returns the revision the
    /// answer will carry.
    pub fn set_query(&self, query: String) -> Result<u64, BridgeError> {
        self.inner.set_query(&query).map_err(BridgeError::from)
    }
}

browse_window! {
    /// One requested window of an artist search, with the artists that
    /// landed in it.
    window: BridgeArtistSearchWindow,
    row: bae_views::album_detail::ArtistSearchResult => crate::types::BridgeArtistSearchResult,
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

impl BridgeArtistSearchSnapshot {
    fn from_core(snapshot: bae_views::ArtistSearchSnapshot) -> Self {
        Self {
            windows: snapshot
                .windows
                .into_iter()
                .map(BridgeArtistSearchWindow::from_core)
                .collect(),
            total_count: snapshot.total_count,
            request_revision: snapshot.request_revision,
        }
    }
}

pulled_read! {
    /// An artist picker's live search of the library's artists: one query for
    /// as long as the picker is open, moved in place to each text the person
    /// types and to the windows of its list on screen. It has no text and no
    /// windows until `set_view` names them.
    ArtistSearchSubscription: bae_views::LiveArtistSearch => BridgeArtistSearchSnapshot,
    converted by BridgeArtistSearchSnapshot::from_core,
    built by artist_search
}

#[uniffi::export]
impl ArtistSearchSubscription {
    /// Read `windows` of what `query` finds from now on; a blank one is no
    /// search. Returns the revision the answer will carry.
    pub fn set_view(
        &self,
        query: String,
        windows: Vec<crate::types::BridgeLibraryPageWindow>,
    ) -> Result<u64, BridgeError> {
        self.inner.set_view(&query, super::core_windows(windows)).map_err(BridgeError::from)
    }
}

/// One release an import pane offers, as the library-membership check reads
/// it: the catalog's own release id and, when the catalog surfaces one, its
/// group id.
#[cfg(feature = "desktop")]
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeLibraryCheck {
    pub source: crate::types::BridgeLookupCatalog,
    pub release_id: String,
    pub source_group_id: Option<String>,
}

/// The library membership of every checked release, by release id, as of
/// one revision of the checks.
#[cfg(feature = "desktop")]
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeLibraryStatusSnapshot {
    pub statuses: std::collections::HashMap<String, crate::types::BridgeLibraryStatus>,
    /// The revision of the `set_checks` call these statuses answer.
    pub request_revision: u64,
}

#[cfg(feature = "desktop")]
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

#[cfg(feature = "desktop")]
pulled_read! {
    /// Whether the releases an import pane offers are already in the
    /// library: one live query for as long as the pane is open, whose checks
    /// move in place as its offers change. It starts with no checks.
    LibraryStatusSubscription: bae_library::LiveLibraryStatus => BridgeLibraryStatusSnapshot,
    converted by BridgeLibraryStatusSnapshot::from_core,
    built by library_statuses
}

#[cfg(feature = "desktop")]
#[uniffi::export]
impl LibraryStatusSubscription {
    /// Check `checks` from now on. Returns the revision the answer will
    /// carry.
    pub fn set_checks(&self, checks: Vec<BridgeLibraryCheck>) -> Result<u64, BridgeError> {
        self.inner
            .set_checks(checks.into_iter().map(|check| bae_db::LibraryCheck {
                release_id: check.release_id,
                source: check.source.into_core().catalog(),
                source_group_id: check.source_group_id,
            }))
            .map_err(BridgeError::from)
    }
}
