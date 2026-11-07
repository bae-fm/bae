//! A failure's class apart from the reason or counts it carries, and the name
//! a stored failure goes by: one name per class, so every table that keeps a
//! failure spells it the same way.

use super::UiErrorCategory;

/// The class of a [`UiErrorCategory`], without what the category carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiErrorClass {
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
    MetadataTrackCount,
    TiedMediums,
    ReleaseChangedSinceOpened,
    ChangedMeanwhile,
    GroupingBlocked,
    Export,
    Save,
    Storage,
    StorageCheck,
    S3VersioningUnknown,
    S3VersioningUnsupported,
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
    ReplacementUnfinished,
    RestoreRegistered,
    MemberKeysReplaced,
    StorageSetupUndone,
    StorageSetupFinishing,
    DeletionUnfinished,
    DeletionSignInRequired,
}

impl UiErrorClass {
    /// Every class, each once.
    const ALL: [Self; 44] = [
        Self::Database,
        Self::Config,
        Self::Internal,
        Self::SyncUpdateRequired,
        Self::DeviceResetRequired,
        Self::DeviceRemoved,
        Self::Import,
        Self::ImportData,
        Self::CandidateImportInProgress,
        Self::CandidateBeingIdentified,
        Self::FilesAlreadyImported,
        Self::MetadataTrackCount,
        Self::TiedMediums,
        Self::ReleaseChangedSinceOpened,
        Self::ChangedMeanwhile,
        Self::GroupingBlocked,
        Self::Export,
        Self::Save,
        Self::Storage,
        Self::StorageCheck,
        Self::S3VersioningUnknown,
        Self::S3VersioningUnsupported,
        Self::StorageLocationChangeUnsupported,
        Self::Credentials,
        Self::Network,
        Self::Keyring,
        Self::KeyringLocked,
        Self::KeychainEntitlementMissing,
        Self::SavedRestoreCodesFull,
        Self::SavedRestoreCodeTooLarge,
        Self::Membership,
        Self::InviteJoining,
        Self::StoreOwnerAccount,
        Self::Renderer,
        Self::QueueChanged,
        Self::AirPlayUnsupported,
        Self::AirPlayUnreadableSettings,
        Self::ReplacementUnfinished,
        Self::RestoreRegistered,
        Self::MemberKeysReplaced,
        Self::StorageSetupUndone,
        Self::StorageSetupFinishing,
        Self::DeletionUnfinished,
        Self::DeletionSignInRequired,
    ];

    /// The name a stored failure of this class goes by.
    pub fn name(self) -> &'static str {
        match self {
            Self::Database => "database",
            Self::Config => "config",
            Self::Internal => "internal",
            Self::SyncUpdateRequired => "sync_update_required",
            Self::DeviceResetRequired => "device_reset_required",
            Self::DeviceRemoved => "device_removed",
            Self::Import => "import",
            Self::ImportData => "import_data",
            Self::CandidateImportInProgress => "candidate_import_in_progress",
            Self::CandidateBeingIdentified => "candidate_being_identified",
            Self::FilesAlreadyImported => "files_already_imported",
            Self::MetadataTrackCount => "metadata_track_count",
            Self::TiedMediums => "tied_mediums",
            Self::ReleaseChangedSinceOpened => "release_changed_since_opened",
            Self::ChangedMeanwhile => "changed_meanwhile",
            Self::GroupingBlocked => "grouping_blocked",
            Self::Export => "export",
            Self::Save => "save",
            Self::Storage => "storage",
            Self::StorageCheck => "storage_check",
            Self::S3VersioningUnknown => "versioning_unknown",
            Self::S3VersioningUnsupported => "versioning_unsupported",
            Self::StorageLocationChangeUnsupported => "storage_location_change",
            Self::Credentials => "credentials",
            Self::Network => "network",
            Self::Keyring => "keyring",
            Self::KeyringLocked => "keyring_locked",
            Self::KeychainEntitlementMissing => "keychain_entitlement_missing",
            Self::SavedRestoreCodesFull => "saved_restore_codes_full",
            Self::SavedRestoreCodeTooLarge => "saved_restore_code_too_large",
            Self::Membership => "membership",
            Self::InviteJoining => "invite_joining",
            Self::StoreOwnerAccount => "store_owner_account",
            Self::Renderer => "renderer",
            Self::QueueChanged => "queue_changed",
            Self::AirPlayUnsupported => "airplay_unsupported",
            Self::AirPlayUnreadableSettings => "airplay_unreadable_settings",
            Self::ReplacementUnfinished => "replacement_unfinished",
            Self::RestoreRegistered => "restore_registered",
            Self::MemberKeysReplaced => "member_keys_replaced",
            Self::StorageSetupUndone => "storage_setup_undone",
            Self::StorageSetupFinishing => "storage_setup_finishing",
            Self::DeletionUnfinished => "deletion_unfinished",
            Self::DeletionSignInRequired => "deletion_sign_in_required",
        }
    }

    /// The class `name` names, or `None` for a name no class goes by.
    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|class| class.name() == name)
    }

    /// Whether a failure of this class shows its detail under its line: true
    /// when the detail says something the line doesn't, false when the line
    /// already names the whole fault and the detail is that sentence again,
    /// in English. A surface that shows the class's line shows the detail
    /// only when this says so; one that words its own headline over the
    /// failure keeps the detail, since the line it repeats isn't shown.
    pub fn detail_adds_to_line(self) -> bool {
        match self {
            // A generic line over the fault the detail names: the query that
            // failed, the setting refused, the broken invariant, the source
            // that couldn't be read or written.
            Self::Database
            | Self::Config
            | Self::Internal
            | Self::Import
            | Self::ImportData
            | Self::Export
            | Self::Save
            | Self::Keyring
            | Self::Membership
            // The provider's or the network's own message: what the storage
            // answered, the check that failed and why, the sign-in's refusal.
            | Self::Storage
            | Self::StorageCheck
            | Self::S3VersioningUnknown
            | Self::Credentials
            | Self::Network
            // What the device playback was sent to said when it refused.
            | Self::Renderer
            // Which AirPlay limit a receiver hit, and the setting it
            // advertises that can't be read.
            | Self::AirPlayUnsupported
            | Self::AirPlayUnreadableSettings
            // The cause the line leaves out: why the device must be reset (a
            // damaged database, another copy using its id), why the keychain
            // refused this second, whether the invite's person is joining or
            // the invite ended.
            | Self::DeviceResetRequired
            | Self::KeyringLocked
            | Self::InviteJoining
            // What the line names without saying which: the folder a grouped
            // release can't be read from, the bucket's versioning state.
            | Self::GroupingBlocked
            | Self::S3VersioningUnsupported
            // Why a storage setup was undone: bae stopping, or the failure
            // or refusal that ended it.
            | Self::StorageSetupUndone => true,
            // The line names the whole fault, with the counts, discs or
            // folder it carries; the detail says it again, at most with an id
            // the person can't use.
            Self::SyncUpdateRequired
            | Self::DeviceRemoved
            | Self::CandidateImportInProgress
            | Self::CandidateBeingIdentified
            | Self::FilesAlreadyImported
            | Self::MetadataTrackCount
            | Self::TiedMediums
            | Self::ReleaseChangedSinceOpened
            | Self::ChangedMeanwhile
            | Self::StorageLocationChangeUnsupported
            | Self::KeychainEntitlementMissing
            | Self::SavedRestoreCodesFull
            | Self::SavedRestoreCodeTooLarge
            | Self::StoreOwnerAccount
            | Self::QueueChanged
            | Self::ReplacementUnfinished
            | Self::RestoreRegistered
            | Self::MemberKeysReplaced
            | Self::StorageSetupFinishing
            | Self::DeletionUnfinished
            | Self::DeletionSignInRequired => false,
        }
    }
}

impl UiErrorCategory {
    /// This category's class.
    pub fn class(&self) -> UiErrorClass {
        use UiErrorClass as K;
        match self {
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
            Self::ChangedMeanwhile => K::ChangedMeanwhile,
            Self::GroupingBlocked(_) => K::GroupingBlocked,
            Self::Export => K::Export,
            Self::Save => K::Save,
            Self::Storage(_) => K::Storage,
            Self::StorageCheck { .. } => K::StorageCheck,
            Self::S3VersioningUnknown(_) => K::S3VersioningUnknown,
            Self::S3VersioningUnsupported(_) => K::S3VersioningUnsupported,
            Self::StorageLocationChangeUnsupported => K::StorageLocationChangeUnsupported,
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
            Self::ReplacementUnfinished(_) => K::ReplacementUnfinished,
            Self::RestoreRegistered => K::RestoreRegistered,
            Self::MemberKeysReplaced => K::MemberKeysReplaced,
            Self::StorageSetupUndone => K::StorageSetupUndone,
            Self::StorageSetupFinishing => K::StorageSetupFinishing,
            Self::DeletionUnfinished => K::DeletionUnfinished,
            Self::DeletionSignInRequired => K::DeletionSignInRequired,
        }
    }

    /// The category of `class` when the class carries nothing more, or
    /// `None` for a class that carries a reason or counts.
    pub fn plain(class: UiErrorClass) -> Option<Self> {
        use UiErrorClass as K;
        Some(match class {
            K::Database => Self::Database,
            K::Config => Self::Config,
            K::Internal => Self::Internal,
            K::SyncUpdateRequired => Self::SyncUpdateRequired,
            K::DeviceResetRequired => Self::DeviceResetRequired,
            K::DeviceRemoved => Self::DeviceRemoved,
            K::Import => Self::Import,
            K::ImportData => Self::ImportData,
            K::CandidateImportInProgress => Self::CandidateImportInProgress,
            K::CandidateBeingIdentified => Self::CandidateBeingIdentified,
            K::FilesAlreadyImported => Self::FilesAlreadyImported,
            K::ReleaseChangedSinceOpened => Self::ReleaseChangedSinceOpened,
            K::ChangedMeanwhile => Self::ChangedMeanwhile,
            K::Export => Self::Export,
            K::Save => Self::Save,
            K::StorageLocationChangeUnsupported => Self::StorageLocationChangeUnsupported,
            K::Credentials => Self::Credentials,
            K::Network => Self::Network,
            K::Keyring => Self::Keyring,
            K::KeyringLocked => Self::KeyringLocked,
            K::KeychainEntitlementMissing => Self::KeychainEntitlementMissing,
            K::SavedRestoreCodesFull => Self::SavedRestoreCodesFull,
            K::SavedRestoreCodeTooLarge => Self::SavedRestoreCodeTooLarge,
            K::Membership => Self::Membership,
            K::InviteJoining => Self::InviteJoining,
            K::StoreOwnerAccount => Self::StoreOwnerAccount,
            K::Renderer => Self::Renderer,
            K::QueueChanged => Self::QueueChanged,
            K::AirPlayUnsupported => Self::AirPlayUnsupported,
            K::AirPlayUnreadableSettings => Self::AirPlayUnreadableSettings,
            K::RestoreRegistered => Self::RestoreRegistered,
            K::MemberKeysReplaced => Self::MemberKeysReplaced,
            K::StorageSetupUndone => Self::StorageSetupUndone,
            K::StorageSetupFinishing => Self::StorageSetupFinishing,
            K::DeletionUnfinished => Self::DeletionUnfinished,
            K::DeletionSignInRequired => Self::DeletionSignInRequired,
            K::MetadataTrackCount
            | K::TiedMediums
            | K::GroupingBlocked
            | K::Storage
            | K::StorageCheck
            | K::S3VersioningUnknown
            | K::S3VersioningUnsupported
            | K::ReplacementUnfinished => return None,
        })
    }
}

#[cfg(test)]
#[path = "class_tests.rs"]
mod tests;
