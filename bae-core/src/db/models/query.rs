use super::*;

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
}

#[derive(Debug, Clone, PartialEq)]
pub struct DbComposerSummary {
    pub artist: DbArtist,
    pub work_count: i64,
    pub linked_release_count: i64,
    pub unlinked_credit_count: i64,
}

/// Raw artist-summary aggregate: the artist row plus the number of distinct
/// albums whose credit list names it.
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
    /// The artist's other printed names, its merged artists' too.
    pub credit_names: Vec<crate::credits::ArtistCreditName>,
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
    pub media: Vec<bae_types::pressing::MediaCount>,
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
/// counts are not here; `OutboxQueue` is the only source for those.
#[derive(Debug, Clone)]
pub struct DbReleaseStorageSummary {
    pub release_id: String,
    pub album_id: String,
    pub album_title: String,
    pub artist_names: String,
    pub media: Vec<bae_types::pressing::MediaCount>,
    /// Whether all audio files are attached for sharing. Pending uploads are
    /// reported independently by the upload queue.
    pub remote: bool,
    /// Whether this device keeps every file of the release pinned, as the
    /// release's listing counts its pinned files.
    pub pinned: bool,
    pub file_count: i64,
    pub total_size: i64,
    /// What the library records this device holding for each audio file whose
    /// row names no stored file.
    pub unstored_audio: Vec<DbUnstoredAudio>,
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
    pub artists: Vec<crate::credits::LibraryCredit>,
    /// The album's releases in album order.
    pub releases: Vec<DbReleaseDetail>,
    /// The album's lowest-ordered release.
    pub primary_release_id: String,
}

/// A release detail plus the album context needed to project it: the album's
/// artists (the per-track artist fallback), this release's index among the
/// album's releases, and whether the album is a compilation (which decides each
/// track's display artist). Returned by `find_release_detail_context`.
#[derive(Debug, Clone, PartialEq)]
pub struct ReleaseDetailContext {
    pub detail: DbReleaseDetail,
    pub album_artists: Vec<crate::credits::LibraryCredit>,
    pub release_index: usize,
    pub is_compilation: bool,
}

/// Raw release-detail aggregate. The resolver in `LibraryManager` produces
/// the display-ready `crate::album_detail::ReleaseDetail`.
#[derive(Debug, Clone, PartialEq)]
pub struct DbReleaseDetail {
    pub release: DbRelease,
    /// The release's mediums, which its tracks' places are read against.
    pub mediums: DbReleaseMediums,
    pub tracks: Vec<DbTrackWithArtists>,
    pub files: Vec<DbFile>,
    /// Audio-format rows for this release's tracks: codec, sample rate, bit
    /// depth, channels. The file-backed windows live in `audio_segments`.
    pub audio_formats: Vec<DbAudioFormat>,
    pub audio_segments: Vec<DbAudioSegment>,
    /// Every catalog's description of this release, in the order surfaces list
    /// catalogs. Empty when no catalog describes it.
    pub records: Vec<crate::import::ReleaseRecord>,
    /// What the library records this device holding for each audio file whose
    /// row names no stored file.
    pub unstored_audio: Vec<DbUnstoredAudio>,
    /// Whether this device keeps every file of the release pinned, as the
    /// release's listing counts its pinned files.
    pub pinned: bool,
}

/// What the library records this device holding for one of a release's audio
/// files whose row names no stored file yet. Whether that original is still on
/// the disk is the disk's to say, not the database's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DbUnstoredAudio {
    /// No original here: another device imported the release.
    Elsewhere,
    /// This device's original, as it was recorded at import.
    Original(RecordedOriginal),
}

/// Where one of a release's original audio files sat on this device when it
/// was imported, and the size and modification time it had then.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RecordedOriginal {
    pub path: std::path::PathBuf,
    pub size: i64,
    pub modified_at: std::time::SystemTime,
}

/// A track row with its resolved artist rows (many-to-many join from the DB).
#[derive(Debug, Clone, PartialEq)]
pub struct DbTrackWithArtists {
    pub track: DbTrack,
    /// The track's own credits, in list order; none when it takes the
    /// album's.
    pub artists: Vec<crate::credits::LibraryCredit>,
}

/// Raw per-release slim aggregate for summary views (storage rows, release
/// pickers): `DbReleaseStorageSummary` minus the album-level joins. The resolver
/// in `LibraryManager` produces the display-ready
/// `crate::album_detail::ReleaseSummary`. Pending-upload counts are not here;
/// `OutboxQueue` is the only source for those.
#[derive(Debug, Clone, PartialEq)]
pub struct DbReleaseSummary {
    pub id: String,
    pub album_id: String,
    pub media: Vec<bae_types::pressing::MediaCount>,
    /// Whether all audio files are attached for sharing. Pending uploads are
    /// reported independently by the upload queue.
    pub remote: bool,
    /// Whether this device keeps every file of the release pinned, as the
    /// release's listing counts its pinned files.
    pub pinned: bool,
    pub file_count: i64,
    pub total_size: i64,
    /// What the library records this device holding for each audio file whose
    /// row names no stored file.
    pub unstored_audio: Vec<DbUnstoredAudio>,
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

/// Coven's committed upload queue with each file's bae display context.
#[derive(Debug, Clone, Default)]
pub struct DbOutboxQueue {
    pub paused: bool,
    pub uploads: Vec<DbOutboxUpload>,
}

/// One queued file and what it is.
#[derive(Debug, Clone)]
pub struct DbOutboxUpload {
    pub upload: coven::QueuedUpload,
    pub subject: crate::library::UploadSubject,
}
