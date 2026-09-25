//! Which user-facing class a coven failure falls in: what the person can do
//! about it — fix the cloud setup, retry once it is reachable, unlock the
//! keyring — or an internal fault.

/// A cloud-home failure the user must fix (bad credentials, missing bucket) vs a
/// transient one to retry (unreachable backend, local I/O).
pub(super) fn cloud_home_category(error: &coven::CloudHomeError) -> crate::ui::UiErrorCategory {
    use crate::ui::UiErrorCategory as C;
    if error.is_retryable() {
        C::Network
    } else {
        C::Credentials
    }
}

pub(super) fn cloud_setup_category(
    error: &coven::CloudHomeSetupError,
) -> crate::ui::UiErrorCategory {
    cloud_setup_failure_category(error.failure())
}

pub(super) fn cloud_setup_failure_category(
    failure: coven::CloudHomeSetupFailure,
) -> crate::ui::UiErrorCategory {
    crate::ui::UiErrorCategory::CloudSetup(failure)
}

pub(super) fn cloud_unlock_category(
    error: &coven::CloudHomeUnlockError,
) -> crate::ui::UiErrorCategory {
    use coven::CloudHomeUnlockError;
    match error {
        CloudHomeUnlockError::Connection(error) => sync_category(error),
        CloudHomeUnlockError::Rollback { failure, .. } => cloud_unlock_category(failure),
        CloudHomeUnlockError::KeyNotRequired => crate::ui::UiErrorCategory::Config,
        CloudHomeUnlockError::MasterKey(_) | CloudHomeUnlockError::Commit(_) => {
            crate::ui::UiErrorCategory::Keyring
        }
    }
}

/// Classify a coven sync/membership failure into a user-facing class: keyring vs
/// cloud credentials/network vs the membership chain itself.
pub(super) fn sync_category(error: &coven::SyncError) -> crate::ui::UiErrorCategory {
    use crate::ui::UiErrorCategory as C;
    use coven::SyncError;
    if error.is_retryable() {
        return C::Network;
    }
    match error {
        SyncError::Key(coven::KeyError::NoDeviceIdentity) => C::DeviceIdentityMissing,
        SyncError::Key(_) => C::Keyring,
        SyncError::CloudHome(e) => cloud_home_category(e),
        SyncError::Setup(_) => C::Credentials,
        SyncError::Membership(_) => C::Membership,
        SyncError::DeviceJoin(_) => C::Membership,
        // The handshake's storage transport failing (including the deadline
        // that means the other device never took its step) is the membership
        // operation failing, not the library or this device's credentials.
        SyncError::DeviceJoinTransport(_) => C::Membership,
        // The other membership operations that carry a pasted/scanned code —
        // excluding a device from the store, promoting a member to owner — and a
        // code that doesn't decode as the operation it was pasted into. Same
        // class as an invalid membership-operation code: the membership operation failed, not
        // the library or this device's credentials.
        SyncError::InvalidMembershipOperationCode(_) => C::Membership,
        SyncError::DeviceExclusion(_) => C::Membership,
        SyncError::OwnerPromotion(_) => C::Membership,
        SyncError::StorageSetup(_) => C::Network,
        SyncError::NotConfigured
        | SyncError::LoopNotRunning
        | SyncError::NotEncryptedHome
        | SyncError::MasterKeyNotEstablished
        | SyncError::Init(_)
        | SyncError::Store(_)
        | SyncError::Circle(_)
        | SyncError::Database(_)
        | SyncError::RoutingEncryption(_)
        | SyncError::BlobUpload(_)
        | SyncError::StuckReclaim(_)
        | SyncError::Loop(_) => C::Internal,
    }
}

/// Classify a keyring failure: a keychain that refused this second is waited
/// out, a missing device identity is its own state, anything else is a broken
/// keyring.
pub(crate) fn key_category(error: &coven::KeyError) -> crate::ui::UiErrorCategory {
    use crate::ui::UiErrorCategory as C;
    match error {
        coven::KeyError::KeychainTemporarilyUnavailable => C::KeyringLocked,
        coven::KeyError::NoDeviceIdentity => C::DeviceIdentityMissing,
        _ => C::Keyring,
    }
}

/// Classify why a restore or join failed to bootstrap a store from the cloud:
/// the cloud's credentials vs an unreachable backend vs a code that does not
/// decode vs the keyring vs the membership handshake. What is left is a fault
/// in this device's own store work.
pub(crate) fn bootstrap_category(error: &coven::BootstrapError) -> crate::ui::UiErrorCategory {
    use crate::ui::UiErrorCategory as C;
    use coven::BootstrapError as B;
    match error {
        B::CloudHome(error) => cloud_home_category(error),
        B::Key(error) => key_category(error),
        B::RestoreCode(_)
        | B::UnsupportedDeviceInviteVersion(_)
        | B::InvalidStoreId(_)
        | B::Config(_) => C::Config,
        B::MembershipMutation(_)
        | B::DeviceJoin(_)
        | B::DeviceJoinTransport(_)
        | B::DeviceInvite(_)
        | B::Pairing(_)
        | B::PairingState(_)
        | B::StoreRegistration(_) => C::Membership,
        B::Provider(_) | B::ExactSlotsUnavailable { .. } => C::Credentials,
        #[cfg(feature = "oauth-providers")]
        B::OAuthClient(_) => C::Credentials,
        B::Cleanup { cause, .. } => bootstrap_category(cause),
        B::Encryption(_)
        | B::Snapshot(_)
        | B::Pull(_)
        | B::StorePull(_)
        | B::Storage(_)
        | B::Io(_)
        | B::StoreExists(_)
        | B::TornBootstrapCleanup { .. }
        | B::CancelledJoinCleanup { .. }
        | B::DatabaseOpen(_)
        | B::InvalidSigningKey(_)
        | B::Cancelled => C::Internal,
    }
}

/// A make-Remote with no provider connected is the cloud being out of reach,
/// which the person retries; the rest are faults in this device's own state.
pub(super) fn make_remote_category(error: &coven::MakeRemoteError) -> crate::ui::UiErrorCategory {
    use crate::ui::UiErrorCategory as C;
    use coven::MakeRemoteError as E;
    match error {
        E::SyncNotReady => C::Network,
        E::Db(_) => C::Database,
        E::EmptyBatch
        | E::DuplicateRoot(_)
        | E::NotGated(_)
        | E::RemoteRoot(_)
        | E::AlreadyRemote(..)
        | E::UnresolvedLocality(..)
        | E::NothingToMakeRemote(..)
        | E::NotExternal(_)
        | E::SourcePath { .. } => C::Internal,
    }
}

/// A make-Local that cannot reach the cloud, or whose reads fail there, is
/// retried; a destination it cannot write is this device's own fault.
pub(super) fn make_local_category(error: &coven::MakeLocalError) -> crate::ui::UiErrorCategory {
    use crate::ui::UiErrorCategory as C;
    use coven::MakeLocalError as E;
    match error {
        E::SyncNotReady => C::Network,
        E::Read { source, .. } => blob_category(source),
        E::Cleanup { operation, .. } => make_local_category(operation),
        E::Db(_) => C::Database,
        E::NotGated(_)
        | E::RemoteRoot(_)
        | E::AlreadyLocal(..)
        | E::UnresolvedLocality(..)
        | E::TransitionInProgress(..)
        | E::MissingDest(_)
        | E::NonUtf8Dest { .. }
        | E::MissingStoredReference(_)
        | E::WritePath { .. }
        | E::WriteFile { .. }
        | E::CommitFile { .. }
        | E::Cancelled => C::Internal,
    }
}

/// A blob read that could not reach the cloud is retried; one the cloud
/// refused is about its credentials or setup; the rest are this device's own
/// store or files.
pub(super) fn blob_category(error: &coven::BlobCacheError) -> crate::ui::UiErrorCategory {
    use crate::ui::UiErrorCategory as C;
    use coven::BlobCacheError as E;
    match error {
        E::Storage(error) => storage_category(error),
        E::StorageSetup(_) => C::Credentials,
        E::NoCloudHome => C::Config,
        E::Metadata(_) => C::Database,
        E::Path(_)
        | E::File(_)
        | E::Commit(_)
        | E::OpeningAuthority(_)
        | E::ExternalMissing { .. }
        | E::ExternalSizeMismatch { .. }
        | E::LocalSizeMismatch { .. }
        | E::NoLocalCopy { .. }
        | E::LocalityUnresolved { .. }
        | E::NoExternalRef { .. }
        | E::LocalIntegrity { .. }
        | E::RangeOverflow { .. }
        | E::RangeOutOfBounds { .. } => C::Internal,
    }
}

/// Classify a cloud storage failure: one that never reached the backend is
/// retried, a storage configuration coven refused is about the cloud setup.
/// coven keeps the backend's own answer (bad credentials, missing bucket) in a
/// kind it does not export, so a refusal the backend gave reads as internal
/// until it does.
fn storage_category(error: &coven::StorageError) -> crate::ui::UiErrorCategory {
    use crate::ui::UiErrorCategory as C;
    if error.is_transport() {
        return C::Network;
    }
    match error {
        coven::StorageError::Configuration(_) => C::Credentials,
        coven::StorageError::Key(error) => key_category(error),
        _ => C::Internal,
    }
}
