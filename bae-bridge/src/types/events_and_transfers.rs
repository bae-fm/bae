use super::*;

/// What one analyzer pass over an image read. Mirrors
/// `bae_import::extraction::ArtworkAnalysis`.
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
    Complete { receipt: BridgeUploadReceipt },
    Partial { receipt: Option<BridgeUploadReceipt>, failure: BridgeUploadBatchFailure },
}

mirror_struct! {
    BridgeUploadReceipt = bae_library::UploadReceipt,
    from_core: fn,
    fields: { release_ids },
}

mirror_struct! {
    BridgeUploadBatchFailure = bae_library::UploadBatchFailure,
    from_core: fn,
    fields: { release_ids, error: (BridgeError) },
}

mirror_enum! {
    BridgeUploadReleasesOutcome = bae_library::UploadReleasesOutcome,
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
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeUploadProgress {
    pub queued: u32,
    pub uploading: u32,
    pub activity: BridgeUploadActivity,
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
    ReleaseUpload { transfer: BridgeReleaseUploadTransfer },
    /// How far the files being sent have gone together, and how fast; a
    /// stream's one key, `""`, while any file is being sent.
    UploadTotal { transfer: BridgeUploadTransfer },
    /// The share of a file in flight its object has moved, from 0 to 1;
    /// keyed by file.
    FileShare { fraction: f64 },
    /// A downloading release's bytes and rate; keyed by release.
    Download { progress: BridgeDownloadTransferProgress },
    /// How far a release being made local has downloaded into the
    /// person's folder; keyed by release.
    MakeLocal { progress: BridgeMakeLocalProgress },
    /// An exporting release's percent; keyed by release.
    ExportPercent { percent: u8 },
    /// Where the playing track is; keyed by track, the playing one alone.
    TrackPosition { position: BridgeTrackPosition },
    /// The parts of the playing track whose bytes this device holds; keyed
    /// by track, the playing one alone.
    TrackBuffered { buffered: BridgeTrackBuffered },
    /// The playing track's load waits on storage; keyed by track, the
    /// playing one alone, while the person waits on its load and it reads
    /// from storage.
    TrackWaitsOnStorage,
    /// Where the preview is in its clip; a stream's one key, `""`, while a
    /// preview is loaded.
    PreviewPosition { position: BridgePreviewPosition },
    /// How far through its step a running import is; keyed by candidate.
    ImportPercent { percent: u32 },
    /// How far an open of a library has come; a stream's one key, `""`,
    /// while the open runs a step.
    OpenProgress { progress: super::BridgeOpenProgress },
    /// How far a setup call has come; a stream's one key, `""`, while it
    /// runs a step.
    SetupProgress { progress: super::BridgeSetupProgress },
    /// The counted step a restore, a join or a device reset runs; a
    /// stream's one key, `""`, while it runs one.
    BootstrapStep { step: super::BridgeBootstrapStep },
    /// How far a storage setup has come; a stream's one key, `""`, while it
    /// runs a step.
    StorageSetupProgress { progress: super::BridgeStorageSetupProgress },
    /// The step a library's close or removal runs; a stream's one key, `""`,
    /// while it runs one.
    ClosingStep { step: super::BridgeClosingStep },
    /// How far a bulk action over an import list's selection has got; a
    /// stream's one key, `""`, while the action runs.
    #[cfg(desktop)]
    SelectionActionProgress { progress: super::BridgeSelectionActionProgress },
    /// The network read an apply of a catalog release waits on; a stream's
    /// one key, `""`, while it waits on one.
    #[cfg(desktop)]
    CatalogApplyStep { step: super::BridgeCatalogApplyStep },
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

/// How far a release being made local has downloaded: the bytes of the file
/// coven is downloading, and of every file of the release. Mirrors
/// `MakeLocalTransferProgress`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct BridgeMakeLocalProgress {
    pub file_bytes_done: u64,
    pub file_bytes_total: u64,
    pub bytes_done: u64,
    pub bytes_total: u64,
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
    BridgeQueuedRelease = bae_transfers::QueuedRelease,
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
    /// Waiting, with `ahead` entries before it that have not failed.
    Queued {
        ahead: u32,
        wait: Option<BridgeQueueWait>,
    },
    /// Bytes are arriving; how many is the release's value in the download
    /// `KeyedValueStream`.
    Downloading {
        wait: Option<BridgeQueueWait>,
    },
    Failed {
        error: String,
    },
    Available,
}

mirror_enum! {
    BridgeReleaseDownloadStatus = bae_views::album_detail::ReleaseDownloadStatus,
    from_core: pub(crate) fn,
    variants: {
        Downloaded,
        Queued { ahead, wait: (opt BridgeQueueWait) },
        Downloading { wait: (opt BridgeQueueWait) },
        Failed { error },
        Available,
    },
}

/// What keeps the download queue from taking its next entry. Mirrors
/// `QueueWait`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeQueueWait {
    /// The device has no usable network.
    Offline,
    /// The person paused the queue.
    Paused,
}

mirror_enum! {
    BridgeQueueWait = bae_transfers::QueueWait,
    from_core: pub(crate) fn,
    variants: {
        Offline,
        Paused,
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

/// A catalog key whose message takes `count`, and the count: a part of a
/// queue summary line, or a browse summary's count line. Mirrors
/// `CountLabel`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeCountLabel {
    pub key: String,
    pub count: u32,
}

mirror_struct! {
    BridgeCountLabel = bae_types::count_label::CountLabel,
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
    BridgeTransferStage = bae_transfers::TransferStage,
    from_core: fn,
    variants: { Moving, Waiting, Done },
}

mirror_struct! {
    BridgeTransferCount = bae_transfers::TransferCount,
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
