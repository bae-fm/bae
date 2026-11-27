//! What this device keeps for the open library, in bytes on disk: its pinned
//! releases, its cache, the library itself and what a reset left behind. The
//! read is one live query that sums coven's byte tables and the tables of the
//! catalog image cache and the tile cache; the commands clear the cache, set how much audio it keeps,
//! and delete what a reset kept.

use super::*;
use bae_records::CacheLimit;
use bae_store::DbError;

/// What a device keeps for a library, as Settings → Storage shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceStorage {
    /// Every part's bytes together.
    pub total: u64,
    /// The kinds that show, in the fixed order Pinned, Cache, Library, From
    /// before the reset: Pinned and Cache while they hold bytes, Library
    /// always, From before the reset while an archive is here.
    pub parts: Vec<StoragePart>,
    /// The cache's bytes, which Clear Cache removes.
    pub cache: u64,
    /// Whether Clear Cache has anything to remove.
    pub can_clear_cache: bool,
    /// How much audio the cache keeps before the least recently played goes.
    pub cache_limit: CacheLimit,
    /// The bytes of the archives resets kept, while there is one.
    pub before_reset: Option<u64>,
}

/// One kind of what the device keeps, with its bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoragePart {
    pub kind: StorageKind,
    pub bytes: u64,
}

/// A kind of what a device keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageKind {
    /// Releases kept whole, and the pins on the way to being kept.
    Pinned,
    /// Tracks played from the cloud, covers and pictures: what downloads
    /// again when needed.
    Cache,
    /// The library's database, settings and keys.
    Library,
    /// The installation a reset kept.
    BeforeReset,
}

// The sums Settings → Storage shows, in bytes, from coven's byte tables, the
// catalog image cache's files and the tile cache's: the audio cache's budget,
// of the namespace `:audio`, NULL when coven has no row for it; the pinned
// whole copies with the partial downloads that stopped pins left; the cached
// chunks and headers with the two caches' files; the library's own files; the
// archives that resets kept, NULL while there is none; and every part's bytes
// together.
sqlive_compose::read!(
    DeviceStorageSums(audio: &'static str) {
        audio_budget: Option<u64>,
        pinned: u64,
        cache: u64,
        library: u64,
        before_reset: Option<u64>,
        total: u64
    },
    r#"
    WITH sums AS (
        SELECT
            (SELECT counted.budget FROM coven_cache_bytes AS counted
             WHERE counted.namespace = :audio) AS audio_budget,
            (SELECT COALESCE(SUM(counted.pinned + counted.pinning), 0)
             FROM coven_cache_bytes AS counted) AS pinned,
            (SELECT COALESCE(SUM(counted.cached), 0) FROM coven_cache_bytes AS counted)
                + (SELECT COALESCE(SUM(images.bytes), 0)
                   FROM @bae_catalogs::remote_image::cached_rows::CachedImageBytes AS images)
                + (SELECT COALESCE(SUM(tiles.bytes), 0)
                   FROM @bae_pictures::tile_cache::CachedTileBytes AS tiles) AS cache,
            (SELECT COALESCE(SUM(stored.bytes), 0)
             FROM coven_library_bytes AS stored) AS library,
            (SELECT SUM(archived.bytes)
             FROM coven_archive_bytes AS archived) AS before_reset
    )
    SELECT sums.audio_budget, sums.pinned, sums.cache, sums.library, sums.before_reset,
           sums.pinned + sums.cache + sums.library + COALESCE(sums.before_reset, 0) AS total
    FROM sums
"#
);

/// What this device keeps for the library, as the page shows it.
fn device_storage_on(sql: coven::SqlReadContext<'_>) -> Result<DeviceStorage, DbError> {
    let params = DeviceStorageSumsParams { audio: bae_records::synced::RELEASE_FILES_NAMESPACE };
    DeviceStorage::from_sums(&bae_store::statement_row(&sql, &params)?)
}

impl DeviceStorage {
    /// The page's parts from its query's sums. Fails when the audio budget
    /// is one no choice names: no writer here sets another.
    pub(crate) fn from_sums(sums: &DeviceStorageSumsRow) -> Result<Self, DbError> {
        let cache_limit = sums
            .audio_budget
            .and_then(CacheLimit::from_bytes)
            .ok_or_else(|| DbError::unreadable("the audio cache budget", sums.audio_budget))?;
        let mut parts = Vec::new();
        if sums.pinned > 0 {
            parts.push(StoragePart { kind: StorageKind::Pinned, bytes: sums.pinned });
        }
        if sums.cache > 0 {
            parts.push(StoragePart { kind: StorageKind::Cache, bytes: sums.cache });
        }
        parts.push(StoragePart { kind: StorageKind::Library, bytes: sums.library });
        if let Some(kept) = sums.before_reset {
            parts.push(StoragePart { kind: StorageKind::BeforeReset, bytes: kept });
        }
        Ok(Self {
            total: sums.total,
            parts,
            cache: sums.cache,
            can_clear_cache: sums.cache > 0,
            cache_limit,
            before_reset: sums.before_reset,
        })
    }
}

impl LibraryManager {
    /// The open library's storage on this device: the first value at once,
    /// then one after each change.
    pub fn subscribe_device_storage(&self) -> coven::LiveQuery<DeviceStorage, DbError> {
        self.store.subscribe(device_storage_on)
    }

    /// Remove everything this device caches for the library: every
    /// unpinned file and chunk, the pictures' tiles, and the catalog images
    /// on a desktop. Pinned releases and pins on the way stay. Each is
    /// cleared in turn, so a failure of one returns to the caller with what
    /// was cleared still cleared, all of it cache; calling again finishes.
    pub async fn clear_cache(&self) -> Result<(), LibraryError> {
        let namespaces: Vec<String> = crate::SCHEMA
            .namespaces()
            .iter()
            .map(|namespace| namespace.name().to_string())
            .collect();
        self.store
            .call(move |sql| {
                for namespace in &namespaces {
                    sql.clear_cache(namespace)?;
                }
                Ok(())
            })
            .await?;
        self.tiles.clear().await?;
        #[cfg(desktop)]
        self.remote_images.clear().await?;
        Ok(())
    }

    /// Keep up to `limit` of audio and a release's other files cached on
    /// this device, removing the least recently played down to it in the
    /// same write. Pinned releases don't count toward it.
    pub async fn set_cache_limit(&self, limit: CacheLimit) -> Result<(), LibraryError> {
        self.store
            .call(move |sql| {
                Ok(sql.set_cache_budget(
                    bae_records::synced::RELEASE_FILES_NAMESPACE,
                    limit.bytes(),
                )?)
            })
            .await?;
        Ok(())
    }

    /// Delete the installations a reset kept on this device. The library
    /// isn't affected.
    pub async fn delete_data_from_before_reset(&self) -> Result<(), LibraryError> {
        Ok(self.store.delete_discarded_devices().await?)
    }
}

#[cfg(test)]
#[path = "device_storage_tests.rs"]
mod tests;
