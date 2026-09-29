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
/// barcodes and text. Core calls it on a blocking task.
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

/// The dominant activity of a slice of the upload queue. Mirrors bae-core's
/// `UploadActivity`; `Uploaded` still awaits publication.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeUploadActivity {
    Cancelling,
    Publishing,
    Uploading,
    Preparing,
    Retrying,
    Prepared,
    Queued,
    Uploaded,
}

/// What a retrying upload needs from the person; a transient failure has none.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeUploadIssue {
    SourceUnavailable { paths: Vec<String> },
}

/// The releases one move-to-cloud command queued, and the outbox revision
/// published before it returned.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeMakeRemoteReceipt {
    pub outbox_revision: u64,
    pub release_ids: Vec<String>,
}

/// The releases one move-to-cloud command refused, and why.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeMakeRemoteBatchFailure {
    pub release_ids: Vec<String>,
    pub error: BridgeError,
}

/// Per-release admission outcome for one move-to-cloud command.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeMakeReleasesRemoteOutcome {
    Complete {
        receipt: BridgeMakeRemoteReceipt,
    },
    Partial {
        receipt: Option<BridgeMakeRemoteReceipt>,
        failure: BridgeMakeRemoteBatchFailure,
    },
}

mirror_struct! {
    BridgeMakeRemoteReceipt = bae_core::library::MakeRemoteReceipt,
    from_core: fn,
    fields: { outbox_revision, release_ids },
}

mirror_struct! {
    BridgeMakeRemoteBatchFailure = bae_core::library::MakeRemoteBatchFailure,
    from_core: fn,
    fields: { release_ids, error: (BridgeError) },
}

mirror_enum! {
    BridgeMakeReleasesRemoteOutcome = bae_core::library::MakeReleasesRemoteOutcome,
    from_core: pub(crate) fn,
    variants: {
        Complete { receipt: (BridgeMakeRemoteReceipt) },
        Partial {
            receipt: (opt BridgeMakeRemoteReceipt),
            failure: (BridgeMakeRemoteBatchFailure),
        },
    },
}

/// Which phase's bytes a progress bar counts: plaintext source bytes while
/// preparing, encrypted bytes while uploading. Mirrors `UploadPhase`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeUploadPhase {
    Preparing,
    Uploading,
}

/// One phase's progress bar, both numbers in that phase's bytes, so its fill
/// and label count the same thing. Mirrors `UploadBar`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct BridgeUploadBar {
    pub phase: BridgeUploadPhase,
    pub bytes_done: u64,
    pub bytes_total: u64,
}

/// Localization key for a progress bar's label ("Preparing 3 MB of 224.2 MB").
#[uniffi::export]
pub fn bridge_upload_phase_bytes_key(phase: BridgeUploadPhase) -> String {
    match phase {
        BridgeUploadPhase::Preparing => "core.outbox.bytes.preparing",
        BridgeUploadPhase::Uploading => "core.outbox.bytes.uploading",
    }
    .to_string()
}

/// One file's state in the queue pane; `Uploaded` while the rest of its
/// release is still going.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeUploadFileState {
    Queued,
    Preparing,
    Prepared,
    Uploading,
    Retrying,
    Uploaded,
}

/// The label for one queued upload; image roles are typed so each platform
/// localizes them.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeUploadFileLabel {
    Filename {
        name: String,
    },
    Cover,
    ArtistImage,
    /// A file of a release being removed from the cloud, which has no name
    /// left.
    Unwinding,
}

/// A slice of the upload queue — one release's or the whole queue's — with its
/// counts, bar and badge. Uploaded files count until publication.
#[derive(Debug, Clone, Default, uniffi::Record)]
pub struct BridgeUploadProgress {
    pub queued: u32,
    pub preparing: u32,
    pub prepared: u32,
    pub uploading: u32,
    pub retrying: u32,
    pub uploaded: u32,
    pub publishing: u32,
    pub cancelling: u32,
    /// `None` while there are no bytes to count.
    pub bar: Option<BridgeUploadBar>,
    /// `None` when idle.
    pub activity: Option<BridgeUploadActivity>,
    /// Whether the transition can still be unwound.
    pub can_cancel: bool,
    /// What the retry needs from the person, if anything.
    pub issue: Option<BridgeUploadIssue>,
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

/// Byte progress for the active download.
#[derive(Debug, Clone, Default, PartialEq, uniffi::Record)]
pub struct BridgeDownloadTransferProgress {
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub fraction: f64,
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

impl BridgeDownloadOp {
    fn into_core(self) -> bae_core::library::DownloadOp {
        let Self {
            release_id,
            // The download status reads the queue entry, not the library's release.
            release: _,
            created_at,
            state,
        } = self;
        bae_core::library::release_queue::ReleaseQueueOp {
            release_id,
            created_at,
            // Downloads carry no operation-specific payload.
            payload: (),
            state: state.into_core(),
        }
    }
}

/// What the album-detail download control shows. Mirrors
/// `ReleaseDownloadStatus`.
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

mirror_enum! {
    BridgeReleaseDownloadStatus = bae_core::album_detail::ReleaseDownloadStatus,
    from_core: fn,
    variants: {
        Downloaded,
        Queued,
        Downloading { progress: (BridgeDownloadTransferProgress) },
        Failed { error },
        Available,
    },
}

/// The download control's state for one release, or `None` when there is no
/// control to show. Core decides it, so no app re-derives it from its gates.
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

/// Per-state counts for the download queue, per release or overall.
#[derive(Debug, Clone, Default, uniffi::Record)]
pub struct BridgeDownloadProgress {
    pub queued: u32,
    pub active: u32,
    pub failed: u32,
}

/// The download queue as the Downloads pane renders it.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeDownloadSnapshot {
    pub downloads: Vec<BridgeDownloadOp>,
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

/// A queued export's state. Mirror of bae-core's `OutputState`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeOutputState {
    Queued,
    Active { percent: u8 },
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

/// Per-state counts for the export queue.
#[derive(Debug, Clone, Default, uniffi::Record)]
pub struct BridgeOutputProgress {
    pub queued: u32,
    pub active: u32,
    pub failed: u32,
}

/// The export queue as the Exporting pane renders it. Mirrors `OutputSnapshot`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeOutputSnapshot {
    pub outputs: Vec<BridgeOutputOp>,
    pub total: BridgeOutputProgress,
    /// The summary line's parts, in core's order.
    pub summary_parts: Vec<BridgeCountLabel>,
    /// Whether the person paused the queue.
    pub paused: bool,
}

/// One file in a release's upload group. Mirrors `UploadFileOp` with its state
/// flattened; `bar` is present only while the file is moving bytes.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeUploadFileOp {
    pub file_id: String,
    pub label: BridgeUploadFileLabel,
    pub bar: Option<BridgeUploadBar>,
    pub source_bytes_total: u64,
    pub throughput_bps: u64,
    pub state: BridgeUploadFileState,
    pub last_error: Option<String>,
}

/// A release's uploads, files in queue order. Mirrors `UploadReleaseGroup`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeUploadReleaseGroup {
    pub release_id: String,
    pub files: Vec<BridgeUploadFileOp>,
    pub progress: BridgeUploadProgress,
    /// The release's current transfer rate.
    pub throughput_bps: u64,
}

/// One storage row's progress paired with the transfer rate for that release.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeReleaseUploadProgress {
    pub progress: BridgeUploadProgress,
    pub throughput_bps: u64,
}

/// Whether the upload queue is running or suspended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeOutboxPauseState {
    Running,
    Paused,
}

/// The upload queue as the Storage Manager renders it.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeOutboxSnapshot {
    /// Increases with each publication, so a subscriber that skipped values
    /// can still tell an enqueue it was told about has finished.
    pub revision: u64,
    /// A group leaves only once its release is published.
    pub upload_groups: Vec<BridgeUploadReleaseGroup>,
    /// `upload_groups` per release id.
    pub per_release: std::collections::HashMap<String, BridgeReleaseUploadProgress>,
    /// Across all uploads.
    pub total: BridgeUploadProgress,
    /// The summary line's parts, in core's order.
    pub summary_parts: Vec<BridgeCountLabel>,
    pub pause_state: BridgeOutboxPauseState,
    /// Bytes per second.
    pub throughput_bps: u64,
    /// Seconds remaining at the current rate.
    pub eta_seconds: Option<u64>,
}
