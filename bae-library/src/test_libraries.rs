//! Test libraries for the tests of this crate and the crates above it: a
//! library opened as the app opens one, the services a test starts over it,
//! the runtime the tests around them need, playback on a capture device,
//! profiles, a library over in-memory storage, a release imported into a
//! fresh library, and the devices of one person's library syncing over shared
//! storage.

mod devices;
mod playback;
mod profiles;
mod records;
mod storage;

pub use devices::{run_library_test, run_two_device_test, DeviceLibrary, TestDevice, TwoDevices};
pub use playback::{
    start_capture_playback, start_manual_playback, start_manual_playback_with_clock,
};
pub use profiles::{made_profile, made_profile_on, requested_profile, requested_profile_on};
pub use records::{
    insert_local_release_with_files, insert_release, insert_release_with_queued_uploads,
    test_album, test_release,
};
pub use storage::{connect_storage, remove_uploaded_file, storage_library};

use crate::{CancellationToken, LibraryManager};
use bae_catalogs::remote_image::RemoteImageCache;
use bae_diagnostics::Diagnostics;
use bae_import::{CatalogSessions, ImportServiceHandle};
use bae_store::Store;
use bae_sync_status::SyncStatus;
use coven::{ClockRef, IdSourceRef};
use std::sync::Arc;

/// An open test library: its manager, and what the app would hand the
/// library's services beside it. A test starts the services it needs over it,
/// each over the same store, catalog sessions and image cache.
pub struct TestLibrary {
    manager: LibraryManager,
    store: Store,
    records: bae_records::Records,
    people: bae_people::People,
    membership: bae_people::Membership,
    sync_status: SyncStatus,
    catalogs: CatalogSessions,
    import_tables: bae_import::tables::ImportTables,
    closing: CancellationToken,
    clock: ClockRef,
    ids: IdSourceRef,
    diagnostics: Diagnostics,
    remote_images: RemoteImageCache,
}

impl TestLibrary {
    /// Open the library over `store` on the real clock and ids, asking
    /// `providers`, which were built on `http`, reporting nowhere.
    pub async fn open(
        store: Store,
        app_dir: bae_app_dir::AppDir,
        providers: bae_catalogs::providers::Providers,
        http: bae_util::http::Http,
    ) -> Self {
        Self::open_with(
            store,
            app_dir,
            providers,
            http,
            Arc::new(coven::SystemClock),
            Arc::new(coven::UuidIds),
            Diagnostics::noop(),
        )
        .await
    }

    /// Open the library over `store` through the app's opening step, on
    /// `clock` and `ids`, reporting to `diagnostics`. `http` is the transport
    /// `providers` were built on. The catalog sessions, the image cache over
    /// `http` and the closing token are built here, as the bootstrap builds
    /// them from the one transport it holds.
    pub async fn open_with(
        store: Store,
        app_dir: bae_app_dir::AppDir,
        providers: bae_catalogs::providers::Providers,
        http: bae_util::http::Http,
        clock: ClockRef,
        ids: IdSourceRef,
        diagnostics: Diagnostics,
    ) -> Self {
        let remote_images = RemoteImageCache::for_test(http, diagnostics.clone());
        let preferences = bae_preferences::PreferencesRow::new(store.clone());
        let records = bae_records::Records::new(store.clone(), ids.clone());
        let people = bae_people::People::new(store.clone());
        let membership = bae_people::Membership::new(store.clone());
        let sync_status = SyncStatus::new(store.clone());
        let catalogs = CatalogSessions::new(
            providers,
            store.clone(),
            preferences.clone(),
            diagnostics.clone(),
            tokio::runtime::Handle::current(),
        );
        let import_tables = bae_import::tables::ImportTables::new(store.clone());
        let release_reads = bae_import::ReleaseReads::new(import_tables.clone(), catalogs.clone());
        let closing = CancellationToken::new();
        let manager = crate::open_library_manager(
            store.clone(),
            preferences.clone(),
            records.clone(),
            people.clone(),
            membership.clone(),
            sync_status.clone(),
            release_reads,
            closing.clone(),
            app_dir,
            clock.clone(),
            ids.clone(),
            diagnostics.clone(),
            tokio::runtime::Handle::current(),
            remote_images.clone(),
        )
        .await
        .expect("open the test library");
        Self {
            manager,
            store,
            records,
            people,
            membership,
            sync_status,
            catalogs,
            import_tables,
            closing,
            clock,
            ids,
            diagnostics,
            remote_images,
        }
    }

    /// The library's manager.
    pub fn manager(&self) -> &LibraryManager {
        &self.manager
    }

    /// The library's store.
    pub fn store(&self) -> &Store {
        &self.store
    }

    /// The library's records.
    pub fn records(&self) -> &bae_records::Records {
        &self.records
    }

    /// The library's people.
    pub fn people(&self) -> &bae_people::People {
        &self.people
    }

    /// The library's members and admin changes.
    pub fn membership(&self) -> &bae_people::Membership {
        &self.membership
    }

    /// The library's sync status.
    pub fn sync_status(&self) -> &SyncStatus {
        &self.sync_status
    }

    /// The catalog sessions the library asks through.
    pub fn catalogs(&self) -> &CatalogSessions {
        &self.catalogs
    }

    /// Import's tables over the library's store, which the import service
    /// started on it holds.
    pub fn import_tables(&self) -> &bae_import::tables::ImportTables {
        &self.import_tables
    }

    /// The cast picker's list over its store.
    pub fn cast_picker(&self) -> bae_playback::renderer::CastPicker {
        bae_playback::renderer::CastPicker::new(self.store.clone())
    }

    /// The token whose cancel ends what runs while the library is open.
    pub fn closing(&self) -> &CancellationToken {
        &self.closing
    }

    /// The clock the library reads.
    pub fn clock(&self) -> &ClockRef {
        &self.clock
    }

    /// The ids the library mints.
    pub fn ids(&self) -> &IdSourceRef {
        &self.ids
    }

    /// Where the library reports.
    pub fn diagnostics(&self) -> &Diagnostics {
        &self.diagnostics
    }

    /// The image cache the library fetches covers through.
    pub fn remote_images(&self) -> &RemoteImageCache {
        &self.remote_images
    }

    /// The import service alone, over the library's store, ending as the
    /// library closes.
    pub async fn start_import(&self) -> Result<ImportServiceHandle, bae_import::ImportError> {
        bae_import::ImportService::start(
            tokio::runtime::Handle::current(),
            self.store.clone(),
            self.records.clone(),
            self.catalogs.clone(),
            self.remote_images.clone(),
            self.diagnostics.clone(),
            self.import_tables.clone(),
            self.clock.clone(),
            self.ids.clone(),
            self.closing.child_token(),
        )
        .await
    }

    /// [`Self::start_import`] with the Discogs key stored that seeded Discogs
    /// fixtures are asked for with.
    pub async fn start_discogs_import(&self) -> ImportServiceHandle {
        self.catalogs
            .store_discogs_key("test-discogs-token", bae_preferences::DiscogsValidation::Valid)
            .await
            .expect("test Discogs key is stored");
        self.start_import().await.expect("test import service starts")
    }

    /// Import `album_dir` as the already-seeded Discogs release
    /// `release_id_key`, under candidate key `"test"` and a fresh import id,
    /// and return the release id the import landed. Panics if the import
    /// fails.
    pub async fn import_folder(
        &self,
        album_dir: impl Into<std::path::PathBuf>,
        release_id_key: String,
    ) -> String {
        let (release_id, _album_id) = self
            .import_folder_as(
                album_dir.into(),
                release_id_key,
                "test",
                uuid::Uuid::new_v4().to_string(),
            )
            .await
            .expect("the test import service accepts and completes the folder import");
        release_id
    }

    /// Send one folder import of `album_dir` as the already-seeded Discogs
    /// release `release_id_key`, under `candidate_key` and `import_id`, and
    /// wait for the worker to finish it, returning (release_id, album_id).
    /// The service is started here and dropped on return — by then the import
    /// has landed everything it writes.
    async fn import_folder_as(
        &self,
        album_dir: std::path::PathBuf,
        release_id_key: String,
        candidate_key: &str,
        import_id: String,
    ) -> Result<(String, String), Box<dyn std::error::Error>> {
        let import = self.start_discogs_import().await;
        import
            .send_command(bae_import::ImportCommand {
                candidate_key: bae_import::CandidateKey::from(candidate_key.as_bytes().to_vec()),
                ..bae_import::test_imports::folder_import(
                    &import_id,
                    album_dir,
                    bae_import::test_imports::discogs_release(release_id_key),
                )
            })
            .await
            .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
        let mut progress_rx = import.subscribe_import(import_id);
        Ok(bae_import::test_imports::wait_for_import_complete(&mut progress_rx).await)
    }

    /// The playback service alone, on a device that plays nothing, with no
    /// stored queue restored.
    pub async fn start_playback(&self) -> bae_playback::PlaybackHandle {
        self.start_playback_reporting(
            Box::new(bae_playback::audio_output::FailingAudioDevice),
            Arc::new(bae_playback::WallPlaybackClock::new(self.clock.clone())),
            50,
            false,
        )
        .await
    }

    /// The playback service alone, playing on `device`, timing side-pause
    /// countdowns on `clock` and reporting its position every 100 ms. With
    /// `restore_playback` ("Restore on launch") it resumes the stored queue;
    /// without, the queue and the resume point are forgotten first.
    pub async fn start_playback_on(
        &self,
        device: Box<dyn bae_playback::AudioOutputDevice>,
        clock: bae_playback::PlaybackClockRef,
        restore_playback: bool,
    ) -> bae_playback::PlaybackHandle {
        self.start_playback_reporting(device, clock, 100, restore_playback).await
    }

    async fn start_playback_reporting(
        &self,
        device: Box<dyn bae_playback::AudioOutputDevice>,
        clock: bae_playback::PlaybackClockRef,
        position_update_interval_ms: u32,
        restore_playback: bool,
    ) -> bae_playback::PlaybackHandle {
        self.manager
            .settle_playback_queue(restore_playback)
            .await
            .expect("settle the playback queue");
        bae_playback::PlaybackService::start_with_audio_device(
            self.store.clone(),
            self.records.clone(),
            self.people.clone(),
            self.ids.clone(),
            bae_playback::tables::PlaybackQueueTables::new(self.store.clone()),
            bae_playback::tables::PlaybackNowRow::new(self.store.clone()),
            bae_playback::tables::PlaybackTimelineRow::new(self.store.clone()),
            self.diagnostics.clone(),
            tokio::runtime::Handle::current(),
            position_update_interval_ms,
            restore_playback,
            device,
            clock,
        )
    }
}

/// Open `dir/test.db` under the real clock and real UUID provider.
async fn open_test_db(dir: &std::path::Path) -> Store {
    Store::new_test(
        crate::SCHEMA,
        dir.join("test.db").to_str().expect("test database path is valid UTF-8"),
        Arc::new(coven::SystemClock),
    )
    .await
    .expect("open the test database")
}

/// Open `dir/test.db` as a library, through the app's own opening step, with
/// `dir` as the library's store dir. Takes the directory rather than making
/// one, so a test that already owns its library directory (a cloned fixture
/// template, a `db/` subdirectory next to an `album/`) uses it too.
pub async fn open_test_library(dir: &std::path::Path) -> TestLibrary {
    open_test_library_with(
        dir,
        bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
        bae_util::http::Http::for_test(),
    )
    .await
}

/// [`open_test_library`] asking `providers` — ones routed to a test's own
/// fakes — rather than providers that reach nothing.
pub async fn open_test_library_with(
    dir: &std::path::Path,
    providers: bae_catalogs::providers::Providers,
    http: bae_util::http::Http,
) -> TestLibrary {
    let store = open_test_db(dir).await;
    TestLibrary::open(store, bae_app_dir::AppDir::under_home(dir).unwrap(), providers, http).await
}

/// A library over an installation the test created with its own storage and
/// custody.
pub async fn test_library_from_handle(
    handle: coven::CovenHandle,
    dir: &std::path::Path,
    providers: bae_catalogs::providers::Providers,
    http: bae_util::http::Http,
) -> TestLibrary {
    TestLibrary::open(
        bae_store::test_devices::store_under(handle, Arc::new(coven::SystemClock)),
        bae_app_dir::AppDir::under_home(dir).unwrap(),
        providers,
        http,
    )
    .await
}

/// [`open_test_library`] over a fresh temp directory. The returned `TempDir`
/// owns the database file, so it must outlive the library.
pub async fn setup_test_library() -> (TestLibrary, tempfile::TempDir) {
    setup_test_library_with(
        bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
        bae_util::http::Http::for_test(),
    )
    .await
}

/// [`setup_test_library`] asking `providers`, ones built over the test's own
/// fakes.
pub async fn setup_test_library_with(
    providers: bae_catalogs::providers::Providers,
    http: bae_util::http::Http,
) -> (TestLibrary, tempfile::TempDir) {
    bae_util::test_logs::install();
    let temp_dir = tempfile::TempDir::new().expect("test library temp dir");
    let library = open_test_library_with(temp_dir.path(), providers, http).await;
    (library, temp_dir)
}

/// [`setup_test_library_with`] with an `album/` directory made alongside it,
/// for a test that writes audio into a folder and imports that folder. The
/// `TempDir` owns both directories, so it must outlive the library.
pub async fn setup_test_library_with_album_dir(
    providers: bae_catalogs::providers::Providers,
    http: bae_util::http::Http,
) -> (TestLibrary, std::path::PathBuf, tempfile::TempDir) {
    let (library, temp_dir) = setup_test_library_with(providers, http).await;
    let album_dir = temp_dir.path().join("album");
    std::fs::create_dir_all(&album_dir).expect("test album dir");
    (library, album_dir, temp_dir)
}

/// Set up a fresh library through the production creation and open paths.
/// Sync is not set up: tests configure it themselves via
/// connect_*/save_s3_config.
pub fn setup_fresh_library(runtime: &tokio::runtime::Runtime) -> (TestLibrary, tempfile::TempDir) {
    let tmp = tempfile::TempDir::new().unwrap();
    let app_dir = bae_app_dir::AppDir::under_home(tmp.path()).unwrap();
    let installation = coven::Coven::in_memory(app_dir.coven_app_dir());
    let library = runtime.block_on(async {
        let handle = bae_store::configure_store(installation.builder(), crate::SCHEMA)
            .create_store("Test Library")
            .await
            .expect("create fresh test store");
        let store = bae_store::test_devices::store_under(handle, Arc::new(coven::SystemClock));
        TestLibrary::open(
            store,
            app_dir,
            bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
            bae_util::http::Http::for_test(),
        )
        .await
    });
    (library, tmp)
}

/// A multi-threaded runtime with every driver enabled, for a `#[test]` that has
/// to `block_on` its async work rather than being a `#[tokio::test]` itself —
/// the shape any test that also owns a runtime-driven service needs.
pub fn multi_thread_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("multi-threaded test runtime")
}

/// The bytes of `release_id`'s stored cover, if it has one.
pub async fn read_cover_image_bytes(mgr: &LibraryManager, release_id: &str) -> Option<Vec<u8>> {
    mgr.read_cover_image_bytes(release_id).await.unwrap()
}

/// What an import landed, beside the library it landed in.
pub struct ImportedRelease {
    pub track_ids: Vec<String>,
    pub release_id: String,
    /// The folder the audio was written into, for a test that reads a real file.
    pub album_dir: std::path::PathBuf,
    /// Owns the library's files, so it must outlive the library.
    pub temp_dir: tempfile::TempDir,
}

/// Import one Discogs-identified release from a folder, on the calling test's
/// runtime, and return the library beside what it landed: open a fresh
/// library, seed `release` for the fake Discogs endpoint, write the audio with
/// `generate_files`, run the import to completion, and read back the tracks it
/// landed. The arrange every playback test binary starts from.
pub async fn imported_release_setup<G>(
    release: bae_catalogs::discogs::DiscogsRelease,
    candidate_key: &str,
    import_id: String,
    generate_files: G,
) -> Result<(TestLibrary, ImportedRelease), Box<dyn std::error::Error>>
where
    G: FnOnce(&std::path::Path),
{
    imported_release_setup_with(
        &bae_catalogs::fake_catalogs::FakeCatalogs::offline(),
        release,
        candidate_key,
        import_id,
        generate_files,
    )
    .await
}

/// [`imported_release_setup`] over a library asking `fakes`, the test's own
/// catalog fakes, which it keeps to seed the releases it imports next.
pub async fn imported_release_setup_with<G>(
    fakes: &bae_catalogs::fake_catalogs::FakeCatalogs,
    release: bae_catalogs::discogs::DiscogsRelease,
    candidate_key: &str,
    import_id: String,
    generate_files: G,
) -> Result<(TestLibrary, ImportedRelease), Box<dyn std::error::Error>>
where
    G: FnOnce(&std::path::Path),
{
    let (library, album_dir, temp_dir) =
        setup_test_library_with_album_dir(fakes.providers(), bae_util::http::Http::for_test())
            .await;

    let release_id_key = fakes.seed_discogs_test_release(release);
    generate_files(&album_dir);

    let (release_id, _album_id) = library
        .import_folder_as(album_dir.clone(), release_id_key, candidate_key, import_id)
        .await?;
    let tracks = library.manager().get_tracks_for_release(&release_id).await?;
    let track_ids: Vec<String> = tracks.iter().map(|t| t.id.clone()).collect();

    Ok((library, ImportedRelease { track_ids, release_id, album_dir, temp_dir }))
}
