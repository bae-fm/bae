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
    Source,
    Bits16,
    Bits24,
    Bits32,
}

/// One piece of an export filename pattern — an ordered token list; rendering
/// substitutes each token's value and joins the non-empty values with single
/// spaces. Mirror of bae-core's `SaveFilenameToken`.
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
    pub extension: String,
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
    BridgeGroupingBlock = bae_core::ui::GroupingBlockReason,
    from_core: pub(crate) fn,
    variants: {
        SourceChanged,
        SourceGone,
        FolderFilesTaken,
        FolderFilesContested,
        FolderFilesDownloading,
    },
}

#[cfg(feature = "desktop")]
impl From<&bae_core::import::GroupingBlock> for BridgeError {
    fn from(block: &bae_core::import::GroupingBlock) -> Self {
        BridgeError::from_core(block.ui_error())
    }
}

/// The kind of diagnostic failure. The UI shows one generic localized line per
/// category; `detail` is the underlying Rust error chain — logged and offered in
/// a copyable disclosure, never translated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
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
    /// The release read for the candidate lists another number of tracks than
    /// its folder holds; the line takes both counts.
    MetadataTrackCount {
        folder_tracks: u32,
        release_tracks: u32,
    },
    Export,
    Save,
    CloudSetup {
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
    DeviceIdentityMissing,
    /// A cloud provider rejected the request or the setup is misconfigured (bad
    /// credentials, denied permission, a bucket/folder that isn't set).
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
    /// A library-sharing membership operation failed (the membership chain, an
    /// invite, or cross-device key rotation).
    Membership,
    /// A release read from several folders cannot be worked on as it stands,
    /// or cannot be made, for a reason the person can act on.
    GroupingBlocked {
        reason: BridgeGroupingBlock,
    },
    /// An AirPlay receiver can't be driven — it demands a PIN, or offers only
    /// audio encryption the sender doesn't implement.
    AirPlayUnsupported,
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
    NotFound {
        entity: BridgeEntityKind,
        id: String,
    },
    /// A diagnostic failure. The UI shows a generic per-category line; `detail`
    /// is the opaque Rust error chain for logs / a copyable disclosure, never
    /// translated.
    #[error("{category:?}: {detail}")]
    Diagnostic {
        category: BridgeErrorCategory,
        detail: String,
    },
}

impl BridgeError {
    pub(crate) fn diagnostic(
        category: BridgeErrorCategory,
        detail: impl std::fmt::Display,
    ) -> Self {
        BridgeError::Diagnostic {
            category,
            detail: detail.to_string(),
        }
    }
    pub(crate) fn database(detail: impl std::fmt::Display) -> Self {
        Self::diagnostic(BridgeErrorCategory::Database, detail)
    }

    /// A failed database read or write, carrying the fault alone.
    ///
    /// Both layers a database failure crosses on its way up prefix themselves
    /// onto it — `CovenError::Database` and `LibraryError::Database` each
    /// render as "database error: {inner}", and the `DbError::Message` inside
    /// does too — so what a person was shown read "database error: database
    /// error: …". Neither prefix belongs in the detail: the `Database`
    /// category beside it is what says which kind of failure this is, and each
    /// UI renders that as its own localized line.
    pub(crate) fn database_query(error: impl DatabaseFault) -> Self {
        Self::database(error.database_fault())
    }
    pub(crate) fn config(detail: impl std::fmt::Display) -> Self {
        Self::diagnostic(BridgeErrorCategory::Config, detail)
    }
    pub(crate) fn internal(detail: impl std::fmt::Display) -> Self {
        Self::diagnostic(BridgeErrorCategory::Internal, detail)
    }
    #[cfg(feature = "desktop")]
    pub(crate) fn import(detail: impl std::fmt::Display) -> Self {
        Self::diagnostic(BridgeErrorCategory::Import, detail)
    }
    /// Desktop-only with the output surface it reports on: iOS/Android compile
    /// out the output queue and the track saver entirely.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) fn export(detail: impl std::fmt::Display) -> Self {
        Self::diagnostic(BridgeErrorCategory::Export, detail)
    }
    /// Desktop-only, like [`Self::export`]: reports a save (rendered-output)
    /// failure.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) fn save(detail: impl std::fmt::Display) -> Self {
        Self::diagnostic(BridgeErrorCategory::Save, detail)
    }
}

/// Why playback couldn't start or continue. The cases where this device can't
/// reach the audio are user-actionable and keyed (`bridge_playback_error_reason_key`);
/// every in-core failure is un-enumerable and rides in `Diagnostic` — the UI
/// renders it through the same `BridgeError` path (generic per-category line +
/// copyable diagnostic detail).
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgePlaybackErrorReason {
    /// A remote cloud-only track isn't downloaded and sync is disconnected —
    /// the user reconnects to play it.
    SyncDisconnected,
    /// Another device attached the track and its upload hasn't reached
    /// storage yet — the user waits for it to finish.
    UploadPending,
    /// A local release imported on another device, which hasn't uploaded it.
    NotUploaded,
    /// A local release's original files are gone or changed on this device.
    OriginalMissing,
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
        BridgeErrorCategory::MetadataTrackCount { .. } => "core.import.error.metadata_track_count",
        BridgeErrorCategory::Export => "core.error.category.export",
        BridgeErrorCategory::Save => "core.error.category.save",
        BridgeErrorCategory::CloudSetup { failure } => {
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
        BridgeErrorCategory::DeviceIdentityMissing => "core.error.identity_missing",
        BridgeErrorCategory::Credentials => "core.error.category.credentials",
        BridgeErrorCategory::Network => "core.error.category.network",
        BridgeErrorCategory::Keyring => "core.error.category.keyring",
        BridgeErrorCategory::KeyringLocked => "core.error.keyring.locked",
        BridgeErrorCategory::KeychainEntitlementMissing => "core.error.keyring.entitlement_missing",
        BridgeErrorCategory::Membership => "core.error.category.membership",
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

/// The message a `DbError` carries, without its own "database error:" prefix.
///
/// Non-message failures retain their complete diagnostic chain.
fn db_fault(error: bae_core::db::DbError) -> String {
    match error {
        bae_core::db::DbError::Message(message) => message,
        other => other.to_string(),
    }
}

impl DatabaseFault for coven::CovenError {
    fn database_fault(self) -> String {
        match self {
            coven::CovenError::Database(inner) => inner.to_string(),
            other => other.to_string(),
        }
    }
}

impl DatabaseFault for bae_core::library::LibraryError {
    fn database_fault(self) -> String {
        match self {
            bae_core::library::LibraryError::Database(inner) => db_fault(inner),
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
        BridgeError::Diagnostic { category, .. } => Some(bridge_error_category_key(*category)),
    }
}

/// Localization key for the actionable `BridgePlaybackErrorReason` variants, or
/// `None` for `Diagnostic` (which the UI renders through the `BridgeError`
/// category path instead). One source of these keys for every platform.
#[uniffi::export]
pub fn bridge_playback_error_reason_key(reason: &BridgePlaybackErrorReason) -> Option<String> {
    match reason {
        BridgePlaybackErrorReason::SyncDisconnected => {
            Some("core.playback.error.sync_disconnected".to_string())
        }
        BridgePlaybackErrorReason::UploadPending => {
            Some("core.playback.error.upload_pending".to_string())
        }
        BridgePlaybackErrorReason::NotUploaded => {
            bridge_release_playability_key(BridgeReleasePlayability::NotUploaded)
        }
        BridgePlaybackErrorReason::OriginalMissing => {
            bridge_release_playability_key(BridgeReleasePlayability::OriginalMissing)
        }
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
// extra bridge-only arguments (`BridgeReleaseDetail`), and `LibraryError`
// crosses through a `From` impl.
//
// Record converters exhaustively destructure their core input(s) — no `..` in
// any struct or enum-variant pattern — so a new bae-core field fails the build
// here instead of silently never crossing the bridge. A dropped field is named
// explicitly (`field: _`), with a comment when the drop isn't obvious. Fields of
// external-crate types (coven's `StorageConfig`, …) are exempt — the
// bridge can't police a pinned crate's field list — and stay dotted reads.
// =========================================================================

mirror_enum! {
    /// The bridge enum carries variants core's has no counterpart for — an
    /// AirPlay receiver bae cannot drive, and a library whose replacement of
    /// this device's copy is unfinished, which the bootstrap reports — so
    /// only the outward direction is a mirror.
    BridgeErrorCategory = bae_core::ui::UiErrorCategory,
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
        MetadataTrackCount { folder_tracks, release_tracks },
        GroupingBlocked(reason: (BridgeGroupingBlock)),
        Export,
        Save,
        CloudSetup(failure: (BridgeStorageFailure)),
        StorageCheck {check:(BridgeStorageCheck),failure:(BridgeStorageFailure)},
        S3VersioningUnknown(failure:(BridgeStorageFailure)),
        S3VersioningUnsupported(state:(BridgeS3BucketVersioning)),
        StorageLocationChangeUnsupported,
        DeviceIdentityMissing,
        Credentials,
        Network,
        Keyring,
        KeyringLocked,
        KeychainEntitlementMissing,
        Membership,
    },
}

mirror_enum! {
    /// `Cancelled` and `NotFound` are the bridge's own: core reports a
    /// cancellation through the operation's result rather than as a `UiError`,
    /// and the only missing entity anyone is told about — the library the
    /// bootstrap could not open — is found before a `UiError` exists.
    BridgeError = bae_core::ui::UiError,
    from_core: pub(crate) fn,
    variants: {
        Diagnostic { category: (BridgeErrorCategory), detail },
    },
}

/// A failed config read or write. The `Config` category is what every one of
/// these reports, so `?` carries them.
impl From<bae_core::config::ConfigError> for BridgeError {
    fn from(error: bae_core::config::ConfigError) -> Self {
        BridgeError::config(error)
    }
}

/// An import failure crosses in the class core gave it.
#[cfg(feature = "desktop")]
impl From<bae_core::import::ImportError> for BridgeError {
    fn from(error: bae_core::import::ImportError) -> Self {
        BridgeError::from_core(error.ui_error())
    }
}

/// Carry a core `LibraryError`'s diagnostic class across the bridge: the class
/// (keyring vs cloud credentials vs network vs membership vs …) becomes the
/// `BridgeErrorCategory` the UI renders a localized line for; the error chain
/// rides along as opaque diagnostic detail.
impl From<bae_core::library::LibraryError> for BridgeError {
    fn from(error: bae_core::library::LibraryError) -> Self {
        if matches!(
            error,
            bae_core::library::LibraryError::CloudSetup(coven::StorageSetupError::OAuth(
                coven::OAuthError::Cancelled
            ))
        ) {
            return BridgeError::Cancelled;
        }
        BridgeError::Diagnostic {
            category: BridgeErrorCategory::from_core(error.category()),
            detail: error.to_string(),
        }
    }
}

impl From<bae_core::library::CreateLibraryError> for BridgeError {
    fn from(error: bae_core::library::CreateLibraryError) -> Self {
        BridgeError::Diagnostic {
            category: BridgeErrorCategory::from_core(error.category()),
            detail: error.to_string(),
        }
    }
}

mirror_enum! {
    BridgePlaybackErrorReason = bae_core::ui::PlaybackErrorReason,
    from_core: pub(crate) fn,
    variants: {
        SyncDisconnected,
        UploadPending,
        NotUploaded,
        OriginalMissing,
        Diagnostic { error: (BridgeError) },
    },
}

#[cfg(feature = "desktop")]
impl BridgeCoverSelection {
    pub(crate) fn into_core(self) -> bae_core::import::CoverSelection {
        match self {
            BridgeCoverSelection::ReleaseImage { file_id } => {
                bae_core::import::CoverSelection::Local(file_id)
            }
            BridgeCoverSelection::RemoteCover { selection } => {
                bae_core::import::CoverSelection::Remote(
                    selection.image.into_core(),
                    selection.source.into_core(),
                )
            }
            BridgeCoverSelection::EmbeddedCover { source_file_id } => {
                bae_core::import::CoverSelection::Embedded(source_file_id)
            }
        }
    }
}

impl BridgeCloudProvider {
    pub(crate) fn from_core(p: &bae_core::config::CloudProvider) -> Self {
        use bae_core::config::CloudProvider;
        match p {
            CloudProvider::S3 => BridgeCloudProvider::S3,
            CloudProvider::Dropbox => BridgeCloudProvider::Dropbox,
            CloudProvider::OneDrive => BridgeCloudProvider::OneDrive,
            CloudProvider::CloudKit => BridgeCloudProvider::CloudKit,
        }
    }

    #[cfg(feature = "oauth-providers")]
    pub(crate) fn into_core(self) -> bae_core::config::CloudProvider {
        use bae_core::config::CloudProvider;
        match self {
            BridgeCloudProvider::S3 => CloudProvider::S3,
            BridgeCloudProvider::Dropbox => CloudProvider::Dropbox,
            BridgeCloudProvider::OneDrive => CloudProvider::OneDrive,
            BridgeCloudProvider::CloudKit => CloudProvider::CloudKit,
        }
    }
}

mirror_enum! {
    BridgeStorageSortField = bae_core::db::StorageSortField,
    from_core: fn,
    into_core: fn,
    variants: { AlbumTitle, ArtistNames, Media, FileCount, TotalSize },
}

mirror_enum! {
    BridgeStorageSortDirection = bae_core::db::SortDirection,
    from_core: fn,
    into_core: fn,
    variants: { Ascending, Descending },
}

mirror_struct! {
    BridgeStorageSort = bae_core::db::StorageSortCriterion,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    fields: {
        field: (BridgeStorageSortField),
        direction: (BridgeStorageSortDirection),
    },
}

mirror_enum! {
    BridgeStorageFilter = bae_core::db::StorageFilter,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    variants: { All, Remote, Local, Uploading },
}

mirror_enum! {
    BridgeComposerSortField = bae_core::db::ComposerSortField,
    into_core: fn,
    variants: { Name, WorkCount, LinkedReleaseCount },
}

mirror_struct! {
    BridgeComposerSortCriterion = bae_core::db::ComposerSortCriterion,
    into_core: pub(crate) fn,
    fields: {
        field: (BridgeComposerSortField),
        direction: (BridgeSortDirection),
    },
}

mirror_enum! {
    BridgeArtistSortField = bae_core::db::ArtistSortField,
    into_core: fn,
    variants: { Name, AlbumCount },
}

mirror_struct! {
    BridgeArtistSortCriterion = bae_core::db::ArtistSortCriterion,
    into_core: pub(crate) fn,
    fields: {
        field: (BridgeArtistSortField),
        direction: (BridgeSortDirection),
    },
}

mirror_enum! {
    BridgeSortDirection = bae_core::db::SortDirection,
    into_core: pub(crate) fn,
    variants: { Ascending, Descending },
}

mirror_enum! {
    BridgeSortField = bae_core::db::AlbumSortField,
    into_core: fn,
    variants: { Title, Artist, Year, DateAdded },
}

mirror_struct! {
    BridgeSortCriterion = bae_core::db::AlbumSortCriterion,
    into_core: pub(crate) fn,
    fields: {
        field: (BridgeSortField),
        direction: (BridgeSortDirection),
    },
}

#[cfg(all(test, feature = "desktop"))]
#[path = "settings_tests.rs"]
mod tests;
