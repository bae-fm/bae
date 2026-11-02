//! The raw rows the library's views read, before `LibraryManager` resolves
//! them into what a screen shows.

use crate::db::*;

/// Raw combined search-result aggregate. No formatting — the resolver in
/// `LibraryManager` produces the display-ready `crate::album_detail::SearchResults`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DbLibrarySearchResults {
    pub albums: Vec<DbAlbumSearchResult>,
    pub artists: Vec<DbArtistSummary>,
    pub tracks: Vec<DbTrackSearchResult>,
    pub composers: Vec<DbComposerSummary>,
    pub works: Vec<DbWorkSummary>,
}

/// Raw album search-result row; `artist_name` is the album's credits,
/// comma-joined.
#[derive(Debug, Clone, PartialEq)]
pub struct DbAlbumSearchResult {
    pub id: String,
    pub title: String,
    pub year: Option<i32>,
    /// The album's lowest-ordered release.
    pub primary_release_id: String,
    /// The version of that release's cover; `None` when it has none.
    pub cover_version: Option<String>,
    pub artist_name: String,
}

/// Raw track search-result row with album and artist info joined in SQL;
/// `artist_name` is the track's own credits, else its album's. No formatted
/// duration label — the resolver in `LibraryManager` formats it.
#[derive(Debug, Clone, PartialEq)]
pub struct DbTrackSearchResult {
    pub id: String,
    pub title: String,
    pub duration_ms: u64,
    /// The release the track belongs to — the key the resolver looks the
    /// track's cover up by.
    pub release_id: String,
    pub album_id: String,
    pub album_title: String,
    pub artist_name: String,
    /// The version of the release's cover; `None` when it has none.
    pub cover_version: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlbumSortField {
    Title,
    Artist,
    Year,
    DateAdded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AlbumSortCriterion {
    pub field: AlbumSortField,
    pub direction: SortDirection,
}

/// Raw album-detail aggregate. The resolver in `LibraryManager` produces
/// the display-ready `crate::album_detail::AlbumDetail`.
#[derive(Debug, Clone, PartialEq)]
pub struct DbAlbumDetail {
    pub album: DbAlbum,
    pub artists: Vec<crate::db::credits::LibraryCredit>,
    /// The album's releases in album order.
    pub releases: Vec<DbReleaseDetail>,
    /// The album's lowest-ordered release.
    pub primary_release_id: String,
    /// The version of that release's cover; `None` when it has none.
    pub primary_cover_version: Option<String>,
}

/// One row of the album grid: an album, listed once when the grid is flat and
/// under each artist its credits show when grouped by artist.
#[derive(Debug, Clone, PartialEq)]
pub struct DbAlbumAppearance {
    pub album: DbAlbumSummary,
    /// Grouped by artist: the album's credits other than the artist whose
    /// group lists it, printed as a line; `None` when that artist is its only
    /// credit, or when the grid is flat.
    pub credited_with: Option<String>,
}
