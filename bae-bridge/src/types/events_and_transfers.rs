use super::*;

/// Callback for the unified UI event stream.
#[uniffi::export(callback_interface)]
pub trait UiEventCallback: Send + Sync {
    fn on_event(&self, event: BridgeUiEvent);
}

/// Everything one Vision pass over an image surfaces — barcode payloads and
/// recognized text lines from a single image decode. Mirrors
/// `bae_core::signals::ArtworkAnalysis`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeArtworkAnalysis {
    pub barcodes: Vec<String>,
    pub text_lines: Vec<String>,
}

/// Platform-provided artwork analyzer. One `analyze` pass over an image yields
/// both barcodes and text, so the signal-extraction pass decodes each image
/// exactly once.
///
/// Sync by design: `VNImageRequestHandler.perform` is synchronous, and the Rust
/// side calls this from `tokio::task::spawn_blocking` so the async runtime isn't
/// parked while Vision churns.
///
/// Unlike `UiEventCallback` (fire-and-forget), this one returns a value.
#[uniffi::export(callback_interface)]
pub trait ArtworkAnalyzerCallback: Send + Sync {
    /// Detect barcodes and recognize text in one image decode. Empty
    /// payloads/lines on failure or when absent.
    fn analyze(&self, path: String) -> BridgeArtworkAnalysis;
}

/// A scoped state key the UI should requery from core. Mirrors
/// `bae_core::ui::Invalidation`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeInvalidation {
    AlbumList,
    Album { album_id: String },
    Release { release_id: String },
    ComposerList,
    Composer { composer_id: String },
    ArtistList,
    Queue,
    Config,
    SyncStatus,
    Outbox,
    DownloadQueue,
    OutputQueue,
    ImportCandidateList,
    ImportCandidate { key: String },
    WatchedFolders,
    CastDevices,
}

impl BridgeInvalidation {
    pub(crate) fn from_core(invalidation: bae_core::ui::Invalidation) -> Self {
        use bae_core::ui::Invalidation as CoreInvalidation;

        match invalidation {
            CoreInvalidation::AlbumList => Self::AlbumList,
            CoreInvalidation::Album { album_id } => Self::Album { album_id },
            CoreInvalidation::Release { release_id } => Self::Release { release_id },
            CoreInvalidation::ComposerList => Self::ComposerList,
            CoreInvalidation::Composer { composer_id } => Self::Composer { composer_id },
            CoreInvalidation::ArtistList => Self::ArtistList,
            CoreInvalidation::Queue => Self::Queue,
            CoreInvalidation::Config => Self::Config,
            CoreInvalidation::SyncStatus => Self::SyncStatus,
            CoreInvalidation::Outbox => Self::Outbox,
            CoreInvalidation::DownloadQueue => Self::DownloadQueue,
            CoreInvalidation::OutputQueue => Self::OutputQueue,
            CoreInvalidation::ImportCandidateList => Self::ImportCandidateList,
            CoreInvalidation::ImportCandidate { key } => Self::ImportCandidate { key },
            CoreInvalidation::WatchedFolders => Self::WatchedFolders,
            CoreInvalidation::CastDevices => Self::CastDevices,
        }
    }
}

/// Top-level UI event. Every distinct state is a top-level variant with
/// fields inlined, except query-backed state changes which carry a scoped
/// invalidation key.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeUiEvent {
    Invalidated {
        invalidation: BridgeInvalidation,
    },

    // ── Playback ───────────────────────────────────────────────────
    PlaybackStopped,
    /// Playback couldn't start or continue — e.g. a cloud-only track that isn't
    /// downloaded yet, or an in-core decode failure. The UI renders `reason`
    /// for its locale; playback itself falls back to stopped.
    PlaybackError {
        reason: BridgePlaybackErrorReason,
    },
    PlaybackLoading {
        track_id: String,
        /// The target track's metadata, once core has resolved it. `None` in the
        /// first loading event; `Some` once the prepared track is in hand, so the
        /// UI can switch the now-playing bar to the target while audio is still
        /// downloading.
        track: Option<BridgeLoadingTrackInfo>,
    },
    PlaybackPlaying {
        track_id: String,
        track_title: String,
        artist_names: String,
        artist_id: String,
        album_id: String,
        album_title: String,
        cover_image: Option<BridgeImageRef>,
        duration_ms: u64,
    },
    PlaybackPaused {
        track_id: String,
        track_title: String,
        artist_names: String,
        artist_id: String,
        album_id: String,
        album_title: String,
        cover_image: Option<BridgeImageRef>,
        duration_ms: u64,
        reason: BridgePlaybackPauseReason,
    },
    /// Position tick — goes to NSView.
    PlaybackProgress {
        track_id: String,
        position_ms: u64,
        /// User-facing track duration (pregap-adjusted), so the media-control
        /// update reads it from the event instead of the now-playing slice.
        duration_ms: u64,
        progress: f64,
    },
    /// Position after a seek completes — goes to NSView.
    PlaybackSeeked {
        track_id: String,
        position_ms: u64,
        /// User-facing track duration (pregap-adjusted), so the media-control
        /// update reads it from the event instead of the now-playing slice.
        duration_ms: u64,
        progress: f64,
    },
    VolumeChanged {
        volume: f32,
    },
    MuteChanged {
        is_muted: bool,
    },
    RepeatModeChanged {
        mode: BridgeRepeatMode,
    },
    QueueUpdated {
        snapshot: BridgeQueueSnapshot,
    },
    /// Tracks were appended/inserted into the queue. Carries the count for
    /// a transient "+N" badge in the UI. Suppressed when count is zero.
    QueueItemsAdded {
        count: u32,
    },

    // ── Preview ────────────────────────────────────────────────────
    PreviewIdle,
    PreviewPlaying {
        path: String,
        duration_ms: u64,
    },
    PreviewPaused {
        path: String,
        duration_ms: u64,
    },
    /// High-frequency tick — goes to NSView, not store.
    PreviewProgress {
        position_ms: u64,
        progress: f64,
    },

    // ── Import live progress ───────────────────────────────────────
    /// High-frequency loudness-measurement tick — the UI routes it to a native
    /// leaf view (a determinate bar driven by `fraction`, labelled "N / M"), not
    /// the coarse candidate row.
    CandidateImportLoudnessProgress {
        key: String,
        tracks_done: u32,
        tracks_total: u32,
        fraction: f32,
    },
    /// How much of the import queue the background sweep has answered — the
    /// sidebar header's line and bar. Both numbers are the queue's; a view
    /// must not derive `total` from the rows it holds, which are filtered.
    ImportQueueIdentifyProgress {
        identified: u32,
        total: u32,
    },

    // ── Release transfer ───────────────────────────────────────────
    /// A pin/unpin/manage/unmanage transition started. The UI composes the
    /// localized line and shows an in-flight indicator on the release row until
    /// `ReleaseTransferEnded`.
    ReleaseTransferProgress {
        release_id: String,
        action: BridgeReleaseStorageAction,
    },
    /// A transition finished (success or failure) — the UI clears its transfer
    /// indicator. Failure text still arrives via the thrown error.
    ReleaseTransferEnded {
        release_id: String,
    },

    // ── Cast ───────────────────────────────────────────────────────
    /// The active renderer changed: `Some(name)` when playback moved to a Cast
    /// device, `None` when it returned to local output. Drives the cast button's
    /// active state and the "Casting to `<name>`" row.
    CastStatusChanged {
        device_name: Option<String>,
    },

    // ── Errors ─────────────────────────────────────────────────────
    Error {
        error: BridgeError,
    },
}

/// The dominant activity of a slice of the upload queue (a release's uploads,
/// or the whole queue), for the storage-row badge. Mirror of bae-core's
/// `UploadActivity`. No terminal variant: a release with nothing left to ship
/// stops being rendered at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeUploadActivity {
    Uploading,
    Retrying,
    Queued,
}

/// One file's state in the queue pane's per-file rows. Unlike the slice badge,
/// a file inside a still-uploading release does render as `Done` — the row
/// shows which files already shipped while the rest transfer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeUploadFileState {
    Queued,
    Uploading,
    Retrying,
    Done,
}

/// One cloud object still owed a removal.
///
/// The row that named the object is gone — that is what makes the removal
/// outstanding — so there is no filename or album to show, and no cancel: the
/// object exists in the cloud and abandoning the tombstone would strand it.
/// `namespace` and `blob_id` together identify it and serve as the row's
/// identity for list diffing.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeDeleteOp {
    pub namespace: String,
    pub blob_id: String,
    /// Enqueue time as Unix epoch milliseconds, for the queued relative label.
    pub created_at: i64,
}

/// Per-state counts, byte progress, and a derived badge `activity`. Used
/// per-release (the storage-row badge reads `activity`; storage-action gates
/// read the counts) and as the overall total (queue counts, ETA, summary band).
/// Files completed during the current queue burst count in `bytes_done` and
/// `bytes_total`, so the fractions are cumulative over the burst.
#[derive(Debug, Clone, Default, uniffi::Record)]
pub struct BridgeUploadProgress {
    pub queued: u32,
    pub active: u32,
    pub failed: u32,
    pub bytes_done: u64,
    pub bytes_total: u64,
    /// The badge activity for this slice; `None` when idle. Per-release entries
    /// always have pending work (finished releases aren't rendered), so theirs
    /// is always set.
    pub activity: Option<BridgeUploadActivity>,
}

/// A queued download's state.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum BridgeDownloadState {
    Queued,
    Active {
        progress: BridgeDownloadTransferProgress,
    },
    Failed {
        error: String,
    },
}

/// Byte progress for the active download. Mirrors the payload emitted by the
/// transfer reading the release's blobs.
#[derive(Debug, Clone, Default, PartialEq, uniffi::Record)]
pub struct BridgeDownloadTransferProgress {
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub fraction: f64,
}

/// One queued download — a whole release being pinned. Mirror of bae-core's
/// `DownloadOp`; carries raw fields the UI renders directly.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeDownloadOp {
    pub release_id: String,
    /// Album title for display.
    pub title: String,
    pub file_count: i64,
    /// Total size in bytes across the release's files. The UI formats it.
    pub total_size: i64,
    /// Enqueue time as Unix epoch milliseconds, for the queued relative label.
    pub created_at: i64,
    pub state: BridgeDownloadState,
}

impl BridgeDownloadTransferProgress {
    fn into_core(self) -> bae_core::library::DownloadTransferProgress {
        let Self {
            bytes_done,
            bytes_total,
            fraction,
        } = self;
        bae_core::library::DownloadTransferProgress {
            bytes_done,
            bytes_total,
            fraction,
        }
    }
}

impl BridgeDownloadState {
    fn into_core(self) -> bae_core::library::DownloadState {
        use bae_core::library::DownloadState;
        match self {
            Self::Queued => DownloadState::Queued,
            Self::Active { progress } => DownloadState::Active {
                progress: progress.into_core(),
            },
            Self::Failed { error } => DownloadState::Failed { error },
        }
    }
}

impl BridgeDownloadOp {
    fn into_core(self) -> bae_core::library::DownloadOp {
        let Self {
            release_id,
            title,
            file_count,
            total_size,
            created_at,
            state,
        } = self;
        bae_core::library::release_queue::ReleaseQueueOp {
            release_id,
            title,
            file_count,
            total_size,
            created_at,
            // Downloads carry no operation-specific payload.
            payload: (),
            state: state.into_core(),
        }
    }
}

/// What the album-detail download control shows for one release. Mirror of
/// bae-core's `ReleaseDownloadStatus`.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum BridgeReleaseDownloadStatus {
    Downloaded,
    Queued,
    Downloading {
        progress: BridgeDownloadTransferProgress,
    },
    Failed {
        error: String,
    },
    Available,
}

impl BridgeReleaseDownloadStatus {
    fn from_core(status: bae_core::album_detail::ReleaseDownloadStatus) -> Self {
        use bae_core::album_detail::ReleaseDownloadStatus;
        match status {
            ReleaseDownloadStatus::Downloaded => Self::Downloaded,
            ReleaseDownloadStatus::Queued => Self::Queued,
            ReleaseDownloadStatus::Downloading { progress } => Self::Downloading {
                progress: BridgeDownloadTransferProgress::from_core(progress),
            },
            ReleaseDownloadStatus::Failed { error } => Self::Failed { error },
            ReleaseDownloadStatus::Available => Self::Available,
        }
    }
}

/// The download control's state for one release, or `None` when there is no
/// control to show (no cloud home, or a release whose audio is already local).
///
/// The whole join is core's — including finding this release's entry in the
/// queue. A live entry outranks `pinned`, and `Available` means exactly "core
/// offers Pin"; both are properties of core's own storage-action gate, so an app
/// that re-derived either would drift from it.
#[uniffi::export]
pub fn bridge_release_download_status(
    pinned: bool,
    storage_actions: Vec<BridgeReleaseStorageAction>,
    downloads: BridgeDownloadSnapshot,
    release_id: String,
) -> Option<BridgeReleaseDownloadStatus> {
    let actions: Vec<_> = storage_actions
        .into_iter()
        .map(BridgeReleaseStorageAction::into_core)
        .collect();
    let ops: Vec<_> = downloads
        .downloads
        .into_iter()
        .map(BridgeDownloadOp::into_core)
        .collect();
    bae_core::album_detail::release_download_status(pinned, &actions, &ops, &release_id)
        .map(BridgeReleaseDownloadStatus::from_core)
}

/// Per-state counts for the download queue. Used per-release (the storage-row
/// "Downloading" badge) and as the overall total (the pane header).
#[derive(Debug, Clone, Default, uniffi::Record)]
pub struct BridgeDownloadProgress {
    pub queued: u32,
    pub active: u32,
    pub failed: u32,
}

/// The in-memory download (pin) queue snapshot the Storage Manager's Downloads
/// pane renders. The rolled-up counts and the one-line `summary` are computed in
/// bae-core; the UI renders them verbatim.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeDownloadSnapshot {
    pub downloads: Vec<BridgeDownloadOp>,
    pub total: BridgeDownloadProgress,
    /// The one-line queue summary's parts (downloading/failed/queued, each
    /// dropped when zero), decided by core. The UI resolves each key with its
    /// count and joins — it does not choose which counts appear or their order.
    pub summary_parts: Vec<BridgeCountLabel>,
    /// True when the user paused the download queue. Drives the pause/resume
    /// toggle in the Downloads pane.
    pub paused: bool,
}

/// One part of a queue summary line — a catalog key and its count. Mirror of
/// bae-core's `CountLabel`. Which parts appear, in what order, and that a zero
/// drops out is core's decision; the UI resolves the key and joins the parts.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeCountLabel {
    pub key: String,
    pub count: u32,
}

impl BridgeCountLabel {
    pub(crate) fn from_core(label: bae_core::library::CountLabel) -> Self {
        let bae_core::library::CountLabel { key, count } = label;
        Self { key, count }
    }
}

/// A queued export's state. Mirror of bae-core's `OutputState`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeOutputState {
    Queued,
    Active { percent: u8 },
    Failed { error: String },
}

/// What a queued release-level output produces, for display in the queue row.
/// Mirror of bae-core's `OutputKind`; a save carries its preset's display name
/// (resolved at enqueue, not an id — the row never dereferences a preset).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeOutputKind {
    Export,
    Save { preset_name: String },
}

/// One queued release output — a whole release being written out to a folder,
/// either a verbatim export or a preset save. Mirror of bae-core's `OutputOp`;
/// carries raw fields the UI renders directly.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeOutputOp {
    pub release_id: String,
    /// The chosen destination directory; the release's source folder is
    /// reconstructed under it.
    pub target_dir: String,
    /// Album title for display.
    pub title: String,
    pub file_count: i64,
    /// Total size in bytes across the release's files. The UI formats it.
    pub total_size: i64,
    /// Enqueue time as Unix epoch milliseconds, for the queued relative label.
    pub created_at: i64,
    pub state: BridgeOutputState,
    /// Whether this row is a verbatim export or a preset save; drives the row's
    /// state text and (for saves) the preset name in the detail line.
    pub kind: BridgeOutputKind,
}

/// Per-state counts for the export queue, driving the pane header. No bytes:
/// outputs track an overall percent per release, not aggregate bytes.
#[derive(Debug, Clone, Default, uniffi::Record)]
pub struct BridgeOutputProgress {
    pub queued: u32,
    pub active: u32,
    pub failed: u32,
}

/// The in-memory export queue snapshot the Storage Manager's Exporting pane
/// renders. Mirror of bae-core's `OutputSnapshot`; the UI renders it verbatim.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeOutputSnapshot {
    pub outputs: Vec<BridgeOutputOp>,
    pub total: BridgeOutputProgress,
    /// The one-line queue summary's parts (exporting/failed/queued), decided by
    /// core. The UI resolves each key and joins.
    pub summary_parts: Vec<BridgeCountLabel>,
    /// True when the user paused the export queue. Drives the pause/resume
    /// toggle in the Exporting pane.
    pub paused: bool,
}

/// One file in a release's upload group: what the queue pane's per-file rows
/// render. Mirror of bae-core's `UploadFileOp`, with the state flattened into
/// `state` + `bytes_done` + `last_error` so the UI doesn't switch on
/// associated data: `bytes_done` is the live count while `Uploading`, equal to
/// `bytes_total` when `Done`, and 0 otherwise; `last_error` is set only when
/// `Retrying`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeUploadFileOp {
    pub file_id: String,
    /// The file's original name, resolved by core (its cloud key / file id
    /// when no release-file row backs it).
    pub display_name: String,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub state: BridgeUploadFileState,
    pub last_error: Option<String>,
}

/// A release's uploads, grouped for the queue pane's expandable per-release
/// rows. Mirror of bae-core's `UploadReleaseGroup`. `release_id` is `None` for
/// the orphaned-files bucket; `display_title` is the row's label, resolved by
/// core. `files` runs completed files first (in completion order), then the
/// remaining queue in order.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeUploadReleaseGroup {
    pub release_id: Option<String>,
    pub display_title: String,
    pub files: Vec<BridgeUploadFileOp>,
    pub progress: BridgeUploadProgress,
}

/// The cloud-outbox processing snapshot the Storage Manager renders. The
/// counts, per-release aggregates, one-line `summary`, throughput, and ETA
/// are computed from bae-core's grouped snapshot; the UI renders them verbatim.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeOutboxSnapshot {
    /// Uploads grouped by release for the queue pane's rows. Groups whose
    /// files all completed stay listed (as done rows) until the whole queue
    /// drains.
    pub upload_groups: Vec<BridgeUploadReleaseGroup>,
    pub deletes: Vec<BridgeDeleteOp>,
    /// Per-release aggregate derived from `upload_groups`, keyed by release id.
    /// Releases with no work this burst are absent from the map.
    pub per_release: std::collections::HashMap<String, BridgeUploadProgress>,
    /// Sum across all uploads. The master progress bar shows
    /// `total.bytes_done` of `total.bytes_total` — cumulative over the burst,
    /// completed files included.
    pub total: BridgeUploadProgress,
    /// Derived from `deletes.len()`.
    pub pending_deletes: u32,
    /// The one-line queue summary's parts (uploading/failed/queued/pending
    /// deletes, each dropped when zero), decided by core. The UI resolves each
    /// key and joins.
    pub summary_parts: Vec<BridgeCountLabel>,
    /// True when the user has paused the upload pipeline. Drives the
    /// pause/resume toggle and suppresses throughput/ETA in the UI.
    pub paused: bool,
    /// Rolling-window upload throughput in bytes per second. The UI formats it.
    pub throughput_bps: u64,
    /// Estimated seconds remaining at the current rate. The UI formats it.
    pub eta_seconds: Option<u64>,
}
