//! The bounded cache every surface reads catalog images through: one answer
//! per address on disk, and one download shared by every caller asking for an
//! address at once.

use super::fetch::{send_image_request, ImageResponse};
use super::RemoteImageError;
use bae_diagnostics::Diagnostics;
use bae_util::content_type::ContentType;
use bae_util::http::Http;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tracing::{debug, warn};

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

/// A bounded disk cache for provider images.
///
/// The SHA-256 filename turns an arbitrary URL into one safe path. The file's
/// first line records either an image's content type or that the URL serves no
/// image. Reads update the modified time, and writes evict the least recently
/// read files until the directory is within its byte budget.
struct DiskImageCache {
    dir: std::path::PathBuf,
    budget: u64,
    access: Mutex<DiskImageAccess>,
    /// Counts each change to the folder's files: a write, an eviction, a
    /// clear.
    changes: tokio::sync::watch::Sender<u64>,
}

/// The order this session read and wrote cache files in, which eviction
/// prefers over the files' modified times. It only orders eviction, so it can
/// always start over empty.
#[derive(Default)]
struct DiskImageAccess {
    next: u64,
    current_session: HashMap<std::path::PathBuf, u64>,
}

impl DiskImageCache {
    fn new(dir: std::path::PathBuf, budget: u64) -> Self {
        Self {
            dir,
            budget,
            access: Mutex::new(DiskImageAccess::default()),
            changes: tokio::sync::watch::channel(0).0,
        }
    }

    fn changed(&self) {
        self.changes.send_modify(|count| *count += 1);
    }

    /// The bytes of the files in the folder; none when it isn't there.
    fn bytes(&self) -> std::io::Result<u64> {
        match self.measure() {
            Ok(entries) => Ok(entries.iter().map(|(_, _, size)| size).sum()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
            Err(error) => Err(error),
        }
    }

    /// Remove every file in the folder and start the access order over.
    fn clear(&self) -> std::io::Result<()> {
        let entries = match self.measure() {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        };
        let mut failure = None;
        for (path, _, _) in entries {
            if let Err(error) = std::fs::remove_file(&path) {
                if error.kind() != std::io::ErrorKind::NotFound {
                    failure.get_or_insert(error);
                }
            }
        }
        self.with_access(|access| *access = DiskImageAccess::default());
        self.changed();
        failure.map_or(Ok(()), Err)
    }

    fn path_for(&self, url: &str) -> std::path::PathBuf {
        self.dir.join(bae_util::fs::hash_bytes(url.as_bytes()))
    }

    fn get(&self, url: &str) -> Option<DiskImageEntry> {
        let path = self.path_for(url);
        let mut raw = match std::fs::read(&path) {
            Ok(raw) => raw,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
            Err(error) => {
                warn!("Could not read the cached image at {}: {error}", path.display());
                return None;
            }
        };
        let Some(header_end) = raw.iter().position(|byte| *byte == b'\n') else {
            return self.discard(&path, "it has no header");
        };
        let header = match std::str::from_utf8(&raw[..header_end]) {
            Ok(header) => header,
            Err(error) => return self.discard(&path, &format!("its header is invalid: {error}")),
        };
        let entry = if header == "none" {
            DiskImageEntry::Nothing
        } else if let Some(content_type) = header.strip_prefix("image ") {
            let content_type = ContentType::from_mime(content_type);
            raw.drain(..=header_end);
            DiskImageEntry::Image(RemoteImage { bytes: raw, content_type })
        } else {
            return self.discard(&path, "its entry kind is invalid");
        };

        if let Err(error) = std::fs::File::open(&path).and_then(|file| {
            file.set_times(std::fs::FileTimes::new().set_modified(std::time::SystemTime::now()))
        }) {
            warn!(
                "Could not mark the cached image at {} as recently used: {error}",
                path.display()
            );
        }
        self.record_access(path);
        Some(entry)
    }

    fn put(&self, url: &str, entry: &DiskImageEntry) {
        if let Err(error) = self.write(url, entry) {
            warn!("Could not cache the image answer from {url}: {error}");
            return;
        }
        self.evict_to_budget();
        self.changed();
    }

    fn write(&self, url: &str, entry: &DiskImageEntry) -> std::io::Result<()> {
        use std::io::Write;

        std::fs::create_dir_all(&self.dir)?;
        let path = self.path_for(url);
        let spool = tempfile::NamedTempFile::new_in(&self.dir)?;
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
        spool.persist(&path).map_err(|error| error.error)?;
        self.record_access(path);
        Ok(())
    }

    fn evict_to_budget(&self) {
        let mut entries = match self.measure() {
            Ok(entries) => entries,
            Err(error) => {
                warn!("Could not measure the image cache at {}: {error}", self.dir.display());
                return;
            }
        };
        let mut held: u64 = entries.iter().map(|(_, _, size)| size).sum();
        if held <= self.budget {
            return;
        }

        self.with_access(|access| {
            entries.sort_by_key(|(path, modified, _)| match access.current_session.get(path) {
                Some(sequence) => CacheAccess::Current(*sequence),
                None => CacheAccess::Prior(*modified),
            });
        });

        let mut removed = Vec::new();
        for (path, _, size) in entries {
            if held <= self.budget {
                break;
            }
            if self.remove(&path) {
                held = held.saturating_sub(size);
                removed.push(path);
            }
        }

        if !removed.is_empty() {
            debug!(
                directory = %self.dir.display(),
                removed = removed.len(),
                held,
                budget = self.budget,
                "Evicted remote image cache entries"
            );
            self.with_access(|access| {
                for path in removed {
                    access.current_session.remove(&path);
                }
            });
        }
    }

    fn measure(&self) -> std::io::Result<Vec<(std::path::PathBuf, std::time::SystemTime, u64)>> {
        let mut entries = Vec::new();
        for entry in std::fs::read_dir(&self.dir)? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            if metadata.is_file() {
                entries.push((
                    entry.path(),
                    metadata.modified().unwrap_or(std::time::UNIX_EPOCH),
                    metadata.len(),
                ));
            }
        }
        Ok(entries)
    }

    /// A cache file that cannot be read back as an entry is deleted; the caller
    /// then has nothing cached and fetches the image again.
    fn discard(&self, path: &std::path::Path, reason: &str) -> Option<DiskImageEntry> {
        warn!("Discarding the cached image at {}: {reason}", path.display());
        self.remove(path);
        self.changed();
        None
    }

    fn remove(&self, path: &std::path::Path) -> bool {
        match std::fs::remove_file(path) {
            Ok(()) => true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
            Err(error) => {
                warn!("Could not remove the cached image at {}: {error}", path.display());
                false
            }
        }
    }

    fn record_access(&self, path: std::path::PathBuf) {
        self.with_access(|access| {
            access.next =
                access.next.checked_add(1).expect("disk image cache access counter overflow");
            let sequence = access.next;
            access.current_session.insert(path, sequence);
        });
    }

    /// Run `change` on the access order. A panic that poisoned the lock left
    /// the order partway through a change; the order is only eviction's
    /// preference, so it starts over empty rather than failing every later
    /// read and write of the cache.
    fn with_access<T>(&self, change: impl FnOnce(&mut DiskImageAccess) -> T) -> T {
        let mut access = match self.access.lock() {
            Ok(access) => access,
            Err(poisoned) => {
                warn!("the image cache's access order was left partway through a change by a panic; starting it over");
                let mut access = poisoned.into_inner();
                *access = DiskImageAccess::default();
                self.access.clear_poison();
                access
            }
        };
        change(&mut access)
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum CacheAccess {
    Prior(std::time::SystemTime),
    Current(u64),
}

type InFlightImages = HashMap<String, Arc<tokio::sync::OnceCell<Option<RemoteImage>>>>;

/// A bounded persistent cache plus one entry for every active download.
///
/// Provider URLs identify static content for bae's use. Completed answers live
/// only on disk; the in-memory map exists to make concurrent callers share the
/// same request and is cleared when that request completes. Disk work runs on
/// Tokio's blocking pool rather than its async workers.
#[derive(Clone)]
pub struct RemoteImageCache {
    http: Http,
    in_flight: Arc<Mutex<InFlightImages>>,
    disk: Arc<DiskImageCache>,
    /// Where a panic in the cache's disk work is reported.
    diagnostics: Diagnostics,
    #[cfg(any(test, feature = "test-utils"))]
    _owned_dir: Option<Arc<tempfile::TempDir>>,
}

impl RemoteImageCache {
    pub fn new(library_path: &std::path::Path, http: Http, diagnostics: Diagnostics) -> Self {
        Self::in_dir(
            http,
            library_path.join("cache").join("remote-images-v2"),
            REMOTE_IMAGE_DISK_BUDGET,
            diagnostics,
        )
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn for_test(http: Http, diagnostics: Diagnostics) -> Self {
        let directory =
            tempfile::TempDir::new().expect("a temp directory for the remote image cache");
        let mut cache = Self::in_dir(
            http,
            directory.path().to_path_buf(),
            REMOTE_IMAGE_DISK_BUDGET,
            diagnostics,
        );
        cache._owned_dir = Some(Arc::new(directory));
        cache
    }

    fn in_dir(http: Http, dir: std::path::PathBuf, budget: u64, diagnostics: Diagnostics) -> Self {
        Self {
            http,
            in_flight: Arc::new(Mutex::new(HashMap::new())),
            disk: Arc::new(DiskImageCache::new(dir, budget)),
            diagnostics,
            #[cfg(any(test, feature = "test-utils"))]
            _owned_dir: None,
        }
    }

    /// Fetch a provider URL from the bounded disk cache or its host.
    pub async fn fetch(&self, url: &str) -> Result<Option<RemoteImage>, RemoteImageError> {
        if let Some(entry) = self.read_disk(url).await? {
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
        let diagnostics = self.diagnostics.clone();
        let owned_url = url.to_string();

        let result = active
            .get_or_try_init(|| async {
                if let Some(entry) =
                    read_disk(&diagnostics, Arc::clone(&disk), owned_url.clone()).await?
                {
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
                write_disk(&diagnostics, Arc::clone(&disk), owned_url.clone(), entry).await?;
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

    async fn read_disk(&self, url: &str) -> Result<Option<DiskImageEntry>, RemoteImageError> {
        read_disk(&self.diagnostics, Arc::clone(&self.disk), url.to_string()).await
    }

    /// The bytes the cache's files take on disk now.
    pub async fn bytes(&self) -> std::io::Result<u64> {
        measured_bytes(&self.diagnostics, Arc::clone(&self.disk)).await
    }

    /// Remove every file the cache holds. A file another write puts meanwhile
    /// is a new entry.
    pub async fn clear(&self) -> std::io::Result<()> {
        let disk = Arc::clone(&self.disk);
        self.diagnostics
            .blocking("clear the remote image cache", move || disk.clear())
            .await
            .map_err(std::io::Error::other)?
    }

    /// A watch on the cache's files, for a reader that measures them after
    /// each change.
    pub fn watch(&self) -> RemoteImageWatch {
        RemoteImageWatch {
            disk: Arc::clone(&self.disk),
            diagnostics: self.diagnostics.clone(),
            changes: self.disk.changes.subscribe(),
        }
    }

    /// Cache `bytes` as the image at `url`, for a test that needs the cache
    /// to hold one.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn put_for_test(&self, url: &str, bytes: &[u8]) {
        let image = RemoteImage { bytes: bytes.to_vec(), content_type: ContentType::Jpeg };
        write_disk(
            &self.diagnostics,
            Arc::clone(&self.disk),
            url.to_string(),
            DiskImageEntry::Image(image),
        )
        .await
        .expect("the cache writes the image");
    }

    /// The image at `url`, which the caller needs: [`RemoteImageError::NoImage`]
    /// where its host serves none.
    pub async fn fetch_required(&self, url: &str) -> Result<RemoteImage, RemoteImageError> {
        self.fetch(url).await?.ok_or_else(|| RemoteImageError::NoImage { url: url.to_string() })
    }
}

async fn measured_bytes(
    diagnostics: &Diagnostics,
    disk: Arc<DiskImageCache>,
) -> std::io::Result<u64> {
    diagnostics
        .blocking("measure the remote image cache", move || disk.bytes())
        .await
        .map_err(std::io::Error::other)?
}

/// One subscriber's watch on the cache's files: it hears each write,
/// eviction and clear, and measures the bytes the files take.
pub struct RemoteImageWatch {
    disk: Arc<DiskImageCache>,
    diagnostics: Diagnostics,
    changes: tokio::sync::watch::Receiver<u64>,
}

impl RemoteImageWatch {
    /// Wait for the next change to the cache's files; `false` once the cache
    /// is gone.
    pub async fn changed(&mut self) -> bool {
        self.changes.changed().await.is_ok()
    }

    /// The bytes the cache's files take on disk now.
    pub async fn bytes(&self) -> std::io::Result<u64> {
        measured_bytes(&self.diagnostics, Arc::clone(&self.disk)).await
    }
}

impl DiskImageEntry {
    fn into_image(self) -> Option<RemoteImage> {
        match self {
            Self::Image(image) => Some(image),
            Self::Nothing => None,
        }
    }
}

async fn read_disk(
    diagnostics: &Diagnostics,
    disk: Arc<DiskImageCache>,
    url: String,
) -> Result<Option<DiskImageEntry>, RemoteImageError> {
    diagnostics
        .blocking("read the remote image cache", move || disk.get(&url))
        .await
        .map_err(RemoteImageError::CacheTask)
}

async fn write_disk(
    diagnostics: &Diagnostics,
    disk: Arc<DiskImageCache>,
    url: String,
    entry: DiskImageEntry,
) -> Result<(), RemoteImageError> {
    diagnostics
        .blocking("write the remote image cache", move || disk.put(&url, &entry))
        .await
        .map_err(RemoteImageError::CacheTask)
}

#[cfg(test)]
#[path = "cache_tests.rs"]
mod tests;
