use super::*;

/// Raw combined search-result aggregate. No formatting — the resolver in
/// `LibraryManager` produces the display-ready `crate::album_detail::SearchResults`.
#[derive(Debug, Clone, PartialEq)]
pub struct DbLibrarySearchResults {
    pub albums: Vec<DbAlbumSearchResult>,
    pub artists: Vec<DbArtistSummary>,
    pub tracks: Vec<DbTrackSearchResult>,
    pub composers: Vec<DbComposerSummary>,
    pub works: Vec<DbWorkSummary>,
}

/// Raw album search-result row with the primary artist name joined in SQL.
#[derive(Debug, Clone, PartialEq)]
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
#[derive(Debug, Clone, PartialEq)]
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

#[derive(Debug, Clone, PartialEq)]
pub struct DbComposerSummary {
    pub artist: DbArtist,
    pub work_count: i64,
    pub linked_release_count: i64,
    pub unlinked_credit_count: i64,
}

/// Raw artist-summary aggregate: the artist row plus its distinct album
/// count over both album-artist links (the primary `albums.artist_id` FK
/// and `album_artists` junction rows).
#[derive(Debug, Clone, PartialEq)]
pub struct DbArtistSummary {
    pub artist: DbArtist,
    pub album_count: i64,
}

/// Raw artist-detail aggregate. The resolver in `LibraryManager` produces
/// the display-ready `crate::album_detail::ArtistDetail`.
#[derive(Debug, Clone, PartialEq)]
pub struct DbArtistDetail {
    pub artist: DbArtistSummary,
    pub albums: Vec<DbAlbumSummary>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DbWorkSummary {
    pub work: DbWork,
    pub parent_work_id: Option<String>,
    pub representative_release_id: Option<String>,
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
    pub format: Option<String>,
    pub release_index: i64,
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
#[derive(Debug, Clone, PartialEq)]
pub struct DbAlbumDetail {
    pub album: DbAlbum,
    pub artists: Vec<DbArtist>,
    pub releases: Vec<DbReleaseDetail>,
}

/// A release detail plus the album context needed to project it: the album's
/// artists (the per-track artist fallback), this release's index among the
/// album's releases, and whether the album is a compilation (which decides each
/// track's display artist). Returned by `find_release_detail_context`.
#[derive(Debug, Clone, PartialEq)]
pub struct ReleaseDetailContext {
    pub detail: DbReleaseDetail,
    pub album_artists: Vec<DbArtist>,
    pub release_index: usize,
    pub is_compilation: bool,
}

/// Raw release-detail aggregate. The resolver in `LibraryManager` produces
/// the display-ready `crate::album_detail::ReleaseDetail`.
#[derive(Debug, Clone, PartialEq)]
pub struct DbReleaseDetail {
    pub release: DbRelease,
    pub tracks: Vec<DbTrackWithArtists>,
    pub files: Vec<DbFile>,
    /// Audio-format rows for this release's tracks: codec, sample rate, bit
    /// depth, channels. The file-backed windows live in `audio_segments`.
    pub audio_formats: Vec<DbAudioFormat>,
    pub audio_segments: Vec<DbAudioSegment>,
    /// All external identity rows for this release. Empty for File Tags and direct entry.
    pub identities: Vec<crate::import::ReleaseIdentity>,
}

/// A track row with its resolved artist rows (many-to-many join from the DB).
#[derive(Debug, Clone, PartialEq)]
pub struct DbTrackWithArtists {
    pub track: DbTrack,
    pub artists: Vec<DbArtist>,
}

/// Raw per-release slim aggregate for summary views (storage rows, release
/// pickers): `DbReleaseStorageSummary` minus the album-level joins. The resolver
/// in `LibraryManager` produces the display-ready
/// `crate::album_detail::ReleaseSummary`. Pending-upload counts are not here;
/// `OutboxSnapshot` is the only source for those.
#[derive(Debug, Clone, PartialEq)]
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
#[derive(Debug, Clone, PartialEq)]
pub struct DbStorageRow {
    pub release: DbReleaseSummary,
    pub album: DbAlbumSummary,
}

/// The columns the Storage Manager view renders, as sort keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageSortField {
    AlbumTitle,
    ArtistNames,
    Media,
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
    pub make_remotes: Vec<DbMakeRemote>,
}

/// One queued blob upload plus its bae context.
#[derive(Debug, Clone)]
pub struct DbOutboxUpload {
    /// The release whose make-Remote enqueued this upload — the gated root
    /// every one of a release's uploads shares, and what the queue pane groups
    /// by. Roots outside `releases` fail the outbox projection.
    pub release_id: String,
    /// Coven's exact blob-bearing row version. It carries the table-scoped row
    /// identity, immutable cloud blob identity, and declared plaintext size as
    /// one value shared with upload lifecycle callbacks.
    pub blob: coven::RowBlobRef,
    /// Coven's durable upload handoff. Transient preparation/upload callbacks
    /// refine this while work is moving, but never replace it as the restart
    /// truth.
    pub phase: coven::QueuedUploadPhase,
    /// Exact encrypted/browsable object bytes once preparation has produced
    /// the provider payload. Pending preparation has no provider denominator.
    pub provider_bytes_total: Option<u64>,
    /// Failed transfer attempts so far; 0 for one never yet tried.
    pub attempt_count: u64,
    /// Why the last attempt failed, if one has. Coven preserves the failure's
    /// actionable kind rather than reducing it to display prose.
    pub last_failure: Option<coven::OutboxFailure>,
    /// Enqueue time as Unix epoch milliseconds, taken from coven's HLC stamp.
    pub created_at: i64,
    /// The domain label for this upload. Platforms localize the named image
    /// kinds and render an original filename verbatim.
    pub label: crate::library::UploadFileLabel,
    /// The album title of `release_id`, for the group heading.
    pub album_title: String,
}

/// One durable make-Remote intent plus the bae title its release group renders.
#[derive(Debug, Clone)]
pub struct DbMakeRemote {
    pub transition: coven::QueuedMakeRemote,
    pub album_title: String,
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
