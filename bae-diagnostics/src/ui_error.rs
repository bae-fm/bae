//! How a failure is told to the person: a category the UI shows one
//! localized line for, and the untranslated detail behind it.

mod class;
pub use class::UiErrorClass;

/// The kind of diagnostic failure. The UI shows one generic localized line per
/// category; the paired `detail` is the underlying Rust error chain — logged and
/// offered in a copyable disclosure, never translated. Mirrors
/// `BridgeErrorCategory`; the bridge maps one to the other.
#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// This device already imported a release from the candidate's files.
    FilesAlreadyImported,
    /// The release read for the candidate lists another number of tracks than
    /// its folder holds; the line names both counts.
    MetadataTrackCount {
        folder_tracks: u32,
        release_tracks: u32,
    },
    /// Several sets of the release's mediums fit the candidate's audio
    /// equally and read differently; the line names every medium of them.
    TiedMediums {
        mediums: Vec<u32>,
    },
    /// A release edit session's save found the release changed since the
    /// session opened on it — another editor saved it, or another device's
    /// edit arrived — and wrote nothing.
    ReleaseChangedSinceOpened,
    /// A release read from several folders cannot be worked on as it stands,
    /// or cannot be made, for a reason the person can act on.
    GroupingBlocked(GroupingBlockReason),
    Export,
    Save,
    /// Storage failed — in setup, in sync, reading a file — and the line is
    /// coven's exact provider reason, so every UI can tell the person what
    /// happened: the storage is full, this device's clock is off, the provider
    /// refused the credentials, the network is down.
    Storage(coven::StorageFailure),
    StorageCheck {
        check: coven::StorageCheck,
        failure: coven::StorageFailure,
    },
    S3VersioningUnknown(coven::StorageFailure),
    S3VersioningUnsupported(coven::S3BucketVersioning),
    StorageLocationChangeUnsupported,
    /// The S3 key prefix the person typed is not a folder path coven can use:
    /// two slashes in a row, a character other than a letter, a digit, `-` or
    /// `_`, or a folder name over 255 characters. The line names the folder
    /// at fault.
    S3Prefix(coven::S3PrefixError),
    /// A cloud provider's sign-in failed (OAuth): the person signs in again.
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
    /// The phone's Block Store already holds the 16 entries Android gives an
    /// app, so the restore code coven saves for the person can't be saved.
    SavedRestoreCodesFull,
    /// The restore code doesn't fit in one Block Store entry, even with the
    /// library's name shortened.
    SavedRestoreCodeTooLarge,
    /// A library-sharing membership operation failed: the membership chain, an
    /// invite, or key rotation across devices.
    Membership,
    /// Cancelling an invite came too late: the inviting device has begun
    /// letting its person in, or the invite has ended (§5.6).
    InviteJoining,
    /// Setting up storage or restoring signed in with an account other than
    /// the one that holds the library's storage, which its member's access
    /// can't move from (§6.4).
    StoreOwnerAccount,
    /// The device playback was sent to (Cast, DLNA) refused a command — a load,
    /// a play, a seek — while the link to it stayed up.
    Renderer,
    /// A queue command named an entry, or an entry to go before, that the
    /// queue no longer holds there, and changed nothing.
    QueueChanged,
    /// An AirPlay receiver can't be driven: it demands a PIN, a pairing the
    /// sender doesn't do, or audio encryption the sender doesn't implement.
    AirPlayUnsupported,
    /// An AirPlay receiver advertises a setting bae can't read, so how to
    /// connect to it can't be decided.
    AirPlayUnreadableSettings,
    /// The library didn't open because a reset or a restore of this device's
    /// copy stopped partway; only running the call `by` names again finishes
    /// it.
    ReplacementUnfinished(coven::ReplacedBy),
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
    Diagnostic { category: UiErrorCategory, detail: String },
}

impl UiError {
    /// A diagnostic error in the given category, with the underlying error's
    /// `Display` text as the opaque, log-only detail. bae's and coven's errors
    /// each print their cause in their own message, so that text is the whole
    /// chain. Walking `source()` instead would print what a message leaves out
    /// on purpose: coven's S3 error keeps the SDK's unredacted error as its
    /// source, with whole URLs and the provider's reply, which can echo
    /// credentials.
    pub fn diagnostic(category: UiErrorCategory, detail: impl std::fmt::Display) -> Self {
        Self::Diagnostic { category, detail: detail.to_string() }
    }
    pub fn internal(detail: impl std::fmt::Display) -> Self {
        Self::diagnostic(UiErrorCategory::Internal, detail)
    }
    pub fn import(detail: impl std::fmt::Display) -> Self {
        Self::diagnostic(UiErrorCategory::Import, detail)
    }
}
