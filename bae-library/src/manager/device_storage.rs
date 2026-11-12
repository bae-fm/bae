//! What this device keeps for the open library, in bytes on disk: its pinned
//! releases, its cache, the library itself and what a reset left behind. The
//! reads are coven's byte measurements beside the catalog image cache's; the
//! commands clear the cache, set how much audio it keeps, and delete what a
//! reset kept.

use super::*;
use bae_records::CacheLimit;
use bae_store::live_query::{LiveSource, NextValue};
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

impl DeviceStorage {
    /// What coven measured and the catalog image cache holds, as the page
    /// shows it. Fails when the audio budget is one no choice names: no
    /// writer here sets another.
    pub fn from_measurements(
        local: &coven::LocalBytes,
        catalog_images: u64,
    ) -> Result<Self, DbError> {
        let audio = local
            .namespaces
            .iter()
            .find(|namespace| namespace.namespace == bae_records::synced::RELEASE_FILES_NAMESPACE)
            .ok_or_else(|| {
                DbError::unreadable("the declared cache namespaces", &local.namespaces)
            })?;
        let cache_limit = CacheLimit::from_bytes(audio.budget)
            .ok_or_else(|| DbError::unreadable("the audio cache budget", audio.budget))?;
        let pinned =
            local.namespaces.iter().map(|namespace| namespace.pinned).sum::<u64>() + local.pinning;
        let cache =
            local.namespaces.iter().map(|namespace| namespace.cached).sum::<u64>() + catalog_images;
        let mut parts = Vec::new();
        if pinned > 0 {
            parts.push(StoragePart { kind: StorageKind::Pinned, bytes: pinned });
        }
        if cache > 0 {
            parts.push(StoragePart { kind: StorageKind::Cache, bytes: cache });
        }
        parts.push(StoragePart { kind: StorageKind::Library, bytes: local.library });
        if let Some(bytes) = local.before_reset {
            parts.push(StoragePart { kind: StorageKind::BeforeReset, bytes });
        }
        Ok(Self {
            total: parts.iter().map(|part| part.bytes).sum(),
            parts,
            cache,
            can_clear_cache: cache > 0,
            cache_limit,
            before_reset: local.before_reset,
        })
    }
}

/// The catalog image cache a desktop follows; a phone has none.
struct CatalogImages {
    #[cfg(desktop)]
    watch: Option<bae_catalogs::remote_image::RemoteImageWatch>,
}

impl CatalogImages {
    /// Resolves when the cache changes; never where there is none to follow.
    async fn changed(&mut self) {
        #[cfg(desktop)]
        if let Some(watch) = &mut self.watch {
            if watch.changed().await {
                return;
            }
            self.watch = None;
        }
        std::future::pending::<()>().await;
    }

    /// The bytes the images take: none where there is no cache.
    async fn bytes(&self) -> std::io::Result<u64> {
        #[cfg(desktop)]
        if let Some(watch) = &self.watch {
            return watch.bytes().await;
        }
        Ok(0)
    }
}

/// The device's storage as it changes: the first value at once, then one
/// after each change to coven's measurements or to the catalog image cache.
pub struct LiveDeviceStorage {
    bytes: coven::LocalBytesWatch,
    latest: Option<coven::LocalBytes>,
    images: CatalogImages,
    last: Option<DeviceStorage>,
}

impl LiveDeviceStorage {
    /// Wait for the next change to what either measures; `None` once the
    /// store has closed.
    async fn changed(&mut self) -> Option<Result<(), DbError>> {
        if self.latest.is_none() {
            self.latest = Some(match self.bytes.next().await? {
                Ok(local) => local,
                Err(error) => return Some(Err(error.into())),
            });
            return Some(Ok(()));
        }
        tokio::select! {
            next = self.bytes.next() => match next? {
                Ok(local) => self.latest = Some(local),
                Err(error) => return Some(Err(error.into())),
            },
            () = self.images.changed() => {}
        }
        Some(Ok(()))
    }
}

impl LiveSource<DeviceStorage> for LiveDeviceStorage {
    fn next(&mut self) -> NextValue<'_, DeviceStorage> {
        Box::pin(async move {
            loop {
                if let Err(error) = self.changed().await? {
                    return Some(Err(error));
                }
                let images = match self.images.bytes().await {
                    Ok(images) => images,
                    Err(error) => return Some(Err(error.into())),
                };
                let local = self.latest.as_ref().expect("a measurement was taken");
                let storage = match DeviceStorage::from_measurements(local, images) {
                    Ok(storage) => storage,
                    Err(error) => return Some(Err(error)),
                };
                if self.last.as_ref() != Some(&storage) {
                    self.last = Some(storage.clone());
                    return Some(Ok(storage));
                }
            }
        })
    }
}

impl LibraryManager {
    /// The open library's storage on this device: the first value at once,
    /// then one after each change.
    pub fn subscribe_device_storage(&self) -> LiveDeviceStorage {
        let images = CatalogImages {
            #[cfg(desktop)]
            watch: Some(self.remote_images.watch()),
        };
        LiveDeviceStorage {
            bytes: self.store.subscribe_local_bytes(),
            latest: None,
            images,
            last: None,
        }
    }

    /// Remove everything this device caches for the library: every
    /// unpinned file and chunk, and the catalog images on a desktop. Pinned
    /// releases and pins on the way stay. The stores are two, so a failure
    /// of either returns to the caller with what was cleared still cleared,
    /// all of it cache; calling again finishes.
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
