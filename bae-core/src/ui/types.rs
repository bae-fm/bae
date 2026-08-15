/// The kind of diagnostic failure. The UI shows one generic localized line per
/// category; the paired `detail` is the underlying Rust error chain — logged and
/// offered in a copyable disclosure, never translated. Mirrors
/// `BridgeErrorCategory`; the bridge maps one to the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiErrorCategory {
    Database,
    Config,
    Internal,
    Import,
    Export,
    Save,
    /// A failed cloud-home installation keeps Coven's exact reason so every UI
    /// can tell the user what must change before retrying.
    CloudSetup(coven::CloudHomeSetupFailure),
    /// The local store has no device identity. Create, join, and restore establish
    /// that identity before exposing a library.
    DeviceIdentityMissing,
    /// A cloud provider rejected the request or the setup is misconfigured: bad
    /// credentials, denied permission, a bucket/folder that isn't set. The user
    /// fixes the cloud settings; retrying unchanged won't help.
    Credentials,
    /// The cloud backend or the network to it was unreachable — a transient
    /// transport failure the user retries.
    Network,
    /// The device's OS keyring (secure credential store) couldn't be read or
    /// written — the local secret store, not the cloud.
    Keyring,
    /// A library-sharing membership operation failed: the membership chain, an
    /// invite, or key rotation across devices.
    Membership,
}

/// What a `UiError::NotFound` was looking for, so the UI can localize "… not
/// found". Mirrors `BridgeEntityKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiEntityKind {
    Library,
    Album,
    Release,
    Track,
    File,
}

/// A user-facing error carried on a UI event. The locale never crosses the
/// bridge: this is a typed reason plus, for diagnostics, the opaque Rust error
/// chain (`detail`) the UI logs and offers in a copyable disclosure but never
/// translates. The bridge maps this to `BridgeError`, which the macOS renderer
/// turns into a generic per-category line.
#[derive(Debug, Clone)]
pub enum UiError {
    /// A specific entity was missing. Keyed; the UI localizes it.
    NotFound { entity: UiEntityKind, id: String },
    /// A diagnostic failure. The UI shows a generic per-category line; `detail`
    /// is the opaque Rust error chain, never translated.
    Diagnostic {
        category: UiErrorCategory,
        detail: String,
    },
}

impl UiError {
    /// A diagnostic error in the given category, with the underlying error's
    /// `Display` text as the opaque, log-only detail.
    pub fn diagnostic(category: UiErrorCategory, detail: impl std::fmt::Display) -> Self {
        Self::Diagnostic {
            category,
            detail: detail.to_string(),
        }
    }
    pub fn internal(detail: impl std::fmt::Display) -> Self {
        Self::diagnostic(UiErrorCategory::Internal, detail)
    }
    pub fn import(detail: impl std::fmt::Display) -> Self {
        Self::diagnostic(UiErrorCategory::Import, detail)
    }
}

/// Why playback couldn't start or continue. The two cloud-only "not playable
/// yet" cases are user-actionable and keyed; every in-core failure (decode,
/// audio output, IO, missing file) is un-enumerable and routes to `Diagnostic`.
#[derive(Debug, Clone)]
pub enum PlaybackErrorReason {
    /// A remote cloud-only track has no local copy and sync is disconnected —
    /// the user reconnects cloud sync to play it. Actionable, keyed.
    SyncDisconnected,
    /// A track's cloud upload is still queued and its source file is gone, so
    /// there's nothing to play yet — the user waits for the upload.
    /// Actionable, keyed.
    UploadPending,
    /// Any other failure (decode, audio output, IO, DB, missing file). Carries
    /// the underlying `UiError`; the UI renders its generic per-category line
    /// plus the opaque, log-only detail.
    Diagnostic { error: UiError },
}

impl PlaybackErrorReason {
    /// An internal-category diagnostic reason carrying the given opaque,
    /// log-only detail. For the in-core failure paths (decode, audio output,
    /// IO) that have no user-actionable distinction.
    pub fn internal(detail: impl std::fmt::Display) -> Self {
        Self::Diagnostic {
            error: UiError::internal(detail),
        }
    }
}

/// Transient notifications that do not describe retained application state.
#[derive(Debug, Clone)]
pub enum UiBusEvent {
    /// Playback couldn't start or continue — e.g. a cloud-only track that isn't
    /// downloaded yet, or an in-core decode failure. Carries a typed reason the
    /// UI renders for its locale; playback falls back to stopped.
    PlaybackError {
        reason: PlaybackErrorReason,
    },
    /// Tracks were just appended/inserted into the queue. Carries the count
    /// for a transient "+N" UI indicator. Fires only on add operations
    /// (AddToQueue, AddNext, AddReleaseToQueue, AddReleaseNext, InsertInQueue),
    /// never on remove/reorder/clear. Suppressed when count is zero.
    QueueItemsAdded {
        count: u32,
    },

    // ── Import live progress ───────────────────────────────────────
    /// High-frequency loudness-measurement tick — goes to a native leaf view, not
    /// the @Observable store, so the sub-track cadence never churns the candidate
    /// row. `key` routes it to the importing candidate's confirm pane; `fraction`
    /// drives the determinate bar when available and `tracks_done`/`tracks_total`
    /// label which track ("N / M").
    CandidateImportLoudnessProgress {
        key: String,
        tracks_done: u32,
        tracks_total: u32,
        fraction: Option<f32>,
    },
    /// How much of the import queue the background sweep has answered. The
    /// sidebar header renders it as a line and a bar. Both numbers are the
    /// queue's, not the list's — the sidebar is filtered, so a view counting
    /// the rows it holds would report a different, wrong total.
    ImportQueueIdentifyProgress {
        identified: u32,
        total: u32,
    },

    // ── Errors ─────────────────────────────────────────────────────
    Error {
        error: UiError,
    },
}
