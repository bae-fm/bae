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

/// A scoped state key the UI should requery from core.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Invalidation {
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
    // The discovered Cast device list changed; the UI requeries it.
    CastDevices,
}

/// Top-level UI event. One enum for everything. Events that carry a new value
/// inline use a top-level variant; query-backed state changes use a scoped
/// invalidation key.
/// High-frequency events (PlaybackProgress, PreviewProgress) go to NSViews on
/// the native side. Everything else goes to the @Observable store.
#[derive(Debug, Clone)]
pub enum UiBusEvent {
    Invalidated(Invalidation),

    // ── Playback ───────────────────────────────────────────────────
    PlaybackStopped,
    /// Playback couldn't start or continue — e.g. a cloud-only track that isn't
    /// downloaded yet, or an in-core decode failure. Carries a typed reason the
    /// UI renders for its locale; playback falls back to stopped.
    PlaybackError {
        reason: PlaybackErrorReason,
    },
    PlaybackLoading {
        track_id: String,
        /// The target track's display metadata, once core has resolved it.
        /// `None` in the first loading event (before the DB lookup), `Some`
        /// once the prepared track is in hand — letting the UI switch the
        /// now-playing bar from the prior track to the target while audio is
        /// still downloading.
        track: Option<crate::playback::LoadingTrack>,
    },
    PlaybackPlaying {
        track_id: String,
        track_title: String,
        artist_names: String,
        artist_id: String,
        album_id: String,
        album_title: String,
        cover_image_id: Option<String>,
        duration_ms: u64,
    },
    PlaybackPaused {
        track_id: String,
        track_title: String,
        artist_names: String,
        artist_id: String,
        album_id: String,
        album_title: String,
        cover_image_id: Option<String>,
        duration_ms: u64,
        reason: crate::playback::PlaybackPauseReason,
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
        mode: crate::playback::RepeatMode,
    },
    /// The queue's two lanes, kept separate so each UI renders them as distinct
    /// sections: the manual lane ("Up Next") in order, and the context (the
    /// release being played from) as its not-yet-played tail plus its shuffled
    /// flag, or `None` when nothing plays from a release.
    QueueUpdated(crate::queue::ResolvedQueueSnapshot),
    /// Tracks were just appended/inserted into the queue. Carries the count
    /// for a transient "+N" UI indicator. Fires only on add operations
    /// (AddToQueue, AddNext, AddReleaseToQueue, AddReleaseNext, InsertInQueue),
    /// never on remove/reorder/clear. Suppressed when count is zero.
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
    /// High-frequency loudness-measurement tick — goes to a native leaf view, not
    /// the @Observable store, so the sub-track cadence never churns the candidate
    /// row. `key` routes it to the importing candidate's confirm pane; `fraction`
    /// (0..1) drives the determinate bar and `tracks_done`/`tracks_total` label
    /// which track ("N / M").
    CandidateImportLoudnessProgress {
        key: String,
        tracks_done: u32,
        tracks_total: u32,
        fraction: f32,
    },

    // ── Release transfer ───────────────────────────────────────────
    /// A pin/unpin/manage/unmanage transition started. The UI shows an
    /// in-flight indicator on the release row until `ReleaseTransferEnded`.
    ReleaseTransferProgress {
        release_id: String,
        action: crate::album_detail::ReleaseStorageAction,
    },
    /// A transition finished (success or failure) — the UI clears its transfer
    /// indicator. Failure text still arrives via the thrown error.
    ReleaseTransferEnded {
        release_id: String,
    },

    // ── Cast ───────────────────────────────────────────────────────
    /// The active renderer changed: `Some(name)` when playback moved to a Cast
    /// device, `None` when it returned to local output. Drives the cast button's
    /// active state and the "Casting to <name>" row.
    CastStatusChanged {
        device_name: Option<String>,
    },

    // ── Errors ─────────────────────────────────────────────────────
    Error {
        error: UiError,
    },
}
