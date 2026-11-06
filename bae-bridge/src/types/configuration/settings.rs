use super::super::*;

#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeSortField {
    Title,
    Artist,
    Year,
    DateAdded,
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeSortDirection {
    Ascending,
    Descending,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeSortCriterion {
    pub field: BridgeSortField,
    pub direction: BridgeSortDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeSaveBitDepth {
    Bits16,
    Bits24,
    Bits32,
}

/// One piece of an export filename pattern — an ordered token list; rendering
/// substitutes each token's value and joins the non-empty values with single
/// spaces. Mirror of bae-library's `SaveFilenameToken`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeSaveFilenameToken {
    Title,
    Artist,
    Album,
    Year,
    TrackNumber,
    DiscNumber,
    TrackTotal,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeSaveCodec {
    Flac { bit_depth: BridgeSaveBitDepth },
    Mp3 { bitrate_kbps: u32 },
    Aac { bitrate_kbps: u32 },
    OpusOgg { bitrate_kbps: u32 },
    Wav { bit_depth: BridgeSaveBitDepth },
    Aiff { bit_depth: BridgeSaveBitDepth },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeSavePregapPlacement {
    AppendToPreviousExceptHtoa,
    AppendToPreviousIncludingHtoa,
    Exclude,
    SingleFileWithCue,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeSavePreset {
    pub id: String,
    pub name: String,
    pub codec: BridgeSaveCodec,
    pub filename_tokens: Vec<BridgeSaveFilenameToken>,
    pub pregap_placement: BridgeSavePregapPlacement,
    pub applies_to_track: bool,
    pub applies_to_release: bool,
    /// Whether saved files embed the release's cover art.
    pub embed_cover: bool,
}

/// Why a release read from several folders cannot be worked on as it stands,
/// or cannot be made. Each is its own line: what the person does next differs
/// — combine the folders again, separate the release in the way, or wait for
/// a download. The folder and release names ride in the untranslated detail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeGroupingBlock {
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

mirror_enum! {
    BridgeGroupingBlock = bae_diagnostics::GroupingBlockReason,
    from_core: pub(crate) fn,
    variants: {
        SourceChanged,
        SourceGone,
        FolderFilesTaken,
        FolderFilesContested,
        FolderFilesDownloading,
    },
}

#[cfg(desktop)]
impl From<&bae_import::GroupingBlock> for BridgeError {
    fn from(block: &bae_import::GroupingBlock) -> Self {
        BridgeError::from_core(block.ui_error())
    }
}

/// The kind of diagnostic failure. The UI shows one generic localized line per
/// category; `detail` is the underlying Rust error chain — logged and offered in
/// a copyable disclosure, never translated.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeErrorCategory {
    Database,
    Config,
    Internal,
    SyncUpdateRequired,
    DeviceResetRequired,
    DeviceRemoved,
    Import,
    /// Source metadata or artwork could not be parsed or decoded.
    ImportData,
    CandidateImportInProgress,
    /// A bulk import reached a row identification is still answering.
    CandidateBeingIdentified,
    /// This device already imported a release from the candidate's files.
    FilesAlreadyImported,
    /// The release read for the candidate lists another number of tracks than
    /// its folder holds; the line takes both counts.
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
    /// session opened on it, and wrote nothing.
    ReleaseChangedSinceOpened,
    /// A queue reorder named an entry, or an entry to go before, that the
    /// queue no longer holds there, and moved nothing.
    QueueChanged,
    Export,
    Save,
    /// Storage failed — in setup, in sync, reading a file — with coven's
    /// exact provider reason, which the line names.
    Storage {
        failure: BridgeStorageFailure,
    },
    StorageCheck {
        check: BridgeStorageCheck,
        failure: BridgeStorageFailure,
    },
    S3VersioningUnknown {
        failure: BridgeStorageFailure,
    },
    S3VersioningUnsupported {
        state: BridgeS3BucketVersioning,
    },
    StorageLocationChangeUnsupported,
    /// The S3 key prefix the person typed is not a folder path coven can
    /// use; the line names the folder at fault.
    // Sentinel: nothing makes it now that coven has no key prefixes; the S3
    // bucket-of-its-own commit removes it with the apps' lines for it.
    S3Prefix {
        problem: BridgeS3PrefixProblem,
    },
    /// A cloud provider's sign-in (OAuth) failed: the person signs in again.
    Credentials,
    /// The cloud backend or the network to it was unreachable — retryable.
    Network,
    /// The device's OS keyring (secure credential store) couldn't be read/written.
    Keyring,
    /// The OS keyring refused *right now* — locked session, sleeping display,
    /// no UI session to prompt in. Nothing is missing or misconfigured and the
    /// same read succeeds after unlock, so the host retries rather than
    /// reporting the library as broken.
    KeyringLocked,
    /// This build was signed without its keychain entitlement, so no keychain
    /// call succeeds; neither retrying nor unlocking fixes it.
    KeychainEntitlementMissing,
    /// Android's Block Store already holds as many saved restore codes as it
    /// keeps for an app.
    SavedRestoreCodesFull,
    /// The restore code is too large for a Block Store entry.
    SavedRestoreCodeTooLarge,
    /// A library-sharing membership operation failed (the membership chain, an
    /// invite, or cross-device key rotation).
    Membership,
    /// Cancelling an invite came too late: its person is already joining.
    InviteJoining,
    /// Storage setup or a restore signed in with an account other than the
    /// one that holds the library's storage.
    StoreOwnerAccount,
    /// The device playback was sent to refused a command while the link to it
    /// stayed up.
    Renderer,
    /// A release read from several folders cannot be worked on as it stands,
    /// or cannot be made, for a reason the person can act on.
    GroupingBlocked {
        reason: BridgeGroupingBlock,
    },
    /// An AirPlay receiver can't be driven — it demands a PIN, or offers only
    /// audio encryption the sender doesn't implement.
    AirPlayUnsupported,
    /// An AirPlay receiver advertises a setting bae can't read (a features or
    /// status-flags value that isn't hex, or no features value at all), so
    /// how to connect to it can't be decided.
    AirPlayUnreadableSettings,
    /// The library didn't open because a reset or a restore of this device's
    /// copy stopped partway; only running the call `by` names again finishes
    /// it.
    ReplacementUnfinished {
        by: BridgeReplacedBy,
    },
}

/// What a `NotFound` was looking for, so the UI can localize "… not found".
/// Only the bootstrap's missing library is reported this way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeEntityKind {
    Library,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, uniffi::Error)]
pub enum BridgeError {
    /// The user cancelled — the UI shows nothing.
    #[error("cancelled")]
    Cancelled,
    /// A specific entity was missing. User-facing and keyed; the UI localizes it.
    #[error("not found: {entity:?} {id}")]
    NotFound { entity: BridgeEntityKind, id: String },
    /// A diagnostic failure. The UI shows a generic per-category line; `detail`
    /// is the opaque Rust error chain for logs / a copyable disclosure, never
    /// translated.
    #[error("{category:?}: {detail}")]
    Diagnostic { category: BridgeErrorCategory, detail: String },
}

impl BridgeError {
    pub(crate) fn diagnostic(
        category: BridgeErrorCategory,
        detail: impl std::fmt::Display,
    ) -> Self {
        BridgeError::Diagnostic { category, detail: detail.to_string() }
    }
    /// A core failure in the class core gave it, `category`, with `detail`
    /// as its untranslated text.
    pub(crate) fn categorized(
        category: bae_diagnostics::UiErrorCategory,
        detail: impl std::fmt::Display,
    ) -> Self {
        Self::diagnostic(BridgeErrorCategory::from_core(category), detail)
    }
    pub(crate) fn database(detail: impl std::fmt::Display) -> Self {
        Self::diagnostic(BridgeErrorCategory::Database, detail)
    }

    /// A failed database read or write, carrying the fault alone.
    ///
    /// The layers a database failure crosses on its way up prefix themselves
    /// onto it — `CovenError::Database` and `LibraryError::Database` each
    /// render as "database error: {inner}". No prefix belongs in the detail:
    /// the `Database` category beside it is what says which kind of failure
    /// this is, and each UI renders that as its own localized line. A bare
    /// `DbError` crosses as its own text, through `From<DbError>`.
    pub(crate) fn database_query(error: impl DatabaseFault) -> Self {
        Self::database(error.database_fault())
    }
    pub(crate) fn config(detail: impl std::fmt::Display) -> Self {
        Self::diagnostic(BridgeErrorCategory::Config, detail)
    }
    pub(crate) fn internal(detail: impl std::fmt::Display) -> Self {
        Self::diagnostic(BridgeErrorCategory::Internal, detail)
    }
    /// The error work that did not finish fails with — it panicked, or its
    /// runtime shut down under it — once `reported` has gone to diagnostics
    /// as the bug a panic is.
    pub(crate) fn unfinished(reported: bae_util::task_failure::TaskFailed) -> Self {
        Self::internal(reported)
    }
}

/// Why playback couldn't start or continue. The cases where this device can't
/// reach the audio are user-actionable and have a line (`bridge_playback_error_reason_line`);
/// every in-core failure is un-enumerable and rides in `Diagnostic` — the UI
/// renders it through the same `BridgeError` path (generic per-category line +
/// copyable diagnostic detail).
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgePlaybackErrorReason {
    /// A remote cloud-only track isn't downloaded and sync is disconnected —
    /// the user reconnects to play it.
    SyncDisconnected,
    /// A local release whose audio is on another device only, the one named,
    /// which hasn't uploaded it.
    OnOtherDevice { device: crate::types::BridgeOtherDevice },
    /// A local release's original files are gone or changed on this device.
    OriginalMissing,
    /// The audio output didn't start, for a reason of the device's or the
    /// speaker's.
    AudioOutput { problem: BridgeAudioOutputProblem },
    /// Any other failure. Carries the underlying `BridgeError`; the UI renders
    /// its generic per-category line plus the opaque diagnostic detail.
    Diagnostic { error: BridgeError },
}

/// Localization key for a `BridgeErrorCategory`'s generic user-facing line. The
/// UI resolves this through its string catalog; `detail` is never translated.
/// One source of these keys for every platform.
#[uniffi::export]
pub fn bridge_error_category_key(category: BridgeErrorCategory) -> String {
    match category {
        BridgeErrorCategory::Database => "core.error.category.database",
        BridgeErrorCategory::Config => "core.error.category.config",
        BridgeErrorCategory::Internal => "core.error.category.internal",
        BridgeErrorCategory::DeviceResetRequired => "core.error.device_reset_required",
        BridgeErrorCategory::DeviceRemoved => "core.error.device_removed",
        BridgeErrorCategory::SyncUpdateRequired => "core.error.sync_update_required",
        BridgeErrorCategory::Import | BridgeErrorCategory::ImportData => {
            "core.error.category.import"
        }
        BridgeErrorCategory::CandidateImportInProgress => {
            "core.import.error.candidate_import_in_progress"
        }
        BridgeErrorCategory::CandidateBeingIdentified => {
            "core.import.error.candidate_being_identified"
        }
        BridgeErrorCategory::FilesAlreadyImported => "core.import.error.files_already_imported",
        BridgeErrorCategory::MetadataTrackCount { .. } => "core.import.error.metadata_track_count",
        BridgeErrorCategory::TiedMediums { .. } => "core.import.error.tied_mediums",
        BridgeErrorCategory::ReleaseChangedSinceOpened => {
            "core.release_edit.error.changed_since_opened"
        }
        BridgeErrorCategory::QueueChanged => "core.queue.error.changed",
        BridgeErrorCategory::Export => "core.error.category.export",
        BridgeErrorCategory::Save => "core.error.category.save",
        BridgeErrorCategory::Storage { failure } => {
            super::super::storage_failure::storage_failure_key(failure)
        }
        BridgeErrorCategory::StorageCheck { .. } => "core.error.storage.check",
        BridgeErrorCategory::S3VersioningUnknown { .. } => "core.error.storage.versioning_unknown",
        BridgeErrorCategory::S3VersioningUnsupported { .. } => {
            "core.error.storage.versioning_unsupported"
        }
        BridgeErrorCategory::StorageLocationChangeUnsupported => {
            "core.error.storage.location_change"
        }
        BridgeErrorCategory::S3Prefix { problem } => match problem {
            BridgeS3PrefixProblem::EmptySegment => "core.error.storage.prefix.empty_segment",
            BridgeS3PrefixProblem::InvalidCharacter { .. } => {
                "core.error.storage.prefix.invalid_character"
            }
            BridgeS3PrefixProblem::SegmentTooLong { .. } => {
                "core.error.storage.prefix.segment_too_long"
            }
        },
        BridgeErrorCategory::Credentials => "core.error.category.credentials",
        BridgeErrorCategory::Network => "core.error.category.network",
        BridgeErrorCategory::Keyring => "core.error.category.keyring",
        BridgeErrorCategory::KeyringLocked => "core.error.keyring.locked",
        BridgeErrorCategory::KeychainEntitlementMissing => "core.error.keyring.entitlement_missing",
        BridgeErrorCategory::SavedRestoreCodesFull => "core.error.keyring.saved_codes_full",
        BridgeErrorCategory::SavedRestoreCodeTooLarge => "core.error.keyring.saved_code_too_large",
        BridgeErrorCategory::Membership => "core.error.category.membership",
        BridgeErrorCategory::InviteJoining => "core.error.category.invite_joining",
        BridgeErrorCategory::StoreOwnerAccount => "core.error.category.store_owner_account",
        BridgeErrorCategory::Renderer => "core.error.category.renderer",
        BridgeErrorCategory::GroupingBlocked { reason } => match reason {
            BridgeGroupingBlock::SourceChanged => "core.import.grouping.source_changed",
            BridgeGroupingBlock::SourceGone => "core.import.grouping.source_gone",
            BridgeGroupingBlock::FolderFilesTaken => "core.import.grouping.folder_files_taken",
            BridgeGroupingBlock::FolderFilesContested => {
                "core.import.grouping.folder_files_contested"
            }
            BridgeGroupingBlock::FolderFilesDownloading => {
                "core.import.grouping.folder_files_downloading"
            }
        },
        BridgeErrorCategory::AirPlayUnsupported => "core.error.category.airplay_unsupported",
        BridgeErrorCategory::AirPlayUnreadableSettings => {
            "core.error.category.airplay_unreadable_settings"
        }
        BridgeErrorCategory::ReplacementUnfinished { by } => match by {
            BridgeReplacedBy::Reset => "core.error.replacement_unfinished.reset",
            BridgeReplacedBy::Restore => "core.error.replacement_unfinished.restore",
        },
    }
    .to_string()
}

/// Localization key for a `BridgeEntityKind`'s "… not found" line. One source
/// of these keys for every platform.
#[uniffi::export]
pub fn bridge_entity_not_found_key(entity: BridgeEntityKind) -> String {
    match entity {
        BridgeEntityKind::Library => "core.error.not_found.library",
    }
    .to_string()
}

/// An error that carries a database failure behind a prefix of its own.
///
/// Implemented for the two wrappers a failed read crosses on its way to a
/// person, so the detail they are shown names the fault once.
pub(crate) trait DatabaseFault {
    fn database_fault(self) -> String;
}

impl DatabaseFault for coven::CovenError {
    fn database_fault(self) -> String {
        match self {
            coven::CovenError::Database(inner) => inner.to_string(),
            other => other.to_string(),
        }
    }
}

impl DatabaseFault for bae_library::LibraryError {
    fn database_fault(self) -> String {
        match self {
            bae_library::LibraryError::Database(inner) => inner.to_string(),
            other => other.to_string(),
        }
    }
}

/// The catalog key for an error's user-facing line, or `None` when the error has
/// no line to show.
///
/// A cancellation is the user's own doing and says nothing back to them, so it
/// has no line — and `None` is how that is said. Left to each app, "no line"
/// became `""` on Swift and Kotlin (which then opened a blank error alert,
/// because nothing filters an empty string) and "an internal error occurred" on
/// Windows, which is neither internal nor an error. Whether an error is worth
/// showing is a decision about the error, so it is made once, here.
#[uniffi::export]
pub fn bridge_error_line_key(error: &BridgeError) -> Option<String> {
    match error {
        BridgeError::Cancelled => None,
        BridgeError::NotFound { entity, .. } => Some(bridge_entity_not_found_key(*entity)),
        BridgeError::Diagnostic { category, .. } => {
            Some(bridge_error_category_key(category.clone()))
        }
    }
}

/// The detail an error shows under its line, or `None` when it shows the
/// line alone: it has no detail, or its category's line already names the
/// whole fault and the detail would say it again, in English. Core decides
/// per category ([`bae_diagnostics::UiErrorClass::detail_adds_to_line`]), so
/// no app judges which details are worth showing.
#[uniffi::export]
pub fn bridge_error_shown_detail(error: &BridgeError) -> Option<String> {
    match error {
        BridgeError::Cancelled | BridgeError::NotFound { .. } => None,
        BridgeError::Diagnostic { category, detail } => {
            category.detail_adds_to_line().then(|| detail.clone())
        }
    }
}

impl BridgeErrorCategory {
    /// Whether this category's detail says something its line doesn't: core's
    /// answer for the category's class.
    fn detail_adds_to_line(&self) -> bool {
        use bae_diagnostics::UiErrorClass as K;
        let class = match self {
            Self::Database => K::Database,
            Self::Config => K::Config,
            Self::Internal => K::Internal,
            Self::SyncUpdateRequired => K::SyncUpdateRequired,
            Self::DeviceResetRequired => K::DeviceResetRequired,
            Self::DeviceRemoved => K::DeviceRemoved,
            Self::Import => K::Import,
            Self::ImportData => K::ImportData,
            Self::CandidateImportInProgress => K::CandidateImportInProgress,
            Self::CandidateBeingIdentified => K::CandidateBeingIdentified,
            Self::FilesAlreadyImported => K::FilesAlreadyImported,
            Self::MetadataTrackCount { .. } => K::MetadataTrackCount,
            Self::TiedMediums { .. } => K::TiedMediums,
            Self::ReleaseChangedSinceOpened => K::ReleaseChangedSinceOpened,
            Self::GroupingBlocked { .. } => K::GroupingBlocked,
            Self::Export => K::Export,
            Self::Save => K::Save,
            Self::Storage { .. } => K::Storage,
            Self::StorageCheck { .. } => K::StorageCheck,
            Self::S3VersioningUnknown { .. } => K::S3VersioningUnknown,
            Self::S3VersioningUnsupported { .. } => K::S3VersioningUnsupported,
            Self::StorageLocationChangeUnsupported => K::StorageLocationChangeUnsupported,
            // Sentinel: nothing makes it now that coven has no key prefixes;
            // the S3 bucket-of-its-own commit removes it.
            Self::S3Prefix { .. } => K::Storage,
            Self::Credentials => K::Credentials,
            Self::Network => K::Network,
            Self::Keyring => K::Keyring,
            Self::KeyringLocked => K::KeyringLocked,
            Self::KeychainEntitlementMissing => K::KeychainEntitlementMissing,
            Self::SavedRestoreCodesFull => K::SavedRestoreCodesFull,
            Self::SavedRestoreCodeTooLarge => K::SavedRestoreCodeTooLarge,
            Self::Membership => K::Membership,
            Self::InviteJoining => K::InviteJoining,
            Self::StoreOwnerAccount => K::StoreOwnerAccount,
            Self::Renderer => K::Renderer,
            Self::QueueChanged => K::QueueChanged,
            Self::AirPlayUnsupported => K::AirPlayUnsupported,
            Self::AirPlayUnreadableSettings => K::AirPlayUnreadableSettings,
            Self::ReplacementUnfinished { .. } => K::ReplacementUnfinished,
        };
        class.detail_adds_to_line()
    }
}

/// What kept the audio output from starting. Mirrors
/// `bae_playback::AudioOutputProblem`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeAudioOutputProblem {
    /// The audio device was disconnected or is no longer available.
    DeviceUnavailable,
    /// The audio device can't play the track's sample rate and channels.
    UnsupportedFormat,
    /// The AirPlay speaker couldn't be reached or didn't accept the session.
    AirPlayRefused,
}

/// The line for the actionable `BridgePlaybackErrorReason` variants, or
/// `None` for `Diagnostic` (which the UI renders through the `BridgeError`
/// category path instead). A release on another device names it, as the
/// release itself does. One source of these lines for every platform.
#[uniffi::export]
pub fn bridge_playback_error_reason_line(
    reason: &BridgePlaybackErrorReason,
) -> Option<BridgeMessage> {
    let keyed = |key: &str| Some(BridgeMessage { key: key.to_string(), args: Vec::new() });
    match reason {
        BridgePlaybackErrorReason::SyncDisconnected => {
            keyed("core.playback.error.sync_disconnected")
        }
        BridgePlaybackErrorReason::OnOtherDevice { device } => Some(device.line()),
        BridgePlaybackErrorReason::OriginalMissing => keyed("core.release.original_missing"),
        BridgePlaybackErrorReason::AudioOutput { problem } => keyed(match problem {
            BridgeAudioOutputProblem::DeviceUnavailable => {
                "core.playback.error.audio_device_unavailable"
            }
            BridgeAudioOutputProblem::UnsupportedFormat => {
                "core.playback.error.audio_format_unsupported"
            }
            BridgeAudioOutputProblem::AirPlayRefused => "core.playback.error.airplay_refused",
        }),
        BridgePlaybackErrorReason::Diagnostic { .. } => None,
    }
}

// =========================================================================
// Core <-> Bridge conversions
//
// Convention, crate-wide — conversions also sit above this banner, in
// `handle.rs`, and in `bridge_utils.rs`: a conversion is an associated function
// on the `Bridge*` type, `BridgeX::from_core(core) -> Self` and
// `BridgeX::into_core(self) -> core::X`. Two exceptions: some `from_core`s take
// extra bridge-only arguments (`BridgeFolderCandidate`), and `LibraryError`
// crosses through a `From` impl.
//
// Record converters exhaustively destructure their core input(s) — no `..` in
// any struct or enum-variant pattern — so a new bae-library field fails the build
// here instead of silently never crossing the bridge. A dropped field is named
// explicitly (`field: _`), with a comment when the drop isn't obvious. Fields of
// external-crate types (coven's `StorageConfig`, …) are exempt — the
// bridge can't police a pinned crate's field list — and stay dotted reads.
// =========================================================================

mirror_enum! {
    BridgeErrorCategory = bae_diagnostics::UiErrorCategory,
    from_core: pub(crate) fn,
    variants: {
        Database,
        Config,
        Internal,
        SyncUpdateRequired,
        DeviceResetRequired,
        DeviceRemoved,
        Import,
        ImportData,
        CandidateImportInProgress,
        CandidateBeingIdentified,
        FilesAlreadyImported,
        MetadataTrackCount { folder_tracks, release_tracks },
        TiedMediums { mediums },
        ReleaseChangedSinceOpened,
        GroupingBlocked(reason: (BridgeGroupingBlock)),
        Export,
        Save,
        Storage(failure: (BridgeStorageFailure)),
        StorageCheck {check:(BridgeStorageCheck),failure:(BridgeStorageFailure)},
        S3VersioningUnknown(failure:(BridgeStorageFailure)),
        S3VersioningUnsupported(state:(BridgeS3BucketVersioning)),
        StorageLocationChangeUnsupported,
        Credentials,
        Network,
        Keyring,
        KeyringLocked,
        KeychainEntitlementMissing,
        SavedRestoreCodesFull,
        SavedRestoreCodeTooLarge,
        Membership,
        InviteJoining,
        StoreOwnerAccount,
        Renderer,
        QueueChanged,
        AirPlayUnsupported,
        AirPlayUnreadableSettings,
        ReplacementUnfinished(by: (BridgeReplacedBy)),
    },
}

mirror_enum! {
    /// `Cancelled` and `NotFound` are the bridge's own: core reports a
    /// cancellation through the operation's result rather than as a `UiError`,
    /// and the only missing entity anyone is told about — the library the
    /// bootstrap could not open — is found before a `UiError` exists.
    BridgeError = bae_diagnostics::UiError,
    from_core: pub(crate) fn,
    variants: {
        Diagnostic { category: (BridgeErrorCategory), detail },
    },
}

/// A failed config read or write, in the class core gave it: a setting the
/// person can fix, or a write that never finished.
/// A subscription's end is the caller's cancellation; a failed read is a
/// database query's failure.
impl From<bae_store::live_query::LiveQueryError> for BridgeError {
    fn from(error: bae_store::live_query::LiveQueryError) -> Self {
        match error {
            bae_store::live_query::LiveQueryError::Cancelled => Self::Cancelled,
            bae_store::live_query::LiveQueryError::Query(error) => Self::database_query(error),
        }
    }
}

impl From<bae_preferences::PreferencesError> for BridgeError {
    fn from(error: bae_preferences::PreferencesError) -> Self {
        BridgeError::categorized(error.category(), error)
    }
}

/// A failed read or write of bae's directory, in the class core gave it.
impl From<bae_app_dir::AppDirError> for BridgeError {
    fn from(error: bae_app_dir::AppDirError) -> Self {
        BridgeError::categorized(error.category(), error)
    }
}

/// A metadata source's switch failed on its preference or on the searches it
/// closes, each crossing as that error does.
impl From<bae_app::MetadataSourceSwitchError> for BridgeError {
    fn from(error: bae_app::MetadataSourceSwitchError) -> Self {
        match error {
            bae_app::MetadataSourceSwitchError::Preference(error) => error.into(),
            bae_app::MetadataSourceSwitchError::Searches(error) => error.into(),
        }
    }
}

/// An import failure crosses in the class core gave it.
#[cfg(desktop)]
impl From<bae_import::ImportError> for BridgeError {
    fn from(error: bae_import::ImportError) -> Self {
        BridgeError::from_core(error.ui_error())
    }
}

/// Carry a core `LibraryError`'s diagnostic class across the bridge: the class
/// (keyring vs cloud credentials vs network vs membership vs …) becomes the
/// `BridgeErrorCategory` the UI renders a localized line for; the error chain
/// rides along as opaque diagnostic detail.
impl From<bae_library::LibraryError> for BridgeError {
    fn from(error: bae_library::LibraryError) -> Self {
        if matches!(
            error,
            bae_library::LibraryError::CloudSetup(coven::StorageSetupError::OAuth(
                coven::OAuthError::Cancelled
            ))
        ) {
            return BridgeError::Cancelled;
        }
        let category = error.category();
        BridgeError::categorized(category, error.database_fault())
    }
}

impl From<bae_library::CreateLibraryError> for BridgeError {
    fn from(error: bae_library::CreateLibraryError) -> Self {
        BridgeError::categorized(error.category(), error)
    }
}

impl BridgePlaybackErrorReason {
    pub(crate) fn from_core(reason: bae_playback::PlaybackErrorReason) -> Self {
        use bae_playback::PlaybackErrorReason;
        match reason {
            PlaybackErrorReason::SyncDisconnected => Self::SyncDisconnected,
            PlaybackErrorReason::OnOtherDevice(device) => {
                Self::OnOtherDevice { device: crate::types::BridgeOtherDevice::from_core(device) }
            }
            PlaybackErrorReason::OriginalMissing => Self::OriginalMissing,
            PlaybackErrorReason::AudioOutput { problem } => {
                Self::AudioOutput { problem: BridgeAudioOutputProblem::from_core(problem) }
            }
            PlaybackErrorReason::Diagnostic { failure, detail } => {
                Self::Diagnostic { error: BridgeError::categorized(failure.category(), detail) }
            }
        }
    }
}

mirror_enum! {
    BridgeAudioOutputProblem = bae_playback::AudioOutputProblem,
    from_core: pub(crate) fn,
    variants: { DeviceUnavailable, UnsupportedFormat, AirPlayRefused },
}

#[cfg(desktop)]
impl BridgeCoverSelection {
    pub(crate) fn into_core(self) -> bae_import::CoverSelection {
        match self {
            BridgeCoverSelection::ReleaseImage { file_id } => {
                bae_import::CoverSelection::Local(file_id)
            }
            BridgeCoverSelection::RemoteCover { selection } => bae_import::CoverSelection::Remote(
                selection.image.into_core(),
                selection.source.into_core(),
            ),
            BridgeCoverSelection::EmbeddedCover { source_file_id } => {
                bae_import::CoverSelection::Embedded(source_file_id)
            }
        }
    }
}

impl BridgeCloudProvider {
    pub(crate) fn from_core(p: &coven::CloudProvider) -> Self {
        use coven::CloudProvider;
        match p {
            CloudProvider::S3 => BridgeCloudProvider::S3,
            CloudProvider::Dropbox => BridgeCloudProvider::Dropbox,
            CloudProvider::OneDrive => BridgeCloudProvider::OneDrive,
            CloudProvider::CloudKit => BridgeCloudProvider::CloudKit,
        }
    }

    #[cfg(feature = "oauth-providers")]
    pub(crate) fn into_core(self) -> coven::CloudProvider {
        use coven::CloudProvider;
        match self {
            BridgeCloudProvider::S3 => CloudProvider::S3,
            BridgeCloudProvider::Dropbox => CloudProvider::Dropbox,
            BridgeCloudProvider::OneDrive => CloudProvider::OneDrive,
            BridgeCloudProvider::CloudKit => CloudProvider::CloudKit,
        }
    }
}

mirror_enum! {
    BridgeStorageSortField = bae_views::StorageSortField,
    from_core: fn,
    into_core: fn,
    variants: { AlbumTitle, ArtistNames, Media, FileCount, TotalSize, Storage, DateAdded },
}

mirror_enum! {
    BridgeStorageSortDirection = bae_store::SortDirection,
    from_core: fn,
    into_core: fn,
    variants: { Ascending, Descending },
}

mirror_struct! {
    BridgeStorageSort = bae_views::StorageSortCriterion,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    fields: {
        field: (BridgeStorageSortField),
        direction: (BridgeStorageSortDirection),
    },
}

mirror_enum! {
    BridgeStorageFilter = bae_views::StorageFilter,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    variants: { All, Remote, Local },
}

mirror_enum! {
    BridgeComposerSortField = bae_views::ComposerSortField,
    into_core: fn,
    variants: { Name, WorkCount, LinkedReleaseCount },
}

mirror_struct! {
    BridgeComposerSortCriterion = bae_views::ComposerSortCriterion,
    into_core: pub(crate) fn,
    fields: {
        field: (BridgeComposerSortField),
        direction: (BridgeSortDirection),
    },
}

mirror_enum! {
    BridgeArtistSortField = bae_views::ArtistSortField,
    into_core: fn,
    variants: { Name, AlbumCount },
}

mirror_struct! {
    BridgeArtistSortCriterion = bae_views::ArtistSortCriterion,
    into_core: pub(crate) fn,
    fields: {
        field: (BridgeArtistSortField),
        direction: (BridgeSortDirection),
    },
}

mirror_enum! {
    BridgeSortDirection = bae_store::SortDirection,
    into_core: pub(crate) fn,
    variants: { Ascending, Descending },
}

mirror_enum! {
    BridgeSortField = bae_views::AlbumSortField,
    into_core: fn,
    variants: { Title, Artist, Year, DateAdded },
}

mirror_struct! {
    BridgeSortCriterion = bae_views::AlbumSortCriterion,
    into_core: pub(crate) fn,
    fields: {
        field: (BridgeSortField),
        direction: (BridgeSortDirection),
    },
}

#[cfg(all(test, desktop))]
#[path = "settings_tests.rs"]
mod tests;
