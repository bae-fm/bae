#![cfg(feature = "test-utils")]
//! The durable active-library pointer (`~/.bae/active-library`) must name a
//! library the user actually landed in. `app::bootstrap` writes it only after a
//! fully-realized open, and never for a library that opened locked (encryption
//! configured but this device's keyring lacks the key) — so cancelling the
//! unlock screen leaves the previously-active library in charge.
//!
//! These drive `app::bootstrap` end to end and each overrides `HOME`, so they
//! run in their own process and `#[serial]` to keep the env mutation from
//! racing sibling tests in this binary.

use std::path::Path;

use bae_core::app::bootstrap;
use bae_core::config::{CloudProvider, Config};
use bae_core::library::{create_library, unlock_library};
use coven::{EncryptionService, StoreDir, UuidProvider};
use serial_test::serial;
use tempfile::TempDir;
use uuid::Uuid;

/// A valid 32-byte encryption key in hex, used to compute a fixture's stored
/// fingerprint.
/// The stored master key is coven's keyring format (every generation), not a
/// bare hex key — that is what `unlock_library` validates and stores.
fn stored_key() -> String {
    coven::MasterKeyring::from(coven::EncryptionService::from_key([
        0u8, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24,
        25, 26, 27, 28, 29, 30, 31,
    ]))
    .to_serialized()
}

/// A `TempDir` standing in for the user's home directory, with the env isolated
/// so no ambient dev secret un-locks a fixture. Bind it for the whole test — the
/// directory (and thus `~/.bae`) is deleted when it drops.
fn fake_home() -> TempDir {
    let tmp = TempDir::new().unwrap();
    std::env::set_var("HOME", tmp.path());
    // A `.env` in an ancestor directory (loaded into the process env) would let
    // `seed_dev_keyring` mint the key for a locked fixture; drop the dev secrets
    // so the locked branch is actually reached.
    std::env::remove_var("BAE_ENCRYPTION_KEY");
    std::env::remove_var("BAE_CLOUD_HOME_CREDENTIALS");
    std::env::remove_var("BAE_DISCOGS_API_KEY");
    bae_core::config::install_test_keyring();
    tmp
}

/// The registered-library directory `~/.bae/libraries/<id>/`. The config layer's
/// own path helpers (`registered_library_path` / `registered_library_dir`) are
/// `pub(crate)`, so an integration test can't reach them; the literal layout is
/// duplicated here.
fn registered_library_dir(home: &Path, id: &str) -> StoreDir {
    StoreDir::new(home.join(".bae").join("libraries").join(id))
}

/// Write a fixture library under `~/.bae/libraries/<id>/` and return its id.
/// `configure` mutates the config before it is saved — a plain local library
/// passes an empty closure; a locked fixture sets its encryption + cloud fields.
/// Does not touch the active pointer.
fn write_library(home: &Path, name: &str, configure: impl FnOnce(&mut Config)) -> String {
    let id = Uuid::new_v4().to_string();
    let mut config = Config::with_defaults(
        id.clone(),
        Uuid::new_v4().to_string(),
        registered_library_dir(home, &id),
        name.to_string(),
    );
    configure(&mut config);
    config.save_to_config_yaml().unwrap();
    id
}

/// Stamp the encryption fields that make a fixture present as locked on a device
/// whose keyring lacks the key: the recorded key fingerprint plus the
/// `encryption_key_stored` hint. This pair alone is what `bootstrap` reads to
/// decide "locked" — the "keychain wiped, config preserved" shape a returning
/// user hits.
fn mark_encryption_locked(config: &mut Config) {
    config.encryption_key_stored = true;
    config.encryption_key_fingerprint =
        Some(EncryptionService::new(&stored_key()).unwrap().fingerprint());
}

/// A fixture library that presents as locked on this device: the config records
/// an encryption key (fingerprint + `encryption_key_stored`) and an opaque S3
/// home, but no key is placed in the keyring.
fn write_locked_library(home: &Path, name: &str) -> String {
    write_library(home, name, |config| {
        mark_encryption_locked(config);
        config.cloud_home.provider = Some(CloudProvider::S3);
        config.cloud_home.s3_bucket = Some("bae-locked-fixture".to_string());
        config.cloud_home.s3_region = Some("us-east-1".to_string());
    })
}

/// A locked fixture with no cloud home: encryption is configured but this
/// device's keyring lacks the key. The active-pointer logic is independent of the
/// provider, and leaving the home unset keeps a subsequent unlocked reopen
/// hermetic — a provider-configured library would attach sync to a live cloud
/// home on reopen, whereas a home-less store's `start_sync` is a no-op.
fn write_locked_library_without_home(home: &Path, name: &str) -> String {
    write_library(home, name, mark_encryption_locked)
}

/// A plain local fixture library: no encryption, no cloud home.
fn write_plain_library(home: &Path, name: &str) -> String {
    write_library(home, name, |_| {})
}

fn active_pointer() -> Option<String> {
    Config::active_library_id().unwrap()
}

/// Bootstrapping a locked library succeeds with sync deferred, but leaves the
/// active pointer naming the library the user last actually opened.
#[test]
#[serial]
fn bootstrap_of_locked_library_leaves_active_pointer() {
    let home = fake_home();
    let a = create_library(
        bae_core::library_name::LibraryName::parse("Library A").unwrap(),
        &UuidProvider,
    )
    .unwrap();
    let a_id = a.store_id.clone();
    let b_id = write_locked_library(home.path(), "Library B");

    let app = bootstrap(
        b_id,
        200,
        true,
        bae_core::diagnostics::Diagnostics::noop(),
        None,
    )
    .expect("a locked open completes with sync deferred");

    assert!(
        !app.services.library_manager().has_encryption(),
        "the locked library must have taken the sync-deferred branch"
    );
    assert_eq!(
        active_pointer().as_deref(),
        Some(a_id.as_str()),
        "a locked open must not advance the active pointer"
    );
    drop(app);
}

/// Bootstrapping an unlocked library advances the active pointer to it — the
/// guard against overshooting into "never advance".
#[test]
#[serial]
fn bootstrap_of_unlocked_library_advances_active_pointer() {
    let home = fake_home();
    create_library(
        bae_core::library_name::LibraryName::parse("Library A").unwrap(),
        &UuidProvider,
    )
    .unwrap();
    let b_id = write_plain_library(home.path(), "Library B");

    let app = bootstrap(
        b_id.clone(),
        200,
        true,
        bae_core::diagnostics::Diagnostics::noop(),
        None,
    )
    .expect("a plain local open completes");

    assert_eq!(
        active_pointer().as_deref(),
        Some(b_id.as_str()),
        "a fully-realized open advances the active pointer"
    );
    drop(app);
}

/// A bootstrap that fails opening the database does not advance the active
/// pointer — the write is the last step, after every fallible step above.
#[test]
#[serial]
fn bootstrap_that_fails_leaves_active_pointer() {
    let home = fake_home();
    let a = create_library(
        bae_core::library_name::LibraryName::parse("Library A").unwrap(),
        &UuidProvider,
    )
    .unwrap();
    let a_id = a.store_id.clone();
    let b_id = write_plain_library(home.path(), "Library B");

    // A directory where the SQLite file belongs makes the DB open fail.
    let db_path = registered_library_dir(home.path(), &b_id).db_path();
    std::fs::create_dir(&db_path).unwrap();

    let result = bootstrap(
        b_id,
        200,
        true,
        bae_core::diagnostics::Diagnostics::noop(),
        None,
    );
    assert!(
        result.is_err(),
        "a directory at the db path must fail the open"
    );
    assert_eq!(
        active_pointer().as_deref(),
        Some(a_id.as_str()),
        "a failed open must not advance the active pointer"
    );
}

/// Dropping a `RunningApp` releases coven's exclusive store-open lock, so the same
/// library can be reopened in-process — even when the caller never ran the
/// graceful `shutdown`. The lock is held by every `LibraryManager` clone through
/// the shared coven handle; the playback and import services each run on their
/// own thread holding one such clone and only stop on an explicit command, so
/// without a teardown join on drop those threads — and the lock — outlive the
/// `RunningApp`, and the reopen fails with "store is already open" (the import
/// worker's exit raced the reopen before it was joined, so this passed only
/// most of the time).
#[test]
#[serial]
fn dropping_running_app_releases_the_store_lock_for_reopen() {
    let _home = fake_home();
    let lib = create_library(
        bae_core::library_name::LibraryName::parse("Library A").unwrap(),
        &UuidProvider,
    )
    .unwrap();
    let id = lib.store_id.clone();

    let app = bootstrap(
        id.clone(),
        200,
        true,
        bae_core::diagnostics::Diagnostics::noop(),
        None,
    )
    .expect("first open succeeds");
    drop(app);

    // Reopen the same store immediately, with no intervening shutdown().
    bootstrap(
        id.clone(),
        200,
        true,
        bae_core::diagnostics::Diagnostics::noop(),
        None,
    )
    .expect("reopening the store after dropping the RunningApp must succeed");
}

/// The full locked-then-unlocked transition in one process: opening B while it is
/// locked leaves the pointer at A, and after `unlock_library` places the key,
/// reopening the *same* store advances the pointer to B. This is the counterpart
/// to `bootstrap_of_locked_library_leaves_active_pointer` — what a *locked* open
/// withholds (B stays at A), the unlock grants — driven end to end: a locked
/// open, a drop, then an unlocked reopen of the store the drop just released.
/// `unlock_library` places the key this device's keyring lacked, so the reopen
/// fully realizes — encryption resolves (`has_encryption` true) and the pointer
/// advances. The fixture carries no cloud home, so that open installs an
/// encryption-holding but home-less sync manager with no cloud round-trip.
#[test]
#[serial]
fn unlock_then_reopen_advances_active_pointer() {
    let home = fake_home();
    let a = create_library(
        bae_core::library_name::LibraryName::parse("Library A").unwrap(),
        &UuidProvider,
    )
    .unwrap();
    let a_id = a.store_id.clone();
    let b_id = write_locked_library_without_home(home.path(), "Library B");

    // A is the library the user last actually landed in; B is registered but
    // locked (its key absent from this device's keyring).
    assert_eq!(active_pointer().as_deref(), Some(a_id.as_str()));

    // Phase 1: open B while it is locked. The open completes with sync deferred
    // but does not realize encryption, so the pointer stays at A. Dropping the app
    // releases coven's store-open lock so B can be reopened below.
    let locked = bootstrap(
        b_id.clone(),
        200,
        true,
        bae_core::diagnostics::Diagnostics::noop(),
        None,
    )
    .expect("locked open of B completes");
    assert!(!locked.services.library_manager().has_encryption());
    assert_eq!(active_pointer().as_deref(), Some(a_id.as_str()));
    drop(locked);

    // Phase 2: unlock B (validates the stored key against the fingerprint and
    // saves it to the keyring), then reopen the same store: the key is now
    // present, so the open fully realizes — encryption resolves and the pointer
    // advances from A to B.
    unlock_library(&b_id, &stored_key()).expect("unlock B");
    let unlocked = bootstrap(
        b_id.clone(),
        200,
        true,
        bae_core::diagnostics::Diagnostics::noop(),
        None,
    )
    .expect("unlocked reopen of B succeeds");
    assert!(unlocked.services.library_manager().has_encryption());
    assert_eq!(
        active_pointer().as_deref(),
        Some(b_id.as_str()),
        "a successful unlock advances the active pointer to the reopened library"
    );
    drop(unlocked);
}

/// A returning user who launches offline must still open their library — bootstrap
/// used to `?` the launch-time `attach_and_start_sync` into `BootstrapError`, so a
/// correctly-set-up library that merely couldn't reach its provider failed to open
/// at all, even though local browse and pinned playback need no network.
///
/// A browsable home takes the attach path without any encryption to seed, and
/// bootstrapping with `cloudkit_ops: None` makes the connect fail immediately and
/// hermetically at the CloudKit-driver lookup — the deterministic stand-in for a
/// connect that can't complete offline. The library must open, and sync must report
/// itself not connected (no cloud home installed) yet still configured, which is the
/// state that drives the reconnect banner.
#[test]
#[serial]
fn bootstrapping_offline_opens_the_library_and_reports_not_connected() {
    let home = fake_home();
    let id = write_library(home.path(), "Offline Library", |config| {
        config.cloud_home.provider = Some(CloudProvider::CloudKit);
        config.cloud_home.storage = coven::HomeStorage::Browsable;
    });

    let app = bootstrap(
        id,
        200,
        true,
        bae_core::diagnostics::Diagnostics::noop(),
        None,
    )
    .expect("launching offline must open the library, not abort bootstrap");

    let manager = app.services.library_manager();
    assert!(
        !manager.has_cloud_home(),
        "the failed connect installs no cloud home — sync reports not connected"
    );
    assert!(
        manager.is_sync_configured(),
        "the provider is still configured, so the state reads as connect-pending, \
         not as an un-configured local library"
    );
    drop(app);
}
