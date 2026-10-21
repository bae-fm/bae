//! User-facing classes for coven's typed failures; diagnostics retain each cause.
//!
//! Every coven variant is classed by name: a variant coven adds fails the
//! build here rather than falling into a catch-all class that shows the person
//! the wrong line.
#![deny(clippy::wildcard_enum_match_arm)]

use crate::ui::UiErrorCategory as C;

/// A storage failure keeps coven's exact reason: the line says the storage is
/// full, this device's clock is off, the provider refused the credentials.
pub(super) fn cloud_home_category(error: &coven::StorageError) -> C {
    C::Storage(error.failure())
}

pub(super) fn cloud_setup_category(error: &coven::StorageSetupError) -> C {
    use coven::StorageSetupError as E;
    match error {
        E::Storage(error) => cloud_home_category(error),
        E::ProviderCheck { check, source } => C::StorageCheck {
            check: *check,
            failure: source.failure(),
        },
        E::S3VersioningUnknown { source } => C::S3VersioningUnknown(source.failure()),
        E::S3VersioningUnsupported { state } => C::S3VersioningUnsupported(*state),
        E::LocationChangeUnsupported => C::StorageLocationChangeUnsupported,
        E::OAuth(_) => C::Credentials,
        E::SecureStorage(error) => key_category(error),
        E::Removed => C::DeviceRemoved,
        E::UpdateRequired(_) => C::SyncUpdateRequired,
        E::Internal(_) => C::Internal,
    }
}

pub(super) fn cloud_unlock_category(error: &coven::StoreKeyUnlockError) -> C {
    use coven::StoreKeyUnlockError as E;
    match error {
        E::NoStorage => C::Config,
        E::Storage(error) => cloud_home_category(error),
        E::SecureStorage(error) => key_category(error),
        E::Removed => C::DeviceRemoved,
        E::UpdateRequired(_) => C::SyncUpdateRequired,
        E::Crypto(_) | E::Other(_) => C::Internal,
    }
}

/// The class of the condition that blocked a slot or holds a record.
pub(super) fn pending_reason_category(reason: &coven::PendingReason) -> C {
    use coven::PendingReason as R;
    match reason {
        R::Storage { failure, .. } => C::Storage(*failure),
        R::NoStorage => C::Config,
        R::UpdateRequired(_) | R::Refused(_) => C::SyncUpdateRequired,
        R::Removed => C::DeviceRemoved,
        R::KeyUnavailable { .. }
        | R::WritesRefused { .. }
        | R::MemberKeysReplaced { .. }
        | R::PendingOwner { .. }
        | R::ProviderPending { .. }
        | R::AccountInUse { .. }
        | R::AccessRemains { .. }
        | R::DeleteAccessKey(_)
        | R::AdminAccessRequired
        | R::Rejected(_) => C::Membership,
        R::Failed(failure) => local_failure_category(*failure),
        R::Missing { .. } | R::Waits(_) | R::FileUnavailable(_) | R::Paused => C::Internal,
        R::Cleanup { operation, .. } => pending_reason_category(operation),
    }
}

/// The class of a local failure coven met on this device.
fn local_failure_category(failure: coven::LocalFailure) -> C {
    use coven::LocalFailure as F;
    match failure {
        F::Database => C::Database,
        F::KeyCustody => C::Keyring,
        F::Disk | F::Crypto | F::Operation => C::Internal,
    }
}

pub(super) fn sync_category(error: &coven::SyncError) -> C {
    use coven::SyncError as E;
    match error {
        E::Pending(record) => pending_reason_category(&record.reason),
        E::Failed { failure, .. } => local_failure_category(*failure),
        E::Storage(error) => cloud_home_category(error),
        E::SecureStorage(error) => key_category(error),
        E::Database(_) => C::Database,
        E::NoStorage => C::Config,
        E::UpdateRequired(_) => C::SyncUpdateRequired,
        E::Removed => C::DeviceRemoved,
        // Another copy uses this device's id: only a reset answers it.
        E::DeviceIdInUse(_) => C::DeviceResetRequired,
        // Administrative work: a member, an invitation, a key a member has not
        // sealed for this device yet, a reload sharing waits on, an access key.
        E::StoreOwner
        | E::InvitationChanged
        | E::MemberKeysReplaced { .. }
        | E::Rejected(_)
        | E::ReloadPending(_)
        | E::AccessKeyNotDeleted(_)
        | E::Key(_)
        | E::KeyUnavailable(_) => C::Membership,
        E::Cleanup { operation, .. } => sync_category(operation),
        // Broken invariants and damaged local data: nothing the person can
        // change but report.
        E::NotPending(_)
        | E::OperationData(_)
        | E::SequenceExhausted
        | E::Crypto(_)
        | E::Format(_)
        | E::KeyAlreadyUsed(_)
        | E::Damaged(_)
        | E::WrongStore { .. } => C::Internal,
    }
}

pub(super) fn key_category(error: &coven::KeyError) -> C {
    use coven::KeyError as E;
    match error {
        // A keychain that refused this second is not a broken keyring. It is
        // the one keyring failure a host should wait out, so it keeps its own
        // class all the way to the UI.
        E::KeychainTemporarilyUnavailable(_) => C::KeyringLocked,
        // The build lacks its keychain entitlement: no keyring the person
        // could fix, and no retry that helps.
        E::MissingKeychainEntitlement(_) => C::KeychainEntitlementMissing,
        // The person left the store, so coven refuses to write their synced
        // restore code or S3 key again: the removal, not the keychain, failed it.
        E::Left => C::DeviceRemoved,
        E::DeviceIdEncoding
        | E::File(_)
        | E::Crypto(_)
        | E::Material(_)
        | E::PassphraseAuthentication
        | E::PassphraseHeader
        | E::PassphraseParameters
        | E::Unavailable(_)
        | E::Keychain(_)
        | E::UnsupportedKeyringPlatform
        | E::Unsupported
        | E::RestoreCodeStoreId => C::Keyring,
        // The custody's handle closed under the call, or the keyring service
        // was registered wrong at startup: bae's own sequencing, not the
        // keyring.
        E::StoreClosed
        | E::ServiceNotRegistered
        | E::ServiceAlreadyRegistered
        | E::InvalidServiceName => C::Internal,
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

pub(super) fn file_read_category(error: &coven::FileReadError) -> C {
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
