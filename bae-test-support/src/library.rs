//! Test libraries: a database, a library opened over it as the app opens one,
//! and the runtimes and ids the tests around them need.

/// Install the test log subscriber; the first test in the process to call
/// this installs it, and every later call finds it there.
pub fn tracing_init() {
    #[expect(
        clippy::let_underscore_must_use,
        reason = "only the first test in the process installs the subscriber; later calls find it installed"
    )]
    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_line_number(true)
        .with_target(false)
        .with_file(true)
        .try_init();
}

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
    bae_core::library::LibraryManager,
    bae_db::Database,
    bae_core::import::CatalogSessions,
    bae_core::library::CancellationToken,
) {
    open_test_library_with(dir, bae_catalogs::providers::Providers::offline()).await
}

/// [`open_test_library`] asking `providers` — ones routed to a test's own
/// fakes — rather than providers that reach nothing.
pub async fn open_test_library_with(
    dir: &std::path::Path,
    providers: bae_catalogs::providers::Providers,
) -> (
    bae_core::library::LibraryManager,
    bae_db::Database,
    bae_core::import::CatalogSessions,
    bae_core::library::CancellationToken,
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
    bae_core::library::LibraryManager,
    bae_db::Database,
    bae_core::import::CatalogSessions,
    bae_core::library::CancellationToken,
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
    bae_core::library::LibraryManager,
    bae_db::Database,
    bae_core::import::CatalogSessions,
    bae_core::library::CancellationToken,
) {
    let remote_images =
        bae_catalogs::remote_image::RemoteImageCache::for_test(providers.http().clone());
    let catalogs = bae_core::import::CatalogSessions::new(
        providers,
        database.clone(),
        bae_diagnostics::Diagnostics::noop(),
        tokio::runtime::Handle::current(),
    );
    let closing = bae_core::library::CancellationToken::new();
    let manager = bae_core::app::open_library_manager(
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
    manager: bae_core::library::LibraryManager,
    database: bae_db::Database,
    catalogs: bae_core::import::CatalogSessions,
    closing: bae_core::library::CancellationToken,
) -> bae_core::library::AppServices {
    let remote_images =
        bae_catalogs::remote_image::RemoteImageCache::for_test(bae_util::http::Http::for_test());
    bae_core::app::start_library_services(
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
        crate::wall_playback_clock(),
        bae_core::app::PlaybackOutput::Device(Box::new(
            bae_core::playback::audio_output::FailingAudioDevice,
        )),
    )
    .await
    .expect("the test library's services start")
}

/// [`open_test_library`] over a fresh temp directory. The returned `TempDir`
/// owns the database file, so it must outlive the manager.
pub async fn setup_test_library() -> (
    (
        bae_core::library::LibraryManager,
        bae_db::Database,
        bae_core::import::CatalogSessions,
        bae_core::library::CancellationToken,
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
        bae_core::library::LibraryManager,
        bae_db::Database,
        bae_core::import::CatalogSessions,
        bae_core::library::CancellationToken,
    ),
    tempfile::TempDir,
) {
    tracing_init();
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
        bae_core::library::LibraryManager,
        bae_db::Database,
        bae_core::import::CatalogSessions,
        bae_core::library::CancellationToken,
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
        bae_core::library::LibraryManager,
        bae_db::Database,
        bae_core::import::CatalogSessions,
        bae_core::library::CancellationToken,
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
/// [`AppServices`]: bae_core::library::AppServices
pub fn runtime_with_services(
) -> (tokio::runtime::Runtime, bae_core::library::AppServices, tempfile::TempDir) {
    let runtime = multi_thread_runtime();
    let ((manager, database, catalogs, closing), tmp) = setup_fresh_library(&runtime);
    let services = runtime.block_on(start_test_services(manager, database, catalogs, closing));
    (runtime, services, tmp)
}
