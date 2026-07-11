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

mod support;

use std::path::Path;

use bae_core::app::bootstrap;
use bae_core::config::{CloudProvider, Config};
use bae_core::keys::StoreKeys;
use bae_core::library::{create_library, unlock_library};
use coven::{EncryptionService, StoreDir, UuidProvider};
use serial_test::serial;
use tempfile::TempDir;
use uuid::Uuid;

/// A valid 32-byte encryption key in hex, used to compute a fixture's stored
/// fingerprint.
const KEY_HEX: &str = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";

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

/// A fixture library that presents as locked on this device: the config records
/// an encryption key (fingerprint + `encryption_key_stored`) and an opaque S3
/// home, but no key is placed in the keyring — the "keychain wiped, config
/// preserved" shape a returning user hits.
fn write_locked_library(home: &Path, name: &str) -> String {
    write_library(home, name, |config| {
        config.encryption_key_stored = true;
        config.encryption_key_fingerprint =
            Some(EncryptionService::new(KEY_HEX).unwrap().fingerprint());
        config.cloud_home.provider = Some(CloudProvider::S3);
        config.cloud_home.s3_bucket = Some("bae-locked-fixture".to_string());
        config.cloud_home.s3_region = Some("us-east-1".to_string());
    })
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
    let a = create_library("Library A".into(), &UuidProvider).unwrap();
    let a_id = a.store_id.clone();
    let b_id = write_locked_library(home.path(), "Library B");

    let app = bootstrap(b_id, 200, true, None).expect("a locked open completes with sync deferred");

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
    create_library("Library A".into(), &UuidProvider).unwrap();
    let b_id = write_plain_library(home.path(), "Library B");

    let app = bootstrap(b_id.clone(), 200, true, None).expect("a plain local open completes");

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
    let a = create_library("Library A".into(), &UuidProvider).unwrap();
    let a_id = a.store_id.clone();
    let b_id = write_plain_library(home.path(), "Library B");

    // A directory where the SQLite file belongs makes the DB open fail.
    let db_path = registered_library_dir(home.path(), &b_id).db_path();
    std::fs::create_dir(&db_path).unwrap();

    let result = bootstrap(b_id, 200, true, None);
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

/// A successful unlock re-runs bootstrap unlocked, which advances the pointer.
/// Needs a reachable cloud home to mint the key and attach sync, so it is minio-
/// backed and `#[ignore]`d like the rest of the S3 suite.
#[test]
#[ignore]
#[serial]
fn unlock_then_reopen_advances_active_pointer() {
    use bae_core::keys::BaeStoreKeysExt;
    use support::TestS3Endpoint;

    let _home = fake_home();
    let s3 = TestS3Endpoint::from_env();
    let bucket = format!("bae-unlock-reopen-{}", Uuid::new_v4());

    // Library B: create it, open it, and configure an opaque S3 home. save_s3_config
    // mints the encryption key and records its fingerprint.
    let b = create_library("Library B".into(), &UuidProvider).unwrap();
    let b_id = b.store_id.clone();
    let app = bootstrap(b_id.clone(), 200, true, None).expect("open the fresh library");
    app.runtime.block_on(s3.provision_bucket(&bucket));
    app.runtime
        .block_on(
            app.services
                .library_manager()
                .save_s3_config(s3.config(&bucket, None)),
        )
        .expect("configure the opaque S3 home");
    let key_hex = StoreKeys::new(b_id.clone())
        .get_encryption_key()
        .unwrap()
        .expect("save_s3_config minted an encryption key");
    drop(app);

    // Library A becomes the active library.
    create_library("Library A".into(), &UuidProvider).unwrap();

    // Lock B (the "Lock Library" core call removes the key from the keyring).
    StoreKeys::new(b_id.clone())
        .forget_encryption_key()
        .expect("lock library B");

    // Reopening B is now locked: it succeeds with sync deferred, pointer stays A.
    let locked = bootstrap(b_id.clone(), 200, true, None).expect("locked open of B succeeds");
    assert!(!locked.services.library_manager().has_encryption());
    let a_after_lock = active_pointer();
    drop(locked);
    assert_ne!(a_after_lock.as_deref(), Some(b_id.as_str()));

    // Unlock B, then reopen: sync attaches and the pointer advances to B.
    unlock_library(&b_id, &key_hex).expect("unlock B");
    let unlocked = bootstrap(b_id.clone(), 200, true, None).expect("unlocked open of B succeeds");
    assert!(unlocked.services.library_manager().has_encryption());
    assert_eq!(active_pointer().as_deref(), Some(b_id.as_str()));
    drop(unlocked);
}
