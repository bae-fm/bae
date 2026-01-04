#[link(name = "bae_storage", kind = "static")]
#[link(name = "torrent-rasterbar")]
#[link(name = "sodium")]
extern "C" {}
use crate::db::Database;
use tracing::{error, info, warn};
mod cache;
mod cd;
mod cloud_storage;
mod config;
mod cue_flac;
mod db;
mod discogs;
mod encryption;
#[cfg(feature = "demo")]
mod fixtures;
mod flac_decoder;
mod flac_encoder;
mod flac_frame_scanner;
mod import;
mod library;
mod media_controls;
mod musicbrainz;
mod network;
mod playback;
mod sodium_ffi;
mod storage;
mod subsonic;
mod test_support;
mod torrent;
mod ui;
use library::SharedLibraryManager;
use subsonic::create_router;
/// Root application context containing all top-level dependencies
pub use ui::AppContext;

/// Initialize cache manager
async fn create_cache_manager() -> cache::CacheManager {
    let cache_manager = cache::CacheManager::new()
        .await
        .expect("Failed to create cache manager");
    info!("Cache manager created");
    cache_manager
}

/// Initialize database
async fn create_database(config: &config::Config) -> Database {
    let library_path = config.get_library_path();
    info!("Creating library directory: {}", library_path.display());
    std::fs::create_dir_all(&library_path).expect("Failed to create library directory");
    let db_path = library_path.join("library.db");
    info!("Initializing database at: {}", db_path.display());
    let database = Database::new(db_path.to_str().unwrap())
        .await
        .expect("Failed to create database");
    info!("Database created");
    database
}

/// Initialize library manager with all dependencies
fn create_library_manager(database: Database) -> SharedLibraryManager {
    let library_manager = library::LibraryManager::new(database);
    info!("Library manager created");
    let shared_library = SharedLibraryManager::new(library_manager);
    info!("SharedLibraryManager created");
    shared_library
}

fn configure_logging() {
    use tracing_subscriber::prelude::*;

    // Default to info level if RUST_LOG not set
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_line_number(true)
        .with_target(false)
        .with_file(true);

    // Always log to console. In release mode, also log to macOS Console.app.
    if config::Config::is_dev_mode() {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .init();
    } else {
        let oslog_layer = tracing_oslog::OsLogger::new("com.bae.app", "default");

        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .with(oslog_layer)
            .init();
    }
}

fn main() {
    let config = config::Config::load();
    configure_logging();
    let screenshot_mode = std::env::var("BAE_SCREENSHOT_MODE").is_ok();
    if screenshot_mode {
        info!("Screenshot mode enabled - disabling network features");
    }
    let runtime = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
    let runtime_handle = runtime.handle().clone();

    info!("Building dependencies...");
    let cache_manager = runtime_handle.block_on(create_cache_manager());
    let database = runtime_handle.block_on(create_database(&config));
    let library_manager = create_library_manager(database.clone());

    let encryption_service = encryption::EncryptionService::new(&config).expect(
        "Failed to initialize encryption service. Check your encryption key configuration.",
    );

    let torrent_manager = if screenshot_mode {
        torrent::LazyTorrentManager::new_noop(runtime_handle.clone())
    } else {
        let torrent_options = torrent_options_from_config(&config);
        torrent::LazyTorrentManager::new(cache_manager.clone(), database.clone(), torrent_options)
    };

    let import_handle = import::ImportService::start(
        runtime_handle.clone(),
        library_manager.clone(),
        encryption_service.clone(),
        torrent_manager.clone(),
        std::sync::Arc::new(database.clone()),
    );

    let playback_handle = playback::PlaybackService::start(
        library_manager.get().clone(),
        encryption_service.clone(),
        runtime_handle.clone(),
    );

    let media_controls = match media_controls::setup_media_controls(
        playback_handle.clone(),
        library_manager.clone(),
        runtime_handle.clone(),
    ) {
        Ok(controls) => {
            info!("Media controls setup successful");
            Some(controls)
        }
        Err(e) => {
            error!("Failed to setup media controls: {:?}", e);
            error!("Media key support will not be available");
            None
        }
    };
    let _keep_alive = media_controls;

    let ui_context = AppContext {
        library_manager: library_manager.clone(),
        config: config.clone(),
        import_handle,
        playback_handle,
        torrent_manager,
        cache: cache_manager.clone(),
        encryption_service: encryption_service.clone(),
    };

    if !screenshot_mode && config.subsonic_enabled {
        let subsonic_library = library_manager.clone();
        let subsonic_encryption = encryption_service.clone();
        let subsonic_port = config.subsonic_port;
        runtime_handle.spawn(async move {
            start_subsonic_server(subsonic_library, subsonic_encryption, subsonic_port).await
        });
    }

    info!("Starting UI");
    ui::launch_app(ui_context);
    info!("UI quit");
}

/// Start the Subsonic API server
async fn start_subsonic_server(
    library_manager: SharedLibraryManager,
    encryption_service: encryption::EncryptionService,
    port: u16,
) {
    info!("Starting Subsonic API server...");
    let app = create_router(library_manager, encryption_service);
    let addr = format!("127.0.0.1:{}", port);
    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(listener) => {
            info!("Subsonic API server listening on http://{}", addr);
            listener
        }
        Err(e) => {
            error!("Failed to bind Subsonic server: {}", e);
            return;
        }
    };
    if let Err(e) = axum::serve(listener, app).await {
        error!("Subsonic server error: {}", e);
    }
}

/// Create torrent client options from application config
fn torrent_options_from_config(config: &config::Config) -> torrent::client::TorrentClientOptions {
    let bind_interface = if let Some(interface) = &config.torrent_bind_interface {
        match network::validate_network_interface(interface) {
            Ok(()) => {
                info!(
                    "Torrent client configured to bind to interface: {}",
                    interface
                );
                Some(interface.clone())
            }
            Err(e) => {
                warn!(
                    "Configured torrent interface '{}' not available: {}. Using default binding.",
                    interface, e
                );
                None
            }
        }
    } else {
        info!("Torrent client using default network binding");
        None
    };
    torrent::client::TorrentClientOptions {
        bind_interface,
        listen_port: config.torrent_listen_port,
        enable_upnp: config.torrent_enable_upnp,
        enable_natpmp: config.torrent_enable_natpmp,
        max_connections: config.torrent_max_connections,
        max_uploads: config.torrent_max_uploads,
    }
}
