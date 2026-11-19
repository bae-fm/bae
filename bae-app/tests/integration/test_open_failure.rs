//! A failed open of a library says whether a reset from the cloud loads it
//! again: only when what failed is this device's copy of its data, and the
//! library has storage to load it from.

use bae_app::{bootstrap, OpenFailure};
use bae_app_dir::AppDir;
use bae_util::test_files::TestFolder;
use coven::Storage;

struct Home {
    directory: TestFolder,
    app_dir: AppDir,
    installation: coven::Coven,
    storage: std::sync::Arc<coven::MemoryStorage>,
}

impl Home {
    fn new() -> Self {
        let directory = TestFolder::new();
        let app_dir = AppDir::under_home(directory.path()).unwrap();
        let installation = coven::Coven::in_memory(app_dir.coven_app_dir());
        let storage = std::sync::Arc::new(
            coven::MemoryStorage::builder()
                .clock(bae_store::TestSources::system().clock().clone())
                .build()
                .unwrap(),
        );
        Self { directory, app_dir, installation, storage }
    }

    fn library_builder(&self) -> coven::CovenBuilder {
        bae_store::test_devices::test_builder(&self.installation)
            .storage_connector(self.storage.clone())
    }

    /// A closed library whose preferences hold a value nothing writes, kept
    /// in storage when `stored`, else on this device only.
    fn damaged_library(&self, stored: bool) -> coven::StoreId {
        let runtime = bae_library::test_libraries::multi_thread_runtime();
        let id = runtime.block_on(async {
            let handle = bae_library::create_library(
                self.library_builder(),
                bae_store::EnteredName::parse("Damaged").unwrap(),
                bae_store::EnteredName::parse("Creator").unwrap(),
                "Test Device",
                &bae_diagnostics::Diagnostics::noop(),
            )
            .await
            .unwrap();
            if stored {
                handle
                    .setup_s3_storage(
                        self.storage.config(),
                        "key".into(),
                        coven::SecretText::new("secret".into()),
                        &|_| (),
                    )
                    .await
                    .unwrap();
            }
            let id = handle.store_id();
            handle.close().await.unwrap();
            id
        });
        bae_preferences::plant_unknown_replay_gain_mode_for_test(self.directory.path());
        id
    }

    fn open(&self, id: coven::StoreId) -> Result<(), OpenFailure> {
        bootstrap(
            self.app_dir.clone(),
            id.to_string(),
            200,
            true,
            bae_app::PlaybackOutput::Device(Box::new(
                bae_playback::audio_output::FailingAudioDevice,
            )),
            bae_diagnostics::Diagnostics::noop(),
            std::sync::Arc::new(coven::SystemClock),
            coven::FileBudget::for_test(),
            std::sync::Arc::new(coven::SequentialIds::new()),
            &self.installation,
            self.library_builder(),
            |_| (),
            |services, runtime| {
                runtime.block_on(services.close()).unwrap();
                Ok(())
            },
        )
    }
}

/// bae can't read this device's copy of a library kept in storage: a reset
/// from the cloud loads the library again, so the open offers it.
#[test]
fn a_damaged_library_with_storage_offers_the_reset() {
    let home = Home::new();
    let id = home.damaged_library(true);

    let failure = home.open(id).unwrap_err();

    assert!(matches!(failure, OpenFailure::ResetFromCloud(_)), "{failure:?}");
}

/// A library kept on this device only has nothing to load from, so the open
/// of its damaged copy fails without offering a reset.
#[test]
fn a_damaged_library_kept_on_this_device_only_offers_no_reset() {
    let home = Home::new();
    let id = home.damaged_library(false);

    let failure = home.open(id).unwrap_err();

    assert!(matches!(failure, OpenFailure::Failed(_)), "{failure:?}");
    assert!(failure.error().ui_error().is_ok_and(|error| {
        let bae_diagnostics::UiError::Diagnostic { category, .. } = error;
        category.reset_from_cloud_replaces()
    }));
}

/// Opening a library that isn't there fails without offering a reset,
/// because a reset can't load it.
#[test]
fn a_library_that_is_not_there_offers_no_reset() {
    let home = Home::new();
    let missing = coven::StoreId(coven::IdSource::new_id(&coven::UuidIds));

    let failure = home.open(missing).unwrap_err();

    assert!(matches!(failure, OpenFailure::Failed(_)), "{failure:?}");
}
