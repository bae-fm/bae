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
    /// The track length as a clock label's fields ("3:07"). The raw
    /// milliseconds do not cross — the search row only ever shows the clock,
    /// never the number.
    pub duration_clock: BridgeDurationClock,
    /// The release the track is on — what playing it from a search starts.
    pub release_id: String,
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
    /// The line under its name: how many works it is credited on.
    pub count_line: BridgeCountLabel,
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
    pub source: BridgeCatalog,
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
    pub source: BridgeCatalog,
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
    pub name: BridgeReleaseName,
    /// What the release is made of, worded: "2×CD", "Vinyl".
    pub media: Vec<crate::types::BridgeFactTerm>,
    pub cover: Option<BridgeImageRef>,
}

/// A composer's page above its rows: the composer, and the work the page
/// opens first. Its rows are read in windows
/// (`AppHandle::subscribe_composer_detail_rows`).
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeComposerDetail {
    pub composer: BridgeComposerSummary,
    pub default_work_id: Option<String>,
}

/// One row of a composer's page, in the page's order: its works, then its
/// unlinked release credits, then its unlinked track credits.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeComposerDetailRow {
    /// What a screen tells the row apart by.
    pub id: i64,
    /// The heading to draw above the row: set on the first row of each part
    /// of the page.
    pub heading: Option<BridgeComposerHeading>,
    pub item: BridgeComposerDetailItem,
}

/// The heading over a part of a composer's page.
#[derive(Debug, Clone, Copy, uniffi::Enum)]
pub enum BridgeComposerHeading {
    Works,
    Credits,
    Recordings,
}

/// What a row of a composer's page shows.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeComposerDetailItem {
    /// A work; `part` when it is listed under the work heading its group,
    /// which the page indents it below.
    Work {
        work: BridgeWorkSummary,
        part: bool,
    },
    ReleaseCredit {
        credit: BridgeReleaseRoleSummary,
    },
    TrackCredit {
        credit: BridgeTrackRoleSummary,
    },
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
    /// The line under its name: how many albums credit it.
    pub count_line: BridgeCountLabel,
    pub image: Option<BridgeImageRef>,
}

/// One existing library artist offered by an artist picker. This keeps every
/// stored identity field so equal display names remain distinguishable.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeArtistSearchResult {
    pub artist: BridgeExistingArtist,
    pub image: Option<BridgeImageRef>,
    /// The artist's other printed names, which the picker offers under it.
    pub credit_names: Vec<BridgeArtistCreditName>,
}

impl BridgeArtistSearchResult {
    pub(crate) fn from_core(result: bae_views::album_detail::ArtistSearchResult) -> Self {
        let bae_views::album_detail::ArtistSearchResult { artist, image, credit_names } = result;
        Self {
            artist: BridgeExistingArtist::from_core(bae_records::ExistingArtist::from(artist)),
            image: image.map(BridgeImageRef::from_core),
            credit_names: credit_names.into_iter().map(BridgeArtistCreditName::from_core).collect(),
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeArtistDetail {
    pub artist: BridgeArtistSummary,
    /// The artist's other printed names, in name order, which its page lists
    /// and edits.
    pub credit_names: Vec<BridgeArtistCreditName>,
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
/// album, what its storage state shows, and when the release was added. The
/// UI splits these into separate slices (releases + summaries) so in-band
/// metadata changes (album rename, pin toggle) re-render affected rows
/// without list rebuilds.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeStorageRow {
    pub release: BridgeReleaseSummary,
    pub album: BridgeAlbum,
    pub status: BridgeStorageRowStatus,
    pub date_added: std::time::SystemTime,
}

/// What a Storage Manager row's storage state shows: where its files are, in
/// the order the list sorts by storage, or a transition this process runs
/// while they rest. Mirrors `StorageRowStatus`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeStorageRowStatus {
    Uploading,
    Pinning,
    Transferring { action: crate::types::BridgeReleaseStorageAction },
    Local,
    Pinned,
    Cloud,
}

/// Column the Storage Manager can sort by. Mirrors `StorageSortField`.
#[derive(Debug, Clone, Copy, uniffi::Enum)]
pub enum BridgeStorageSortField {
    AlbumTitle,
    ArtistNames,
    Media,
    FileCount,
    TotalSize,
    /// Uploading, pinning, local, pinned, then in the cloud only, ascending.
    Storage,
    /// When the release was added, oldest first ascending.
    DateAdded,
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

/// Filter applied to the Storage Manager list: every release, the uploaded
/// ones, or the local ones. Mirrors `StorageFilter`.
#[derive(Debug, Clone, Copy, uniffi::Enum)]
pub enum BridgeStorageFilter {
    All,
    Remote,
    Local,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeQueueEntry {
    /// The row's id, unique across both lanes and never given to another row:
    /// every row is an entry of its own, so the same track queued twice is two
    /// rows. The UI keys each row on it and targets remove/reorder/skip at one
    /// row.
    pub entry_id: String,
    pub track_id: String,
    pub display: BridgeTrackDisplay,
    /// The track length as a clock label's fields ("3:07"). The raw
    /// milliseconds do not cross — a queue row only ever shows the clock, never
    /// the number.
    pub duration_clock: BridgeDurationClock,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeQueueSnapshot {
    pub manual: Vec<BridgeQueueEntry>,
    pub context: Option<BridgePlaybackContext>,
    pub has_next: bool,
    pub has_previous: bool,
    /// How many writes have changed Up Next: a screen holding an Up Next row
    /// it moved shows this value's order once it reaches the version the
    /// move answered with.
    pub manual_version: u64,
}

/// The last time tracks went into Up Next: which addition since playback
/// started with the library it was, counting from one, and how many tracks it
/// put in. Each addition is a value of its own, even two alike in a row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct BridgeQueueAddition {
    pub number: u64,
    pub tracks: u32,
}

mirror_struct! {
    BridgeQueueAddition = bae_playback::QueueAddition,
    from_core: pub(crate) fn,
    fields: { number, tracks },
}

/// Which kind of source the context plays from, so the UI labels the section
/// (a release's "Playing from", a list of tracks, or the whole library). A
/// single- or multi-release source is both `Release` here — the queue pane's
/// "Playing from" title is the same for one album or several. A single
/// release's id stays in core (the UI labels by kind, not by id here).
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgePlaybackSourceKind {
    Release,
    Tracks,
    Library,
}

impl BridgePlaybackSourceKind {
    pub(crate) fn from_core(source: &bae_playback::ContextSource) -> Self {
        match source {
            bae_playback::ContextSource::Release(_) | bae_playback::ContextSource::Releases => {
                Self::Release
            }
            bae_playback::ContextSource::Tracks => Self::Tracks,
            bae_playback::ContextSource::Library => Self::Library,
        }
    }
}

/// The context lane (what the queue is playing from), delivered alongside the
/// manual lane so each UI renders the two as distinct sections: its kind
/// (release, list of tracks or library, for the section label) and whether it
/// was ordered by shuffle (the UI shows a shuffle indicator when so). Its
/// tracks are read through `subscribe_queue_upcoming`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgePlaybackContext {
    pub kind: BridgePlaybackSourceKind,
    /// The display title of what the context plays from — the album title when
    /// the source is a single release, `None` for a multi-release source or
    /// the whole library. The UI appends it to the section label; the label
    /// prose itself stays UI-side (localized).
    pub source_title: Option<String>,
    pub shuffled: bool,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeSyncStatusSnapshot {
    pub error: Option<BridgeError>,
    /// What coven waits on, each with the action the person can take.
    pub pending: Vec<crate::types::BridgePendingRecord>,
    /// What the status line names, or `None` while this device has no
    /// storage set up. A library with a recorded location in that state has
    /// lost this device's credentials for it: the app offers signing in to
    /// that location again.
    pub indicator: Option<BridgeSyncIndicator>,
    /// Whether the app offers Reconnect beside `error`: a start failed and
    /// left sync not running with storage set up on this device.
    /// `AppHandle::reconnect_sync` starts it again.
    pub reconnect_offered: bool,
}

impl BridgeSyncStatusSnapshot {
    pub(crate) fn from_core(snapshot: bae_db::SyncStatusSnapshot) -> Self {
        let indicator = snapshot.indicator().map(crate::types::BridgeSyncIndicator::from_core);
        let error = snapshot.shown_failure();
        let reconnect_offered = snapshot.reconnect_offered();
        let bae_db::SyncStatusSnapshot { coven: _, pending, error: _, retry_effect: _ } = snapshot;
        Self {
            indicator,
            reconnect_offered,
            error: error.map(crate::types::BridgeError::from_core),
            pending: pending
                .into_iter()
                .map(crate::types::BridgePendingRecord::from_core)
                .collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeSyncIndicator {
    Stopped,
    Syncing,
    Synced,
    Offline,
    Failed,
    Waiting { items: u64 },
}
impl BridgeSyncIndicator {
    pub(crate) fn from_core(value: bae_db::SyncIndicator) -> Self {
        match value {
            bae_db::SyncIndicator::Stopped => Self::Stopped,
            bae_db::SyncIndicator::Syncing => Self::Syncing,
            bae_db::SyncIndicator::Synced => Self::Synced,
            bae_db::SyncIndicator::Offline => Self::Offline,
            bae_db::SyncIndicator::Failed => Self::Failed,
            bae_db::SyncIndicator::Waiting { items } => Self::Waiting { items: items as u64 },
        }
    }
}

/// Waiting uses the indicator's `items` as the catalog's `count` argument.
#[uniffi::export]
pub fn bridge_sync_indicator_key(indicator: &BridgeSyncIndicator) -> String {
    match indicator {
        BridgeSyncIndicator::Stopped => "core.sync.stopped",
        BridgeSyncIndicator::Syncing => "core.sync.syncing",
        BridgeSyncIndicator::Synced => "core.sync.synced",
        BridgeSyncIndicator::Offline => "core.sync.offline",
        BridgeSyncIndicator::Failed => "core.sync.failed",
        BridgeSyncIndicator::Waiting { .. } => "core.sync.waiting",
    }
    .to_string()
}
