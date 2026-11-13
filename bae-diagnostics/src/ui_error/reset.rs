//! Whether resetting a library from the cloud replaces what a failure met:
//! decided where the class is known, so a surface offers *Reset from
//! cloud…* by core's answer, as it offers *Retry* by `retry_can_help`.

use super::UiErrorCategory;

impl UiErrorCategory {
    /// Whether the failure is in this device's copy of the library's data
    /// (the database refused the call, or its file is damaged), so replacing
    /// that copy from the cloud ends it. False for a failure that comes from
    /// the cloud, the network, the keychain, what was asked, or bae itself,
    /// which a new copy would meet again.
    #[deny(clippy::wildcard_enum_match_arm)]
    pub fn reset_from_cloud_replaces(&self) -> bool {
        match self {
            Self::Database | Self::DatabaseDamaged => true,
            Self::Storage(..)
            | Self::StorageCheck { .. }
            | Self::S3VersioningUnknown(..)
            | Self::GroupingBlocked(..)
            | Self::DatabaseBusy
            | Self::CandidateBeingIdentified
            | Self::StorageSetupFinishing
            | Self::ChangedMeanwhile
            | Self::Import
            | Self::Network
            | Self::KeyringLocked
            | Self::Credentials
            | Self::Membership
            | Self::Config
            | Self::Internal
            | Self::SyncUpdateRequired
            | Self::DeviceResetRequired
            | Self::DeviceRemoved
            | Self::ReplacementUnfinished(..)
            | Self::RestoreRegistered
            | Self::MemberKeysReplaced
            | Self::StorageSetupUndone
            | Self::DeletionUnfinished
            | Self::DeletionSignInRequired
            | Self::UnfinishedInstallationUnreadable { .. }
            | Self::StoreOwnerAccount
            | Self::InviteJoining
            | Self::ImportData
            | Self::CandidateImportInProgress
            | Self::FilesAlreadyImported
            | Self::AlbumGone
            | Self::MetadataTrackCount { .. }
            | Self::TiedMediums { .. }
            | Self::ReleaseChangedSinceOpened
            | Self::InvalidEdit(..)
            | Self::WatchedFolderOverlap
            | Self::FilenamePatternInvalid
            | Self::FolderPatternInvalid
            | Self::Export
            | Self::Save
            | Self::Renderer
            | Self::AirPlayUnsupported
            | Self::AirPlayUnreadableSettings
            | Self::AirPlayPairingFailed
            | Self::AirPlayLockedOut
            | Self::AirPlayBusy
            | Self::AirPlayBackOff { .. }
            | Self::AirPlayNotPairing
            | Self::AirPlayFull
            | Self::AirPlayUnreachable
            | Self::AirPlayHomeOnly
            | Self::AirPlayAdminsOnly
            | Self::AirPlayOwnerOnly
            | Self::S3VersioningUnsupported(..)
            | Self::StorageLocationChangeUnsupported
            | Self::Keyring
            | Self::KeychainEntitlementMissing
            | Self::SavedRestoreCodesFull
            | Self::SavedRestoreCodeTooLarge => false,
        }
    }
}

#[cfg(test)]
#[path = "reset_tests.rs"]
mod tests;
