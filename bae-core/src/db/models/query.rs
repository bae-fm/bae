use super::*;

/// Raw combined search-result aggregate. No formatting — the resolver in
/// `LibraryManager` produces the display-ready `crate::album_detail::SearchResults`.
#[derive(Debug, Clone)]
pub struct DbLibrarySearchResults {
    pub albums: Vec<DbAlbumSearchResult>,
    pub artists: Vec<DbArtistSummary>,
    pub tracks: Vec<DbTrackSearchResult>,
    pub composers: Vec<DbComposerSummary>,
    pub works: Vec<DbWorkSummary>,
}

/// Raw album search-result row with the primary artist name joined in SQL.
#[derive(Debug, Clone)]
pub struct DbAlbumSearchResult {
    pub id: String,
    pub title: String,
    pub year: Option<i32>,
    /// User-chosen primary release. `None` if unset — the resolver applies
    /// `resolve_primary_release_id` over `release_ids`.
    pub primary_release_id: Option<String>,
    /// The album's releases in `created_at` order — what the fallback picks from.
    pub release_ids: Vec<String>,
    pub artist_name: String,
}

/// Raw track search-result row with album and artist info joined in SQL.
/// No formatted duration label — the resolver in `LibraryManager` formats it.
#[derive(Debug, Clone)]
pub struct DbTrackSearchResult {
    pub id: String,
    pub title: String,
    pub duration_ms: Option<i64>,
    /// The release the track belongs to — the key the resolver looks the
    /// track's cover up by.
    pub release_id: String,
    pub album_id: String,
    pub album_title: String,
    pub artist_name: String,
}

#[derive(Debug, Clone)]
pub struct DbComposerSummary {
    pub artist: DbArtist,
    pub work_count: i64,
    pub linked_release_count: i64,
    pub unlinked_credit_count: i64,
}

/// Raw artist-summary aggregate: the artist row plus its distinct album
/// count over both album-artist links (the primary `albums.artist_id` FK
/// and `album_artists` junction rows).
#[derive(Debug, Clone)]
pub struct DbArtistSummary {
    pub artist: DbArtist,
    pub album_count: i64,
}

/// Raw artist-detail aggregate. The resolver in `LibraryManager` produces
/// the display-ready `crate::album_detail::ArtistDetail`.
#[derive(Debug, Clone)]
pub struct DbArtistDetail {
    pub artist: DbArtistSummary,
    pub albums: Vec<DbAlbumSummary>,
}

#[derive(Debug, Clone)]
pub struct DbWorkSummary {
    pub work: DbWork,
    pub parent_work_id: Option<String>,
    pub representative_release_id: Option<String>,
    pub composer_names: Option<String>,
    pub linked_release_count: i64,
}

#[derive(Debug, Clone)]
pub struct DbComposerDetail {
    pub composer: DbComposerSummary,
    pub work_groups: Vec<DbComposerWorkGroup>,
    pub unlinked_release_roles: Vec<DbReleaseRoleSummary>,
    pub unlinked_track_roles: Vec<DbTrackRoleSummary>,
}

#[derive(Debug, Clone)]
pub struct DbComposerWorkGroup {
    pub id: String,
    pub parent: Option<DbWorkSummary>,
    pub works: Vec<DbWorkSummary>,
}

#[derive(Debug, Clone)]
pub struct DbWorkDetail {
    pub work: DbWorkSummary,
    pub child_works: Vec<DbWorkSummary>,
    pub releases: Vec<DbWorkReleaseSummary>,
    pub tracks: Vec<DbWorkTrackSummary>,
}

#[derive(Debug, Clone)]
pub struct DbWorkReleaseSummary {
    pub release_id: String,
    pub album_id: String,
    pub album_title: String,
    pub release_name: Option<String>,
    pub year: Option<i32>,
    pub format: Option<String>,
    pub release_index: i64,
}

#[derive(Debug, Clone)]
pub struct DbReleaseRoleSummary {
    pub role: DbReleaseArtistRole,
    pub album: DbAlbum,
}

#[derive(Debug, Clone)]
pub struct DbTrackRoleSummary {
    pub role: DbTrackArtistRole,
    pub track: DbTrack,
    pub album: DbAlbum,
    pub artist: DbArtist,
}

#[derive(Debug, Clone)]
pub struct DbWorkTrackSummary {
    pub link: DbTrackWork,
    pub track: DbTrack,
    pub album: DbAlbum,
}

/// Raw per-release storage summary, assembled in one SQL query (no N+1). No
/// formatting, no derivation — the resolver in `LibraryManager` produces the
/// display-ready `crate::album_detail::ReleaseStorageSummary` (deriving
/// `storage_state` from `remote` and formatting `total_size`). Pending-upload
/// counts are not here; `OutboxSnapshot` is the only source for those.
#[derive(Debug, Clone)]
pub struct DbReleaseStorageSummary {
    pub release_id: String,
    pub album_id: String,
    pub album_title: String,
    pub artist_names: String,
    pub format: Option<String>,
    /// The shared `releases.remote` fact: audio in the cloud vs local to a device.
    /// The resolver reads `Local` straight off `!remote`; for a remote release it
    /// asks coven's cache (via `any_file_id`) whether it is pinned.
    pub remote: bool,
    /// The id of one of the release's files, or `None` when it has none. Used to
    /// ask coven's cache whether the release is pinned — pin/unpin act on all a
    /// release's blobs together, so any one file stands for the release.
    pub any_file_id: Option<String>,
    pub file_count: i64,
    pub total_size: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlbumSortField {
    Title,
    Artist,
    Year,
    DateAdded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDirection {
    Ascending,
    Descending,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AlbumSortCriterion {
    pub field: AlbumSortField,
    pub direction: SortDirection,
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

/// Raw album-detail aggregate. The resolver in `LibraryManager` produces
/// the display-ready `crate::album_detail::AlbumDetail`.
#[derive(Debug, Clone)]
pub struct DbAlbumDetail {
    pub album: DbAlbum,
    pub artists: Vec<DbArtist>,
    pub releases: Vec<DbReleaseDetail>,
}

/// A release detail plus the album context needed to project it: the album's
/// artists (the per-track artist fallback), this release's index among the
/// album's releases, and whether the album is a compilation (which decides each
/// track's display artist). Returned by `find_release_detail_context`.
#[derive(Debug, Clone)]
pub struct ReleaseDetailContext {
    pub detail: DbReleaseDetail,
    pub album_artists: Vec<DbArtist>,
    pub release_index: usize,
    pub is_compilation: bool,
}

/// Raw release-detail aggregate. The resolver in `LibraryManager` produces
/// the display-ready `crate::album_detail::ReleaseDetail`.
#[derive(Debug, Clone)]
pub struct DbReleaseDetail {
    pub release: DbRelease,
    pub tracks: Vec<DbTrackWithArtists>,
    pub files: Vec<DbFile>,
    /// Audio-format rows for this release's tracks: codec, sample rate, bit
    /// depth, channels. The file-backed windows live in `audio_segments`.
    pub audio_formats: Vec<DbAudioFormat>,
    pub audio_segments: Vec<DbAudioSegment>,
    /// All identity rows for this release. Empty for Unknown imports.
    pub identities: Vec<crate::import::ReleaseIdentity>,
}

/// A track row with its resolved artist rows (many-to-many join from the DB).
#[derive(Debug, Clone)]
pub struct DbTrackWithArtists {
    pub track: DbTrack,
    pub artists: Vec<DbArtist>,
}

/// Raw per-release slim aggregate for summary views (storage rows, release
/// pickers): `DbReleaseStorageSummary` minus the album-level joins. The resolver
/// in `LibraryManager` produces the display-ready
/// `crate::album_detail::ReleaseSummary`. Pending-upload counts are not here;
/// `OutboxSnapshot` is the only source for those.
#[derive(Debug, Clone)]
pub struct DbReleaseSummary {
    pub id: String,
    pub album_id: String,
    pub format: Option<String>,
    /// The shared `releases.remote` fact: audio in the cloud vs local to a device.
    /// The resolver reads `Local` straight off `!remote`; for a remote release it
    /// asks coven's cache (via `any_file_id`) whether it is pinned.
    pub remote: bool,
    /// The id of one of the release's files, or `None` when it has none. Used to
    /// ask coven's cache whether the release is pinned — pin/unpin act on all a
    /// release's blobs together, so any one file stands for the release.
    pub any_file_id: Option<String>,
    pub file_count: i64,
    pub total_size: i64,
}

/// Raw storage-page row: a release summary joined with its parent album summary,
/// both halves from one SQL query. The resolver in `LibraryManager` turns them
/// into `ReleaseSummary` / `AlbumSummary`, which the UI normalizes into slices.
#[derive(Debug, Clone)]
pub struct DbStorageRow {
    pub release: DbReleaseSummary,
    pub album: DbAlbumSummary,
}

/// The columns the Storage Manager view renders, as sort keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageSortField {
    AlbumTitle,
    ArtistNames,
    Format,
    FileCount,
    TotalSize,
}

/// A single sort criterion for storage-page queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StorageSortCriterion {
    pub field: StorageSortField,
    pub direction: SortDirection,
}

/// Filter applied to a storage-page query — the four mutually-exclusive chips
/// the Storage Manager shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageFilter {
    All,
    Remote,
    Local,
    Uploading,
}

/// What coven's durable cloud queue is holding, as the Storage Manager renders
/// it: pending uploads and pending cloud tombstones, each oldest first.
///
/// coven owns the queue itself — which blob, under which gated root, how many
/// attempts and why the last one failed. bae owns only the context a person
/// reads: the file's name and size, its release, and that release's album
/// title. [`Database::outbox_queue`](crate::db::Database::outbox_queue) reads
/// the first from coven and joins the second from bae's own tables.
#[derive(Debug, Clone, Default)]
pub struct DbOutboxQueue {
    pub uploads: Vec<DbOutboxUpload>,
    pub deletes: Vec<DbOutboxDelete>,
}

/// One queued upload plus its bae context.
///
/// The context fields are `None` when the `release_files` row the blob hangs
/// off is gone — the upload is still queued and still rendered, just labelled
/// by its file id instead of a filename.
#[derive(Debug, Clone)]
pub struct DbOutboxUpload {
    /// The release whose make-Remote enqueued this upload — the gated root
    /// every one of a release's uploads shares, and what the queue pane groups
    /// by. `None` when coven reports a root outside `releases`, which is not a
    /// shape bae enqueues.
    pub release_id: Option<String>,
    /// The `release_files` row carrying the blob. Progress is reported under
    /// this id, and it is the blob's id too — the table declares no separate
    /// id column, so coven reads the blob id off the primary key.
    pub file_id: String,
    /// Failed transfer attempts so far; 0 for one never yet tried.
    pub attempt_count: u64,
    /// Why the last attempt failed, if one has.
    pub last_error: Option<String>,
    /// Enqueue time as Unix epoch milliseconds, taken from coven's HLC stamp.
    pub created_at: i64,
    pub file_name: Option<String>,
    pub file_size: Option<i64>,
    /// The album title of `release_id`, for the group heading.
    pub album_title: Option<String>,
}

/// One cloud object still owed a removal. A tombstone outlives the row that
/// named it, so there is no bae context to join — the blob's namespace and id
/// are all that is left of it.
#[derive(Debug, Clone)]
pub struct DbOutboxDelete {
    pub namespace: String,
    pub blob_id: String,
    /// Enqueue time as Unix epoch milliseconds, taken from coven's HLC stamp.
    pub created_at: i64,
}
