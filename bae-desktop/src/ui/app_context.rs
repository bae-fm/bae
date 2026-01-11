use bae_core::cache;
use bae_core::config;
use bae_core::encryption;
use bae_core::import;
use bae_core::library::SharedLibraryManager;
use bae_core::playback;
use bae_core::torrent;
use dioxus::prelude::*;

#[derive(Clone)]
pub struct AppContext {
    pub library_manager: SharedLibraryManager,
    pub config: config::Config,
    pub import_handle: import::ImportServiceHandle,
    pub playback_handle: playback::PlaybackHandle,
    pub cache: cache::CacheManager,
    pub encryption_service: Option<encryption::EncryptionService>,
    pub torrent_manager: torrent::LazyTorrentManager,
}

/// Hook to access the shared library manager from components
pub fn use_library_manager() -> SharedLibraryManager {
    let context = use_context::<AppContext>();
    context.library_manager
}

/// Hook to access the import service handle from components
pub fn use_import_service() -> import::ImportServiceHandle {
    let context = use_context::<AppContext>();
    context.import_handle
}

/// Hook to access the config from components
pub fn use_config() -> config::Config {
    use_context::<AppContext>().config
}
