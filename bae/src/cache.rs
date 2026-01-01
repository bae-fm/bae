use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;
use tokio::fs;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
/// Errors that can occur during cache operations
#[derive(Error, Debug)]
pub enum CacheError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
/// Configuration for the cache manager
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Directory where cached files are stored
    pub cache_dir: PathBuf,
    /// Maximum cache size in bytes (default: 1GB)
    pub max_size_bytes: u64,
    /// Maximum number of cached files (default: 10,000)
    pub max_files: usize,
}
impl Default for CacheConfig {
    fn default() -> Self {
        let home_dir = dirs::home_dir().expect("Failed to get home directory");
        CacheConfig {
            cache_dir: home_dir.join(".bae").join("cache"),
            max_size_bytes: 1024 * 1024 * 1024,
            max_files: 10_000,
        }
    }
}
/// Metadata about a cached file
#[derive(Debug, Clone)]
struct CacheEntry {
    /// File path in cache
    file_path: PathBuf,
    /// Size in bytes
    size_bytes: u64,
    /// Last access time (for LRU)
    last_accessed: std::time::SystemTime,
}

/// LRU cache manager for downloaded files
#[derive(Clone)]
pub struct CacheManager {
    config: CacheConfig,
    /// In-memory index of cached files (cache_key -> CacheEntry)
    entries: Arc<RwLock<HashMap<String, CacheEntry>>>,
    /// Current cache size in bytes
    current_size: Arc<RwLock<u64>>,
    /// Set of pinned cache keys that should not be evicted
    pinned: Arc<RwLock<HashSet<String>>>,
}
impl CacheManager {
    /// Create a new cache manager with default configuration
    pub async fn new() -> Result<Self, CacheError> {
        Self::with_config(CacheConfig::default()).await
    }

    /// Create a new cache manager with custom configuration
    pub async fn with_config(config: CacheConfig) -> Result<Self, CacheError> {
        fs::create_dir_all(&config.cache_dir).await?;
        let cache_manager = CacheManager {
            config,
            entries: Arc::new(RwLock::new(HashMap::new())),
            current_size: Arc::new(RwLock::new(0)),
            pinned: Arc::new(RwLock::new(HashSet::new())),
        };
        cache_manager.load_existing_cache().await?;
        Ok(cache_manager)
    }

    /// Get a file from cache if it exists
    pub async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, CacheError> {
        let mut entries = self.entries.write().await;
        if let Some(entry) = entries.get_mut(key) {
            entry.last_accessed = std::time::SystemTime::now();
            match fs::read(&entry.file_path).await {
                Ok(data) => {
                    debug!("Cache hit for {}", key);
                    Ok(Some(data))
                }
                Err(e) => {
                    warn!("Cache entry corrupted for {}, removing: {}", key, e);
                    let mut current_size = self.current_size.write().await;
                    *current_size = current_size.saturating_sub(entry.size_bytes);
                    entries.remove(key);
                    Ok(None)
                }
            }
        } else {
            debug!("Cache miss for {}", key);
            Ok(None)
        }
    }

    /// Put a file into the cache
    pub async fn put(&self, key: &str, data: &[u8]) -> Result<(), CacheError> {
        let size = data.len() as u64;
        self.ensure_space_available(size).await?;
        let cache_file_path = self.config.cache_dir.join(format!("{}.enc", key));
        fs::write(&cache_file_path, data).await?;
        let entry = CacheEntry {
            file_path: cache_file_path,
            size_bytes: size,
            last_accessed: std::time::SystemTime::now(),
        };
        let mut entries = self.entries.write().await;
        let mut current_size = self.current_size.write().await;
        if let Some(old_entry) = entries.get(key) {
            *current_size = current_size.saturating_sub(old_entry.size_bytes);
        }
        entries.insert(key.to_string(), entry);
        *current_size += size;

        debug!(
            "Cached {} ({} bytes, total cache: {} bytes)",
            key, size, *current_size
        );
        Ok(())
    }

    /// Load existing cache entries from disk on startup
    async fn load_existing_cache(&self) -> Result<(), CacheError> {
        let mut entries = self.entries.write().await;
        let mut current_size = self.current_size.write().await;
        let mut dir_entries = fs::read_dir(&self.config.cache_dir).await?;
        while let Some(entry) = dir_entries.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("enc") {
                if let Some(file_stem) = path.file_stem().and_then(|s| s.to_str()) {
                    let key = file_stem.to_string();
                    match entry.metadata().await {
                        Ok(metadata) => {
                            let cache_entry = CacheEntry {
                                file_path: path,
                                size_bytes: metadata.len(),
                                last_accessed: metadata
                                    .accessed()
                                    .unwrap_or(std::time::SystemTime::now()),
                            };
                            *current_size += cache_entry.size_bytes;
                            entries.insert(key, cache_entry);
                        }
                        Err(e) => {
                            warn!(
                                "Failed to read metadata for cache file {}: {}",
                                path.display(),
                                e
                            );
                        }
                    }
                }
            }
        }

        info!(
            "Loaded {} existing cache entries ({} bytes)",
            entries.len(),
            *current_size
        );
        Ok(())
    }

    /// Ensure there's enough space for a new file, evicting old files if necessary
    async fn ensure_space_available(&self, needed_bytes: u64) -> Result<(), CacheError> {
        let mut entries = self.entries.write().await;
        let mut current_size = self.current_size.write().await;
        while *current_size + needed_bytes > self.config.max_size_bytes && !entries.is_empty() {
            self.evict_lru(&mut entries, &mut current_size).await?;
        }
        while entries.len() >= self.config.max_files && !entries.is_empty() {
            self.evict_lru(&mut entries, &mut current_size).await?;
        }
        Ok(())
    }

    /// Evict the least recently used entry
    async fn evict_lru(
        &self,
        entries: &mut HashMap<String, CacheEntry>,
        current_size: &mut u64,
    ) -> Result<(), CacheError> {
        let pinned = self.pinned.read().await;
        let lru_key = entries
            .iter()
            .filter(|(id, _)| !pinned.contains(*id))
            .min_by_key(|(_, entry)| entry.last_accessed)
            .map(|(id, _)| id.clone());
        if let Some(key) = lru_key {
            if let Some(entry) = entries.remove(&key) {
                if let Err(e) = fs::remove_file(&entry.file_path).await {
                    warn!(
                        "Failed to remove evicted cache file {}: {}",
                        entry.file_path.display(),
                        e
                    );
                }
                *current_size = current_size.saturating_sub(entry.size_bytes);

                debug!("Evicted {} ({} bytes)", key, entry.size_bytes);
            }
        }
        Ok(())
    }

    /// Pin a cache entry to prevent it from being evicted
    pub async fn pin(&self, key: &str) {
        let mut pinned = self.pinned.write().await;
        pinned.insert(key.to_string());
    }

    /// Unpin a cache entry, allowing it to be evicted again
    pub async fn unpin(&self, key: &str) {
        let mut pinned = self.pinned.write().await;
        pinned.remove(key);
    }

    /// Pin multiple cache entries at once
    pub async fn pin_all(&self, keys: &[String]) {
        let mut pinned = self.pinned.write().await;
        for key in keys {
            pinned.insert(key.clone());
        }
    }

    /// Unpin multiple cache entries at once
    pub async fn unpin_all(&self, keys: &[String]) {
        let mut pinned = self.pinned.write().await;
        for key in keys {
            pinned.remove(key);
        }
    }
}
