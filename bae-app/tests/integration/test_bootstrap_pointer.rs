//! Only a fully opened, unlocked library advances the active-library pointer.

use bae_app::{bootstrap, BootstrapError};
use bae_app_dir::AppDir;
use coven::Storage;
use tempfile::TempDir;

struct Home {
    _directory: TempDir,
    app_dir: AppDir,
    installation: coven::Coven,
}

impl Home {
    fn new() -> Self {
        let directory = TempDir::new().unwrap();
        let app_dir = AppDir::under_home(directory.path()).unwrap();
        let installation = coven::Coven::in_memory(app_dir.coven_app_dir());
        Self { _directory: directory, app_dir, installation }
    }

    fn builder(&self) -> coven::CovenBuilder {
        self.installation.builder()
    }

    fn create(&self, name: &str) -> coven::StoreId {
        let runtime = bae_library::test_libraries::multi_thread_runtime();
        runtime.block_on(async {
            let handle = bae_library::create_library(
                self.builder(),
                bae_store::EnteredName::parse(name).unwrap(),
            )
            .await
            .unwrap();
            let id = handle.store_id();
            handle.close().await.unwrap();
            id
        })
    }

    fn open(&self, id: coven::StoreId) -> TestApp {
        bootstrap(
            self.app_dir.clone(),
            id.to_string(),
            200,
            true,
            bae_app::PlaybackOutput::Device(Box::new(
                bae_playback::audio_output::FailingAudioDevice,
            )),
            bae_diagnostics::Diagnostics::noop(),
            self.builder(),
            |_| (),
            TestApp::start,
        )
        .unwrap()
    }

    fn active(&self) -> Option<String> {
        self.app_dir.active_library_id().unwrap()
    }
}

struct TestApp {
    services: bae_app::AppServices,
    runtime: tokio::runtime::Runtime,
}

impl TestApp {
    fn start(
        services: bae_app::AppServices,
        runtime: tokio::runtime::Runtime,
    ) -> Result<Self, BootstrapError> {
        Ok(Self { services, runtime })
    }
    fn close(self) {
        self.runtime.block_on(self.services.close()).unwrap();
    }
}

#[test]
fn bootstrap_of_unlocked_library_advances_active_pointer() {
    let home = Home::new();
    let id = home.create("Library A");
    let app = home.open(id);
    assert_eq!(home.active(), Some(id.to_string()));
    app.close();
}

#[test]
fn bootstrap_that_fails_leaves_active_pointer() {
    let home = Home::new();
    let first = home.create("Library A");
    home.open(first).close();
    let second = coven::StoreId(uuid::Uuid::new_v4());
    let result = bootstrap(
        home.app_dir.clone(),
        second.to_string(),
        200,
        true,
        bae_app::PlaybackOutput::Device(Box::new(bae_playback::audio_output::FailingAudioDevice)),
        bae_diagnostics::Diagnostics::noop(),
        home.builder(),
        |_| (),
        TestApp::start,
    );
    assert!(result.is_err());
    assert_eq!(home.active(), Some(first.to_string()));
}

#[test]
fn bootstrap_that_cannot_compose_the_frontend_leaves_active_pointer() {
    let home = Home::new();
    let first = home.create("Library A");
    home.open(first).close();
    let second = home.create("Library B");
    let result: Result<(), _> = bootstrap(
        home.app_dir.clone(),
        second.to_string(),
        200,
        true,
        bae_app::PlaybackOutput::Device(Box::new(bae_playback::audio_output::FailingAudioDevice)),
        bae_diagnostics::Diagnostics::noop(),
        home.builder(),
        |_| (),
        |services, runtime| {
            runtime.block_on(services.close()).unwrap();
            Err(BootstrapError::Internal("frontend owner failed to start".into()))
        },
    );
    assert!(result.is_err());
    assert_eq!(home.active(), Some(first.to_string()));
}

#[test]
fn bootstrap_that_panics_while_composing_the_frontend_returns_an_error() {
    let home = Home::new();
    let id = home.create("Library A");
    let result: Result<(), _> = bootstrap(
        home.app_dir.clone(),
        id.to_string(),
        200,
        true,
        bae_app::PlaybackOutput::Device(Box::new(bae_playback::audio_output::FailingAudioDevice)),
        bae_diagnostics::Diagnostics::noop(),
        home.builder(),
        |_| (),
        |services, runtime| {
            runtime.block_on(services.close()).unwrap();
            panic!("frontend owner panicked")
        },
    );
    assert!(
        matches!(result, Err(BootstrapError::Internal(message)) if message.contains("frontend owner panicked"))
    );
}

#[test]
fn closing_running_app_releases_the_store_lock_for_reopen() {
    let home = Home::new();
    let id = home.create("Library A");
    home.open(id).close();
    home.open(id).close();
}

#[test]
fn bootstrap_of_locked_library_leaves_active_pointer() {
    let home = Home::new();
    let first = home.create("Library A");
    home.open(first).close();
    let storage = std::sync::Arc::new(
        coven::MemoryStorage::builder()
            .clock(std::sync::Arc::new(coven::SystemClock))
            .build()
            .unwrap(),
    );
    let runtime = bae_library::test_libraries::multi_thread_runtime();
    let id = runtime.block_on(async {
        let handle = bae_library::create_library(
            home.builder().storage_connector(storage.clone()),
            bae_store::EnteredName::parse("Locked Library").unwrap(),
        )
        .await
        .unwrap();
        handle
            .setup_s3_storage(
                storage.config(),
                "Device",
                "key".into(),
                coven::SecretText::new("secret".into()),
                &|_| (),
            )
            .await
            .unwrap();
        handle.stop_sync().await.unwrap();
        handle.forget_store_keys().await.unwrap();
        let id = handle.store_id();
        handle.close().await.unwrap();
        id
    });
    let app = bootstrap(
        home.app_dir.clone(),
        id.to_string(),
        200,
        true,
        bae_app::PlaybackOutput::Device(Box::new(bae_playback::audio_output::FailingAudioDevice)),
        bae_diagnostics::Diagnostics::noop(),
        home.builder().storage_connector(storage),
        |_| (),
        TestApp::start,
    )
    .unwrap();
    assert_eq!(app.services.store_key_state().unwrap(), coven::StoreKeyState::Locked);
    assert_eq!(home.active(), Some(first.to_string()));
    app.close();
}
