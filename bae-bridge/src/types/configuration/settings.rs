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

/// The storage state the user picks for an import — Local (keep the files in
/// place) or Remote (upload to the cloud). Mirrors
/// `bae_core::import::StorageMode`. Whether a remote import is kept offline is the
/// ORTHOGONAL `pin` argument on `start_import`, never folded into this enum.
#[cfg(feature = "desktop")]
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeStorageMode {
    Local,
    Remote,
}

/// Decoded restore code info for UI preview (before the actual restore).
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeRestoreCodeInfo {
    pub library_id: String,
    pub library_name: String,
    pub cloud_provider: BridgeCloudProvider,
    pub needs_oauth: bool,
}

/// A device in the library's membership chain.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeMember {
    /// Hex-encoded Ed25519 public key — the device's stable identity.
    pub pubkey: String,
    pub role: BridgeMemberRole,
    /// True for the device this app is running on.
    pub is_self: bool,
    /// Short display identity — the first 8 characters of the pubkey.
    pub fingerprint: String,
    /// Whether the running device may remove this one (owner-only, never self).
    pub can_remove: bool,
}

/// The library's membership: its devices and whether the running device is an
/// owner (the gate the UI uses to show inviting and removal controls).
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeMembership {
    pub members: Vec<BridgeMember>,
    pub self_is_owner: bool,
}

/// A member's role. Mirrors coven's `MemberRole`. bae adds devices as `Member`;
/// the founding device is `Owner`. `Follower` (read-only) exists in coven's model
/// but bae does not create it — it is mapped through so the conversion stays total.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Enum)]
pub enum BridgeMemberRole {
    Owner,
    Member,
    Follower,
}

/// The existing device's pairing offer, decoded before provider authorization.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeDevicePairingOffer {
    pub library_name: String,
    pub cloud_provider: BridgeCloudProvider,
    pub needs_oauth: bool,
    pub expires_at_unix_seconds: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeDevicePairingPhase {
    AwaitingInvitation,
    ProviderAccessPending,
    LibraryInstallationPending,
}

/// A pairing attempt retained on this device and available to resume after an
/// app restart.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgePendingDevicePairingJoin {
    pub pairing_code: String,
    pub offer: BridgeDevicePairingOffer,
    pub fingerprint: String,
    pub phase: BridgeDevicePairingPhase,
}

/// The exact joining identity waiting for approval on the existing device.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgePairingDevice {
    pub fingerprint: String,
    pub email: Option<String>,
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

/// A cloud-home setup failure mirrored across UniFFI. Coven's original enum is
/// used inside bae-core; this mirror exists only for the language boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeCloudHomeSetupFailure {
    Authentication,
    PermissionDenied,
    ContainerNotFound,
    RegionMismatch,
    QuotaExceeded,
    InvalidConfiguration,
    LocationOccupied,
    Network,
    DeviceIdentityMissing,
    SecureStorage,
    Internal,
}

/// How a device-pairing join ended without joining. Each one is a different
/// thing for the user to do next, so each carries its own line rather than
/// sharing the membership category's generic one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeDeviceJoinFailure {
    /// The code's deadline passed. A retry needs a fresh code.
    Expired,
    /// The inviting device never took its next step — both sides have to be
    /// running the join at once.
    OwnerOffline,
    /// The inviting device ended the attempt.
    OwnerEnded,
}

/// The kind of diagnostic failure. The UI shows one generic localized line per
/// category; `detail` is the underlying Rust error chain — logged and offered in
/// a copyable disclosure, never translated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeErrorCategory {
    Database,
    Config,
    Internal,
    Import,
    Export,
    Save,
    CloudSetup {
        failure: BridgeCloudHomeSetupFailure,
    },
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
    /// A library-sharing membership operation failed (the membership chain, an
    /// invite, or cross-device key rotation).
    Membership,
    /// A device-pairing join ended without joining, for a reason the user can
    /// act on. Carries which end it was so the line can say what to do next.
    DeviceJoin {
        failure: BridgeDeviceJoinFailure,
    },
    /// An AirPlay receiver can't be driven — it demands a PIN, or offers only
    /// audio encryption the sender doesn't implement.
    AirPlayUnsupported,
}

/// What a `NotFound` was looking for, so the UI can localize "… not found".
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeEntityKind {
    Library,
    Album,
    Release,
    Track,
    File,
}

#[derive(Debug, Clone, thiserror::Error, uniffi::Error)]
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

/// Why playback couldn't start or continue. The two cloud-only "not playable
/// yet" cases are user-actionable and keyed (`bridge_playback_error_reason_key`);
/// every in-core failure is un-enumerable and rides in `Diagnostic` — the UI
/// renders it through the same `BridgeError` path (generic per-category line +
/// copyable, log-only detail).
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgePlaybackErrorReason {
    /// A remote cloud-only track isn't downloaded and sync is disconnected —
    /// the user reconnects to play it.
    SyncDisconnected,
    /// A remote track's cloud upload is still queued — the user waits for it
    /// to finish.
    UploadPending,
    /// Any other failure. Carries the underlying `BridgeError`; the UI renders
    /// its generic per-category line plus the opaque, log-only detail.
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
        BridgeErrorCategory::Import => "core.error.category.import",
        BridgeErrorCategory::Export => "core.error.category.export",
        BridgeErrorCategory::Save => "core.error.category.save",
        BridgeErrorCategory::CloudSetup { failure } => match failure {
            BridgeCloudHomeSetupFailure::Authentication => "core.error.category.credentials",
            BridgeCloudHomeSetupFailure::PermissionDenied => {
                "core.error.cloud_setup.permission_denied"
            }
            BridgeCloudHomeSetupFailure::ContainerNotFound => {
                "core.error.cloud_setup.container_not_found"
            }
            BridgeCloudHomeSetupFailure::RegionMismatch => "core.error.cloud_setup.region_mismatch",
            BridgeCloudHomeSetupFailure::QuotaExceeded => "core.error.cloud_setup.quota_exceeded",
            BridgeCloudHomeSetupFailure::InvalidConfiguration => {
                "core.error.cloud_setup.invalid_configuration"
            }
            BridgeCloudHomeSetupFailure::LocationOccupied => {
                "core.error.cloud_setup.location_occupied"
            }
            BridgeCloudHomeSetupFailure::Network => "core.error.category.network",
            BridgeCloudHomeSetupFailure::DeviceIdentityMissing => "core.error.identity_missing",
            BridgeCloudHomeSetupFailure::SecureStorage => "core.error.category.keyring",
            BridgeCloudHomeSetupFailure::Internal => "core.error.category.internal",
        },
        BridgeErrorCategory::DeviceIdentityMissing => "core.error.identity_missing",
        BridgeErrorCategory::Credentials => "core.error.category.credentials",
        BridgeErrorCategory::Network => "core.error.category.network",
        BridgeErrorCategory::Keyring => "core.error.category.keyring",
        BridgeErrorCategory::KeyringLocked => "core.error.keyring.locked",
        BridgeErrorCategory::Membership => "core.error.category.membership",
        BridgeErrorCategory::DeviceJoin { failure } => match failure {
            BridgeDeviceJoinFailure::Expired => "core.error.join.expired",
            BridgeDeviceJoinFailure::OwnerOffline => "core.error.join.owner_offline",
            BridgeDeviceJoinFailure::OwnerEnded => "core.error.join.owner_ended",
        },
        BridgeErrorCategory::AirPlayUnsupported => "core.error.category.airplay_unsupported",
    }
    .to_string()
}

/// Localization key for a `BridgeEntityKind`'s "… not found" line. One source
/// of these keys for every platform.
#[uniffi::export]
pub fn bridge_entity_not_found_key(entity: BridgeEntityKind) -> String {
    match entity {
        BridgeEntityKind::Library => "core.error.not_found.library",
        BridgeEntityKind::Album => "core.error.not_found.album",
        BridgeEntityKind::Release => "core.error.not_found.release",
        BridgeEntityKind::Track => "core.error.not_found.track",
        BridgeEntityKind::File => "core.error.not_found.file",
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
/// A catch-all arm rather than an exhaustive match on purpose: `DbError` is
/// coven's, and a variant added there must not stop this from rendering.
fn db_fault(error: coven::DbError) -> String {
    match error {
        coven::DbError::Message(message) => message,
        other => other.to_string(),
    }
}

impl DatabaseFault for coven::CovenError {
    fn database_fault(self) -> String {
        match self {
            coven::CovenError::Database(inner) => db_fault(*inner),
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
        BridgePlaybackErrorReason::Diagnostic { .. } => None,
    }
}

#[cfg(feature = "cloudkit")]
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum CloudKitError {
    #[error("not found: {msg}")]
    NotFound { msg: String },
    #[error("CloudKit error: {msg}")]
    Storage { msg: String },
}

// =========================================================================
// Core <-> Bridge conversions
//
// Convention, crate-wide — conversions also sit above this banner, in
// `handle.rs`, and in `bridge_utils.rs`: a conversion is an associated function
// on the `Bridge*` type, `BridgeX::from_core(core) -> Self` and
// `BridgeX::into_core(self) -> core::X`. Two exceptions: some `from_core`s take
// extra bridge-only arguments (`BridgeReleaseDetail`,
// `BridgeCandidateSearchResults`), and `LibraryError` crosses through a `From`
// impl.
//
// Record converters exhaustively destructure their core input(s) — no `..` in
// any struct or enum-variant pattern — so a new bae-core field fails the build
// here instead of silently never crossing the bridge. A dropped field is named
// explicitly (`field: _`), with a comment when the drop isn't obvious. Fields of
// external-crate types (coven's `Config`/`CloudHomeConfig`, …) are exempt — the
// bridge can't police a pinned crate's field list — and stay dotted reads.
// =========================================================================

impl BridgeErrorCategory {
    pub(crate) fn from_core(category: bae_core::ui::UiErrorCategory) -> Self {
        use bae_core::ui::UiErrorCategory;
        match category {
            UiErrorCategory::Database => BridgeErrorCategory::Database,
            UiErrorCategory::Config => BridgeErrorCategory::Config,
            UiErrorCategory::Internal => BridgeErrorCategory::Internal,
            UiErrorCategory::Import => BridgeErrorCategory::Import,
            UiErrorCategory::Export => BridgeErrorCategory::Export,
            UiErrorCategory::Save => BridgeErrorCategory::Save,
            UiErrorCategory::CloudSetup(failure) => BridgeErrorCategory::CloudSetup {
                failure: BridgeCloudHomeSetupFailure::from_core(failure),
            },
            UiErrorCategory::DeviceIdentityMissing => BridgeErrorCategory::DeviceIdentityMissing,
            UiErrorCategory::Credentials => BridgeErrorCategory::Credentials,
            UiErrorCategory::Network => BridgeErrorCategory::Network,
            UiErrorCategory::Keyring => BridgeErrorCategory::Keyring,
            UiErrorCategory::KeyringLocked => BridgeErrorCategory::KeyringLocked,
            UiErrorCategory::Membership => BridgeErrorCategory::Membership,
        }
    }
}

impl BridgeCloudHomeSetupFailure {
    fn from_core(failure: coven::CloudHomeSetupFailure) -> Self {
        use coven::CloudHomeSetupFailure;
        match failure {
            CloudHomeSetupFailure::Authentication => Self::Authentication,
            CloudHomeSetupFailure::PermissionDenied => Self::PermissionDenied,
            CloudHomeSetupFailure::ContainerNotFound => Self::ContainerNotFound,
            CloudHomeSetupFailure::RegionMismatch => Self::RegionMismatch,
            CloudHomeSetupFailure::QuotaExceeded => Self::QuotaExceeded,
            CloudHomeSetupFailure::InvalidConfiguration => Self::InvalidConfiguration,
            CloudHomeSetupFailure::LocationOccupied => Self::LocationOccupied,
            CloudHomeSetupFailure::Network => Self::Network,
            CloudHomeSetupFailure::DeviceIdentityMissing => Self::DeviceIdentityMissing,
            CloudHomeSetupFailure::SecureStorage => Self::SecureStorage,
            CloudHomeSetupFailure::Internal => Self::Internal,
        }
    }
}

impl BridgeEntityKind {
    fn from_core(entity: bae_core::ui::UiEntityKind) -> Self {
        use bae_core::ui::UiEntityKind;
        match entity {
            UiEntityKind::Library => BridgeEntityKind::Library,
            UiEntityKind::Album => BridgeEntityKind::Album,
            UiEntityKind::Release => BridgeEntityKind::Release,
            UiEntityKind::Track => BridgeEntityKind::Track,
            UiEntityKind::File => BridgeEntityKind::File,
        }
    }
}

impl BridgeError {
    pub(crate) fn from_core(error: bae_core::ui::UiError) -> Self {
        use bae_core::ui::UiError;
        match error {
            UiError::NotFound { entity, id } => BridgeError::NotFound {
                entity: BridgeEntityKind::from_core(entity),
                id,
            },
            UiError::Diagnostic { category, detail } => BridgeError::Diagnostic {
                category: BridgeErrorCategory::from_core(category),
                detail,
            },
        }
    }
}

/// Carry a core `LibraryError`'s diagnostic class across the bridge: the class
/// (keyring vs cloud credentials vs network vs membership vs …) becomes the
/// `BridgeErrorCategory` the UI renders a localized line for; the error chain
/// rides along as opaque, log-only detail.
impl From<bae_core::library::LibraryError> for BridgeError {
    fn from(error: bae_core::library::LibraryError) -> Self {
        if matches!(
            &error,
            bae_core::library::LibraryError::DevicePairingApproval(inner)
                if matches!(**inner, coven::ApproveDevicePairingError::Cancelled)
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

impl BridgePlaybackErrorReason {
    pub(crate) fn from_core(reason: bae_core::ui::PlaybackErrorReason) -> Self {
        use bae_core::ui::PlaybackErrorReason;
        match reason {
            PlaybackErrorReason::SyncDisconnected => BridgePlaybackErrorReason::SyncDisconnected,
            PlaybackErrorReason::UploadPending => BridgePlaybackErrorReason::UploadPending,
            PlaybackErrorReason::Diagnostic { error } => BridgePlaybackErrorReason::Diagnostic {
                error: BridgeError::from_core(error),
            },
        }
    }
}

/// Map the UI's storage-state choice to the core's `StorageMode`. Pinned-ness is
/// orthogonal — the caller passes the import's `pin` choice separately.
#[cfg(feature = "desktop")]
impl BridgeStorageMode {
    pub(crate) fn into_core(self) -> bae_core::import::StorageMode {
        use bae_core::import::StorageMode;
        match self {
            BridgeStorageMode::Local => StorageMode::Local,
            BridgeStorageMode::Remote => StorageMode::Remote,
        }
    }
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
                    selection.url,
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
            CloudProvider::GoogleDrive => BridgeCloudProvider::GoogleDrive,
            CloudProvider::Dropbox => BridgeCloudProvider::Dropbox,
            CloudProvider::OneDrive => BridgeCloudProvider::OneDrive,
            CloudProvider::CloudKit => BridgeCloudProvider::CloudKit,
        }
    }
}

impl BridgeMemberRole {
    pub(crate) fn from_core(role: bae_core::sync::membership::MemberRole) -> Self {
        use bae_core::sync::membership::MemberRole;
        match role {
            MemberRole::Owner => BridgeMemberRole::Owner,
            MemberRole::Member => BridgeMemberRole::Member,
            MemberRole::Follower => BridgeMemberRole::Follower,
        }
    }
}

impl BridgeMember {
    fn from_core(m: bae_core::sync::membership::MembershipMember) -> Self {
        let bae_core::sync::membership::MembershipMember {
            pubkey,
            role,
            is_self,
            fingerprint,
            can_remove,
        } = m;
        BridgeMember {
            pubkey,
            role: BridgeMemberRole::from_core(role),
            is_self,
            fingerprint,
            can_remove,
        }
    }
}

impl BridgeMembership {
    pub(crate) fn from_core(membership: bae_core::sync::membership::Membership) -> Self {
        let bae_core::sync::membership::Membership {
            members,
            self_is_owner,
        } = membership;
        BridgeMembership {
            members: members.into_iter().map(BridgeMember::from_core).collect(),
            self_is_owner,
        }
    }
}

/// Only the OAuth sign-in and authorize flows map a bare bridge provider back to
/// core; gated so non-OAuth builds don't carry a dead mapping.
#[cfg(feature = "oauth-providers")]
impl BridgeCloudProvider {
    pub(crate) fn into_core(self) -> bae_core::config::CloudProvider {
        use bae_core::config::CloudProvider;
        match self {
            BridgeCloudProvider::S3 => CloudProvider::S3,
            BridgeCloudProvider::GoogleDrive => CloudProvider::GoogleDrive,
            BridgeCloudProvider::Dropbox => CloudProvider::Dropbox,
            BridgeCloudProvider::OneDrive => CloudProvider::OneDrive,
            BridgeCloudProvider::CloudKit => CloudProvider::CloudKit,
        }
    }
}

impl BridgeHomeStorage {
    pub(crate) fn into_core(self) -> bae_core::config::HomeStorage {
        use bae_core::config::HomeStorage;
        match self {
            BridgeHomeStorage::Opaque => HomeStorage::Opaque,
            BridgeHomeStorage::Browsable => HomeStorage::Browsable,
        }
    }
}

impl BridgeStorageSort {
    pub(crate) fn into_core(self) -> bae_core::db::StorageSortCriterion {
        let BridgeStorageSort { field, direction } = self;
        bae_core::db::StorageSortCriterion {
            field: match field {
                BridgeStorageSortField::AlbumTitle => bae_core::db::StorageSortField::AlbumTitle,
                BridgeStorageSortField::ArtistNames => bae_core::db::StorageSortField::ArtistNames,
                BridgeStorageSortField::Media => bae_core::db::StorageSortField::Media,
                BridgeStorageSortField::FileCount => bae_core::db::StorageSortField::FileCount,
                BridgeStorageSortField::TotalSize => bae_core::db::StorageSortField::TotalSize,
            },
            direction: match direction {
                BridgeStorageSortDirection::Ascending => bae_core::db::SortDirection::Ascending,
                BridgeStorageSortDirection::Descending => bae_core::db::SortDirection::Descending,
            },
        }
    }
}

impl BridgeStorageFilter {
    pub(crate) fn into_core(self) -> bae_core::db::StorageFilter {
        match self {
            BridgeStorageFilter::All => bae_core::db::StorageFilter::All,
            BridgeStorageFilter::Remote => bae_core::db::StorageFilter::Remote,
            BridgeStorageFilter::Local => bae_core::db::StorageFilter::Local,
            BridgeStorageFilter::Uploading => bae_core::db::StorageFilter::Uploading,
        }
    }
}

impl BridgeComposerSortCriterion {
    pub(crate) fn into_core(self) -> bae_core::db::ComposerSortCriterion {
        let BridgeComposerSortCriterion { field, direction } = self;
        bae_core::db::ComposerSortCriterion {
            field: match field {
                BridgeComposerSortField::Name => bae_core::db::ComposerSortField::Name,
                BridgeComposerSortField::WorkCount => bae_core::db::ComposerSortField::WorkCount,
                BridgeComposerSortField::LinkedReleaseCount => {
                    bae_core::db::ComposerSortField::LinkedReleaseCount
                }
            },
            direction: direction.into_core(),
        }
    }
}

impl BridgeArtistSortCriterion {
    pub(crate) fn into_core(self) -> bae_core::db::ArtistSortCriterion {
        let BridgeArtistSortCriterion { field, direction } = self;
        bae_core::db::ArtistSortCriterion {
            field: match field {
                BridgeArtistSortField::Name => bae_core::db::ArtistSortField::Name,
                BridgeArtistSortField::AlbumCount => bae_core::db::ArtistSortField::AlbumCount,
            },
            direction: direction.into_core(),
        }
    }
}

impl BridgeSortDirection {
    pub(crate) fn into_core(self) -> bae_core::db::SortDirection {
        match self {
            BridgeSortDirection::Ascending => bae_core::db::SortDirection::Ascending,
            BridgeSortDirection::Descending => bae_core::db::SortDirection::Descending,
        }
    }
}

impl BridgeSortCriterion {
    pub(crate) fn into_core(self) -> bae_core::db::AlbumSortCriterion {
        let BridgeSortCriterion { field, direction } = self;
        bae_core::db::AlbumSortCriterion {
            field: match field {
                BridgeSortField::Title => bae_core::db::AlbumSortField::Title,
                BridgeSortField::Artist => bae_core::db::AlbumSortField::Artist,
                BridgeSortField::Year => bae_core::db::AlbumSortField::Year,
                BridgeSortField::DateAdded => bae_core::db::AlbumSortField::DateAdded,
            },
            direction: direction.into_core(),
        }
    }
}
