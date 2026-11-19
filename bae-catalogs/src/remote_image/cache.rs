//! The bounded cache every surface reads catalog images through: one answer
//! per address in a file on disk, one row per file in `cached_image`, and one
//! download shared by every caller asking for an address at once.
//!
//! Its table is `cache.sql`.

use super::fetch::{send_image_request, ImageResponse};
use super::RemoteImageError;
use bae_diagnostics::Diagnostics;
use bae_store::query::to_i64;
use bae_store::{DbError, Store};
use bae_util::content_type::ContentType;
use bae_util::http::Http;
use coven::CovenWrite;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tracing::{debug, warn};

/// The image cache's part of the library's schema.
pub(crate) const SCHEMA: bae_store::SchemaPart =
    bae_store::SchemaPart::new(&[include_str!("cache.sql")]);

/// Encoded provider images retained across launches.
const REMOTE_IMAGE_DISK_BUDGET: u64 = 128 * 1024 * 1024;

/// One decoded provider image's original bytes and detected content type.
#[derive(Debug, Clone, PartialEq)]
pub struct RemoteImage {
    pub bytes: Vec<u8>,
    pub content_type: ContentType,
}

/// The durable answer for one provider URL.
#[derive(Debug, Clone)]
enum DiskImageEntry {
    Image(RemoteImage),
    Nothing,
}

/// A bounded disk cache for provider images, with a row in `cached_image`
/// for each file.
///
/// The SHA-256 filename turns an arbitrary URL into one safe path. The file's
/// first line records either an image's content type or that the URL serves no
/// image. A read finds an answer only through its row, and marks the row as
/// the most recently used. A write evicts the least recently used rows until
/// the rows' bytes are within the budget.
///
/// The cache writes a file before its row, and deletes a row before its file.
/// A failure or a crash between the two steps leaves a file that no row
/// names: it takes space, no read finds it, and a later write of its address
/// or Clear Cache replaces or removes it. No row names a file the cache
/// hasn't written or has removed.
struct DiskImageCache {
    dir: PathBuf,
    budget: u64,
    store: Store,
    /// Where a panic in the cache's file work is reported.
    diagnostics: Diagnostics,
    /// Held by each change to the files and their rows (a write, a discard, a
    /// clear) from its first step to its last, so no change runs between
    /// another's file step and its row step.
    changing: tokio::sync::Mutex<()>,
}

impl DiskImageCache {
    fn new(dir: PathBuf, budget: u64, store: Store, diagnostics: Diagnostics) -> Self {
        Self { dir, budget, store, diagnostics, changing: tokio::sync::Mutex::new(()) }
    }

    /// The answer the cache keeps for `url`, marked as the most recently
    /// used; none when it keeps none.
    #[expect(
        clippy::disallowed_methods,
        reason = "file system: the catalog image cache, files bae keeps itself, which coven does not own"
    )]
    async fn get(&self, url: &str) -> Result<Option<DiskImageEntry>, RemoteImageError> {
        let file = file_for(url);
        let kept = {
            let file = file.clone();
            self.store
                .call(move |sql| mark_used(sql, &file))
                .await
                .map_err(RemoteImageError::CacheStore)?
        };
        if !kept {
            return Ok(None);
        }
        let path = self.dir.join(&file);
        let read = self
            .diagnostics
            .blocking("read the remote image cache", move || std::fs::read(&path))
            .await
            .map_err(RemoteImageError::CacheTask)?;
        let raw = match read {
            Ok(raw) => raw,
            // A removal that deleted the row after this read found it removed
            // the file too, or something other than bae removed the file.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                self.discard(&file, "its file is gone").await?;
                return Ok(None);
            }
            Err(error) => {
                warn!("Could not read the cached image {file}: {error}");
                return Ok(None);
            }
        };
        match DiskImageEntry::parse(raw) {
            Ok(entry) => Ok(Some(entry)),
            Err(reason) => {
                self.discard(&file, &reason).await?;
                Ok(None)
            }
        }
    }

    /// Keep `entry` as the answer for `url`, then evict the least recently
    /// used answers until the rows' bytes fit the budget. A file or row the
    /// cache can't write only means it doesn't keep the answer, so the
    /// failure is logged; a panic in the file work fails the caller.
    async fn put(&self, url: &str, entry: DiskImageEntry) -> Result<(), RemoteImageError> {
        let _changing = self.changing.lock().await;
        let file = file_for(url);
        let written = {
            let dir = self.dir.clone();
            let path = self.dir.join(&file);
            self.diagnostics
                .blocking("write the remote image cache", move || write_file(&dir, &path, &entry))
                .await
                .map_err(RemoteImageError::CacheTask)?
        };
        let bytes = match written {
            Ok(bytes) => bytes,
            Err(error) => {
                warn!("Could not cache the image answer from {url}: {error}");
                return Ok(());
            }
        };
        let budget = self.budget;
        let kept = {
            let file = file.clone();
            self.store.call(move |sql| keep(sql, &file, bytes, budget)).await
        };
        let evicted = match kept {
            Ok(evicted) => evicted,
            Err(error) => {
                warn!("Could not record the cached image answer from {url}: {error}");
                return self.remove_files(vec![file]).await;
            }
        };
        if !evicted.is_empty() {
            debug!(
                directory = %self.dir.display(),
                removed = evicted.len(),
                budget = self.budget,
                "Evicted remote image cache entries"
            );
            self.remove_files(evicted).await?;
        }
        Ok(())
    }

    /// Delete every row, then the folder with every file in it, those no row
    /// names among them.
    #[expect(
        clippy::disallowed_methods,
        reason = "file system: the catalog image cache, files bae keeps itself, which coven does not own"
    )]
    async fn clear(&self) -> Result<(), DbError> {
        let _changing = self.changing.lock().await;
        self.store.call(forget_all).await?;
        let dir = self.dir.clone();
        self.diagnostics
            .blocking("clear the remote image cache", move || match std::fs::remove_dir_all(&dir) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                removed => removed,
            })
            .await
            .map_err(std::io::Error::other)??;
        Ok(())
    }

    /// Delete the row of a cache file that cannot be read back as an entry,
    /// then the file; the caller then has nothing cached and fetches the
    /// image again.
    async fn discard(&self, file: &str, reason: &str) -> Result<(), RemoteImageError> {
        warn!("Discarding the cached image {file}: {reason}");
        let _changing = self.changing.lock().await;
        {
            let file = file.to_string();
            self.store
                .call(move |sql| forget(sql, &file))
                .await
                .map_err(RemoteImageError::CacheStore)?;
        }
        self.remove_files(vec![file.to_string()]).await
    }

    /// Remove `files`, whose rows are gone. A file that stays only takes
    /// space, so the failure is logged.
    #[expect(
        clippy::disallowed_methods,
        reason = "file system: the catalog image cache, files bae keeps itself, which coven does not own"
    )]
    async fn remove_files(&self, files: Vec<String>) -> Result<(), RemoteImageError> {
        let dir = self.dir.clone();
        self.diagnostics
            .blocking("remove remote image cache files", move || {
                for file in files {
                    match std::fs::remove_file(dir.join(&file)) {
                        Ok(()) => {}
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                        Err(error) => warn!("Could not remove the cached image {file}: {error}"),
                    }
                }
            })
            .await
            .map_err(RemoteImageError::CacheTask)
    }
}

/// The name of the file that holds the answer for `url`.
fn file_for(url: &str) -> String {
    bae_util::fs::hash_bytes(url.as_bytes())
}

/// Write `entry` to `path` in `dir` through a spool file that replaces it
/// whole, and return the file's size.
#[expect(
    clippy::disallowed_methods,
    reason = "file system: the catalog image cache, files bae keeps itself, which coven does not own"
)]
fn write_file(dir: &Path, path: &Path, entry: &DiskImageEntry) -> std::io::Result<u64> {
    use std::io::Write;

    std::fs::create_dir_all(dir)?;
    let spool = tempfile::NamedTempFile::new_in(dir)?;
    {
        let mut file = spool.as_file();
        match entry {
            DiskImageEntry::Image(image) => {
                file.write_all(b"image ")?;
                file.write_all(image.content_type.as_str().as_bytes())?;
                file.write_all(b"\n")?;
                file.write_all(&image.bytes)?;
            }
            DiskImageEntry::Nothing => file.write_all(b"none\n")?,
        }
        file.flush()?;
    }
    let bytes = spool.as_file().metadata()?.len();
    spool.persist(path).map_err(|error| error.error)?;
    Ok(bytes)
}

/// Mark `file`'s row as the most recently used; whether there is one.
fn mark_used(sql: &CovenWrite<'_>, file: &str) -> Result<bool, DbError> {
    Ok(sql.execute(
        "UPDATE cached_image SET used = (SELECT MAX(used) + 1 FROM cached_image) WHERE file = ?1",
        [file],
    )? > 0)
}

/// Record `file`, of `bytes`, as the most recently used, then delete the
/// least recently used rows until the rest fit `budget`, and return the
/// files whose rows it deleted.
fn keep(sql: &CovenWrite<'_>, file: &str, bytes: u64, budget: u64) -> Result<Vec<String>, DbError> {
    sql.execute(
        "INSERT INTO cached_image (file, bytes, used) \
         VALUES (?1, ?2, (SELECT COALESCE(MAX(used), 0) + 1 FROM cached_image)) \
         ON CONFLICT (file) DO UPDATE SET bytes = excluded.bytes, used = excluded.used",
        (file, to_i64(bytes, "a cached image's size")?),
    )?;
    Ok(sql.query(
        "DELETE FROM cached_image WHERE file IN ( \
             SELECT file FROM ( \
                 SELECT file, SUM(bytes) OVER (ORDER BY used DESC) AS held FROM cached_image \
             ) WHERE held > ?1 \
         ) RETURNING file",
        [to_i64(budget, "the image cache's budget")?],
        |row| row.get(0),
    )?)
}

/// Delete `file`'s row.
fn forget(sql: &CovenWrite<'_>, file: &str) -> Result<(), DbError> {
    sql.execute("DELETE FROM cached_image WHERE file = ?1", [file])?;
    Ok(())
}

/// Delete every row.
fn forget_all(sql: &CovenWrite<'_>) -> Result<(), DbError> {
    sql.execute("DELETE FROM cached_image", [])?;
    Ok(())
}

type InFlightImages = HashMap<String, Arc<tokio::sync::OnceCell<Option<RemoteImage>>>>;

/// A bounded persistent cache plus one entry for every active download.
///
/// Provider URLs identify static content for bae's use. Completed answers live
/// only on disk; the in-memory map exists to make concurrent callers share the
/// same request and is cleared when that request completes. File work runs on
/// Tokio's blocking pool rather than its async workers.
#[derive(Clone)]
pub struct RemoteImageCache {
    http: Http,
    in_flight: Arc<Mutex<InFlightImages>>,
    disk: Arc<DiskImageCache>,
    #[cfg(any(test, feature = "test-utils"))]
    _owned_dir: Option<Arc<bae_util::test_files::TestFolder>>,
}

impl RemoteImageCache {
    /// The cache of the library at `library_path`, whose rows `store` holds.
    pub fn new(
        library_path: &std::path::Path,
        http: Http,
        store: Store,
        diagnostics: Diagnostics,
    ) -> Self {
        Self::in_dir(
            http,
            store,
            library_path.join("cache").join("remote-images-v2"),
            REMOTE_IMAGE_DISK_BUDGET,
            diagnostics,
        )
    }

    /// A cache over a temporary folder, whose rows `store` holds.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn for_test(http: Http, store: Store, diagnostics: Diagnostics) -> Self {
        let directory = bae_util::test_files::TestFolder::new();
        let mut cache = Self::in_dir(
            http,
            store,
            directory.path().to_path_buf(),
            REMOTE_IMAGE_DISK_BUDGET,
            diagnostics,
        );
        cache._owned_dir = Some(Arc::new(directory));
        cache
    }

    fn in_dir(
        http: Http,
        store: Store,
        dir: PathBuf,
        budget: u64,
        diagnostics: Diagnostics,
    ) -> Self {
        Self {
            http,
            in_flight: Arc::new(Mutex::new(HashMap::new())),
            disk: Arc::new(DiskImageCache::new(dir, budget, store, diagnostics)),
            #[cfg(any(test, feature = "test-utils"))]
            _owned_dir: None,
        }
    }

    /// Fetch a provider URL from the bounded disk cache or its host.
    pub async fn fetch(&self, url: &str) -> Result<Option<RemoteImage>, RemoteImageError> {
        if let Some(entry) = self.disk.get(url).await? {
            return Ok(entry.into_image());
        }

        let active = {
            let mut in_flight =
                self.in_flight.lock().expect("remote image in-flight mutex poisoned");
            in_flight
                .entry(url.to_string())
                .or_insert_with(|| Arc::new(tokio::sync::OnceCell::new()))
                .clone()
        };
        let http = self.http.clone();
        let disk = Arc::clone(&self.disk);
        let owned_url = url.to_string();

        let result = active
            .get_or_try_init(|| async {
                if let Some(entry) = disk.get(&owned_url).await? {
                    return Ok::<Option<RemoteImage>, RemoteImageError>(entry.into_image());
                }

                debug!("Downloading remote image from {owned_url}");
                let entry =
                    match send_image_request(&http, &owned_url, "Cover art download").await? {
                        ImageResponse::Body { bytes, content_type } => {
                            DiskImageEntry::Image(RemoteImage { bytes, content_type })
                        }
                        ImageResponse::Nothing => {
                            debug!("No image is served at {owned_url}");
                            DiskImageEntry::Nothing
                        }
                    };
                let image = entry.clone().into_image();
                disk.put(&owned_url, entry).await?;
                Ok(image)
            })
            .await
            .cloned();

        let mut in_flight = self.in_flight.lock().expect("remote image in-flight mutex poisoned");
        if in_flight.get(url).is_some_and(|current| Arc::ptr_eq(current, &active)) {
            in_flight.remove(url);
        }
        result
    }

    /// Remove every file the cache holds and every row. A write that waits
    /// meanwhile keeps its answer after the clear.
    pub async fn clear(&self) -> Result<(), DbError> {
        self.disk.clear().await
    }

    /// Cache `bytes` as the image at `url`, for a test that needs the cache
    /// to hold one.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn put_for_test(&self, url: &str, bytes: &[u8]) {
        let image = RemoteImage { bytes: bytes.to_vec(), content_type: ContentType::Jpeg };
        self.disk.put(url, DiskImageEntry::Image(image)).await.expect("the cache writes the image");
    }

    /// The image at `url`, which the caller needs: [`RemoteImageError::NoImage`]
    /// where its host serves none.
    pub async fn fetch_required(&self, url: &str) -> Result<RemoteImage, RemoteImageError> {
        self.fetch(url).await?.ok_or_else(|| RemoteImageError::NoImage { url: url.to_string() })
    }
}

impl DiskImageEntry {
    /// The entry a cache file holds, or why it holds none.
    fn parse(mut raw: Vec<u8>) -> Result<Self, String> {
        let header_end = raw
            .iter()
            .position(|byte| *byte == b'\n')
            .ok_or_else(|| "it has no header".to_string())?;
        let header = std::str::from_utf8(&raw[..header_end])
            .map_err(|error| format!("its header is invalid: {error}"))?;
        if header == "none" {
            return Ok(Self::Nothing);
        }
        let Some(content_type) = header.strip_prefix("image ") else {
            return Err("its entry kind is invalid".to_string());
        };
        let content_type = ContentType::from_mime(content_type);
        raw.drain(..=header_end);
        Ok(Self::Image(RemoteImage { bytes: raw, content_type }))
    }

    fn into_image(self) -> Option<RemoteImage> {
        match self {
            Self::Image(image) => Some(image),
            Self::Nothing => None,
        }
    }
}

#[cfg(test)]
#[path = "cache_tests.rs"]
mod tests;
