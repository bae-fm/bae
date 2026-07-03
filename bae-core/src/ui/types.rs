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
    /// A remote track's cloud upload is still queued and its source file is
    /// gone, so there's nothing to play yet — the user waits for the upload.
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

/// Top-level UI event. One enum for everything — every distinct state is a
/// top-level variant with fields inlined, no sub-enums.
/// High-frequency events (PlaybackProgress, PreviewProgress) go to NSViews on
/// the native side. Everything else goes to the @Observable store.
#[derive(Debug, Clone)]
pub enum UiBusEvent {
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
    QueueUpdated {
        manual: Vec<crate::queue::QueueItem>,
        context: Option<crate::queue::ResolvedContext>,
        has_next: bool,
        has_previous: bool,
    },
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

    // ── Candidate-scoped (key inlined) ─────────────────────────────
    /// Identify pipeline transitioned to a new state. One variant per state;
    /// the reducer switches on `state` to update the store. Carries the
    /// pre-shaped signals toolbar (interactive badge row) projected from the
    /// same transition, written onto the candidate wholesale.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    CandidateIdentifyStateChanged {
        key: String,
        state: crate::identify::IdentifyState,
        toolbar: Vec<crate::identify::ToolbarSignal>,
    },
    /// Full snapshot of a candidate's extracted signals (disc ID, barcodes,
    /// classified text). Core emits this on extraction start, each source/OCR
    /// completion, natural end, and cancellation. Reducer writes the whole
    /// snapshot wholesale.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    CandidateSignalsUpdated {
        key: String,
        signals: crate::signals::Signals,
    },
    CandidateImportImporting {
        key: String,
        progress_percent: u32,
        step: Option<crate::import::ImportStep>,
    },
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
    CandidateImportComplete {
        key: String,
        /// The release the import created. Carried so the import UI can
        /// invalidate this candidate when that release is deleted and join
        /// the per-release upload queue while the cloud copy is pending.
        release_id: String,
        album_id: String,
    },
    CandidateImportError {
        key: String,
        error: UiError,
    },

    // ── Scan ───────────────────────────────────────────────────────
    /// The watched-folder list changed (loaded, or after add/remove). The
    /// reducer replaces its copy and drops candidates whose source folder is
    /// no longer watched.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    WatchedFoldersChanged {
        folders: Vec<crate::import::WatchedFolder>,
    },
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    FolderCandidateAdded {
        candidate: crate::import::FolderCandidate,
    },
    /// A leaf folder looked like a release but failed validation. The reducer
    /// surfaces it under the Skipped tab with its reason, dropping the folder
    /// from the valid-candidate list if it was there before.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    InvalidCandidate {
        candidate: crate::import::InvalidCandidate,
    },
    /// A candidate's folder was re-scanned by the watcher and the release is
    /// gone. The reducer removes it by key.
    ScanCandidateRemoved {
        key: String,
    },
    /// The user manually skipped or unskipped a candidate. The reducer flips the
    /// candidate's `skipped` flag in place, re-tabbing it New ↔ Skipped.
    CandidateSkipChanged {
        key: String,
        skipped: bool,
    },
    ScanFinished,

    // ── Library ────────────────────────────────────────────────────
    AlbumAdded {
        album: crate::album_detail::AlbumDetail,
    },
    AlbumUpdated {
        album: crate::album_detail::AlbumDetail,
    },
    AlbumRemoved {
        album_id: String,
        release_ids: Vec<String>,
    },
    ReleaseAdded {
        album: crate::album_detail::AlbumSummary,
        release: crate::album_detail::ReleaseDetail,
    },
    ReleaseUpdated {
        album_id: String,
        release: crate::album_detail::ReleaseDetail,
    },
    ReleaseRemoved {
        album_id: String,
        release_id: String,
        album: Option<crate::album_detail::AlbumSummary>,
    },
    ConfigChanged {
        config: crate::config::Config,
        /// Whether the sync loop is running. Bundled here so every consumer
        /// sees a consistent (config, sync_ready) pair without a second
        /// subscription. The producer reads `LibraryManager::is_sync_ready`
        /// at emit time.
        sync_ready: bool,
    },
    /// Sync loop's current error state. `None` means sync is healthy (clears a
    /// prior failure). When set, it's a `UiError::Diagnostic` whose category
    /// keys the generic line and whose detail is the opaque, log-only error
    /// chain the UI offers in a copyable disclosure under the reconnect banner.
    SyncError {
        error: Option<UiError>,
    },
    /// Wall-clock time of the latest successful sync cycle, as Unix epoch
    /// milliseconds. `None` until the first cycle completes; updated whenever
    /// the timestamp changes.
    SyncTimeChanged {
        time: Option<i64>,
    },
    /// Whether the sync loop is currently mid-cycle. Drives the spinner the
    /// sidebar overlays on the active library row from "Sync Now" through to
    /// the cycle ending.
    SyncingChanged {
        syncing: bool,
    },
    /// The cloud outbox processing snapshot changed — the Storage Manager
    /// re-renders its queue panel from this.
    OutboxChanged {
        snapshot: crate::library::OutboxSnapshot,
    },
    /// A pin/unpin/manage/unmanage transition advanced. `percent` is the
    /// overall release progress; `label` is a ready-to-render line. The UI
    /// shows a determinate bar on the release row until `ReleaseTransferEnded`.
    ReleaseTransferProgress {
        release_id: String,
        action: crate::album_detail::ReleaseStorageAction,
        file_no: Option<u32>,
        total: Option<u32>,
        percent: u8,
    },
    /// A transition finished (success or failure) — the UI clears its transfer
    /// indicator. Failure text still arrives via the thrown error.
    ReleaseTransferEnded {
        release_id: String,
    },
    /// The in-memory download (pin) queue changed — the Storage Manager
    /// re-renders its Downloads pane from this.
    DownloadQueueChanged {
        snapshot: crate::library::DownloadSnapshot,
    },
    /// The in-memory export queue changed — the Storage Manager re-renders its
    /// Exporting pane from this.
    ExportQueueChanged {
        snapshot: crate::library::ExportSnapshot,
    },

    // ── Errors ─────────────────────────────────────────────────────
    Error {
        error: UiError,
    },
    ErrorCleared,
}
