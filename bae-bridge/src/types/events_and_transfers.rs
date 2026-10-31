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
    /// Empty on failure, as when there is nothing to read. `path` is the
    /// image's path as coven's bytes (`coven::path_bytes`).
    fn analyze(&self, path: Vec<u8>) -> BridgeArtworkAnalysis;
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
/// makes. How far they have gone is in the upload `KeyedValueStream`s.
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

/// How far a release's upload has got, of its whole size, and how fast its
/// files being sent go together. Mirrors `ReleaseUploadTransfer`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct BridgeReleaseUploadTransfer {
    pub bar: BridgeUploadBar,
    /// Bytes per second.
    pub throughput_bps: u64,
}

/// A value core sends many times a second, as one key of a
/// `KeyedValueStream` holds it. A stream holds one variant for every key.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum BridgeFastValue {
    /// How far a release's upload has got, of its whole size, and how fast
    /// its files being sent go together; keyed by release.
    ReleaseUpload {
        transfer: BridgeReleaseUploadTransfer,
    },
    /// How far the files being sent have gone together, and how fast; a
    /// stream's one key, `""`, while any file is being sent.
    UploadTotal { transfer: BridgeUploadTransfer },
    /// The share of a file in flight its object has moved, from 0 to 1;
    /// keyed by file.
    FileShare { fraction: f64 },
    /// A downloading release's bytes and rate; keyed by release.
    Download {
        progress: BridgeDownloadTransferProgress,
    },
    /// An exporting release's percent; keyed by release.
    ExportPercent { percent: u8 },
    /// Where the playing track is; keyed by track, the playing one alone.
    TrackPosition { position: BridgeTrackPosition },
    /// Where the preview is in its clip; a stream's one key, `""`, while a
    /// preview is loaded.
    PreviewPosition { position: BridgePreviewPosition },
    /// How far through its step a running import is; keyed by candidate.
    ImportPercent { percent: u32 },
    /// How far a bulk action over an import list's selection has got; a
    /// stream's one key, `""`, while the action runs.
    #[cfg(feature = "desktop")]
    SelectionActionProgress {
        progress: super::BridgeSelectionActionProgress,
    },
}

/// Which keys of a stream started or stopped holding a value.
#[uniffi::export(callback_interface)]
pub trait KeyActivityCallback: Send + Sync {
    /// These keys started or stopped holding a value; `latest` says which.
    /// Called on the thread that writes the stream.
    fn on_keys_changed(&self, keys: Vec<String>);
}

/// A stream's read failing. The stream holds nothing until a read succeeds
/// again.
#[uniffi::export(callback_interface)]
pub trait StreamFailureCallback: Send + Sync {
    fn on_error(&self, error: BridgeError);
}

/// A queued download's phase. How far an active one has gone is its
/// release's value in the download `KeyedValueStream`.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum BridgeDownloadState {
    Queued,
    Active,
    Failed { error: String },
}

/// Byte progress for the active download, and how fast it has gone since it
/// began, in bytes per second, once it has moved bytes.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct BridgeDownloadTransferProgress {
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub fraction: f64,
    pub throughput_bps: Option<u64>,
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
    /// Bytes are arriving; how many is the release's value in the download
    /// `KeyedValueStream`.
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

/// One count of a release transfer's summary line, and where the files it
/// counts stand. Mirrors `TransferCount`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeTransferCount {
    pub label: BridgeCountLabel,
    pub stage: BridgeTransferStage,
}

/// Mirrors `TransferStage`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeTransferStage {
    Moving,
    Waiting,
    Done,
}

mirror_enum! {
    BridgeTransferStage = bae_core::library::TransferStage,
    from_core: fn,
    variants: { Moving, Waiting, Done },
}

mirror_struct! {
    BridgeTransferCount = bae_core::library::TransferCount,
    from_core: pub(crate) fn,
    fields: {
        label: (BridgeCountLabel),
        stage: (BridgeTransferStage),
    },
}

/// A queued export's phase. Mirrors `OutputPhase`; how far an active one
/// has got is its value in the export `KeyedValueStream`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeOutputState {
    Queued,
    Active,
    Failed { error: String },
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
    /// The chosen destination, as `coven::path_text` shows its path; the
    /// release's folder is rebuilt under it.
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
