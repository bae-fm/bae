use super::*;

/// Callback for the unified UI event stream.
#[uniffi::export(callback_interface)]
pub trait UiEventCallback: Send + Sync {
    fn on_event(&self, event: BridgeUiEvent);
}

/// What one analyzer pass over an image read. Mirrors
/// `bae_core::signals::ArtworkAnalysis`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeArtworkAnalysis {
    /// The payloads of the barcodes the detector found.
    pub barcodes: Vec<String>,
    /// The recognized text, one entry per visual line.
    pub text_lines: Vec<String>,
}

/// The platform's artwork analyzer: one synchronous pass per image reads both
/// barcodes and text. Core calls it on blocking tasks, several images at once.
#[uniffi::export(callback_interface)]
pub trait ArtworkAnalyzerCallback: Send + Sync {
    /// Empty on failure, as when there is nothing to read.
    fn analyze(&self, path: String) -> BridgeArtworkAnalysis;
}

/// A UI event; database-backed state arrives through live results instead.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeUiEvent {
    /// Playback couldn't start or continue, and has stopped.
    PlaybackError { reason: BridgePlaybackErrorReason },
    /// Tracks were added to the queue; never sent for zero.
    QueueItemsAdded { count: u32 },

    // ── Import live progress ───────────────────────────────────────
    /// A candidate's extracted text pools as extraction settles them, for the
    /// search pane's autocomplete and scanning indicator.
    #[cfg(feature = "desktop")]
    CandidateSignalsUpdated { key: String, signals: BridgeSignals },
    /// How many running identifications have ended, out of how many; `(0, 0)`
    /// is none. A view must not derive `total` from its filtered rows.
    #[cfg(feature = "desktop")]
    ImportIdentificationProgress { identified: u32, total: u32 },
    /// How many imports are waiting or running.
    #[cfg(feature = "desktop")]
    ImportsInFlight { count: u32 },
}

/// The activity exposed by coven's committed upload queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeUploadActivity {
    Uploading,
    Queued,
}

/// The releases one upload command queued.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeUploadReceipt {
    pub release_ids: Vec<String>,
}

/// The releases one upload command refused, and why.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeUploadBatchFailure {
    pub release_ids: Vec<String>,
    pub error: BridgeError,
}

/// Per-release admission outcome for one upload command.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeUploadReleasesOutcome {
    Complete {
        receipt: BridgeUploadReceipt,
    },
    Partial {
        receipt: Option<BridgeUploadReceipt>,
        failure: BridgeUploadBatchFailure,
    },
}

mirror_struct! {
    BridgeUploadReceipt = bae_core::library::UploadReceipt,
    from_core: fn,
    fields: { release_ids },
}

mirror_struct! {
    BridgeUploadBatchFailure = bae_core::library::UploadBatchFailure,
    from_core: fn,
    fields: { release_ids, error: (BridgeError) },
}

mirror_enum! {
    BridgeUploadReleasesOutcome = bae_core::library::UploadReleasesOutcome,
    from_core: pub(crate) fn,
    variants: {
        Complete { receipt: (BridgeUploadReceipt) },
        Partial {
            receipt: (opt BridgeUploadReceipt),
            failure: (BridgeUploadBatchFailure),
        },
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct BridgeUploadBar {
    pub bytes_done: u64,
    pub bytes_total: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeUploadFileState {
    Waiting,
    Uploading,
}

/// How many files wait and how many are being sent, and the phase that
/// makes. How far they have gone is their transfer.
#[derive(Debug, Clone, Default, PartialEq, Eq, uniffi::Record)]
pub struct BridgeUploadProgress {
    pub queued: u32,
    pub uploading: u32,
    pub activity: Option<BridgeUploadActivity>,
}

/// How far the files being sent have gone together and how fast they go.
/// Mirrors `UploadTransfer`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct BridgeUploadTransfer {
    pub bar: Option<BridgeUploadBar>,
    /// Bytes per second.
    pub throughput_bps: u64,
}

/// What a rate is of: one file being sent, by its `file_id`, or a release's
/// files being sent together.
#[derive(Debug, Clone, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum BridgeUploadTransferKey {
    File { file_id: String },
    Release { release_id: String },
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeKeyedUploadRate {
    pub key: BridgeUploadTransferKey,
    /// Bytes per second.
    pub throughput_bps: u64,
}

/// A change to one rate, or every transfer. Mirrors
/// `OutboxTransferChange`.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeOutboxTransferChange {
    Updated {
        key: BridgeUploadTransferKey,
        throughput_bps: u64,
    },
    /// The file or release has nothing being sent any more.
    Removed { key: BridgeUploadTransferKey },
    /// The files being sent together.
    TotalUpdated { transfer: BridgeUploadTransfer },
    /// Every rate and the total; a key not listed has nothing being sent.
    Reset {
        rates: Vec<BridgeKeyedUploadRate>,
        total: BridgeUploadTransfer,
    },
}

/// A queued download's phase. How far an active one has gone is its
/// release's download transfer.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum BridgeDownloadState {
    Queued,
    Active,
    Failed { error: String },
}

/// Byte progress for the active download.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct BridgeDownloadTransferProgress {
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub fraction: f64,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeKeyedDownloadTransfer {
    pub release_id: String,
    pub progress: BridgeDownloadTransferProgress,
}

/// A change to one release's download progress, or every release's.
/// Mirrors `DownloadTransferChange`.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeDownloadTransferChange {
    Updated {
        release_id: String,
        progress: BridgeDownloadTransferProgress,
    },
    /// The release is no longer downloading.
    Removed { release_id: String },
    /// Every downloading release's progress; a release not listed has none.
    Reset {
        transfers: Vec<BridgeKeyedDownloadTransfer>,
    },
}

/// What the library holds now for a queued release: what its queue row
/// shows. Mirrors `QueuedRelease`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeQueuedRelease {
    /// The release's album title.
    pub title: String,
    pub file_count: i64,
    /// Total size in bytes across the release's files. The UI formats it.
    pub total_size: i64,
}

mirror_struct! {
    BridgeQueuedRelease = bae_core::library::QueuedRelease,
    from_core: pub(crate) fn,
    fields: { title, file_count, total_size },
}

/// One queued download: a whole release being pinned. Mirrors `DownloadRow`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeDownloadOp {
    pub release_id: String,
    /// The release as the library holds it now; `None` once the library no
    /// longer has it.
    pub release: Option<BridgeQueuedRelease>,
    /// Enqueue time as Unix epoch milliseconds, for the queued relative label.
    pub created_at: i64,
    pub state: BridgeDownloadState,
}

/// What the album-detail download control shows. Mirrors
/// `ReleaseDownloadStatus`.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum BridgeReleaseDownloadStatus {
    Downloaded,
    Queued,
    /// Bytes are arriving; how many is the release's download transfer.
    Downloading,
    Failed {
        error: String,
    },
    Available,
}

mirror_enum! {
    BridgeReleaseDownloadStatus = bae_core::album_detail::ReleaseDownloadStatus,
    from_core: pub(crate) fn,
    variants: {
        Downloaded,
        Queued,
        Downloading,
        Failed { error },
        Available,
    },
}

/// Per-state counts for the download queue, per release or overall.
#[derive(Debug, Clone, Default, uniffi::Record)]
pub struct BridgeDownloadProgress {
    pub queued: u32,
    pub active: u32,
    pub failed: u32,
}

/// How many downloads are in each phase, and whether the queue is paused:
/// what the Downloads pane's summary and controls show. Its rows are the
/// pane's windows (`DownloadBrowseSubscription`).
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeDownloadCounts {
    pub total: BridgeDownloadProgress,
    /// The summary line's parts, in core's order; the UI resolves and joins them.
    pub summary_parts: Vec<BridgeCountLabel>,
    /// Whether the person paused the queue.
    pub paused: bool,
}

/// One part of a queue summary line: a catalog key and its count. Mirrors
/// `CountLabel`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeCountLabel {
    pub key: String,
    pub count: u32,
}

mirror_struct! {
    BridgeCountLabel = bae_core::library::CountLabel,
    from_core: pub(crate) fn,
    fields: { key, count },
}

/// A queued export's phase. Mirrors `OutputPhase`; how far an active one
/// has got is its transfer.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeOutputState {
    Queued,
    Active,
    Failed { error: String },
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeKeyedOutputTransfer {
    pub release_id: String,
    pub percent: u8,
}

/// A change to one release's export percent, or every release's. Mirrors
/// `OutputTransferChange`.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeOutputTransferChange {
    Updated {
        release_id: String,
        percent: u8,
    },
    /// The release is no longer exporting.
    Removed {
        release_id: String,
    },
    /// Every exporting release's percent; a release not listed has none.
    Reset {
        transfers: Vec<BridgeKeyedOutputTransfer>,
    },
}

/// What a queued output produces; a save carries its preset's name as it was
/// when queued. Mirrors `OutputKind`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeOutputKind {
    Export,
    Save { preset_name: String },
}

/// One queued release output: an export or a preset save. Mirrors `OutputRow`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeOutputOp {
    pub release_id: String,
    /// The chosen destination; the release's folder is rebuilt under it.
    pub target_dir: String,
    /// The release as the library holds it now; `None` once the library no
    /// longer has it.
    pub release: Option<BridgeQueuedRelease>,
    /// Enqueue time as Unix epoch milliseconds, for the queued relative label.
    pub created_at: i64,
    pub state: BridgeOutputState,
    pub kind: BridgeOutputKind,
}

/// Localization key for one release's upload phase alone, the word beside
/// its progress bar. The UI resolves it against the `Core` table.
#[uniffi::export]
pub fn bridge_upload_activity_key(activity: BridgeUploadActivity) -> String {
    match activity {
        BridgeUploadActivity::Uploading => "core.outbox.phase.uploading",
        BridgeUploadActivity::Queued => "core.outbox.phase.queued",
    }
    .to_string()
}

/// Localization key for one file's upload state, worded as the release phase
/// it shares.
#[uniffi::export]
pub fn bridge_upload_file_state_key(state: BridgeUploadFileState) -> String {
    match state {
        BridgeUploadFileState::Uploading => "core.outbox.phase.uploading",
        BridgeUploadFileState::Waiting => "core.outbox.phase.queued",
    }
    .to_string()
}
