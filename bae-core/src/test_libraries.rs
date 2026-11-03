//! Test libraries for the tests of this crate and the crates above it: a
//! database, a library opened over it as the app opens one, its services, the
//! runtimes the tests around them need, playback on a capture device,
//! profiles, a library over in-memory storage, and a release imported into a
//! fresh library.

mod playback;
mod profiles;
mod storage;

pub use playback::{start_capture_playback, start_capture_playback_with_clock, start_playback_on};
pub use profiles::{made_profile, made_profile_on, requested_profile, requested_profile_on};
pub use storage::{connect_storage, release_storage, remove_uploaded_file, storage_library};

/// Open `dir/test.db` under the real clock and real UUID provider.
async fn open_test_db(dir: &std::path::Path) -> bae_db::Database {
    bae_db::Database::new_test(
        dir.join("test.db").to_str().expect("test database path is valid UTF-8"),
        std::sync::Arc::new(coven::SystemClock),
    )
    .await
    .expect("open the test database")
}

/// Open `dir/test.db` as a library, through the app's own opening step, with
/// `dir` as the library's store dir, and return its manager, the store under
/// it, the catalog sessions it asks through, and the token whose cancel ends
/// what runs while it is open. [`start_test_services`] takes them to start the
/// library's services over the same store. Takes the directory rather than making
/// one, so a test that already owns its library directory (a cloned fixture
/// template, a `db/` subdirectory next to an `album/`) uses it too.
pub async fn open_test_library(
    dir: &std::path::Path,
) -> (
    crate::library::LibraryManager,
    bae_db::Database,
    bae_import::CatalogSessions,
    crate::library::CancellationToken,
) {
    open_test_library_with(dir, bae_catalogs::providers::Providers::offline()).await
}

/// [`open_test_library`] asking `providers` — ones routed to a test's own
/// fakes — rather than providers that reach nothing.
pub async fn open_test_library_with(
    dir: &std::path::Path,
    providers: bae_catalogs::providers::Providers,
) -> (
    crate::library::LibraryManager,
    bae_db::Database,
    bae_import::CatalogSessions,
    crate::library::CancellationToken,
) {
    let database = open_test_db(dir).await;
    open_library(database, bae_app_dir::AppDir::under_home(dir).unwrap(), providers).await
}

/// A library over an installation the test created with its own storage and
/// custody.
pub async fn test_library_from_handle(
    handle: coven::CovenHandle,
    dir: &std::path::Path,
    providers: bae_catalogs::providers::Providers,
) -> (
    crate::library::LibraryManager,
    bae_db::Database,
    bae_import::CatalogSessions,
    crate::library::CancellationToken,
) {
    open_library(
        bae_db::Database::from_handle(
            handle,
            std::sync::Arc::new(coven::SystemClock),
            std::sync::Arc::new(coven::UuidIds),
        ),
        bae_app_dir::AppDir::under_home(dir).unwrap(),
        providers,
    )
    .await
}

/// Open the library over `database` as the app does: the catalog sessions
/// over the same store and the closing token are built here, as the bootstrap
/// builds them, and passed to the app's opening step.
async fn open_library(
    database: bae_db::Database,
    app_dir: bae_app_dir::AppDir,
    providers: bae_catalogs::providers::Providers,
) -> (
    crate::library::LibraryManager,
    bae_db::Database,
    bae_import::CatalogSessions,
    crate::library::CancellationToken,
) {
    let remote_images =
        bae_catalogs::remote_image::RemoteImageCache::for_test(providers.http().clone());
    let catalogs = bae_import::CatalogSessions::new(
        providers,
        database.clone(),
        bae_diagnostics::Diagnostics::noop(),
        tokio::runtime::Handle::current(),
    );
    let closing = crate::library::CancellationToken::new();
    let manager = crate::app::open_library_manager(
        database.clone(),
        catalogs.clone(),
        closing.clone(),
        app_dir,
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidIds),
        bae_diagnostics::Diagnostics::noop(),
        tokio::runtime::Handle::current(),
        remote_images,
    )
    .await
    .expect("open the test library");
    (manager, database, catalogs, closing)
}

/// Start the services of a library [`open_test_library`] opened, through the
/// app's own step, with playback on a device that plays nothing, no stored
/// queue restored, and image downloads that reach nothing.
pub async fn start_test_services(
    manager: crate::library::LibraryManager,
    database: bae_db::Database,
    catalogs: bae_import::CatalogSessions,
    closing: crate::library::CancellationToken,
) -> crate::library::AppServices {
    let remote_images =
        bae_catalogs::remote_image::RemoteImageCache::for_test(bae_util::http::Http::for_test());
    crate::app::start_library_services(
        manager,
        database,
        catalogs,
        closing,
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidIds),
        bae_diagnostics::Diagnostics::noop(),
        tokio::runtime::Handle::current(),
        remote_images,
        50,
        false,
        bae_playback::test_capture::wall_playback_clock(),
        crate::app::PlaybackOutput::Device(Box::new(
            bae_playback::audio_output::FailingAudioDevice,
        )),
    )
    .await
    .expect("the test library's services start")
}

/// [`open_test_library`] over a fresh temp directory. The returned `TempDir`
/// owns the database file, so it must outlive the manager.
pub async fn setup_test_library() -> (
    (
        crate::library::LibraryManager,
        bae_db::Database,
        bae_import::CatalogSessions,
        crate::library::CancellationToken,
    ),
    tempfile::TempDir,
) {
    setup_test_library_with(bae_catalogs::providers::Providers::offline()).await
}

/// [`setup_test_library`] asking `providers`, ones built over the test's own
/// fakes.
pub async fn setup_test_library_with(
    providers: bae_catalogs::providers::Providers,
) -> (
    (
        crate::library::LibraryManager,
        bae_db::Database,
        bae_import::CatalogSessions,
        crate::library::CancellationToken,
    ),
    tempfile::TempDir,
) {
    bae_util::test_logs::install();
    let temp_dir = tempfile::TempDir::new().expect("test library temp dir");
    let library = open_test_library_with(temp_dir.path(), providers).await;
    (library, temp_dir)
}

/// [`setup_test_library_with`] with an `album/` directory made alongside it,
/// for a test that writes audio into a folder and imports that folder. The
/// `TempDir` owns both directories, so it must outlive the manager.
pub async fn setup_test_library_with_album_dir(
    providers: bae_catalogs::providers::Providers,
) -> (
    (
        crate::library::LibraryManager,
        bae_db::Database,
        bae_import::CatalogSessions,
        crate::library::CancellationToken,
    ),
    std::path::PathBuf,
    tempfile::TempDir,
) {
    let (library, temp_dir) = setup_test_library_with(providers).await;
    let album_dir = temp_dir.path().join("album");
    std::fs::create_dir_all(&album_dir).expect("test album dir");
    (library, album_dir, temp_dir)
}

/// Set up a fresh library through the production creation and open paths.
/// Sync is not set up: tests configure it themselves via
/// connect_*/save_s3_config.
pub fn setup_fresh_library(
    runtime: &tokio::runtime::Runtime,
) -> (
    (
        crate::library::LibraryManager,
        bae_db::Database,
        bae_import::CatalogSessions,
        crate::library::CancellationToken,
    ),
    tempfile::TempDir,
) {
    let tmp = tempfile::TempDir::new().unwrap();
    let app_dir = bae_app_dir::AppDir::under_home(tmp.path()).unwrap();
    let installation = coven::TestCoven::new();
    let library = runtime.block_on(async {
        let handle = bae_db::configure_store(installation.builder(app_dir.store_layout()))
            .create_store("Test Library")
            .await
            .expect("create fresh test store");
        let database = bae_db::Database::from_handle(
            handle,
            std::sync::Arc::new(coven::SystemClock),
            std::sync::Arc::new(coven::UuidIds),
        );
        open_library(database, app_dir, bae_catalogs::providers::Providers::offline()).await
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

/// [`multi_thread_runtime`] with a fresh library's [`AppServices`] already built
/// on it. The `TempDir` owns the library's files, so it must outlive both.
///
/// [`AppServices`]: crate::library::AppServices
pub fn runtime_with_services(
) -> (tokio::runtime::Runtime, crate::library::AppServices, tempfile::TempDir) {
    let runtime = multi_thread_runtime();
    let ((manager, database, catalogs, closing), tmp) = setup_fresh_library(&runtime);
    let services = runtime.block_on(start_test_services(manager, database, catalogs, closing));
    (runtime, services, tmp)
}

/// The bytes of `release_id`'s stored cover, if it has one.
pub async fn read_cover_image_bytes(
    mgr: &crate::library::LibraryManager,
    release_id: &str,
) -> Option<Vec<u8>> {
    mgr.read_cover_image_bytes(release_id).await.unwrap()
}

/// What an import landed, beside the library it landed in. Holds no service of
/// its own, so a fixture can keep it whole while handing the library's parts
/// to whatever it starts.
pub struct ImportedRelease {
    pub track_ids: Vec<String>,
    pub release_id: String,
    /// The folder the audio was written into, for a test that reads a real file.
    pub album_dir: std::path::PathBuf,
    /// Owns the library's files, so it must outlive the manager.
    pub temp_dir: tempfile::TempDir,
}

/// Import one Discogs-identified release from a folder, on the calling test's
/// runtime, and return the library beside what it landed: open a fresh library, seed `release` for the fake Discogs
/// endpoint, write the audio with `generate_files`, run the import to
/// completion, and read back the tracks it landed. The arrange every playback
/// test binary starts from.
pub async fn imported_release_setup<G>(
    release: bae_catalogs::discogs::DiscogsRelease,
    candidate_key: &str,
    import_id: String,
    generate_files: G,
) -> Result<
    (
        (
            crate::library::LibraryManager,
            bae_db::Database,
            bae_import::CatalogSessions,
            crate::library::CancellationToken,
        ),
        ImportedRelease,
    ),
    Box<dyn std::error::Error>,
>
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
) -> Result<
    (
        (
            crate::library::LibraryManager,
            bae_db::Database,
            bae_import::CatalogSessions,
            crate::library::CancellationToken,
        ),
        ImportedRelease,
    ),
    Box<dyn std::error::Error>,
>
where
    G: FnOnce(&std::path::Path),
{
    let ((library_manager, database, catalogs, closing), album_dir, temp_dir) =
        setup_test_library_with_album_dir(fakes.providers()).await;

    let release_id_key = fakes.seed_discogs_test_release(release);
    generate_files(&album_dir);

    let (release_id, _album_id) = bae_import::test_imports::send_folder_import_and_wait(
        &database,
        &catalogs,
        &closing,
        album_dir.clone(),
        release_id_key,
        candidate_key,
        import_id,
    )
    .await?;
    let tracks = library_manager.get_tracks_for_release(&release_id).await?;
    let track_ids: Vec<String> = tracks.iter().map(|t| t.id.clone()).collect();

    Ok((
        (library_manager, database, catalogs, closing),
        ImportedRelease { track_ids, release_id, album_dir, temp_dir },
    ))
}
