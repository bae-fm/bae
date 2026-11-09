use super::{classify_key_state_error, BootstrapError};
use bae_library::LibraryError;

#[tokio::test]
async fn a_keychain_that_refused_right_now_is_not_a_config_failure() {
    let temp = tempfile::tempdir().unwrap();
    let installation =
        coven::Coven::in_memory(bae_app_dir::AppDir::at(temp.path()).unwrap().coven_app_dir());
    installation.fail_next_keychain_operation();
    let failure = bae_store::configure_store(installation.builder(), bae_library::SCHEMA)
        .create_store("Unavailable Keychain")
        .await
        .err()
        .unwrap();
    let coven::StoreCreationError::Store(coven::CovenError::Key(key)) = failure else {
        panic!("unexpected failure: {failure}")
    };
    let error = LibraryError::Keyring(key);

    assert!(matches!(classify_key_state_error(error), BootstrapError::KeyringUnavailable));
}

#[tokio::test]
async fn opening_with_a_locked_keychain_remains_retryable() {
    let temp = tempfile::tempdir().unwrap();
    let installation =
        coven::Coven::in_memory(bae_app_dir::AppDir::at(temp.path()).unwrap().coven_app_dir());
    installation.fail_next_keychain_operation();
    let failure = bae_store::configure_store(installation.builder(), bae_library::SCHEMA)
        .create_store("Keychain test")
        .await
        .err()
        .unwrap();
    let coven::StoreCreationError::Store(error) = failure else {
        panic!("unexpected failure: {failure}")
    };
    assert!(matches!(super::classify_open_error(error.into()), BootstrapError::KeyringUnavailable));
}

fn missing_entitlement() -> coven::KeyError {
    coven::KeyError::MissingKeychainEntitlement(
        keyring_core::Error::BadStoreFormat("errSecMissingEntitlement".into()).into(),
    )
}

/// A build signed without its keychain entitlement fails every keychain
/// read the same way. Opening reports that, not a broken library or a
/// locked keychain to wait out.
#[test]
fn a_missing_keychain_entitlement_is_reported_as_itself() {
    assert!(matches!(
        classify_key_state_error(LibraryError::Keyring(missing_entitlement())),
        BootstrapError::KeychainEntitlementMissing(_)
    ));
    assert!(matches!(
        super::classify_open_error(bae_store::DbError::from(coven::CovenError::Key(
            missing_entitlement()
        ))),
        BootstrapError::KeychainEntitlementMissing(_)
    ));
}

/// A step that fails once the library manager has started closes the
/// library before the bootstrap's runtime ends: nothing of it is left
/// running for the runtime's shutdown to cancel under it.
#[test]
fn a_bootstrap_that_fails_after_the_library_starts_closes_it() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_stack_size(16 * 1024 * 1024)
        .enable_all()
        .build()
        .unwrap();
    let temp = tempfile::tempdir().unwrap();
    let app_dir = bae_app_dir::AppDir::at(temp.path()).unwrap();
    let installation = coven::Coven::in_memory(app_dir.coven_app_dir());
    let handle = runtime
        .block_on(
            bae_store::configure_store(installation.builder(), bae_library::SCHEMA)
                .create_store("Failing Bootstrap"),
        )
        .unwrap();
    let library_id = handle.store_id().to_string();

    let failed = super::bootstrap_opening(
        super::Opening::Fixture(temp.path().join("no-such-fixture.json")),
        app_dir,
        library_id,
        50,
        false,
        super::PlaybackOutput::Device(Box::new(bae_playback::audio_output::FailingAudioDevice)),
        bae_diagnostics::Diagnostics::noop(),
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::SequentialIds::new()),
        super::LibraryOpening::Open(handle.clone()),
        |_services, _runtime| -> Result<(), BootstrapError> {
            unreachable!("the bootstrap fails before it composes an owner")
        },
    );

    assert!(
        matches!(failed, Err(BootstrapError::Internal(_))),
        "the fixture step fails: {failed:?}"
    );
    let read = runtime.block_on(handle.pending());
    assert!(
        matches!(read, Err(coven::CovenError::Database(coven::DbError::StoreClosed))),
        "the library's store is closed: {read:?}"
    );
}

#[test]
fn every_other_keyring_failure_still_reads_as_config() {
    // A keyring that is genuinely broken is not something unlocking fixes,
    // so it must not join the retry path.
    let error = LibraryError::Keyring(coven::KeyError::DeviceIdEncoding);

    assert!(matches!(classify_key_state_error(error), BootstrapError::Config(_)));
}

/// A store this device is removing doesn't open, and the bootstrap says its
/// removal didn't finish.
#[test]
fn an_unfinished_deletion_says_so() {
    let error = bae_store::DbError::from(coven::CovenError::Lock(
        coven::StoreLockError::DeletionUnfinished(coven::StoreId(uuid::Uuid::from_u128(1))),
    ));

    assert!(matches!(super::classify_open_error(error), BootstrapError::DeletionUnfinished));
}

/// A store whose copy another call is still replacing doesn't open, and
/// the bootstrap says which call finishes it.
#[test]
fn an_unfinished_replacement_names_the_call_that_finishes_it() {
    let error = bae_store::DbError::from(coven::CovenError::Lock(
        coven::StoreLockError::ReplacementUnfinished {
            store: coven::StoreId(uuid::Uuid::from_u128(1)),
            by: coven::ReplacedBy::Restore,
        },
    ));

    assert!(matches!(
        super::classify_open_error(error),
        BootstrapError::ReplacementUnfinished(coven::ReplacedBy::Restore)
    ));
}

/// An open whose work panicked fails with the bug, which the open reported
/// as it returned; it shows as a bug, not as the database's failure. So
/// does the store failing on an earlier bug.
#[test]
fn an_open_that_failed_on_a_bug_shows_as_one() {
    let bug = || coven::task::Bug { operation: "open a store", message: "boom".into() };
    for error in [coven::DbError::Bug(bug()), coven::DbError::StoreFailed(bug())] {
        assert!(matches!(
            super::classify_open_error(bae_store::DbError::from(error)),
            BootstrapError::Internal(_)
        ));
    }
}
