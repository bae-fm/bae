//! User-facing classes for coven's typed failures; diagnostics retain each cause.

use crate::ui::UiErrorCategory as C;

pub(super) fn cloud_home_category(error: &coven::StorageError) -> C {
    use coven::StorageFailure as F;
    match error.failure() {
        F::Network | F::RateLimited => C::Network,
        F::Authentication
        | F::PermissionDenied
        | F::ContainerNotFound
        | F::RegionMismatch
        | F::QuotaExceeded
        | F::InvalidConfiguration
        | F::AccessNotRegistered { .. }
        | F::AccessNotGranted { .. }
        | F::OwnBucketRequired { .. }
        | F::KeyBeyondStore => C::Credentials,
        _ => C::Internal,
    }
}

pub(super) fn cloud_setup_category(error: &coven::StorageSetupError) -> C {
    use coven::StorageSetupError as E;
    match error {
        E::Storage(error) => C::CloudSetup(error.failure()),
        E::ProviderCheck { check, source } => C::StorageCheck {
            check: *check,
            failure: source.failure(),
        },
        E::S3VersioningUnknown { source } => C::S3VersioningUnknown(source.failure()),
        E::S3VersioningUnsupported { state } => C::S3VersioningUnsupported(*state),
        E::LocationChangeUnsupported => C::StorageLocationChangeUnsupported,
        E::OAuth(_) => C::Credentials,
        E::SecureStorage(error) => key_category(error),
        E::Internal(_) => C::Internal,
    }
}

pub(super) fn cloud_unlock_category(error: &coven::StoreKeyUnlockError) -> C {
    use coven::StoreKeyUnlockError as E;
    match error {
        E::NoStorage => C::Config,
        E::Storage(error) => cloud_home_category(error),
        E::SecureStorage(error) => key_category(error),
        E::Crypto(_) | E::Other(_) => C::Internal,
    }
}

pub(super) fn sync_category(error: &coven::SyncError) -> C {
    use coven::SyncError as E;
    match error {
        E::Storage(error) => cloud_home_category(error),
        E::SecureStorage(error) => key_category(error),
        E::Database(_) => C::Database,
        E::NoStorage => C::Config,
        E::UpdateRequired(_) => C::SyncUpdateRequired,
        E::Removed => C::DeviceRemoved,
        E::StoreOwner | E::InvitationChanged | E::MemberKeysReplaced { .. } | E::Rejected(_) => {
            C::Membership
        }
        E::Cleanup { operation, .. } => sync_category(operation),
        _ => C::Internal,
    }
}

fn key_category(error: &coven::KeyError) -> C {
    match error {
        coven::KeyError::KeychainTemporarilyUnavailable(_) => C::KeyringLocked,
        _ => C::Keyring,
    }
}

pub(crate) fn creation_category(error: &coven::StoreCreationError) -> C {
    use coven::StoreCreationError as E;
    match error {
        E::Store(coven::CovenError::Key(error))
        | E::Identity(coven::IdentityError::Custody(error)) => key_category(error),
        E::Store(_) => C::Database,
        E::Identity(_) | E::Directory(_) => C::Internal,
        E::Sync(error) => sync_category(error),
        E::Cleanup { operation, .. } => creation_category(operation),
        E::PublishedButNotSynced { source, .. } => creation_category(source),
    }
}

pub(crate) fn deletion_category(error: &coven::StoreDeletionError) -> C {
    use coven::StoreDeletionError as E;
    match error {
        E::Sync(error) => sync_category(error),
        E::Key(error) => key_category(error),
        E::Lock(_) | E::Unsent(_) => C::Config,
        E::File(_) => C::Internal,
    }
}

pub(crate) fn bootstrap_category(error: &coven::BootstrapError) -> C {
    use coven::BootstrapError as E;
    match error {
        E::Code(_) => C::Config,
        E::Store(coven::CovenError::Key(error)) | E::SecureStorage(error) => key_category(error),
        E::Store(_) => C::Database,
        E::Sync(error) => sync_category(error),
        E::OAuth(_) => C::Credentials,
        E::Cleanup { operation, .. } => bootstrap_category(operation),
        E::Published { source, .. } => bootstrap_category(source),
        E::Cancelled | E::Directory(_) => C::Internal,
    }
}

pub(super) fn blob_category(error: &coven::FileReadError) -> C {
    use coven::FileReadError as E;
    match error {
        E::Storage(error) => cloud_home_category(error),
        E::Database(_) => C::Database,
        E::UpdateRequired(_) => C::SyncUpdateRequired,
        E::NoStorage => C::Config,
        E::Uploading { .. } => C::Network,
        E::Missing { .. }
        | E::UserFileMissing { .. }
        | E::UserFileChanged { .. }
        | E::Integrity { .. }
        | E::RangeOutOfBounds { .. }
        | E::Disk(_)
        | E::Lock(_) => C::Internal,
    }
}
