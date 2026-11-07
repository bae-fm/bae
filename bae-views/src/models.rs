//! The raw rows the library's views read, before they are resolved into
//! what a screen shows.

use bae_records::*;
use bae_store::*;
use chrono::{DateTime, Utc};

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

/// Raw album search-result row; `artist_name` is the album's credit line,
/// each name followed by the join its source prints.
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
    pub artists: Vec<bae_records::credits::LibraryCredit>,
    /// The album's releases in album order.
    pub releases: Vec<ReleaseDetailRead>,
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
    pub image: Option<bae_records::ImageRef>,
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
    pub image: Option<bae_records::ImageRef>,
    pub album_count: i64,
}

impl DbArtistSummary {
    /// The line under the artist's name in its list, its detail and search:
    /// how many albums credit it.
    pub fn count_line(&self) -> bae_types::count_label::CountLabel {
        bae_types::count_label::CountLabel {
            key: "core.library.album_count".to_string(),
            count: u32::try_from(self.album_count).expect("an album count is never negative"),
        }
    }
}

/// Raw artist-detail aggregate, which the artist views resolve into the
/// display-ready `crate::album_detail::ArtistDetail`.
#[derive(Debug, Clone, PartialEq)]
pub struct DbArtistDetail {
    pub artist: DbArtistSummary,
    /// The artist's other printed names, its merged artists' too.
    pub credit_names: Vec<bae_records::credits::ArtistCreditName>,
    pub albums: Vec<DbAlbumSummary>,
}

impl DbComposerSummary {
    /// The line under the composer's name in its list, its detail and
    /// search: how many works it is credited on.
    pub fn count_line(&self) -> bae_types::count_label::CountLabel {
        bae_types::count_label::CountLabel {
            key: "core.library.work_count".to_string(),
            count: u32::try_from(self.work_count).expect("a work count is never negative"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DbWorkSummary {
    pub work: DbWork,
    pub representative_release_id: Option<String>,
    /// That release's cover, at the version its row read; `None` when it has
    /// none.
    pub representative_cover: Option<bae_records::ImageRef>,
    pub composer_names: Option<String>,
    pub linked_release_count: i64,
}

/// A composer's page above its rows: the composer, and the first work its
/// page lists, if it lists any.
#[derive(Debug, Clone, PartialEq)]
pub struct DbComposerDetail {
    pub composer: DbComposerSummary,
    pub default_work_id: Option<String>,
}

/// One row of a composer's page (`composer_detail_row`): its number, whether
/// it opens a heading over its part of the page, and what it shows.
#[derive(Debug, Clone, PartialEq)]
pub struct DbComposerDetailRow {
    pub entry: i64,
    pub opens_heading: bool,
    pub item: DbComposerDetailItem,
}

/// What a row of a composer's page shows.
#[derive(Debug, Clone, PartialEq)]
pub enum DbComposerDetailItem {
    /// A work, `part` when it is listed under the work heading its group.
    Work {
        work: DbWorkSummary,
        part: bool,
    },
    ReleaseCredit(DbReleaseRoleSummary),
    TrackCredit(DbTrackRoleSummary),
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

/// A release detail plus the album context needed to project it: the album's
/// artists (the per-track artist fallback), this release's index among the
/// album's releases, and whether the album is a compilation (which decides each
/// track's display artist).
#[derive(Debug, Clone, PartialEq)]
pub struct ReleaseDetailContext {
    pub detail: DbReleaseDetail,
    /// The release's entry in the download queue.
    pub download: Option<bae_transfers::DownloadEntry>,
    pub release_index: usize,
    pub is_compilation: bool,
}

/// Raw per-release slim aggregate for summary views (storage rows, release
/// pickers), which resolves into the display-ready
/// `crate::album_detail::ReleaseSummary`.
#[derive(Debug, Clone, PartialEq)]
pub struct DbReleaseSummary {
    pub id: String,
    pub album_id: String,
    pub media: Vec<bae_types::pressing::MediaCount>,
    /// Whether all audio files are attached for sharing.
    pub remote: bool,
    /// Whether this device keeps every file of the release pinned, as the
    /// release's listing counts its pinned files.
    pub pinned: bool,
    /// Whether coven is still copying the release's files to this device
    /// because it was moved to the cloud to be kept here, as the release's
    /// storage row counts the requests.
    pub keeping_here: bool,
    /// What of the release waits in this device's upload queue; `None` when
    /// nothing does.
    pub upload: Option<DbReleaseUpload>,
    /// The release's entry in this device's download queue; `None` when it
    /// has none.
    pub download: Option<bae_transfers::DownloadEntry>,
    pub file_count: i64,
    pub total_size: i64,
    /// What the library records this device holding for each audio file whose
    /// row names no stored file.
    pub unstored_audio: Vec<DbUnstoredAudio>,
}

/// Raw storage-page row: a release summary joined with its parent album summary,
/// both halves from one SQL query, and what its storage state shows. The storage views resolve them into
/// `ReleaseSummary` / `AlbumSummary`, which the UI normalizes into slices.
#[derive(Debug, Clone, PartialEq)]
pub struct DbStorageRow {
    pub release: DbReleaseSummary,
    /// When the release was added to the library.
    pub date_added: DateTime<Utc>,
    /// The version of the release's own cover; `None` when it has none.
    pub release_cover_version: Option<String>,
    pub album: DbAlbumSummary,
    pub status: crate::album_detail::StorageRowStatus,
}

/// The columns the Storage Manager view renders, as sort keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageSortField {
    AlbumTitle,
    ArtistNames,
    Media,
    FileCount,
    TotalSize,
    /// Where the release's files are, ascending: uploading, pinning, local,
    /// pinned, in the cloud only.
    Storage,
    /// When the release was added, oldest first ascending.
    DateAdded,
}

/// A single sort criterion for storage-page queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StorageSortCriterion {
    pub field: StorageSortField,
    pub direction: SortDirection,
}

/// Filter applied to a storage-page query — the three mutually-exclusive
/// choices the Storage Manager shows: every release, the local ones, or the
/// uploaded ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageFilter {
    All,
    Remote,
    Local,
}

/// What the Storage Manager's inspector shows of one release, read at once
/// so its queue entries, its files' pins and its uploads are of one moment.
#[cfg(desktop)]
#[derive(Debug, Clone, PartialEq)]
pub struct DbStorageInspector {
    /// The release's files in their order, each with its pin and upload.
    pub files: Vec<DbStorageInspectorFile>,
    /// What of the release waits in this device's upload queue.
    pub upload: Option<DbReleaseUpload>,
    /// The release's entries in the download and export queues.
    pub entries: bae_transfers::ReleaseQueueEntries,
}

/// One file of a release as the Storage Manager's inspector reads it: the
/// file, where this device's pin of it stands (`None` while it isn't pinned
/// or being pinned), and its upload while it waits in this device's upload
/// queue.
#[cfg(desktop)]
#[derive(Debug, Clone, PartialEq)]
pub struct DbStorageInspectorFile {
    pub file: DbFile,
    pub pin: Option<crate::album_detail::FilePin>,
    pub upload: Option<DbFileUpload>,
}

/// One release file waiting in this device's upload queue, read with its
/// row: the file being uploaded, whether this process is sending it now, and
/// the record of what it waits for.
#[derive(Debug, Clone, PartialEq)]
pub struct DbFileUpload {
    pub file: coven::FileId,
    pub sending: bool,
    pub pending: Option<coven::PendingRecord>,
}

/// A release's rows as its detail reads them, beside its entry in the
/// download queue.
#[derive(Debug, Clone, PartialEq)]
pub struct ReleaseDetailRead {
    pub detail: DbReleaseDetail,
    pub download: Option<bae_transfers::DownloadEntry>,
}
