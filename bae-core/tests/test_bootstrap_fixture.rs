#![cfg(feature = "test-utils")]
//! `app::bootstrap_on_fixture` brings a library up on the state a debug UI
//! test names: the fixture is in the library before any service starts, so
//! the services start on it.

use bae_core::app::bootstrap_on_fixture;
use bae_core::config::AppDir;
use bae_core::library::create_library;
use coven::UuidProvider;
use tempfile::TempDir;

struct TestApp {
    services: bae_core::library::AppServices,
    runtime: tokio::runtime::Runtime,
}

impl TestApp {
    fn start(
        services: bae_core::library::AppServices,
        runtime: tokio::runtime::Runtime,
    ) -> Result<Self, bae_core::app::BootstrapError> {
        Ok(Self { services, runtime })
    }
}

/// A fresh home with one library created in it, and the fixture `json`
/// written beside it. Bind the `TempDir` for the whole test.
fn library_with_fixture(json: serde_json::Value) -> (TempDir, AppDir, String, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    let app_dir = AppDir::under_home(dir.path());
    bae_core::config::install_test_keyring();
    let config = create_library(
        &app_dir,
        bae_core::library_name::LibraryName::parse("Fixture Library").unwrap(),
        &UuidProvider,
    )
    .unwrap();
    let fixture = dir.path().join("fixture.json");
    std::fs::write(&fixture, serde_json::to_vec(&json).unwrap()).unwrap();
    (dir, app_dir, config.store_id.clone(), fixture)
}

fn open(app_dir: &AppDir, library_id: &str, fixture: std::path::PathBuf) -> TestApp {
    bootstrap_on_fixture(
        fixture,
        app_dir.clone(),
        library_id.to_string(),
        200,
        true,
        bae_core::diagnostics::Diagnostics::noop(),
        None,
        coven::OAuthClients::empty(),
        TestApp::start,
    )
    .expect("the library opens on its fixture")
}

/// The library opens holding the fixture's albums.
#[test]
fn a_library_opens_holding_its_fixtures_albums() {
    let (_dir, app_dir, library_id, fixture) = library_with_fixture(serde_json::json!({
        "albums": [
            { "title": "Album Title", "artists": ["Artist Name"], "tracks": ["Track Title"] },
        ],
    }));

    let app = open(&app_dir, &library_id, fixture);

    let albums = app.runtime.block_on(app.services.get_albums(&[])).unwrap();
    assert_eq!(
        albums
            .iter()
            .map(|album| album.title.as_str())
            .collect::<Vec<_>>(),
        ["Album Title"]
    );
}

/// A fixture that cannot be read fails the open, naming why.
#[test]
fn a_fixture_that_cannot_be_read_fails_the_open() {
    let (dir, app_dir, library_id, _) = library_with_fixture(serde_json::json!({}));

    let failed = bootstrap_on_fixture(
        dir.path().join("missing.json"),
        app_dir.clone(),
        library_id,
        200,
        true,
        bae_core::diagnostics::Diagnostics::noop(),
        None,
        coven::OAuthClients::empty(),
        TestApp::start,
    );

    let Err(error) = failed else {
        panic!("an open on a missing fixture must fail");
    };
    assert!(
        error.to_string().contains("missing.json"),
        "the failure names the fixture: {error}"
    );
}
