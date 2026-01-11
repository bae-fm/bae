use crate::cache::CacheManager;
use crate::db::Database;
use crate::torrent::client::TorrentClientOptions;
use crate::torrent::manager::{
    start_torrent_manager, start_torrent_manager_noop, TorrentManagerHandle,
};
use std::sync::{Arc, OnceLock};
use tracing::info;

/// Lazy-initialized torrent manager that only starts the libtorrent session
/// when first accessed. This defers the local network permission prompt until
/// the user actually needs torrent functionality.
#[derive(Clone)]
pub struct LazyTorrentManager {
    inner: Arc<LazyTorrentManagerInner>,
}

enum LazyTorrentManagerInner {
    /// Lazy initialization with real torrent functionality
    Lazy {
        handle: OnceLock<TorrentManagerHandle>,
        cache_manager: CacheManager,
        database: Database,
        options: TorrentClientOptions,
    },
    /// Pre-initialized noop manager (for screenshot mode)
    Noop(TorrentManagerHandle),
}

impl LazyTorrentManager {
    /// Create a new lazy torrent manager with the given dependencies.
    /// Does NOT start the libtorrent session yet.
    pub fn new(
        cache_manager: CacheManager,
        database: Database,
        options: TorrentClientOptions,
    ) -> Self {
        Self {
            inner: Arc::new(LazyTorrentManagerInner::Lazy {
                handle: OnceLock::new(),
                cache_manager,
                database,
                options,
            }),
        }
    }

    /// Create a noop torrent manager that doesn't actually connect to any network.
    /// Used for screenshot mode where we don't want network permission prompts.
    pub fn new_noop(runtime_handle: tokio::runtime::Handle) -> Self {
        Self {
            inner: Arc::new(LazyTorrentManagerInner::Noop(start_torrent_manager_noop(
                runtime_handle,
            ))),
        }
    }

    /// Get the torrent manager handle, initializing it on first access.
    /// This is when the local network permission prompt will appear.
    pub fn get(&self) -> &TorrentManagerHandle {
        match &*self.inner {
            LazyTorrentManagerInner::Lazy {
                handle,
                cache_manager,
                database,
                options,
                ..
            } => handle.get_or_init(|| {
                info!("Initializing torrent manager (first access)...");
                start_torrent_manager(cache_manager.clone(), database.clone(), options.clone())
            }),
            LazyTorrentManagerInner::Noop(handle) => handle,
        }
    }
}
