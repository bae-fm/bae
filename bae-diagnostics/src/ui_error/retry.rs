//! Whether making the same call again can get past a failure: the one
//! answer every surface offers *Retry* by, decided where the class is known.

use super::{GroupingBlockReason, UiErrorCategory};

impl UiErrorCategory {
    /// Whether the same call, made again with nothing else changed in bae,
    /// can succeed: its cause can end by itself (a busy database, the network,
    /// work still under way) or outside bae (an unlocked keychain, space at
    /// the provider). False when the cause is in what was asked (a file that
    /// doesn't parse, an edit to fix), in this device's copy (damage, a
    /// removal, a reset it needs), in bae itself (a bug), or calls for another
    /// call (signing in with another account, finishing a reset).
    #[deny(clippy::wildcard_enum_match_arm)]
    pub fn retry_can_help(&self) -> bool {
        match self {
            Self::Storage(failure)
            | Self::StorageCheck { failure, .. }
            | Self::S3VersioningUnknown(failure) => storage_retry_can_help(*failure),
            Self::GroupingBlocked(reason) => match reason {
                // The download finishes by itself.
                GroupingBlockReason::FolderFilesDownloading => true,
                // The person combines the folders again or separates a
                // release first.
                GroupingBlockReason::SourceChanged
                | GroupingBlockReason::SourceGone
                | GroupingBlockReason::FolderFilesTaken
                | GroupingBlockReason::FolderFilesContested => false,
            },
            // Busy or contended, or work under way that ends by itself.
            Self::DatabaseBusy
            | Self::CandidateBeingIdentified
            | Self::StorageSetupFinishing
            // What the call acted on changed; asking again acts on what
            // stands now.
            | Self::ChangedMeanwhile
            // A catalog or source that couldn't answer, or what an import was
            // prepared against changing before its write.
            | Self::Import
            // The network, the keychain locked this second, a sign-in the
            // provider timed out or the person declined, a membership step
            // waiting on a key or a reload.
            | Self::Network
            | Self::KeyringLocked
            | Self::Credentials
            | Self::Membership => true,
            // Refused, damaged, or a bug: the same call fails the same way.
            Self::Database
            | Self::DatabaseDamaged
            | Self::Config
            | Self::Internal
            // Only another call answers these: updating the app, resetting
            // or removing this device, finishing a reset, a restore or a
            // removal, signing in again first, setting storage up again.
            | Self::SyncUpdateRequired
            | Self::DeviceResetRequired
            | Self::DeviceRemoved
            | Self::ReplacementUnfinished(_)
            | Self::RestoreRegistered
            | Self::MemberKeysReplaced
            | Self::StorageSetupUndone
            | Self::DeletionUnfinished
            | Self::DeletionSignInRequired
            | Self::UnfinishedInstallationUnreadable { .. }
            | Self::StoreOwnerAccount
            | Self::InviteJoining
            // What was asked is what fails: the source's own bytes, an
            // import already running or done, a release that doesn't fit, an
            // edit to fix, a folder to choose again, a save over a release
            // that changed since its editor opened.
            | Self::ImportData
            | Self::CandidateImportInProgress
            | Self::FilesAlreadyImported
            | Self::AlbumGone
            | Self::MetadataTrackCount { .. }
            | Self::TiedMediums { .. }
            | Self::ReleaseChangedSinceOpened
            | Self::InvalidEdit(_)
            | Self::WatchedFolderOverlap
            | Self::FilenamePatternInvalid
            | Self::FolderPatternInvalid
            // An export or a save whose cause bae doesn't know, a device or
            // receiver that refused or can't be driven.
            | Self::Export
            | Self::Save
            | Self::Renderer
            | Self::AirPlayUnsupported
            | Self::AirPlayUnreadableSettings
            | Self::AirPlayHomeOnly
            | Self::AirPlayAdminsOnly
            | Self::AirPlayOwnerOnly
            // The storage location or bucket as it is, and the keychain or
            // Block Store as this build and phone have them.
            | Self::S3VersioningUnsupported(_)
            | Self::StorageLocationChangeUnsupported
            | Self::Keyring
            | Self::KeychainEntitlementMissing
            | Self::SavedRestoreCodesFull
            | Self::SavedRestoreCodeTooLarge => false,
        }
    }
}

/// Whether a provider failure can end by itself, as coven retries it
/// (coven's own split of what clears by itself, by a person, or never): the
/// network, throttling, a conflict or a session another request ended, an
/// object not there yet, a transfer that changed a read, the provider's quota,
/// a local file, or catching up first. A refusal of the credentials, the
/// location or the request itself never passes as asked.
#[deny(clippy::wildcard_enum_match_arm)]
fn storage_retry_can_help(failure: coven::StorageFailure) -> bool {
    use coven::StorageFailure as F;
    match failure {
        F::Network
        | F::Corrupted
        | F::Conflict
        | F::RateLimited
        | F::NotFound
        | F::SessionExpired
        | F::CatchUpRequired
        | F::QuotaExceeded
        | F::File => true,
        F::Authentication
        | F::PermissionDenied
        | F::AccessNotRegistered { .. }
        | F::AccessNotGranted { .. }
        | F::ContainerNotFound
        | F::RegionMismatch
        | F::ClockSkew
        | F::InvalidConfiguration
        | F::NotStoreOwner
        | F::AccountIdUnavailable
        | F::ProviderUnavailable
        | F::MemberKeysMissing
        | F::DropboxAppFolderNotShareable
        | F::AlreadyExists
        | F::StoreExists
        | F::OwnLocationRequired
        | F::Refused
        | F::Protocol
        | F::InvalidPath
        | F::InvalidRange
        | F::InviteeIsStoreOwner
        | F::InvitationMismatch
        | F::SessionMismatch
        | F::InvalidPart
        | F::SingleRequestTooLarge { .. }
        | F::CopyTooLarge { .. }
        | F::Encoding
        | F::SharingRefused(_)
        // A bug in coven's storage work, which the same call meets again.
        | F::Bug => false,
    }
}

#[cfg(test)]
#[path = "retry_tests.rs"]
mod tests;
