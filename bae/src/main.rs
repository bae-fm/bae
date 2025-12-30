#[link(name = "bae_storage", kind = "static")]
#[link(name = "torrent-rasterbar")]
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
mod import;
mod library;
mod media_controls;
mod musicbrainz;
mod network;
mod playback;
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
/// Initialize cloud storage from config (optional - app works without it)
async fn create_cloud_storage_manager(
    config: &config::Config,
) -> cloud_storage::CloudStorageManager {
    info!("Initializing cloud storage...");
    cloud_storage::CloudStorageManager::new_optional(config.s3_config.clone()).await
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
fn create_library_manager(
    database: Database,
    cloud_storage: cloud_storage::CloudStorageManager,
) -> SharedLibraryManager {
    let library_manager = library::LibraryManager::new(database, cloud_storage);
    info!("Library manager created");
    let shared_library = SharedLibraryManager::new(library_manager);
    info!("SharedLibraryManager created");
    shared_library
}
/// Ensure a default storage profile exists
///
/// Creates a default storage profile if none exists:
/// - If cloud storage is available: creates "Cloud Storage" profile (encrypted + chunked + cloud)
/// - If cloud storage is unavailable: skips creation, user can set up storage in settings
async fn ensure_default_storage_profile(
    library_manager: &SharedLibraryManager,
    config: &config::Config,
    cloud_storage: &cloud_storage::CloudStorageManager,
) {
    let manager = library_manager.get();
    match manager.get_default_storage_profile().await {
        Ok(Some(profile)) => {
            info!("Default storage profile exists: {}", profile.name);
        }
        Ok(None) => {
            if cloud_storage.is_available() {
                info!("Creating default cloud storage profile...");
                let profile = db::DbStorageProfile::new_cloud(
                    "Cloud Storage",
                    &config.s3_config.bucket_name,
                    &config.s3_config.region,
                    config.s3_config.endpoint_url.as_deref(),
                    &config.s3_config.access_key_id,
                    &config.s3_config.secret_access_key,
                    true,
                    true,
                )
                .with_default(true);
                match manager.insert_storage_profile(&profile).await {
                    Ok(()) => {
                        if let Err(e) = manager.set_default_storage_profile(&profile.id).await {
                            error!("Failed to set default storage profile: {}", e);
                        } else {
                            info!("Created default storage profile: Cloud Storage");
                        }
                    }
                    Err(e) => {
                        error!("Failed to create default storage profile: {}", e);
                    }
                }
            } else {
                info!("No default storage profile. Cloud storage unavailable - user can configure storage in settings.");
            }
        }
        Err(e) => {
            error!("Failed to check for default storage profile: {}", e);
        }
    }
}
fn configure_logging() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_line_number(true)
        .with_target(false)
        .with_file(true)
        .init();
}
fn main() {
    let config = config::Config::load();
    configure_logging();
    let runtime = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
    let runtime_handle = runtime.handle().clone();
    info!("Building dependencies...");
    let cache_manager = runtime_handle.block_on(create_cache_manager());
    let cloud_storage = runtime_handle.block_on(create_cloud_storage_manager(&config));
    let database = runtime_handle.block_on(create_database(&config));
    let library_manager = create_library_manager(database.clone(), cloud_storage.clone());
    runtime_handle.block_on(ensure_default_storage_profile(
        &library_manager,
        &config,
        &cloud_storage,
    ));
    let encryption_service = encryption::EncryptionService::new(&config).expect(
        "Failed to initialize encryption service. Check your encryption key configuration.",
    );
    let import_config = import::ImportConfig {
        max_encrypt_workers: config.max_import_encrypt_workers,
        max_upload_workers: config.max_import_upload_workers,
        max_db_write_workers: config.max_import_db_write_workers,
        chunk_size_bytes: config.chunk_size_bytes,
    };
    let torrent_options = torrent_options_from_config(&config);
    let torrent_manager = torrent::start_torrent_manager(
        cache_manager.clone(),
        database.clone(),
        config.chunk_size_bytes,
        torrent_options,
    );
    let import_handle = import::ImportService::start(
        import_config,
        runtime_handle.clone(),
        library_manager.clone(),
        encryption_service.clone(),
        cloud_storage.clone(),
        torrent_manager.clone(),
        std::sync::Arc::new(database.clone()),
    );
    let playback_handle = playback::PlaybackService::start(
        library_manager.get().clone(),
        cloud_storage.clone(),
        cache_manager.clone(),
        encryption_service.clone(),
        config.chunk_size_bytes,
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
        cloud_storage: cloud_storage.clone(),
    };
    runtime_handle.spawn(async move {
        start_subsonic_server(
            cache_manager,
            library_manager,
            encryption_service,
            cloud_storage,
            config.chunk_size_bytes,
        )
        .await
    });
    info!("Starting UI");
    ui::launch_app(ui_context);
    info!("UI quit");
}
/// Start the Subsonic API server
async fn start_subsonic_server(
    cache_manager: cache::CacheManager,
    library_manager: SharedLibraryManager,
    encryption_service: encryption::EncryptionService,
    cloud_storage: cloud_storage::CloudStorageManager,
    chunk_size_bytes: usize,
) {
    info!("Starting Subsonic API server...");
    let app = create_router(
        library_manager,
        cache_manager,
        encryption_service,
        cloud_storage,
        chunk_size_bytes,
    );
    let listener = match tokio::net::TcpListener::bind("127.0.0.1:4533").await {
        Ok(listener) => {
            info!("Subsonic API server listening on http://127.0.0.1:4533");
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
///
/// If the configured interface doesn't exist, falls back to default binding.
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
    torrent::client::TorrentClientOptions { bind_interface }
}
