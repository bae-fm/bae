//! User-facing classes for coven's typed failures; diagnostics retain each cause.
//!
//! Every coven variant is classed by name: a variant coven adds fails the
//! build here rather than falling into a catch-all class that shows the person
//! the wrong line.
#![deny(clippy::wildcard_enum_match_arm)]

use super::UiErrorCategory as C;

/// A storage failure keeps coven's exact reason: the line says the storage is
/// full, this device's clock is off, the provider refused the credentials.
pub fn cloud_home_category(error: &coven::StorageError) -> C {
    storage_category(error.failure())
}

/// A storage failure's class: coven's reason, which the line names; or, for a
/// panic in coven's own storage work, a bug the person can only report. Every
/// class made from a `StorageFailure` comes from here, so a storage bug is
/// never shown or kept as a storage reason.
pub fn storage_category(failure: coven::StorageFailure) -> C {
    use coven::StorageFailure as F;
    match failure {
        F::Bug => C::Internal,
        failure @ (F::ProviderUnavailable
        | F::AccessNotRegistered { .. }
        | F::AccessNotGranted { .. }
        | F::DropboxAppFolderNotShareable
        | F::CatchUpRequired
        | F::Network
        | F::Corrupted
        | F::Authentication
        | F::PermissionDenied
        | F::NotFound
        | F::AlreadyExists
        | F::Conflict
        | F::ContainerNotFound
        | F::RegionMismatch
        | F::StoreExists
        | F::ClockSkew
        | F::OwnLocationRequired
        | F::QuotaExceeded
        | F::RateLimited
        | F::InvalidConfiguration
        | F::Refused
        | F::Protocol
        | F::InvalidPath
        | F::InvalidRange
        | F::NotStoreOwner
        | F::InviteeIsStoreOwner
        | F::AccountIdUnavailable
        | F::InvitationMismatch
        | F::SessionMismatch
        | F::SessionExpired
        | F::SessionIdentityUnknown
        | F::InvalidPart
        | F::SingleRequestTooLarge { .. }
        | F::CopyTooLarge { .. }
        | F::Encoding
        | F::File
        | F::MemberKeysMissing) => C::Storage(failure),
    }
}

pub fn cloud_setup_category(error: &coven::StorageSetupError) -> C {
    use coven::StorageSetupError as E;
    match error {
        E::Storage(error) => cloud_home_category(error),
        E::Check(error) => storage_check_category(error),
        E::LocationChangeUnsupported => C::StorageLocationChangeUnsupported,
        E::OAuth(_) => C::Credentials,
        E::SecureStorage(error) => key_category(error),
        E::Removed => C::DeviceRemoved,
        E::StoreOwner => C::StoreOwnerAccount,
        E::UpdateRequired(_) => C::SyncUpdateRequired,
        E::Internal(_) => C::Internal,
        // Another setup runs on this store: a call still under way, or one
        // from before past its point of no return, which only sync's
        // connection finishes. Another starts once it has.
        E::SetupRunning => C::StorageSetupFinishing,
        // The cancel the person asked for reaches them as a cancel, never
        // as an error with a line.
        E::Cancelled => C::Internal,
        // The runtime is shutting down under the work: the process is
        // ending it, which no line the person reads can help with. A panic
        // in coven's work is a bug the person can only report.
        E::Shutdown(_) | E::Bug(_) => C::Internal,
    }
}

/// The check of a provider or of S3's settings that refused a location.
fn storage_check_category(error: &coven::StorageCheckError) -> C {
    use coven::StorageCheckError as E;
    match error {
        // A check whose own work panicked found nothing about the location:
        // a bug the person can only report.
        E::ProviderCheck { source, .. } if source.failure() == coven::StorageFailure::Bug => {
            C::Internal
        }
        E::S3VersioningUnknown { source } if source.failure() == coven::StorageFailure::Bug => {
            C::Internal
        }
        E::ProviderCheck { check, source } => {
            C::StorageCheck { check: *check, failure: source.failure() }
        }
        E::S3VersioningUnknown { source } => C::S3VersioningUnknown(source.failure()),
        E::S3VersioningUnsupported { state } => C::S3VersioningUnsupported(*state),
        E::Storage(error) => cloud_home_category(error),
    }
}

pub fn cloud_unlock_category(error: &coven::StoreKeyUnlockError) -> C {
    use coven::StoreKeyUnlockError as E;
    match error {
        E::NoStorage => C::Config,
        E::Storage(error) => cloud_home_category(error),
        E::SecureStorage(error) => key_category(error),
        E::Removed => C::DeviceRemoved,
        E::UpdateRequired(_) => C::SyncUpdateRequired,
        E::Crypto(_) | E::Other(_) => C::Internal,
        // The runtime is shutting down under the work: the process is
        // ending it, which no line the person reads can help with. A panic
        // in coven's work is a bug the person can only report.
        E::Shutdown(_) | E::Bug(_) => C::Internal,
    }
}

/// The class of the condition that blocked a slot or holds a record.
pub fn pending_reason_category(reason: &coven::PendingReason) -> C {
    use coven::PendingReason as R;
    match reason {
        R::Storage { failure, .. } => storage_category(*failure),
        R::NoStorage => C::Config,
        R::UpdateRequired(_) | R::Refused(_) => C::SyncUpdateRequired,
        R::Removed => C::DeviceRemoved,
        R::KeyUnavailable { .. }
        | R::WritesRefused { .. }
        | R::MemberKeysReplaced { .. }
        | R::ProviderPending { .. }
        | R::AccountInUse { .. }
        | R::AccessRemains { .. }
        | R::Rejected(_) => C::Membership,
        R::Failed(failure) => local_failure_category(*failure),
        R::Missing { .. }
        | R::Waits(_)
        | R::FileUnavailable(_)
        | R::Original { .. }
        | R::Paused => C::Internal,
        R::Cleanup { operation, .. } => pending_reason_category(operation),
    }
}

/// The class of a local failure coven met on this device.
fn local_failure_category(failure: coven::LocalFailure) -> C {
    use coven::LocalFailure as F;
    match failure {
        F::Database => C::Database,
        F::KeyCustody => C::Keyring,
        // A runtime shutting down dropped the work; the next process runs
        // it again. A bug in coven, retried under backoff, is nothing the
        // person can change but report.
        F::Disk | F::Crypto | F::Operation | F::Shutdown | F::Bug => C::Internal,
    }
}

pub fn sync_category(error: &coven::SyncError) -> C {
    use coven::SyncError as E;
    match error {
        E::Pending(record) => pending_reason_category(&record.reason),
        E::Failed { failure, .. } => local_failure_category(*failure),
        E::Storage(error) => cloud_home_category(error),
        E::SecureStorage(error) => key_category(error),
        E::Database(error) => database_category(error),
        E::NoStorage => C::Config,
        // A replacement of this member's S3 key by another key is under way:
        // entering that key again finishes it.
        E::ReplacementPending { .. } => C::Credentials,
        // A code that can't be used here, such as one for another library:
        // the person's state, as a refused code is in a bootstrap.
        E::Code(_) => C::Config,
        E::UpdateRequired(_) => C::SyncUpdateRequired,
        E::Removed => C::DeviceRemoved,
        // Another copy uses this device's id: only a reset answers it.
        E::DeviceIdInUse(_) => C::DeviceResetRequired,
        // Administrative work: a member, an invitation, a key a member has not
        // sealed for this device yet, a reload sharing waits on, a snapshot of
        // this device's that deleting removed members' files waits on.
        E::StoreOwner
        | E::InvitationChanged
        | E::MemberKeysReplaced { .. }
        | E::Rejected(_)
        | E::ReloadPending(_)
        | E::SnapshotPending(_)
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
        | E::Damaged(_) => C::Internal,
        // The runtime is shutting down under the work: the process is
        // ending it, which no line the person reads can help with. A panic
        // in coven's work is a bug the person can only report; a worker's
        // panic answered again is that same bug, which coven's bug sink
        // already reported.
        E::Shutdown(_) | E::Bug(_) | E::WorkerFailed(_) => C::Internal,
    }
}

pub fn key_category(error: &coven::KeyError) -> C {
    use coven::KeyError as E;
    match error {
        // A keychain that refused this second is not a broken keyring. It is
        // the one keyring failure a host should wait out, so it keeps its own
        // class all the way to the UI.
        E::KeychainTemporarilyUnavailable(_) => C::KeyringLocked,
        // The build lacks its keychain entitlement: no keyring the person
        // could fix, and no retry that helps.
        E::MissingKeychainEntitlement(_) => C::KeychainEntitlementMissing,
        // The person left the store, so coven refuses to write their saved
        // restore code or S3 key again: the removal, not the keychain, failed
        // it. Coven counts a person as left once their member's removal
        // applies, by another admin or by themselves, and that removes every
        // device of theirs, this one included: "this device was removed from
        // the library" is true either way, so it shares that class.
        E::Left => C::DeviceRemoved,
        // Android's Block Store limits: the phone keeps no more saved codes,
        // or this one is too large to keep. Neither is a broken keyring.
        E::SavedEntriesFull => C::SavedRestoreCodesFull,
        E::SavedEntryTooLarge => C::SavedRestoreCodeTooLarge,
        E::DeviceIdEncoding
        | E::File(_)
        | E::Crypto(_)
        | E::Material(_)
        | E::PassphraseAuthentication
        | E::PassphraseHeader
        | E::PassphraseParameters
        | E::Unavailable(_)
        | E::Keychain(_)
        | E::Unsupported
        | E::RestoreCodeStoreId => C::Keyring,
        // The custody's handle closed under the call, or bae named an
        // invalid keyring service: bae's own code, not the keyring.
        E::StoreClosed | E::InvalidServiceName => C::Internal,
        // The runtime is shutting down under the work: the process is
        // ending it, which no line the person reads can help with. A panic
        // in coven's work is a bug the person can only report.
        E::Shutdown(_) | E::Bug(_) => C::Internal,
    }
}

pub fn creation_category(error: &coven::StoreCreationError) -> C {
    use coven::StoreCreationError as E;
    match error {
        E::Store(coven::CovenError::Key(error))
        | E::Identity(coven::IdentityError::Custody(error)) => key_category(error),
        // The platform refused coven's watch of the network: the app's
        // packaging or the platform, which the person can't fix; or coven's
        // work panicked, a bug.
        E::Store(coven::CovenError::NetworkWatch(_) | coven::CovenError::Bug(_)) => C::Internal,
        E::Store(coven::CovenError::DeviceLocked { .. }) | E::DeviceLocked { .. } => {
            // bae chooses no file protection class, so coven can't find the
            // device lock keeping a store's files closed: a DeviceLocked means
            // coven or bae went wrong.
            C::Internal
        }
        E::Store(coven::CovenError::Database(error)) => database_category(error),
        E::Store(_) => C::Database,
        E::Identity(_) | E::Directory(_) => C::Internal,
        E::Sync(error) => sync_category(error),
        E::Cleanup { operation, .. } => creation_category(operation),
        E::PublishedButNotSynced { source, .. } => creation_category(source),
        // The runtime is shutting down under the work: the process is
        // ending it, which no line the person reads can help with. A panic
        // in coven's work is a bug the person can only report.
        E::Shutdown(_) | E::Bug(_) => C::Internal,
    }
}

pub fn deletion_category(error: &coven::StoreDeletionError) -> C {
    use coven::StoreDeletionError as E;
    match error {
        E::Sync(error) | E::Closing(error) | E::Unfinished(error) => sync_category(error),
        // This device holds no credentials for the library's storage: the
        // person signs in to it again, then deletes.
        E::SignInRequired(_) => C::DeletionSignInRequired,
        E::Open(error) => write_category(error),
        E::Key(error) => key_category(error),
        E::Lock(_) | E::Unsent(_) | E::NoUnfinishedDeletion(_) => C::Config,
        E::File(_) => C::Internal,
        // The runtime is shutting down under the work: the process is
        // ending it, which no line the person reads can help with. A panic
        // in coven's work is a bug the person can only report.
        E::Shutdown(_) | E::Bug(_) => C::Internal,
    }
}

pub fn bootstrap_category(error: &coven::BootstrapError) -> C {
    use coven::BootstrapError as E;
    match error {
        // A stale resume, a cancel that came after the request may have
        // been sent, or another call's first installation under way, which
        // the bridge hands the app as that call to go on with: the person's
        // state, not a bug.
        E::Code(_)
        | E::NoUnfinishedJoin(_)
        | E::NoUnfinishedRestore(_)
        | E::NoUnfinishedReplacement(_)
        | E::JoinRequestSent
        | E::Directory(coven::BootstrapDirectoryError::Lock(
            coven::StoreLockError::BootstrapPending(_),
        )) => C::Config,
        // A cancel that came after the restoring device registered: coven
        // keeps the restore, which only finishing goes on with.
        E::RestoreRegistered => C::RestoreRegistered,
        E::Store(coven::CovenError::Key(error)) | E::SecureStorage(error) => key_category(error),
        // The platform refused coven's watch of the network: the app's
        // packaging or the platform, which the person can't fix; or coven's
        // work panicked, a bug.
        E::Store(coven::CovenError::NetworkWatch(_) | coven::CovenError::Bug(_)) => C::Internal,
        E::Store(coven::CovenError::DeviceLocked { .. }) | E::DeviceLocked { .. } => {
            // bae chooses no file protection class, so coven can't find the
            // device lock keeping a store's files closed: a DeviceLocked means
            // coven or bae went wrong.
            C::Internal
        }
        E::Store(coven::CovenError::Database(error)) => database_category(error),
        E::Store(_) => C::Database,
        // Restoring as the member whose account holds the store, signed in
        // with another account.
        E::Sync(coven::SyncError::StoreOwner) => C::StoreOwnerAccount,
        // Another of the person's devices replaced their keys, which the
        // code or the copy this call started from doesn't hold.
        E::Sync(coven::SyncError::MemberKeysReplaced { .. }) => C::MemberKeysReplaced,
        E::Sync(error) => sync_category(error),
        E::OAuth(_) => C::Credentials,
        // A join cancelled before sending whose undo of its provider
        // acceptance failed: the undo's failure is the one to act on, and
        // the join stays for another cancel.
        E::Cleanup { operation, cleanup } if matches!(**operation, E::Cancelled) => {
            bootstrap_category(cleanup)
        }
        E::Cleanup { operation, .. } => bootstrap_category(operation),
        E::Published { source, .. } => bootstrap_category(source),
        // A restore or reset of a library this device is removing: only
        // finishing the removal ends it.
        E::Directory(coven::BootstrapDirectoryError::Lock(
            coven::StoreLockError::DeletionUnfinished(_),
        )) => C::DeletionUnfinished,
        E::Cancelled | E::Directory(_) => C::Internal,
        // The runtime is shutting down under the work: the process is
        // ending it, which no line the person reads can help with. A panic
        // in coven's work is a bug the person can only report.
        E::Shutdown(_) | E::Bug(_) => C::Internal,
    }
}

/// A database call's failure, coven's or bae's over it. One holding a bug,
/// its own or an earlier one that failed the store's handle, is a bug the
/// person can only report; any other is the database's.
pub fn database_category(error: &dyn bae_util::task_failure::ReportedBug) -> C {
    if error.reported_bug().is_some() {
        C::Internal
    } else {
        C::Database
    }
}

/// A write coven ran for the app that failed other than in the database, as
/// in a failed rollback: the custody it read, or the database beneath it.
fn write_category(error: &coven::CovenError) -> C {
    use coven::CovenError as E;
    match error {
        E::Key(error) => key_category(error),
        E::Rollback { operation, .. } => write_category(operation),
        E::FileCleanup { write, .. } => write_category(write),
        E::Sync(error) => sync_category(error),
        E::Database(error) => database_category(error),
        E::Migration(_)
        | E::CovenMigration(_)
        | E::Settings(_)
        | E::Lock(_)
        | E::MissingConfiguration { .. }
        | E::DeviceKeyMissing { .. }
        | E::MemberKeysMissing { .. } => C::Database,
        // The runtime is shutting down under the work: the process is
        // ending it, which no line the person reads can help with. A panic
        // in coven's work is a bug the person can only report.
        E::Shutdown(_) | E::Bug(_) => C::Internal,
        // Only opening watches the network, and a watch the platform
        // refuses is the app's packaging or the platform's fault.
        E::NetworkWatch(_) => C::Internal,
        // bae chooses no file protection class, so coven can't find the
        // device lock keeping a store's files closed: a DeviceLocked means
        // coven or bae went wrong.
        E::DeviceLocked { .. } => C::Internal,
    }
}

pub fn device_only_category(error: &coven::DeviceOnlyError) -> C {
    use coven::DeviceOnlyError as E;
    match error {
        E::Download { error, .. } => file_read_category(error),
        E::Database(error) => database_category(error),
        E::Cleanup { error, .. } => device_only_category(error),
        E::RepeatedFile { .. } | E::Path(_) | E::Disk(_) | E::Cancelled => C::Internal,
        // A panic in coven's work is a bug the person can only report.
        E::Bug(_) => C::Internal,
    }
}

pub fn file_read_category(error: &coven::FileReadError) -> C {
    use coven::FileReadError as E;
    match error {
        E::Storage(error) => cloud_home_category(error),
        E::Sync(error) => sync_category(error),
        E::Database(error) => database_category(error),
        E::UpdateRequired(_) => C::SyncUpdateRequired,
        E::NoStorage => C::Config,
        E::Missing { .. }
        | E::DeviceOnly { .. }
        | E::UserFileMissing { .. }
        | E::UserFileChanged { .. }
        | E::Integrity { .. }
        | E::RangeOutOfBounds { .. }
        | E::Disk(_)
        | E::Lock(_) => C::Internal,
        // A panic in coven's work is a bug the person can only report.
        E::Bug(_) => C::Internal,
    }
}

/// Listing this device's libraries failed: a folder or an identity settings
/// file that can't be read is a configuration to fix; a deletion the listing
/// finishes fails as deleting fails; a panic in coven's listing is a bug the
/// person can only report.
pub fn store_list_category(error: &coven::StoreListError) -> C {
    use coven::StoreListError as E;
    match error {
        // coven lifts a file error's bug or shutdown to the listing's own
        // variant, so this one holds neither.
        E::File(_) => C::Config,
        E::Deletion { source, .. } => deletion_category(source),
        // The runtime is shutting down under the work: the process is
        // ending it, which no line the person reads can help with. A panic
        // in coven's work is a bug the person can only report.
        E::Shutdown(_) | E::Bug(_) => C::Internal,
    }
}
