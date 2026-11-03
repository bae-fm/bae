//! The artist picker's live search: one query for as long as the picker is
//! open, moved in place to each text the person types and to the windows of
//! its list on screen.

use super::search::{LibrarySearchQuery, LiveLibrarySearchError};
use crate::album_detail::ArtistSearchResult;
use bae_db::{LibraryBrowseWindow, LibraryPageWindows};

/// What an artist picker reads: the artists its text finds, parsed as every
/// library search parses a query (`None` is no search), in the windows of its
/// list on screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtistSearchView {
    pub query: Option<LibrarySearchQuery>,
    pub windows: LibraryPageWindows,
}

/// One value an artist search delivered: each window it was asked for, with
/// the artists that landed in it, how many artists the text found, and the
/// revision of the request it answers.
#[derive(Debug, Clone)]
pub struct ArtistSearchSnapshot {
    pub windows: Vec<LibraryBrowseWindow<ArtistSearchResult>>,
    pub total_count: u64,
    pub request_revision: u64,
}

/// A live artist search whose text and windows change in place.
pub struct LiveArtistSearch {
    query: crate::live_query::CancellableLiveQuery<
        ArtistSearchView,
        crate::library::views::ArtistSearchProjection,
    >,
    /// The snapshot a projection read for a request revision resolves to.
    resolve: std::sync::Arc<
        dyn Fn(crate::library::views::ArtistSearchProjection, u64) -> ArtistSearchSnapshot
            + Send
            + Sync,
    >,
}

impl LiveArtistSearch {
    pub(crate) fn new(
        query: crate::live_query::CancellableLiveQuery<
            ArtistSearchView,
            crate::library::views::ArtistSearchProjection,
        >,
        resolve: impl Fn(crate::library::views::ArtistSearchProjection, u64) -> ArtistSearchSnapshot
            + Send
            + Sync
            + 'static,
    ) -> Self {
        Self { query, resolve: std::sync::Arc::new(resolve) }
    }

    /// Read `windows` of what `raw` finds from now on; a blank text is no
    /// search. Returns the revision the answer will carry, which repeating the
    /// standing request keeps.
    pub fn set_view(
        &self,
        raw: &str,
        windows: LibraryPageWindows,
    ) -> Result<u64, LiveLibrarySearchError> {
        self.query
            .set(ArtistSearchView { query: LibrarySearchQuery::parse(raw), windows })
            .map_err(LiveLibrarySearchError::from)
    }

    pub async fn next(&self) -> Result<ArtistSearchSnapshot, LiveLibrarySearchError> {
        let event = self.query.next().await.map_err(LiveLibrarySearchError::from)?;
        let request_revision = event.revision.0;
        Ok((self.resolve)(event.result?, request_revision))
    }

    pub async fn cancel(&self) {
        self.query.close().await;
    }
}
