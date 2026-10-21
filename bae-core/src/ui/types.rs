/// The kind of diagnostic failure. The UI shows one generic localized line per
/// category; the paired `detail` is the underlying Rust error chain — logged and
/// offered in a copyable disclosure, never translated. Mirrors
/// `BridgeErrorCategory`; the bridge maps one to the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiErrorCategory {
    Database,
    Config,
    Internal,
    /// Sync encountered data written by a newer app schema.
    SyncUpdateRequired,
    DeviceResetRequired,
    /// This device, or its person, was removed from the library; syncing has
    /// stopped for good and nothing on this device registers it again.
    DeviceRemoved,
    Import,
    /// Source metadata or artwork could not be parsed or decoded.
    ImportData,
    /// The candidate's import is already running.
    CandidateImportInProgress,
    /// Identification is still answering for the candidate.
    CandidateBeingIdentified,
    /// The release read for the candidate lists another number of tracks than
    /// its folder holds; the line names both counts.
    MetadataTrackCount {
        folder_tracks: u32,
        release_tracks: u32,
    },
    /// A release read from several folders cannot be worked on as it stands,
    /// or cannot be made, for a reason the person can act on.
    GroupingBlocked(GroupingBlockReason),
    Export,
    Save,
    /// A failed storage setup keeps coven's exact provider reason so every UI
    /// can tell the user what must change before retrying.
    CloudSetup(coven::StorageFailure),
    StorageCheck {
        check: coven::StorageCheck,
        failure: coven::StorageFailure,
    },
    S3VersioningUnknown(coven::StorageFailure),
    S3VersioningUnsupported(coven::S3BucketVersioning),
    StorageLocationChangeUnsupported,
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
    /// The OS keyring refused *right now* — locked session, sleeping display, no
    /// UI session to prompt in. Distinct from `Keyring` because nothing is
    /// broken and nothing is missing: the same read succeeds after unlock, so
    /// the host retries instead of reporting the library as unusable.
    KeyringLocked,
    /// This build of the app was signed without its keychain entitlement
    /// (Apple's errSecMissingEntitlement), so every keychain call fails the
    /// same way. A packaging fault, not the person's keyring or storage:
    /// retrying or unlocking changes nothing until the app is signed with it.
    KeychainEntitlementMissing,
    /// A library-sharing membership operation failed: the membership chain, an
    /// invite, or key rotation across devices.
    Membership,
    /// The device playback was sent to (Cast, DLNA) refused a command — a load,
    /// a play, a seek — while the link to it stayed up.
    Renderer,
}

/// Why a release read from several folders cannot be worked on as it stands,
/// or cannot be made. Each asks something different of the person: combine
/// the folders again, separate the release in the way, or wait for a
/// download.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupingBlockReason {
    /// A folder the release is read from changed.
    SourceChanged,
    /// A folder the release is read from is gone.
    SourceGone,
    /// The files of the folder its folders sit in go with another release.
    FolderFilesTaken,
    /// More than one release would take the files of the folder its folders
    /// sit in.
    FolderFilesContested,
    /// A download into the folder its folders sit in is still running.
    FolderFilesDownloading,
}

/// A user-facing error carried on a UI event. The locale never crosses the
/// bridge: this is a typed reason plus, for diagnostics, the opaque Rust error
/// chain (`detail`) the UI logs and offers in a copyable disclosure but never
/// translates. The bridge maps this to `BridgeError`, which the macOS renderer
/// turns into a generic per-category line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiError {
    /// A diagnostic failure. The UI shows a generic per-category line; `detail`
    /// is the opaque Rust error chain, never translated.
    Diagnostic {
        category: UiErrorCategory,
        detail: String,
    },
}

impl UiError {
    /// Reconnecting can retry a failed cycle, but cannot teach this app a newer schema.
    pub fn can_reconnect_sync(&self) -> bool {
        !matches!(
            self,
            Self::Diagnostic {
                category: UiErrorCategory::SyncUpdateRequired,
                ..
            }
        )
    }

    /// A diagnostic error in the given category, with the underlying error's
    /// `Display` text as the opaque, log-only detail. bae's and coven's errors
    /// each print their cause in their own message, so that text is the whole
    /// chain. Walking `source()` instead would print what a message leaves out
    /// on purpose: coven's S3 error keeps the SDK's unredacted error as its
    /// source, with whole URLs and the provider's reply, which can echo
    /// credentials.
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

/// Why playback couldn't start or continue. The cases where this device can't
/// reach the release's audio are user-actionable and keyed; every in-core
/// failure (decode, audio output, IO) is un-enumerable and routes to
/// `Diagnostic`.
#[derive(Debug, Clone)]
pub enum PlaybackErrorReason {
    /// A remote cloud-only track has no local copy and sync is disconnected —
    /// the user reconnects cloud sync to play it. Actionable, keyed.
    SyncDisconnected,
    /// Another device attached the release and its upload hasn't reached
    /// storage yet — the user waits for the upload. Actionable, keyed.
    UploadPending,
    /// A local release imported on another device, which hasn't uploaded it.
    /// Actionable, keyed.
    NotUploaded,
    /// A local release's original files are gone or changed on this device.
    /// Actionable, keyed.
    OriginalMissing,
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

/// What the UI hears outside its value subscriptions: playback's notices,
/// and the import runtime's values as they change.
#[derive(Debug, Clone)]
pub enum UiEvent {
    /// Playback couldn't start or continue — e.g. a cloud-only track that isn't
    /// downloaded yet, or an in-core decode failure. Carries a typed reason the
    /// UI renders for its locale; playback falls back to stopped.
    PlaybackError { reason: PlaybackErrorReason },
    /// Tracks were just appended/inserted into the queue. Carries the count
    /// for a transient "+N" UI indicator. Fires only on add operations
    /// (AddToQueue, AddNext, AddReleaseToQueue, AddReleaseNext, InsertInQueue),
    /// never on remove/reorder/clear. Suppressed when count is zero.
    QueueItemsAdded { count: u32 },

    // ── Import live progress ───────────────────────────────────────
    /// A candidate's extracted signals, as extraction settles each one. Goes
    /// to the one form that reads them — the search pane's autocomplete pools
    /// and its scanning indicator — rather than to the candidate's runtime,
    /// which every row and both panes hold: OCR reports several times per
    /// candidate and nothing else on screen changes with it. `key` routes it to
    /// that candidate's form.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    CandidateSignalsUpdated {
        key: String,
        signals: crate::signals::Signals,
    },
    /// How far the identifications running right now have got: how many have
    /// ended, out of how many there are, whoever started them. The sidebar
    /// header renders it as a ring, a line and a bar. Both numbers are the
    /// runtime's, not the list's — the sidebar is filtered, so a view counting
    /// the rows it holds would report a different, wrong total. `(0, 0)` is
    /// none running, which the header shows nothing for.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    ImportIdentificationProgress { identified: u32, total: u32 },
    /// How many imports are waiting or running, whoever started them. The
    /// sidebar header shows them, and a way to cancel them all, while there
    /// are any.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    ImportsInFlight { count: u32 },
}
