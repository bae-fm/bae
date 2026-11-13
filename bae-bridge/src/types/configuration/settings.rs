use super::super::*;

#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeSortField {
    Title,
    Artist,
    Year,
    DateAdded,
}

#[derive(Debug, Clone, Copy, uniffi::Enum)]
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
    SameAsSource,
    Bits16,
}

/// A value a file name pattern fills in from the track being saved. Mirror of
/// bae-preferences' `SaveFilenameValue`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeSaveFilenameValue {
    Title,
    Artist,
    Album,
    /// The year the album first came out.
    AlbumYear,
    /// The year this release came out, else the album's.
    ReleaseYear,
    TrackNumber,
    DiscNumber,
    TrackTotal,
}

/// A value the folder name pattern of a release save fills in from the
/// release. Mirror of bae-preferences' `SaveFolderValue`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeSaveFolderValue {
    Artist,
    Album,
    /// The year the album first came out.
    AlbumYear,
    /// The year this release came out, else the album's.
    ReleaseYear,
    Label,
    CatalogNumber,
}

/// One piece of the file name pattern: a value the save fills in, or text the
/// person typed, kept as typed. Mirror of bae-preferences' `NamePiece`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeSaveFilenamePiece {
    Value { value: BridgeSaveFilenameValue },
    Text { text: String },
}

/// One piece of the folder name pattern, like [`BridgeSaveFilenamePiece`].
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeSaveFolderPiece {
    Value { value: BridgeSaveFolderValue },
    Text { text: String },
}

/// Why a name pattern can't make a name, as core's validation answers.
/// Mirror of bae-preferences' `NamePatternError`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeNamePatternError {
    /// The pattern has no pieces.
    Empty,
    /// The first piece is typed text starting with ".".
    StartsWithDot,
    /// Typed text holds this character, which a name can't have.
    ForbiddenCharacter { character: String },
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

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeSavePreset {
    pub id: String,
    pub name: String,
    pub codec: BridgeSaveCodec,
}

/// The cover and loudness settings every save format shares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct BridgeSaveExtras {
    /// Whether each saved file embeds the release's cover art.
    pub embed_cover: bool,
    /// Whether a release save writes the cover as `cover.jpg` in its folder.
    pub cover_file: bool,
    /// Whether each saved file carries ReplayGain tags.
    pub loudness_tags: bool,
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
    into_core: pub(crate) fn,
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
    /// The runtime the call ran on is shutting down and dropped it before it
    /// finished: the process is ending. Nothing to show, and nothing a retry
    /// reaches; a reader of a stream ends on it.
    #[error("runtime shutting down")]
    ShuttingDown,
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
    /// A failed database read or write, carrying the fault alone.
    ///
    /// The layers a database failure crosses on its way up prefix themselves
    /// onto it — `CovenError::Database` and `LibraryError::Database` each
    /// render as "database error: {inner}". No prefix belongs in the detail:
    /// the `Database` category beside it is what says which kind of failure
    /// this is, and each UI renders that as its own localized line. A bare
    /// `DbError` crosses as its own text, through `From<DbError>`. A failure
    /// holding a bug, which its owner reported as it arrived, is shown as
    /// the bug it is, and any other as what the database met: busy,
    /// damaged or refused.
    pub(crate) fn database_query(
        error: impl DatabaseFault + bae_util::database_failure::FailedDatabaseCall,
    ) -> Self {
        let category = bae_diagnostics::database_category(&error);
        Self::categorized(category, error.database_fault())
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
        match reported {
            bae_util::task_failure::TaskFailed::Shutdown(_) => Self::ShuttingDown,
            panicked @ bae_util::task_failure::TaskFailed::Panicked(_) => Self::internal(panicked),
        }
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

impl DatabaseFault for bae_store::DbError {
    fn database_fault(self) -> String {
        match self {
            bae_store::DbError::Coven(error) => match *error {
                coven::CovenError::Database(inner) => inner.to_string(),
                other => other.to_string(),
            },
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
        BridgeError::Cancelled | BridgeError::ShuttingDown => None,
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
        BridgeError::Cancelled | BridgeError::ShuttingDown | BridgeError::NotFound { .. } => None,
        BridgeError::Diagnostic { category, detail } => {
            category.detail_adds_to_line().then(|| detail.clone())
        }
    }
}

/// Whether making the same call again can get past `error`: core's answer
/// for its category ([`bae_diagnostics::UiErrorCategory::retry_can_help`]),
/// so an app offers *Retry* only then. A cancellation has nothing to get
/// past, and a library that isn't there doesn't appear by asking again.
#[uniffi::export]
pub fn bridge_error_retry_can_help(error: &BridgeError) -> bool {
    match error {
        BridgeError::Cancelled | BridgeError::ShuttingDown | BridgeError::NotFound { .. } => false,
        BridgeError::Diagnostic { category, .. } => category.clone().into_core().retry_can_help(),
    }
}

/// Whether resetting the library from the cloud replaces what `error` met:
/// core's answer for its category
/// ([`bae_diagnostics::UiErrorCategory::reset_from_cloud_replaces`]), set where
/// the failure is in this device's copy of the library's data. An app offers
/// *Reset from cloud…* beside a failed open only then.
#[uniffi::export]
pub fn bridge_error_reset_from_cloud_replaces(error: &BridgeError) -> bool {
    match error {
        BridgeError::Cancelled | BridgeError::ShuttingDown | BridgeError::NotFound { .. } => false,
        BridgeError::Diagnostic { category, .. } => {
            category.clone().into_core().reset_from_cloud_replaces()
        }
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

/// A language tag the names cannot be ordered in is a configuration the
/// caller built wrongly; a failed write crosses as the database's failure.
impl From<bae_views::SortLanguageError> for BridgeError {
    fn from(error: bae_views::SortLanguageError) -> Self {
        match error {
            bae_views::SortLanguageError::Invalid(error) => {
                BridgeError::diagnostic(BridgeErrorCategory::Config, error)
            }
            bae_views::SortLanguageError::Database(error) => error.into(),
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

/// An import read's end is the caller's cancellation; its failed read is the
/// import's failure.
#[cfg(desktop)]
impl From<bae_store::live_query::LiveQueryError<bae_import::ImportError>> for BridgeError {
    fn from(error: bae_store::live_query::LiveQueryError<bae_import::ImportError>) -> Self {
        match error {
            bae_store::live_query::LiveQueryError::Cancelled => Self::Cancelled,
            bae_store::live_query::LiveQueryError::Query(error) => error.into(),
        }
    }
}

/// Carry a core `LibraryError`'s diagnostic class across the bridge: the class
/// (keyring vs cloud credentials vs network vs membership vs …) becomes the
/// `BridgeErrorCategory` the UI renders a localized line for; the error chain
/// rides along as opaque diagnostic detail.
impl From<bae_library::LibraryError> for BridgeError {
    fn from(error: bae_library::LibraryError) -> Self {
        // A sign-in the person dismissed, or a setup they cancelled before
        // its point of no return, which coven undid.
        if matches!(
            error,
            bae_library::LibraryError::StorageSetup(
                coven::StorageSetupError::OAuth(coven::OAuthError::Cancelled)
                    | coven::StorageSetupError::Cancelled
            )
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

mirror_struct! {
    BridgeStorageSort = bae_views::StorageSortCriterion,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    fields: {
        field: (BridgeStorageSortField),
        direction: (BridgeSortDirection),
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
    from_core: fn,
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
