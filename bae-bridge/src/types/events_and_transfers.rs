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

/// Top-level UI event. Every distinct state is a top-level variant with
/// fields inlined. Database-backed state uses live-result subscriptions.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeUiEvent {
    /// Playback couldn't start or continue — e.g. a cloud-only track that isn't
    /// downloaded yet, or an in-core decode failure. The UI renders `reason`
    /// for its locale; playback itself falls back to stopped.
    PlaybackError {
        reason: BridgePlaybackErrorReason,
    },
    /// Tracks were appended/inserted into the queue. Carries the count for
    /// a transient "+N" badge in the UI. Suppressed when count is zero.
    QueueItemsAdded {
        count: u32,
    },

    // ── Import live progress ───────────────────────────────────────
    /// A candidate's extracted signals, as extraction settles each one. The UI
    /// routes it to the one form that reads them — the search pane's
    /// autocomplete pools and its scanning indicator — rather than to the
    /// candidate's runtime, which every row and both panes hold.
    #[cfg(feature = "desktop")]
    CandidateSignalsUpdated {
        key: String,
        signals: BridgeSignals,
    },
    /// How much of the import queue the background sweep has answered — the
    /// sidebar header's line and bar. Both numbers are the queue's; a view
    /// must not derive `total` from the rows it holds, which are filtered.
    #[cfg(feature = "desktop")]
    ImportQueueIdentifyProgress {
        identified: u32,
        total: u32,
    },

    // ── Errors ─────────────────────────────────────────────────────
    Error {
        error: BridgeError,
    },
}

/// The dominant activity of a slice of the upload queue (a release's uploads,
/// or the whole queue), for the storage-row badge. Mirror of bae-core's
/// `UploadActivity`. No terminal variant: `Uploaded` still awaits release
/// publication, and the group leaves only after that transition finishes.
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

/// An actionable condition for a retrying upload. Ordinary transient failures
/// have no issue and continue retrying without asking the person for input.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeUploadIssue {
    SourceUnavailable { paths: Vec<String> },
}

/// The durable queue handoff for releases admitted by one move-to-cloud
/// command. The revision is the final canonical outbox value published before
/// the command returned.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeMakeRemoteReceipt {
    pub outbox_revision: u64,
    pub release_ids: Vec<String>,
}

/// The releases one move-to-cloud command refused, paired with the typed error
/// the UI displays. Other releases from the same command may have a receipt.
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

impl BridgeMakeRemoteReceipt {
    fn from_core(receipt: bae_core::library::MakeRemoteReceipt) -> Self {
        let bae_core::library::MakeRemoteReceipt {
            outbox_revision,
            release_ids,
        } = receipt;
        Self {
            outbox_revision,
            release_ids,
        }
    }
}

impl BridgeMakeRemoteBatchFailure {
    fn from_core(failure: bae_core::library::MakeRemoteBatchFailure) -> Self {
        let bae_core::library::MakeRemoteBatchFailure { release_ids, error } = failure;
        Self {
            release_ids,
            error: BridgeError::from_core(error),
        }
    }
}

impl BridgeMakeReleasesRemoteOutcome {
    pub(crate) fn from_core(outcome: bae_core::library::MakeReleasesRemoteOutcome) -> Self {
        use bae_core::library::MakeReleasesRemoteOutcome;
        match outcome {
            MakeReleasesRemoteOutcome::Complete { receipt } => Self::Complete {
                receipt: BridgeMakeRemoteReceipt::from_core(receipt),
            },
            MakeReleasesRemoteOutcome::Partial { receipt, failure } => Self::Partial {
                receipt: receipt.map(BridgeMakeRemoteReceipt::from_core),
                failure: BridgeMakeRemoteBatchFailure::from_core(failure),
            },
        }
    }
}

/// Which phase's bytes a progress bar counts. Mirror of bae-core's
/// `UploadPhase`. Preparation reads plaintext source bytes; the provider write
/// sends encrypted bytes of a different size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeUploadPhase {
    Preparing,
    Uploading,
}

/// One phase-scoped progress bar: bytes done and the exact total, both in
/// `phase`'s own units. Mirror of bae-core's `UploadBar`. The UI fills the bar
/// from these two numbers and writes its label from the same two, so fill and
/// text always count the same thing; `bridge_upload_phase_bytes_key` gives the
/// label its phase-naming catalog key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct BridgeUploadBar {
    pub phase: BridgeUploadPhase,
    pub bytes_done: u64,
    pub bytes_total: u64,
}

/// Localization key for a progress bar's label, which names the phase and
/// counts its bytes ("Preparing 3 MB of 224.2 MB"). The UI resolves it against
/// the `Core` table with the bar's own `bytes_done` and `bytes_total`.
#[uniffi::export]
pub fn bridge_upload_phase_bytes_key(phase: BridgeUploadPhase) -> String {
    match phase {
        BridgeUploadPhase::Preparing => "core.outbox.bytes.preparing",
        BridgeUploadPhase::Uploading => "core.outbox.bytes.uploading",
    }
    .to_string()
}

/// One file's state in the queue pane's per-file rows. A file inside an
/// unfinished release transition can render as `Uploaded`, showing that its
/// provider write finished while other files or publication remain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeUploadFileState {
    Queued,
    Preparing,
    Prepared,
    Uploading,
    Retrying,
    Uploaded,
}

/// The label for one queued upload. Source filenames cross the bridge as data;
/// image roles cross as typed cases so each platform localizes them.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeUploadFileLabel {
    Filename {
        name: String,
    },
    Cover,
    ArtistImage,
    /// A file whose row went with the release being removed from the cloud.
    /// There is nothing left to name it and nothing to name: the release
    /// leaving is what the group says.
    Unwinding,
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

/// Per-state counts, the phase-scoped progress `bar`, and a derived badge
/// `activity`. Used per-release (the storage-row badge reads `activity`;
/// storage-action gates read `can_cancel`) and as the overall total (queue
/// counts, ETA, summary band). Provider-complete files remain counted until
/// publication, so the slice stays whole over the full durable release
/// transition and across restarts.
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
    /// The bar this slice draws, counting one phase's bytes against that
    /// phase's exact total. `None` while there are no bytes to count — a
    /// release down to its make-Remote transition, or one being cancelled.
    pub bar: Option<BridgeUploadBar>,
    /// The badge activity for this slice; `None` when idle. Per-release entries
    /// always belong to an unfinished transition, so theirs is always set.
    pub activity: Option<BridgeUploadActivity>,
    /// Whether coven can still unwind this transition. False after publication
    /// begins and while cancellation is already in progress.
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
/// `state` + `bar` + `last_error` so the UI doesn't switch on associated data.
/// `source_bytes_total` is the displayed local file size; `bar` is present only
/// while this file is moving bytes, and then counts the phase it is moving them
/// in.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeUploadFileOp {
    pub file_id: String,
    pub label: BridgeUploadFileLabel,
    pub bar: Option<BridgeUploadBar>,
    pub source_bytes_total: u64,
    pub state: BridgeUploadFileState,
    pub last_error: Option<String>,
}

/// A release's uploads, grouped for the queue pane's expandable per-release
/// rows. Mirror of bae-core's `UploadReleaseGroup`. Core resolves the required
/// release id and display title before the bridge value can exist. Files retain
/// their durable queue order.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeUploadReleaseGroup {
    pub release_id: String,
    pub display_title: String,
    pub files: Vec<BridgeUploadFileOp>,
    pub progress: BridgeUploadProgress,
}

/// Whether the upload queue is running or suspended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeOutboxPauseState {
    Running,
    Paused,
}

/// The cloud-outbox processing snapshot the Storage Manager renders. The
/// counts, per-release aggregates, one-line `summary`, throughput, and ETA
/// are computed from bae-core's grouped snapshot; the UI renders them verbatim.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeOutboxSnapshot {
    /// Monotonic publication number from core. A cloud import carries the
    /// revision that first represented its enqueue, allowing a subscriber that
    /// coalesced intermediate values to recognize terminal completion.
    pub revision: u64,
    /// Uploads grouped by release for the queue pane's rows. A group leaves
    /// only after its durable make-Remote transition finishes publication.
    pub upload_groups: Vec<BridgeUploadReleaseGroup>,
    pub deletes: Vec<BridgeDeleteOp>,
    /// Per-release aggregate derived from `upload_groups`, keyed by release id.
    /// Releases with no unfinished make-Remote transition are absent.
    pub per_release: std::collections::HashMap<String, BridgeUploadProgress>,
    /// Sum across all uploads: the queue counts and the queue-wide progress
    /// bar.
    pub total: BridgeUploadProgress,
    /// Derived from `deletes.len()`.
    pub pending_deletes: u32,
    /// The one-line queue summary's parts (uploading/failed/queued/pending
    /// deletes, each dropped when zero), decided by core. The UI resolves each
    /// key and joins.
    pub summary_parts: Vec<BridgeCountLabel>,
    pub pause_state: BridgeOutboxPauseState,
    /// Rolling-window upload throughput in bytes per second. The UI formats it.
    pub throughput_bps: u64,
    /// Estimated seconds remaining at the current rate. The UI formats it.
    pub eta_seconds: Option<u64>,
}
