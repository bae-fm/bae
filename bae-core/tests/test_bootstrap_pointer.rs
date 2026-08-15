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
use bae_core::library::create_library;
use coven::{StoreDir, UuidProvider};
use serial_test::serial;
use tempfile::TempDir;
use uuid::Uuid;

/// A `TempDir` standing in for the user's home directory, with the env isolated
/// so no ambient dev secret un-locks a fixture. Bind it for the whole test — the
/// directory (and thus `~/.bae`) is deleted when it drops.
fn fake_home() -> TempDir {
    let tmp = TempDir::new().unwrap();
    std::env::set_var("HOME", tmp.path());
    // A `.env` in an ancestor directory (loaded into the process env) could
    // unlock the fixture through its opened Coven handle; drop the dev secrets
    // so the locked branch is reached.
    std::env::remove_var("BAE_ENCRYPTION_KEY");
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

/// A returning opaque cloud home whose master key is absent from this device's
/// Coven custody presents as locked. No Bae config flag duplicates that state.
fn write_locked_library(home: &Path, name: &str) -> String {
    write_library(home, name, |config| {
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

struct TestApp {
    services: bae_core::library::AppServices,
    _ui_event_bus: bae_core::ui::UiEventBus,
    _runtime: tokio::runtime::Runtime,
}

impl TestApp {
    fn start(
        services: bae_core::library::AppServices,
        ui_event_bus: bae_core::ui::UiEventBus,
        runtime: tokio::runtime::Runtime,
    ) -> Result<Self, bae_core::app::BootstrapError> {
        Ok(Self {
            services,
            _ui_event_bus: ui_event_bus,
            _runtime: runtime,
        })
    }
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
        TestApp::start,
    )
    .expect("a locked open completes with sync deferred");

    assert!(
        app.services.cloud_home_key_state().unwrap() == coven::CloudHomeKeyState::Locked,
        "the locked library must report Coven custody as locked"
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
        TestApp::start,
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
        TestApp::start,
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

/// A frontend owner is part of the running application, so failure while
/// constructing it must not record the library as successfully opened.
#[test]
#[serial]
fn bootstrap_that_cannot_compose_the_frontend_leaves_active_pointer() {
    let _home = fake_home();
    let a = create_library(
        bae_core::library_name::LibraryName::parse("Library A").unwrap(),
        &UuidProvider,
    )
    .unwrap();
    let a_id = a.store_id.clone();
    let b_id = write_plain_library(_home.path(), "Library B");

    let result: Result<(), _> = bootstrap(
        b_id,
        200,
        true,
        bae_core::diagnostics::Diagnostics::noop(),
        None,
        |_services, _ui_event_bus, _runtime| {
            Err(bae_core::app::BootstrapError::Internal(
                "frontend owner failed to start".to_string(),
            ))
        },
    );

    assert!(
        result.is_err(),
        "frontend construction failure must surface"
    );
    assert_eq!(
        active_pointer().as_deref(),
        Some(a_id.as_str()),
        "a failed frontend owner must not advance the active pointer"
    );
}

/// A frontend panic is contained by the bootstrap thread boundary and returned
/// to the host as a normal bootstrap failure.
#[test]
#[serial]
fn bootstrap_that_panics_while_composing_the_frontend_returns_an_error() {
    let home = fake_home();
    let id = write_plain_library(home.path(), "Library A");

    let result: Result<(), _> = bootstrap(
        id,
        200,
        true,
        bae_core::diagnostics::Diagnostics::noop(),
        None,
        |_services, _ui_event_bus, _runtime| panic!("frontend owner panicked"),
    );

    assert!(
        matches!(
            result,
            Err(bae_core::app::BootstrapError::Internal(message))
                if message.contains("frontend owner panicked")
        ),
        "a frontend panic must cross the bootstrap boundary as an error"
    );
}

/// Dropping the frontend's app owner releases coven's exclusive store-open lock, so the same
/// library can be reopened in-process — even when the caller never ran the
/// graceful `shutdown`. The lock is held by every `LibraryManager` clone through
/// the shared coven handle; the playback and import services each run on their
/// own thread holding one such clone and only stop on an explicit command, so
/// without a teardown join on drop those threads — and the lock — outlive the
/// owner, and the reopen fails with "store is already open" (the import
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
        TestApp::start,
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
        TestApp::start,
    )
    .expect("reopening the store after dropping the app owner must succeed");
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
        TestApp::start,
    )
    .expect("launching offline must open the library, not abort bootstrap");

    assert!(
        !app.services.has_cloud_home(),
        "the failed connect installs no cloud home — sync reports not connected"
    );
    assert!(
        app.services.is_sync_configured(),
        "the provider is still configured, so the state reads as connect-pending, \
         not as an un-configured local library"
    );
    drop(app);
}
