use super::super::*;

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeSearchResults {
    pub albums: Vec<BridgeAlbumSearchResult>,
    pub artists: Vec<BridgeArtistSummary>,
    pub tracks: Vec<BridgeTrackSearchResult>,
    pub composers: Vec<BridgeComposerSummary>,
    pub works: Vec<BridgeWorkSummary>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeAlbumSearchResult {
    pub id: String,
    pub title: String,
    pub year: Option<i32>,
    pub artist_name: String,
    /// Reference to the album's cover (the primary release's cover), or `None`.
    /// The UI fetches the bytes by id and caches under `(id, version)`.
    pub cover: Option<BridgeImageRef>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeTrackSearchResult {
    pub id: String,
    pub title: String,
    /// The track length as a clock label's fields ("3:07"), or `None` when there
    /// is nothing to label. The raw milliseconds do not cross — the search row
    /// only ever shows the clock, never the number.
    pub duration_clock: Option<BridgeDurationClock>,
    pub album_id: String,
    pub album_title: String,
    pub artist_name: String,
    /// The cover of the track's own release, or `None`. Same fetch/caching
    /// contract as [`BridgeAlbumSearchResult::cover`].
    pub cover: Option<BridgeImageRef>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeComposerSummary {
    pub artist_id: String,
    pub name: String,
    pub sort_name: Option<String>,
    pub work_count: i64,
    pub linked_release_count: i64,
    pub unlinked_credit_count: i64,
    pub image: Option<BridgeImageRef>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeWorkSummary {
    pub work_id: String,
    pub title: String,
    pub disambiguation: Option<String>,
    pub work_type: Option<String>,
    pub parent_work_id: Option<String>,
    pub composer_names: Option<String>,
    pub linked_release_count: i64,
    pub representative_release_id: Option<String>,
    pub representative_cover: Option<BridgeImageRef>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeReleaseRoleSummary {
    pub release_id: String,
    pub album_id: String,
    pub album_title: String,
    pub source: BridgeMetadataSource,
    pub source_credit: Option<String>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeTrackRoleSummary {
    pub track_id: String,
    pub track_title: String,
    pub release_id: String,
    pub album_id: String,
    pub album_title: String,
    pub artist_id: String,
    pub artist_name: String,
    pub source: BridgeMetadataSource,
    pub source_credit: Option<String>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeWorkTrackSummary {
    pub track_id: String,
    pub track_title: String,
    pub release_id: String,
    pub album_id: String,
    pub album_title: String,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeWorkReleaseSummary {
    pub release_id: String,
    pub album_id: String,
    pub album_title: String,
    pub display_name: String,
    pub format: Option<String>,
    pub cover: Option<BridgeImageRef>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeComposerDetail {
    pub composer: BridgeComposerSummary,
    pub work_groups: Vec<BridgeComposerWorkGroup>,
    pub unlinked_release_roles: Vec<BridgeReleaseRoleSummary>,
    pub unlinked_track_roles: Vec<BridgeTrackRoleSummary>,
    pub default_work_id: Option<String>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeComposerWorkGroup {
    pub id: String,
    pub parent: Option<BridgeWorkSummary>,
    pub works: Vec<BridgeWorkSummary>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeWorkDetail {
    pub work: BridgeWorkSummary,
    pub child_works: Vec<BridgeWorkSummary>,
    pub releases: Vec<BridgeWorkReleaseSummary>,
    pub tracks: Vec<BridgeWorkTrackSummary>,
}

#[derive(Debug, Clone, Copy, uniffi::Enum)]
pub enum BridgeComposerSortField {
    Name,
    WorkCount,
    LinkedReleaseCount,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeComposerSortCriterion {
    pub field: BridgeComposerSortField,
    pub direction: BridgeSortDirection,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeArtistSummary {
    pub artist_id: String,
    pub name: String,
    /// Distinct albums this artist is an album artist of (primary FK or
    /// `album_artists` junction).
    pub album_count: i64,
    pub image: Option<BridgeImageRef>,
}

/// One existing library artist offered by an artist picker. This keeps every
/// stored identity field so equal display names remain distinguishable.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeArtistSearchResult {
    pub artist: BridgeExistingArtist,
    pub image: Option<BridgeImageRef>,
}

impl BridgeArtistSearchResult {
    pub(crate) fn from_core(result: bae_core::album_detail::ArtistSearchResult) -> Self {
        let bae_core::album_detail::ArtistSearchResult { artist, image } = result;
        Self {
            artist: BridgeExistingArtist::from_core(bae_core::import::ExistingArtist::from(artist)),
            image: image.map(BridgeImageRef::from_core),
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeArtistDetail {
    pub artist: BridgeArtistSummary,
    /// The artist's albums in discography order: year ascending with unknown
    /// years last, then title.
    pub albums: Vec<BridgeAlbum>,
}

#[derive(Debug, Clone, Copy, uniffi::Enum)]
pub enum BridgeArtistSortField {
    Name,
    AlbumCount,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeArtistSortCriterion {
    pub field: BridgeArtistSortField,
    pub direction: BridgeSortDirection,
}

/// One row on the Storage Manager: a release paired with its parent
/// album. The UI splits these into separate slices (releases +
/// summaries) so in-band metadata changes (album rename, pin toggle)
/// re-render affected rows without list rebuilds.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeStorageRow {
    pub release: BridgeReleaseSummary,
    pub album: BridgeAlbum,
}

/// One page of the Storage Manager list. `total_count` reflects the
/// filtered subset, not the full library — so paginated list machinery
/// knows where to stop.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeStoragePage {
    pub rows: Vec<BridgeStorageRow>,
    pub total_count: u64,
}

/// Column the Storage Manager can sort by. Mirrors the sortable
/// columns `StorageManagerView` renders today.
#[derive(Debug, Clone, Copy, uniffi::Enum)]
pub enum BridgeStorageSortField {
    AlbumTitle,
    ArtistNames,
    Format,
    FileCount,
    TotalSize,
}

#[derive(Debug, Clone, Copy, uniffi::Enum)]
pub enum BridgeStorageSortDirection {
    Ascending,
    Descending,
}

#[derive(Debug, Clone, Copy, uniffi::Record)]
pub struct BridgeStorageSort {
    pub field: BridgeStorageSortField,
    pub direction: BridgeStorageSortDirection,
}

/// Filter chip applied to the Storage Manager list. Mirrors the four
/// mutually-exclusive chips the UI exposes.
#[derive(Debug, Clone, Copy, uniffi::Enum)]
pub enum BridgeStorageFilter {
    All,
    Remote,
    Local,
    Uploading,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeQueueEntry {
    /// Per-instance id: the same track queued twice yields two entries with two
    /// ids, so the UI keys each row on a stable unique identity and targets
    /// remove/reorder/skip at one instance.
    pub entry_id: String,
    pub track_id: String,
    pub title: String,
    pub artist_names: String,
    /// The track length as a clock label's fields ("3:07"), or `None` when there
    /// is nothing to label. The raw milliseconds do not cross — a queue row only
    /// ever shows the clock, never the number.
    pub duration_clock: Option<BridgeDurationClock>,
    pub album_title: String,
    /// The track's own release's cover, or `None` when it has none. Versioned,
    /// so the UI's art cache key moves when the cover bytes change.
    pub cover_image: Option<BridgeImageRef>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeQueueSnapshot {
    pub manual: Vec<BridgeQueueEntry>,
    pub context: Option<BridgePlaybackContext>,
    pub has_next: bool,
    pub has_previous: bool,
    /// The queue revision this snapshot was resolved from. The UI accepts page
    /// subscription values only while their revision matches this one.
    pub revision: u64,
}

/// One page of the context's upcoming tail, fetched by offset/limit past the
/// initial window `BridgePlaybackContext.upcoming` already carries.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeQueueUpcomingPage {
    pub revision: u64,
    pub entries: Vec<BridgeQueueEntry>,
}

/// Which kind of source the context plays from, so the UI labels the section
/// (a release's "Playing From" vs the whole library). A single- or multi-release
/// source is both `Release` here — the queue pane's "Playing From" title is the
/// same for one album or several. The release ids stay in core (the UI labels by
/// kind, not by id here).
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgePlaybackSourceKind {
    Release,
    Library,
}

impl BridgePlaybackSourceKind {
    pub(crate) fn from_core(source: &bae_core::playback::ContextSource) -> Self {
        match source {
            bae_core::playback::ContextSource::Release(_)
            | bae_core::playback::ContextSource::Releases(_) => Self::Release,
            bae_core::playback::ContextSource::Library => Self::Library,
        }
    }
}

/// The context lane (what the queue is playing from), delivered alongside the
/// manual lane so each UI renders the two as distinct sections:
/// its kind (release vs library, for the section label), its not-yet-played tail,
/// plus whether it was ordered by shuffle (the UI shows a shuffle indicator when
/// so).
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgePlaybackContext {
    pub kind: BridgePlaybackSourceKind,
    /// The display title of what the context plays from — the album title when
    /// the source is a single release, `None` for a multi-release source or
    /// the whole library. The UI appends it to the section label; the label
    /// prose itself stays UI-side (localized).
    pub source_title: Option<String>,
    pub shuffled: bool,
    /// The first page of the not-yet-played tail — not the whole tail. See
    /// `upcoming_total` for the full length and the upcoming-page subscription
    /// for the rest.
    pub upcoming: Vec<BridgeQueueEntry>,
    /// The full length of the not-yet-played tail, including entries beyond
    /// `upcoming`. The UI renders a placeholder for every index up to this and
    /// pages in the rest as it scrolls.
    pub upcoming_total: u64,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeSyncStatusSnapshot {
    pub error: Option<BridgeError>,
    pub last_sync_time: Option<i64>,
    pub syncing: bool,
    pub sync_ready: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct BridgeEagerCacheFillProgress {
    pub files_done: u64,
    pub files_total: u64,
    pub bytes_done: u64,
    pub bytes_total: u64,
}

impl BridgeEagerCacheFillProgress {
    fn from_core(progress: bae_core::library::EagerCacheFillProgress) -> Self {
        Self {
            files_done: progress.files_done,
            files_total: progress.files_total,
            bytes_done: progress.bytes_done,
            bytes_total: progress.bytes_total,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeEagerCacheFillStatus {
    NotRunning,
    Scanning {
        title_key: String,
    },
    Downloading {
        title_key: String,
        progress: BridgeEagerCacheFillProgress,
    },
    Complete {
        files_total: u64,
        bytes_total: u64,
    },
    Cancelled {
        title_key: String,
        progress: BridgeEagerCacheFillProgress,
    },
    Failed {
        title_key: String,
        progress: BridgeEagerCacheFillProgress,
        error: String,
    },
}

impl BridgeEagerCacheFillStatus {
    pub(crate) fn from_core(status: bae_core::library::EagerCacheFillStatus) -> Self {
        use bae_core::library::EagerCacheFillStatus;
        let title_key = bae_core::sync::eager_cache_fill_title_key(&status).map(str::to_string);
        match status {
            EagerCacheFillStatus::NotRunning => Self::NotRunning,
            EagerCacheFillStatus::Scanning => Self::Scanning {
                title_key: title_key.expect("visible eager cache status has a title key"),
            },
            EagerCacheFillStatus::Downloading(progress) => Self::Downloading {
                title_key: title_key.expect("visible eager cache status has a title key"),
                progress: BridgeEagerCacheFillProgress::from_core(progress),
            },
            EagerCacheFillStatus::Complete {
                files_total,
                bytes_total,
            } => Self::Complete {
                files_total,
                bytes_total,
            },
            EagerCacheFillStatus::Cancelled(progress) => Self::Cancelled {
                title_key: title_key.expect("visible eager cache status has a title key"),
                progress: BridgeEagerCacheFillProgress::from_core(progress),
            },
            EagerCacheFillStatus::Failed { progress, error } => Self::Failed {
                title_key: title_key.expect("visible eager cache status has a title key"),
                progress: BridgeEagerCacheFillProgress::from_core(progress),
                error: error.to_string(),
            },
        }
    }
}

/// What the sync indicator shows, in precedence order. Mirror of bae-core's
/// `SyncIndicator`. The UI maps a variant to a label and colour and renders the
/// `Synced` time; it never decides which state wins — a stale timestamp used to
/// read as "Synced" on a loop that never came up, on Windows, because each app
/// wrote its own precedence.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeSyncIndicator {
    Error,
    Syncing,
    Synced { last_sync_time: Option<i64> },
    Idle,
}

impl BridgeSyncIndicator {
    fn from_core(indicator: bae_core::library::SyncIndicator) -> Self {
        use bae_core::library::SyncIndicator;
        match indicator {
            SyncIndicator::Error => Self::Error,
            SyncIndicator::Syncing => Self::Syncing,
            SyncIndicator::Synced { last_sync_time } => Self::Synced { last_sync_time },
            SyncIndicator::Idle => Self::Idle,
        }
    }
}

/// The sync indicator for a status snapshot — the precedence decided in bae-core.
/// The UI holds the snapshot already; this turns it into the one badge state.
#[uniffi::export]
pub fn bridge_sync_indicator(snapshot: &BridgeSyncStatusSnapshot) -> BridgeSyncIndicator {
    BridgeSyncIndicator::from_core(bae_core::library::SyncIndicator::resolve(
        snapshot.error.is_some(),
        snapshot.syncing,
        snapshot.sync_ready,
        snapshot.last_sync_time,
    ))
}
