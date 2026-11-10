use super::*;

/// Every failure class reads back as the class it was stored as.
#[test]
fn every_failure_class_reads_back_as_itself() {
    use coven::StorageFailure as F;
    use GroupingBlockReason as B;
    use UiErrorCategory as C;
    let classes = [
        C::Database,
        C::Config,
        C::Internal,
        C::SyncUpdateRequired,
        C::DeviceResetRequired,
        C::DeviceRemoved,
        C::Import,
        C::ImportData,
        C::CandidateImportInProgress,
        C::CandidateBeingIdentified,
        C::FilesAlreadyImported,
        C::MetadataTrackCount { folder_tracks: 12, release_tracks: 10 },
        C::TiedMediums { mediums: vec![2, 4] },
        C::ReleaseChangedSinceOpened,
        C::ChangedMeanwhile,
        C::WatchedFolderOverlap,
        C::InvalidEdit(crate::EditValidationError::EmptyAlbumTitle),
        C::InvalidEdit(crate::EditValidationError::NoAlbumArtist),
        C::InvalidEdit(crate::EditValidationError::EmptyArtistName),
        C::InvalidEdit(crate::EditValidationError::InvalidYear),
        C::GroupingBlocked(B::SourceChanged),
        C::GroupingBlocked(B::SourceGone),
        C::GroupingBlocked(B::FolderFilesTaken),
        C::GroupingBlocked(B::FolderFilesContested),
        C::GroupingBlocked(B::FolderFilesDownloading),
        C::Export,
        C::Save,
        C::StorageCheck { check: coven::StorageCheck::CreateOnce, failure: F::Protocol },
        C::S3VersioningUnsupported(coven::S3BucketVersioning::Enabled),
        C::S3VersioningUnknown(F::PermissionDenied),
        C::StorageLocationChangeUnsupported,
        C::Storage(F::Authentication),
        C::Storage(F::PermissionDenied),
        C::Storage(F::ContainerNotFound),
        C::Storage(F::RegionMismatch),
        C::Storage(F::QuotaExceeded),
        C::Storage(F::InvalidConfiguration),
        C::Storage(F::StoreExists),
        C::Storage(F::Network),
        C::Storage(F::MemberKeysMissing),
        C::Storage(F::File),
        C::Storage(F::Protocol),
        C::Credentials,
        C::Network,
        C::Keyring,
        C::KeyringLocked,
        C::KeychainEntitlementMissing,
        C::SavedRestoreCodesFull,
        C::SavedRestoreCodeTooLarge,
        C::Membership,
        C::InviteJoining,
        C::StoreOwnerAccount,
        C::Renderer,
        C::AirPlayUnsupported,
        C::AirPlayUnreadableSettings,
        C::ReplacementUnfinished(coven::ReplacedBy::Reset),
        C::ReplacementUnfinished(coven::ReplacedBy::Restore),
        C::RestoreRegistered,
        C::MemberKeysReplaced,
        C::StorageSetupUndone,
        C::StorageSetupFinishing,
        C::DeletionUnfinished,
        C::DeletionSignInRequired,
    ];
    for class in classes {
        assert_eq!(category_of(&category_column(class.clone())).unwrap(), class);
    }
    assert!(category_of("grouping_blocked.nowhere").is_err());
    assert!(category_of("nothing").is_err());
}
