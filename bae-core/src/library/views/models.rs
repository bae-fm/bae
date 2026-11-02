//! The raw rows the library's views read, before they are resolved into
//! what a screen shows.

use crate::db::*;

/// Raw combined search-result aggregate. No formatting — resolving it
/// produces the display-ready `crate::album_detail::SearchResults`.
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
/// duration label — resolving it formats one.
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

/// Raw album-detail aggregate, which the album views resolve into the
/// display-ready `crate::album_detail::AlbumDetail`.
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

#[derive(Debug, Clone, PartialEq)]
pub struct DbComposerSummary {
    pub artist: DbArtist,
    /// The artist's image, at the version its row read; `None` when it has none.
    pub image: Option<crate::db::ImageRef>,
    pub work_count: i64,
    pub linked_release_count: i64,
    pub unlinked_credit_count: i64,
}

/// Raw artist-summary aggregate: the artist row plus the number of distinct
/// albums whose credit list names it.
#[derive(Debug, Clone, PartialEq)]
pub struct DbArtistSummary {
    pub artist: DbArtist,
    /// The artist's image, at the version its row read; `None` when it has none.
    pub image: Option<crate::db::ImageRef>,
    pub album_count: i64,
}

/// Raw artist-detail aggregate, which the artist views resolve into the
/// display-ready `crate::album_detail::ArtistDetail`.
#[derive(Debug, Clone, PartialEq)]
pub struct DbArtistDetail {
    pub artist: DbArtistSummary,
    /// The artist's other printed names, its merged artists' too.
    pub credit_names: Vec<crate::db::credits::ArtistCreditName>,
    pub albums: Vec<DbAlbumSummary>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DbWorkSummary {
    pub work: DbWork,
    pub parent_work_id: Option<String>,
    pub representative_release_id: Option<String>,
    /// That release's cover, at the version its row read; `None` when it has
    /// none.
    pub representative_cover: Option<crate::db::ImageRef>,
    pub composer_names: Option<String>,
    pub linked_release_count: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DbComposerDetail {
    pub composer: DbComposerSummary,
    pub work_groups: Vec<DbComposerWorkGroup>,
    pub unlinked_release_roles: Vec<DbReleaseRoleSummary>,
    pub unlinked_track_roles: Vec<DbTrackRoleSummary>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DbComposerWorkGroup {
    pub id: String,
    pub parent: Option<DbWorkSummary>,
    pub works: Vec<DbWorkSummary>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DbWorkDetail {
    pub work: DbWorkSummary,
    pub child_works: Vec<DbWorkSummary>,
    pub releases: Vec<DbWorkReleaseSummary>,
    pub tracks: Vec<DbWorkTrackSummary>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DbWorkReleaseSummary {
    pub release_id: String,
    pub album_id: String,
    pub album_title: String,
    pub release_name: Option<String>,
    pub year: Option<i32>,
    pub media: Vec<bae_types::pressing::MediaCount>,
    pub release_index: i64,
    /// The version of the release's cover; `None` when it has none.
    pub cover_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DbReleaseRoleSummary {
    pub role: DbReleaseArtistRole,
    pub album: DbAlbum,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DbTrackRoleSummary {
    pub role: DbTrackArtistRole,
    pub track: DbTrack,
    pub album: DbAlbum,
    pub artist: DbArtist,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DbWorkTrackSummary {
    pub link: DbTrackWork,
    pub track: DbTrack,
    pub album: DbAlbum,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComposerSortField {
    Name,
    WorkCount,
    LinkedReleaseCount,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ComposerSortCriterion {
    pub field: ComposerSortField,
    pub direction: SortDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtistSortField {
    Name,
    AlbumCount,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArtistSortCriterion {
    pub field: ArtistSortField,
    pub direction: SortDirection,
}
