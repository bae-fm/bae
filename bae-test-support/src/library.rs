//! Test libraries: a database, a `LibraryManager` over it, and the runtimes
//! and ids the tests around them need.

pub fn tracing_init() {
    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_line_number(true)
        .with_target(false)
        .with_file(true)
        .try_init();
}

/// Create a test config handle for integration tests.
pub fn test_config(
    library_dir: &coven::StoreDir,
) -> std::sync::Arc<bae_core::config::ConfigHandle> {
    bae_core::config::install_test_keyring();
    crate::discogs::point_discogs_at_dead_port();
    // Unique id per test so keyring entries don't collide in the shared
    // process-global mock store (see `install_test_keyring`).
    let library_id = format!("test-{}", uuid::Uuid::new_v4());
    let config = bae_core::config::Config::with_defaults(
        library_id,
        "test-device".to_string(),
        library_dir,
        "Test Library".to_string(),
    );
    std::sync::Arc::new(bae_core::config::ConfigHandle::new(config))
}

/// Open `dir/test.db` under the real clock and real UUID provider.
async fn open_test_db(dir: &std::path::Path) -> bae_core::db::Database {
    bae_core::db::Database::new_test(
        dir.join("test.db")
            .to_str()
            .expect("test database path is valid UTF-8"),
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
    )
    .await
    .expect("open the test database")
}

/// A database in a fresh temp directory, for tests that assert on rows and need
/// no library manager around them. The returned `TempDir` owns the file.
pub async fn temp_test_db() -> (bae_core::db::Database, tempfile::TempDir) {
    let temp_dir = tempfile::TempDir::new().expect("test database temp dir");
    let database = open_test_db(temp_dir.path()).await;
    (database, temp_dir)
}

/// Open `dir/test.db` and build a [`LibraryManager`] over it, with `dir` as the
/// library's store dir.
///
/// The database-only constructor, not the production `open` path — the same one
/// every test binary was assembling by hand. Takes the directory rather than
/// making one, so a test that already owns its library directory (a cloned
/// fixture template, a `db/` subdirectory next to an `album/`) uses it too.
///
/// [`LibraryManager`]: bae_core::library::LibraryManager
pub async fn open_test_library(
    dir: &std::path::Path,
) -> (bae_core::library::LibraryManager, bae_core::db::Database) {
    let database = open_test_db(dir).await;
    let config_handle = test_config(&coven::StoreDir::new(dir.to_path_buf()));
    let library_manager = bae_core::library::LibraryManager::new(
        database.clone(),
        config_handle,
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
        bae_core::diagnostics::Diagnostics::noop(),
        tokio::runtime::Handle::current(),
        bae_core::import::cover_art::RemoteImageCache::for_test(),
    );
    (library_manager, database)
}

/// [`open_test_library`] over a fresh temp directory. The returned `TempDir`
/// owns the database file, so it must outlive the manager.
pub async fn setup_test_library() -> (
    bae_core::library::LibraryManager,
    bae_core::db::Database,
    tempfile::TempDir,
) {
    tracing_init();
    let temp_dir = tempfile::TempDir::new().expect("test library temp dir");
    let (library_manager, database) = open_test_library(temp_dir.path()).await;
    (library_manager, database, temp_dir)
}

/// [`setup_test_library`] with an `album/` directory made alongside it, for a
/// test that writes audio into a folder and imports that folder. Returns
/// (manager, album_dir, temp_dir); the `TempDir` owns both, so it must outlive
/// the manager.
pub async fn setup_test_library_with_album_dir() -> (
    bae_core::library::LibraryManager,
    std::path::PathBuf,
    tempfile::TempDir,
) {
    let (library_manager, _database, temp_dir) = setup_test_library().await;
    let album_dir = temp_dir.path().join("album");
    std::fs::create_dir_all(&album_dir).expect("test album dir");
    (library_manager, album_dir, temp_dir)
}

/// Set up a fresh library + LibraryManager through the production creation and
/// open paths. No sync manager — tests configure sync themselves via
/// connect_*/save_s3_config.
pub fn setup_fresh_library(
    runtime: &tokio::runtime::Runtime,
) -> (bae_core::library::LibraryManager, tempfile::TempDir) {
    let tmp = tempfile::TempDir::new().unwrap();
    bae_core::config::install_test_keyring();
    let config = bae_core::library::create_library_in_bae_dir_for_test(
        tmp.path(),
        bae_core::library_name::LibraryName::parse("Test Library").unwrap(),
        &coven::UuidProvider,
    )
    .expect("create fresh library");
    let config_handle = std::sync::Arc::new(bae_core::config::ConfigHandle::new(config));
    let lm = bae_core::library::LibraryManager::open(
        config_handle,
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
        bae_core::diagnostics::Diagnostics::noop(),
        runtime.handle().clone(),
        None,
        bae_core::import::cover_art::RemoteImageCache::for_test(),
    )
    .expect("open library manager");

    (lm, tmp)
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
pub fn runtime_with_services() -> (
    tokio::runtime::Runtime,
    bae_core::library::AppServices,
    tempfile::TempDir,
) {
    let runtime = multi_thread_runtime();
    let (manager, tmp) = setup_fresh_library(&runtime);
    let services = runtime
        .block_on(bae_core::library::AppServices::for_test(manager))
        .expect("app services");
    (runtime, services, tmp)
}

/// A stable v4 UUID for a fixture moniker.
///
/// coven validates every synced row's primary key as a canonical RFC 4122 v4
/// UUID (`RowIdentity::IndependentUuid`) — which is what bae's real ids are, so
/// fixtures must carry UUIDs too. Tests that name a row by a readable moniker
/// (`"test-artist-id"`) or mint one per index (`format!("track-{i}")`) get their
/// id through this: the moniker stays visible at the call site, and the same
/// moniker always maps to the same id within and across runs.
pub fn test_uuid(moniker: &str) -> String {
    // FNV-1a over the moniker, run under four seeds for the 128 bits a UUID
    // needs. Any stable spread works — the value only has to be a well-formed
    // v4 UUID and collision-free across a test's handful of monikers.
    let word = |seed: u64| -> u64 {
        let mut hash = seed;
        for byte in moniker.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash
    };
    let (high, low) = (word(0xcbf2_9ce4_8422_2325), word(0x9e37_79b9_7f4a_7c15));
    format!(
        "{:08x}-{:04x}-4{:03x}-8{:03x}-{:012x}",
        (high >> 32) as u32,
        (high >> 16) as u16,
        high & 0x0fff,
        (low >> 48) & 0x0fff,
        low & 0xffff_ffff_ffff_u64,
    )
}
