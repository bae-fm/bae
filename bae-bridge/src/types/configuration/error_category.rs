//! The category of a bridge error: its type, its catalog key, and how it
//! crosses from core's `UiErrorCategory`.

use super::super::*;

/// The kind of diagnostic failure. The UI shows one generic localized line per
/// category; `detail` is the underlying Rust error chain — logged and offered in
/// a copyable disclosure, never translated.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeErrorCategory {
    /// A database call failed, and made again unchanged it fails the same
    /// way.
    Database,
    /// The database was busy, or what the call read changed under it: made
    /// again, the same call can pass.
    DatabaseBusy,
    /// The library's database file is damaged, or holds a value nothing
    /// here writes: no call passes until this device's copy is replaced.
    DatabaseDamaged,
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
    /// The album an import files its release under was deleted after the
    /// import was prepared.
    AlbumGone,
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
    /// A change to the library acted on something that changed between its
    /// read and its write, and wrote nothing; asking again acts on what now
    /// stands.
    ChangedMeanwhile,
    /// What the person typed can't be saved; the line says what it needs.
    InvalidEdit {
        reason: BridgeValidationReason,
    },
    /// A folder to watch holds, or lies in, a watched folder; the detail
    /// names both.
    WatchedFolderOverlap,
    /// A save names its files by a file name pattern that can't make a name;
    /// nothing was written.
    FilenamePatternInvalid,
    /// A release save names its folder by a folder name pattern that can't
    /// make a name; nothing was written.
    FolderPatternInvalid,
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
    /// An AirPlay receiver can't be driven — it asks for a PIN or a password,
    /// or offers only audio encryption the sender doesn't implement.
    AirPlayUnsupported,
    /// An AirPlay receiver refused to pair, for a reason it gave or a reply
    /// that didn't check out.
    AirPlayPairingFailed,
    /// An AirPlay receiver saw too many failed pairing attempts and refuses
    /// more until it is reset.
    AirPlayLockedOut,
    /// An AirPlay receiver is pairing with another device.
    AirPlayBusy,
    /// An AirPlay receiver asks to wait this many seconds before another
    /// try; the line takes the wait, formatted for the locale.
    AirPlayBackOff {
        seconds: u64,
    },
    /// An AirPlay receiver isn't accepting new pairings.
    AirPlayNotPairing,
    /// An AirPlay receiver is paired with as many devices as it allows.
    AirPlayFull,
    /// An AirPlay receiver couldn't be reached: the connection failed, timed
    /// out, or dropped while pairing.
    AirPlayUnreachable,
    /// An AirPlay receiver plays only from people in its home.
    AirPlayHomeOnly,
    /// An AirPlay receiver plays only from its home's admins.
    AirPlayAdminsOnly,
    /// An AirPlay receiver plays only from the account it is set up with.
    AirPlayOwnerOnly,
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
    /// A restore was cancelled after this device registered with the
    /// library, so it can't be taken back: the restore stays unfinished on
    /// this device, and only finishing it goes on.
    RestoreRegistered,
    /// The person's keys were replaced on another of their devices, so a
    /// restore with an older restore code, or a reset of a copy that holds
    /// the replaced keys, can't add this device: only the current recovery
    /// code restores the library.
    MemberKeysReplaced,
    /// A storage setup ended undone with no call there to hear it, such as
    /// one bae stopped partway, or one sync's connection finished and the
    /// library refused: storage isn't set up, and setting it up again is the
    /// way on.
    StorageSetupUndone,
    /// A storage setup past its point of no return is still finishing, so
    /// another can't start until it has.
    StorageSetupFinishing,
    /// The library didn't open because this device is removing it; only
    /// finishing the removal ends it.
    DeletionUnfinished,
    /// Removing the library needs this device to sign in to its storage
    /// again first.
    DeletionSignInRequired,
    /// This device holds an unfinished join or restore of the library
    /// `library_id` that can't resume because what it saved can't be read;
    /// listing the unfinished joins and restores fails on it until the app
    /// calls `discard_unfinished_installation` with the id.
    UnfinishedInstallationUnreadable {
        library_id: String,
    },
}

/// Localization key for a `BridgeErrorCategory`'s generic user-facing line. The
/// UI resolves this through its string catalog; `detail` is never translated.
/// One source of these keys for every platform.
#[uniffi::export]
pub fn bridge_error_category_key(category: BridgeErrorCategory) -> String {
    match category {
        BridgeErrorCategory::Database
        | BridgeErrorCategory::DatabaseBusy
        | BridgeErrorCategory::DatabaseDamaged => "core.error.category.database",
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
        BridgeErrorCategory::AlbumGone => "core.import.error.album_gone",
        BridgeErrorCategory::MetadataTrackCount { .. } => "core.import.error.metadata_track_count",
        BridgeErrorCategory::TiedMediums { .. } => "core.import.error.tied_mediums",
        BridgeErrorCategory::ReleaseChangedSinceOpened => {
            "core.release_edit.error.changed_since_opened"
        }
        BridgeErrorCategory::ChangedMeanwhile => "core.error.changed_meanwhile",
        BridgeErrorCategory::InvalidEdit { reason } => reason.loc_key(),
        BridgeErrorCategory::WatchedFolderOverlap => "core.error.watched_folder_overlap",
        BridgeErrorCategory::FilenamePatternInvalid => "core.error.filename_pattern_invalid",
        BridgeErrorCategory::FolderPatternInvalid => "core.error.folder_pattern_invalid",
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
        BridgeErrorCategory::AirPlayPairingFailed => "core.error.category.airplay_pairing_failed",
        BridgeErrorCategory::AirPlayLockedOut => "core.error.category.airplay_locked_out",
        BridgeErrorCategory::AirPlayBusy => "core.error.category.airplay_busy",
        BridgeErrorCategory::AirPlayBackOff { .. } => "core.error.category.airplay_back_off",
        BridgeErrorCategory::AirPlayNotPairing => "core.error.category.airplay_not_pairing",
        BridgeErrorCategory::AirPlayFull => "core.error.category.airplay_full",
        BridgeErrorCategory::AirPlayUnreachable => "core.error.category.airplay_unreachable",
        BridgeErrorCategory::AirPlayHomeOnly => "core.error.category.airplay_home_only",
        BridgeErrorCategory::AirPlayAdminsOnly => "core.error.category.airplay_admins_only",
        BridgeErrorCategory::AirPlayOwnerOnly => "core.error.category.airplay_owner_only",
        BridgeErrorCategory::AirPlayUnreadableSettings => {
            "core.error.category.airplay_unreadable_settings"
        }
        BridgeErrorCategory::ReplacementUnfinished { by } => match by {
            BridgeReplacedBy::Reset => "core.error.replacement_unfinished.reset",
            BridgeReplacedBy::Restore => "core.error.replacement_unfinished.restore",
        },
        BridgeErrorCategory::RestoreRegistered => "core.error.restore_registered",
        BridgeErrorCategory::MemberKeysReplaced => "core.error.member_keys_replaced",
        BridgeErrorCategory::StorageSetupUndone => "core.error.storage_setup_undone",
        BridgeErrorCategory::StorageSetupFinishing => "core.error.storage_setup_finishing",
        BridgeErrorCategory::DeletionUnfinished => "core.error.deletion_unfinished",
        BridgeErrorCategory::DeletionSignInRequired => "core.error.deletion_sign_in_required",
        BridgeErrorCategory::UnfinishedInstallationUnreadable { .. } => {
            "core.error.unfinished_installation_unreadable"
        }
    }
    .to_string()
}

impl BridgeErrorCategory {
    /// Whether this category's detail says something its line doesn't: core's
    /// answer for the category's class.
    pub(super) fn detail_adds_to_line(&self) -> bool {
        self.clone().into_core().class().detail_adds_to_line()
    }
}

mirror_enum! {
    BridgeErrorCategory = bae_diagnostics::UiErrorCategory,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    variants: {
        Database,
        DatabaseBusy,
        DatabaseDamaged,
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
        AlbumGone,
        MetadataTrackCount { folder_tracks, release_tracks },
        TiedMediums { mediums },
        ReleaseChangedSinceOpened,
        ChangedMeanwhile,
        InvalidEdit(reason: (BridgeValidationReason)),
        WatchedFolderOverlap,
        FilenamePatternInvalid,
        FolderPatternInvalid,
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
        AirPlayUnsupported,
        AirPlayPairingFailed,
        AirPlayLockedOut,
        AirPlayBusy,
        AirPlayBackOff { seconds },
        AirPlayNotPairing,
        AirPlayFull,
        AirPlayUnreachable,
        AirPlayHomeOnly,
        AirPlayAdminsOnly,
        AirPlayOwnerOnly,
        AirPlayUnreadableSettings,
        ReplacementUnfinished(by: (BridgeReplacedBy)),
        RestoreRegistered,
        MemberKeysReplaced,
        StorageSetupUndone,
        StorageSetupFinishing,
        DeletionUnfinished,
        DeletionSignInRequired,
        UnfinishedInstallationUnreadable { library_id },
    },
}
