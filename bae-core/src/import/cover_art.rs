use crate::import::{ImportError, MetadataSource};
use crate::retry::{exponential_backoff, is_transient_status, retry_classified, ClassifiedAttempt};
use crate::util::content_type::ContentType;
use chrono::{DateTime, Utc};
use coven::ClockRef;
use reqwest::header::{
    HeaderMap, CACHE_CONTROL, ETAG, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tracing::{debug, warn};

/// Where the Cover Art Archive serves images from. Every path under it is fixed
/// by the entity's MusicBrainz id, so an image's address is knowable without
/// asking the archive anything.
#[cfg(not(any(test, feature = "test-utils")))]
const COVER_ART_ARCHIVE: &str = "https://coverartarchive.org";

#[cfg(not(any(test, feature = "test-utils")))]
fn archive_base() -> String {
    COVER_ART_ARCHIVE.to_string()
}

/// The redirectable form of [`archive_base`], compiled only into test builds —
/// the same seam the MusicBrainz client carries, for the same reason: the
/// addresses are built by free functions over a fixed host, so the override is
/// a static and production pays neither the lock nor the branch.
///
/// Its default is a port nothing listens on, not the live archive: a cover
/// address is derived from a release id, so any fixture whose release document
/// says the archive holds a front image would otherwise reach the real service.
/// A test that wants bytes served answers them itself through
/// [`set_base_url_for_test`].
#[cfg(any(test, feature = "test-utils"))]
fn archive_base() -> String {
    BASE_URL_OVERRIDE
        .lock()
        .expect("Cover Art Archive base URL mutex poisoned")
        .clone()
        .unwrap_or_else(|| UNSERVED_ARCHIVE.to_string())
}

/// Where a test build's cover addresses point until a test says otherwise.
#[cfg(any(test, feature = "test-utils"))]
const UNSERVED_ARCHIVE: &str = "http://127.0.0.1:9";

#[cfg(any(test, feature = "test-utils"))]
static BASE_URL_OVERRIDE: Mutex<Option<String>> = Mutex::new(None);

/// Point every Cover Art Archive address at `url` (`None` restores the
/// unserved default), so a test can answer image requests from a local server.
/// Process-wide, like the MusicBrainz base it mirrors.
#[cfg(any(test, feature = "test-utils"))]
pub fn set_base_url_for_test(url: Option<String>) {
    *BASE_URL_OVERRIDE
        .lock()
        .expect("Cover Art Archive base URL mutex poisoned") = url;
}

/// Where cover addresses currently point, for a test asserting on one it did
/// not build itself.
#[cfg(any(test, feature = "test-utils"))]
pub fn archive_base_for_test() -> String {
    archive_base()
}

/// A remote cover art option from an external source: where the full image and
/// its thumbnail live, and which service is offering them.
///
/// This is an *address*, not a promise. For the Cover Art Archive it is derived
/// from the entity id; whether the archive actually serves bytes there is
/// answered by fetching it, and — for a MusicBrainz release — stated in advance
/// by the release document's own `cover-art-archive` block.
///
/// `Serialize`/`Deserialize`: reachable from `MetadataResult::cover_art`, which
/// `identify::TerminalVerdict` persists.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RemoteCover {
    pub url: String,
    pub thumbnail_url: String,
    pub label: String,
    pub source: MetadataSource,
}

impl RemoteCover {
    /// The archive's front image for a MusicBrainz release — this pressing's
    /// own cover.
    pub fn musicbrainz_release(release_id: &str) -> Self {
        Self::cover_art_archive("release", release_id, |label| label.to_string())
    }

    /// The archive's front image for a MusicBrainz release group — the cover
    /// the album is represented by, which is some release in the group's and
    /// may not be this pressing's.
    pub fn musicbrainz_release_group(release_group_id: &str) -> Self {
        Self::cover_art_archive("release-group", release_group_id, |label| {
            format!("{label} (Album)")
        })
    }

    fn cover_art_archive(entity: &str, id: &str, label: impl FnOnce(&str) -> String) -> Self {
        let base = archive_base();
        Self {
            url: format!("{base}/{entity}/{id}/front"),
            thumbnail_url: format!("{base}/{entity}/{id}/front-250"),
            label: label(MetadataSource::MusicBrainz.cover_source_label()),
            source: MetadataSource::MusicBrainz,
        }
    }
}

/// The cover options a MusicBrainz release document offers, in the order the
/// picker shows them: the pressing's own front image first, then the album's.
///
/// The pressing's is offered only when the document says the archive serves one
/// — that block is the release's own statement, so nothing has to be asked. The
/// album's has no such statement anywhere in MusicBrainz's data: a release group
/// document carries no `cover-art-archive` block, so the address is offered and
/// the fetch is what answers whether the archive has an image there.
pub fn musicbrainz_covers(response: &crate::musicbrainz::MbReleaseResponse) -> Vec<RemoteCover> {
    let mut covers = Vec::new();
    if response.has_front_cover() {
        covers.push(RemoteCover::musicbrainz_release(&response.id));
    }
    if let Some(group) = response.release_group.as_ref() {
        covers.push(RemoteCover::musicbrainz_release_group(&group.id));
    }
    covers
}

/// Append `cover` unless the list already offers the same image. Two identity
/// rows on one release can name the same archive entity, and the picker should
/// show that image once.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) fn push_unique_cover(covers: &mut Vec<RemoteCover>, cover: RemoteCover) {
    if !covers.iter().any(|existing| existing.url == cover.url) {
        covers.push(cover);
    }
}

/// Max retries for transient HTTP failures (network errors, 5xx responses).
const MAX_RETRIES: u32 = 3;

/// Base delay between retries (doubles each attempt: 1s, 2s, 4s).
const RETRY_BASE_DELAY: Duration = Duration::from_secs(1);

/// How much disk the downloaded-cover cache may hold. Thumbnails run tens of
/// kilobytes and full covers a few hundred, so this keeps thousands of them
/// across restarts while staying a rounding error beside the library itself.
const REMOTE_IMAGE_DISK_BUDGET: u64 = 128 * 1024 * 1024;

/// How long a remote image stays fresh when its response states no
/// `Cache-Control: max-age`. Provider art at a fixed URL rarely changes, so a
/// day between revalidations costs at most one conditional GET.
const DEFAULT_REMOTE_IMAGE_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// Upper bound on a response's declared `max-age`. A host that claims decades
/// still gets revalidated within a year, and the clamp keeps the freshness
/// arithmetic inside `chrono::Duration`'s range.
const MAX_REMOTE_IMAGE_TTL: Duration = Duration::from_secs(365 * 24 * 60 * 60);

fn image_download_client() -> Result<reqwest::Client, ImportError> {
    // The build error is stored as a String because `reqwest::Client`'s builder
    // error is not `Clone` and the cell is cloned on every read; each call
    // re-wraps it in the typed `CoverArt` error.
    static CLIENT: OnceLock<Result<reqwest::Client, String>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            crate::util::http::client_builder()
                .build()
                .map_err(|e| format!("Failed to create HTTP client: {}", e))
        })
        .clone()
        .map_err(|detail| ImportError::CoverArt { detail })
}

/// One remote image's bytes and the token identifying this exact content: the
/// response's `ETag`, or a hash of the bytes when it carries none. A UI keys its
/// decoded copy on the validator, so it re-decodes only when the bytes moved.
#[derive(Debug, Clone)]
pub struct RemoteImage {
    pub bytes: Vec<u8>,
    pub content_type: ContentType,
    pub validator: String,
}

/// The freshness terms of one image response: how long it may be served without
/// asking again, and the validators a conditional GET revalidates it with.
/// `max_age` is `None` when the response declared no lifetime — the caller
/// resolves that to [`DEFAULT_REMOTE_IMAGE_TTL`] rather than storing the default
/// as if the host had stated it.
#[derive(Debug, Clone)]
struct Freshness {
    max_age: Option<Duration>,
    etag: Option<String>,
    last_modified: Option<String>,
}

impl Freshness {
    fn from_headers(headers: &HeaderMap) -> Self {
        Self {
            max_age: max_age_from_cache_control(headers),
            etag: header_string(headers, &ETAG),
            last_modified: header_string(headers, &LAST_MODIFIED),
        }
    }

    /// How long a response with these terms stays fresh.
    fn lifetime(&self) -> chrono::Duration {
        chrono::Duration::from_std(self.max_age.unwrap_or(DEFAULT_REMOTE_IMAGE_TTL))
            .expect("remote image TTLs are clamped to a year")
    }

    /// Apply a 304's headers over the stored entry's: RFC 9111 §4.3.4 says the
    /// stored response takes the headers the 304 carries and keeps the rest.
    fn updated_by(self, revalidated: Freshness) -> Freshness {
        Freshness {
            max_age: revalidated.max_age.or(self.max_age),
            etag: revalidated.etag.or(self.etag),
            last_modified: revalidated.last_modified.or(self.last_modified),
        }
    }
}

fn header_string(headers: &HeaderMap, name: &reqwest::header::HeaderName) -> Option<String> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.to_string())
}

/// `max-age` from a `Cache-Control` response header. `None` when the header is
/// absent, carries no `max-age`, or states one that doesn't parse — all of which
/// mean the response declared no lifetime of its own.
fn max_age_from_cache_control(headers: &HeaderMap) -> Option<Duration> {
    let directives = header_string(headers, &CACHE_CONTROL)?;
    let seconds: u64 = directives
        .split(',')
        .find_map(|directive| {
            let (name, value) = directive.trim().split_once('=')?;
            // Directive names are case-insensitive (RFC 9111 §5.2).
            name.eq_ignore_ascii_case("max-age").then_some(value)
        })?
        .trim()
        .parse()
        .ok()?;
    Some(Duration::from_secs(seconds).min(MAX_REMOTE_IMAGE_TTL))
}

/// One cached remote image: the bytes, when they were fetched, and the terms
/// that decide when they must be revalidated.
#[derive(Debug, Clone)]
struct CachedImage {
    bytes: Vec<u8>,
    content_type: ContentType,
    /// SHA-256 of `bytes`, the validator when the response carries no `ETag`.
    content_hash: String,
    fetched_at: DateTime<Utc>,
    freshness: Freshness,
}

impl CachedImage {
    fn store(
        bytes: Vec<u8>,
        content_type: ContentType,
        fetched_at: DateTime<Utc>,
        freshness: Freshness,
    ) -> Self {
        Self {
            content_hash: crate::util::fs::hash_bytes(&bytes),
            bytes,
            content_type,
            fetched_at,
            freshness,
        }
    }

    fn is_fresh(&self, now: DateTime<Utc>) -> bool {
        now < self.fetched_at + self.freshness.lifetime()
    }

    fn to_remote_image(&self) -> RemoteImage {
        RemoteImage {
            bytes: self.bytes.clone(),
            content_type: self.content_type.clone(),
            validator: match &self.freshness.etag {
                Some(etag) => etag.clone(),
                None => self.content_hash.clone(),
            },
        }
    }
}

/// The downloaded provider art this device keeps.
///
/// One file per address, named by the SHA-256 of the URL and holding a JSON
/// header line — the content type, the bytes' hash, when they were fetched, and
/// the freshness terms a conditional GET revalidates them with — followed by
/// the bytes. Each is written to a temp file and renamed, so a torn write is
/// never read back as an entry.
///
/// Bounded by total bytes: a write that pushes the directory past the budget
/// drops least-recently-read entries until it is back under. A read stamps its
/// file's modified time, which is what "least recently read" is measured by, so
/// the ordering survives a restart along with the bytes.
///
/// Every failure here is a miss, logged: the cache is an optimization over a
/// fetch that still works without it, so a directory that cannot be written —
/// a full disk, a revoked permission — must cost a re-download rather than the
/// cover.
struct DiskImageCache {
    dir: std::path::PathBuf,
    budget: u64,
    state: Mutex<DiskImageCacheState>,
}

struct DiskImageCacheState {
    /// Bytes currently held. `None` until the first write measures the
    /// directory; after that the writes and evictions that are the only things
    /// changing it keep it current.
    held: Option<u64>,
    /// Files touched by this cache instance, in exact access order. Filesystem
    /// modified times carry order across launches, but their precision is not
    /// sufficient to distinguish accesses made close together on every host.
    next_access: u64,
    accessed: HashMap<std::path::PathBuf, u64>,
}

/// What an entry's file records beside the bytes.
#[derive(serde::Serialize, serde::Deserialize)]
struct DiskEntryHeader {
    content_type: String,
    content_hash: String,
    fetched_at: DateTime<Utc>,
    max_age_secs: Option<u64>,
    etag: Option<String>,
    last_modified: Option<String>,
}

impl DiskImageCache {
    fn new(dir: std::path::PathBuf, budget: u64) -> Self {
        Self {
            dir,
            budget,
            state: Mutex::new(DiskImageCacheState {
                held: None,
                next_access: 0,
                accessed: HashMap::new(),
            }),
        }
    }

    fn path_for(&self, url: &str) -> std::path::PathBuf {
        self.dir.join(crate::util::fs::hash_bytes(url.as_bytes()))
    }

    /// The entry stored for `url`, or `None` when nothing is stored, the file
    /// cannot be read, or its header does not parse — all of which are misses a
    /// fetch answers.
    fn get(&self, url: &str) -> Option<CachedImage> {
        let path = self.path_for(url);
        let raw = match std::fs::read(&path) {
            Ok(raw) => raw,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
            Err(error) => {
                warn!(
                    "Could not read the cached image at {}: {error}",
                    path.display()
                );
                return None;
            }
        };
        let split = raw.iter().position(|byte| *byte == b'\n')?;
        let header: DiskEntryHeader = match serde_json::from_slice(&raw[..split]) {
            Ok(header) => header,
            Err(error) => {
                warn!(
                    "Discarding the cached image at {}: its header does not parse: {error}",
                    path.display()
                );
                self.remove(&path);
                return None;
            }
        };
        // Reading is what makes an entry recently used, and the file's modified
        // time is where that is recorded — it is the ordering eviction reads,
        // and the only part of it that survives a restart.
        if let Err(error) = std::fs::File::open(&path).and_then(|file| {
            file.set_times(std::fs::FileTimes::new().set_modified(std::time::SystemTime::now()))
        }) {
            debug!(
                "Could not stamp {} as recently used: {error}",
                path.display()
            );
        }
        self.record_access(&path);
        Some(CachedImage {
            bytes: raw[split + 1..].to_vec(),
            content_type: ContentType::from_mime(&header.content_type),
            content_hash: header.content_hash,
            fetched_at: header.fetched_at,
            freshness: Freshness {
                max_age: header.max_age_secs.map(Duration::from_secs),
                etag: header.etag,
                last_modified: header.last_modified,
            },
        })
    }

    fn put(&self, url: &str, entry: &CachedImage) {
        if let Err(error) = self.write(url, entry) {
            warn!("Could not cache the image from {url}: {error}");
            return;
        }
        self.evict_to_budget();
    }

    fn write(&self, url: &str, entry: &CachedImage) -> std::io::Result<()> {
        use std::io::Write;

        std::fs::create_dir_all(&self.dir)?;
        let header = serde_json::to_vec(&DiskEntryHeader {
            content_type: entry.content_type.as_str().to_string(),
            content_hash: entry.content_hash.clone(),
            fetched_at: entry.fetched_at,
            max_age_secs: entry.freshness.max_age.map(|age| age.as_secs()),
            etag: entry.freshness.etag.clone(),
            last_modified: entry.freshness.last_modified.clone(),
        })
        .map_err(std::io::Error::other)?;

        let path = self.path_for(url);
        let replaced = std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
        let spool = tempfile::NamedTempFile::new_in(&self.dir)?;
        {
            let mut file = spool.as_file();
            file.write_all(&header)?;
            file.write_all(b"\n")?;
            file.write_all(&entry.bytes)?;
            file.flush()?;
        }
        let written = (header.len() + 1 + entry.bytes.len()) as u64;
        spool.persist(&path).map_err(|error| error.error)?;

        let mut state = self.state.lock().expect("image cache state mutex poisoned");
        if let Some(held) = state.held.as_mut() {
            *held = held.saturating_sub(replaced) + written;
        }
        state.record_access(path);
        Ok(())
    }

    /// Drop least-recently-read entries until the directory is back inside the
    /// budget.
    fn evict_to_budget(&self) {
        // A running total that already clears the budget answers on its own: it
        // is raised only by a write and lowered only by an eviction, so nothing
        // else can have pushed the directory over.
        if let Some(held) = self
            .state
            .lock()
            .expect("image cache state mutex poisoned")
            .held
        {
            if held <= self.budget {
                return;
            }
        }
        let mut entries = match self.measure() {
            Ok(entries) => entries,
            Err(error) => {
                warn!(
                    "Could not measure the image cache at {}: {error}",
                    self.dir.display()
                );
                return;
            }
        };
        let mut held: u64 = entries.iter().map(|(_, _, size)| size).sum();
        // Longest unread first, which is the order they go in. Entries touched
        // by this instance are newer than entries inherited from a prior
        // launch, and their counter breaks timestamp ties exactly.
        {
            let state = self.state.lock().expect("image cache state mutex poisoned");
            entries.sort_by_key(|(path, last_read, _)| match state.accessed.get(path) {
                Some(access) => CacheAccess::Current(*access),
                None => CacheAccess::Prior(*last_read),
            });
        }
        let mut dropped = 0usize;
        let mut dropped_paths = Vec::new();
        for (path, _, size) in entries {
            if held <= self.budget {
                break;
            }
            if self.remove(&path) {
                held -= size;
                dropped += 1;
                dropped_paths.push(path);
            }
        }
        if dropped > 0 {
            debug!(
                "Image cache at {} past its {}-byte budget: dropped {dropped}, {held} held",
                self.dir.display(),
                self.budget
            );
        }
        let mut state = self.state.lock().expect("image cache state mutex poisoned");
        state.held = Some(held);
        for path in dropped_paths {
            state.accessed.remove(&path);
        }
    }

    /// Every entry with when it was last read and how big it is.
    fn measure(&self) -> std::io::Result<Vec<(std::path::PathBuf, std::time::SystemTime, u64)>> {
        let mut entries = Vec::new();
        for entry in std::fs::read_dir(&self.dir)? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            if !metadata.is_file() {
                continue;
            }
            entries.push((
                entry.path(),
                metadata.modified().unwrap_or(std::time::UNIX_EPOCH),
                metadata.len(),
            ));
        }
        Ok(entries)
    }

    fn remove(&self, path: &std::path::Path) -> bool {
        match std::fs::remove_file(path) {
            Ok(()) => true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
            Err(error) => {
                warn!(
                    "Could not drop the cached image at {}: {error}",
                    path.display()
                );
                false
            }
        }
    }

    fn record_access(&self, path: &std::path::Path) {
        self.state
            .lock()
            .expect("image cache state mutex poisoned")
            .record_access(path.to_path_buf());
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum CacheAccess {
    Prior(std::time::SystemTime),
    Current(u64),
}

impl DiskImageCacheState {
    fn record_access(&mut self, path: std::path::PathBuf) {
        self.next_access = self
            .next_access
            .checked_add(1)
            .expect("image cache access counter overflow");
        self.accessed.insert(path, self.next_access);
    }
}

/// What one image request returned: a body, a 304 saying the bytes already held
/// are still current, or a 404 saying the host serves no image at this address.
///
/// The third is an answer, not a failure. Cover addresses are derived, not
/// discovered — the Cover Art Archive's path for a release group is knowable
/// without knowing whether the archive has an image there — so "there is
/// nothing here" is the ordinary reply to an address that turned out empty, and
/// a caller that needs bytes is the one that turns it into an error.
enum ImageResponse {
    Body {
        bytes: Vec<u8>,
        content_type: ContentType,
        freshness: Freshness,
    },
    NotModified {
        freshness: Freshness,
    },
    Nothing,
}

/// The cache of remote image bytes, keyed by URL, with HTTP freshness: a fresh
/// entry serves without a request, a stale one revalidates with a conditional
/// GET — a 304 refreshes its clock, a 200 replaces its bytes.
///
/// Provider art is a direct HTTP fetch against arbitrary hosts (CAA, Discogs
/// CDN), not part of either metadata API client, so it caches here. All three
/// readers — the cover picker in the UI, the commit worker, and `change_cover`
/// — share one instance off the library manager, so picking a cover and then
/// importing it hits the wire once.
///
/// The entries live on disk rather than in memory. A cover the app has already
/// downloaded outlives the pane that asked for it and the process that ran it,
/// which is what makes reopening the picker — or relaunching — cost nothing;
/// and holding megabytes of encoded images in RAM bought only the repeat within
/// one session, which each platform's decoded-image cache already covers.
#[derive(Clone)]
pub struct RemoteImageCache {
    clock: ClockRef,
    entries: Arc<DiskImageCache>,
    /// The base backoff a transient download failure is retried with, doubling
    /// each attempt. A test builds the cache with a near-zero one so a fetch
    /// that is meant to fail does not sleep the real 1s + 2s + 4s.
    retry_base_delay: Duration,
    /// Owns the directory `entries` writes into, for a test cache that made one
    /// of its own. Held for its lifetime, never read.
    #[cfg(any(test, feature = "test-utils"))]
    _owned_dir: Option<Arc<tempfile::TempDir>>,
}

impl RemoteImageCache {
    /// The cache for a library, keeping its images under that library's store
    /// directory so they are deleted with it.
    pub fn new(clock: ClockRef, library_path: &std::path::Path) -> Self {
        Self::under(clock, library_path.join("cache").join("remote-images"))
    }

    /// A cache in a directory of its own, whose downloads retry immediately so
    /// a test whose fetch is meant to fail does not sleep the real 1s + 2s + 4s
    /// of backoff. Its clock is the system one: nothing a test asserts through
    /// this constructor turns on image freshness, and a test that does exercise
    /// freshness builds the cache with a clock of its own.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn for_test() -> Self {
        let dir = tempfile::TempDir::new().expect("a temp dir for the test image cache");
        let mut cache = Self::in_dir(
            std::sync::Arc::new(coven::SystemClock),
            dir.path().to_path_buf(),
            Duration::from_millis(1),
        );
        cache._owned_dir = Some(Arc::new(dir));
        cache
    }

    fn under(clock: ClockRef, dir: std::path::PathBuf) -> Self {
        Self::in_dir(clock, dir, RETRY_BASE_DELAY)
    }

    fn in_dir(clock: ClockRef, dir: std::path::PathBuf, retry_base_delay: Duration) -> Self {
        Self {
            clock,
            entries: Arc::new(DiskImageCache::new(dir, REMOTE_IMAGE_DISK_BUDGET)),
            retry_base_delay,
            #[cfg(any(test, feature = "test-utils"))]
            _owned_dir: None,
        }
    }

    /// Bytes for a remote image URL, served from the cache while fresh and
    /// revalidated once stale. Retries transient failures (network errors, 5xx)
    /// up to `MAX_RETRIES` times.
    ///
    /// `Ok(None)` is the host answering that it serves no image at this address
    /// — the reply a derived cover address gets when the archive holds nothing
    /// for that entity. A caller that requires the bytes uses the corresponding
    /// required-fetch operation.
    pub async fn fetch(&self, url: &str) -> Result<Option<RemoteImage>, ImportError> {
        let base_delay = self.retry_base_delay;
        let now = self.clock.now();
        let cached = self.entries.get(url);

        let stale = match cached {
            Some(entry) if entry.is_fresh(now) => {
                debug!("Remote image cache hit for {url}");
                return Ok(Some(entry.to_remote_image()));
            }
            Some(entry) => {
                debug!("Revalidating stale remote image {url}");
                Some(entry)
            }
            None => {
                debug!("Downloading remote image from {url}");
                None
            }
        };

        let response = send_image_request(
            url,
            stale.as_ref().map(|entry| &entry.freshness),
            "Cover art download",
            base_delay,
        )
        .await?;

        let entry = match (response, stale) {
            (ImageResponse::Nothing, _) => {
                debug!("No image is served at {url}");
                return Ok(None);
            }
            (
                ImageResponse::Body {
                    bytes,
                    content_type,
                    freshness,
                },
                _,
            ) => CachedImage::store(bytes, content_type, now, freshness),
            (ImageResponse::NotModified { freshness }, Some(entry)) => CachedImage {
                fetched_at: now,
                freshness: entry.freshness.updated_by(freshness),
                ..entry
            },
            // `send_image_request` only reports a 304 when it sent validators,
            // which only a stored entry supplies.
            (ImageResponse::NotModified { .. }, None) => {
                return Err(ImportError::CoverArt {
                    detail: format!("{url} answered 304 to an unconditional request"),
                })
            }
        };

        let image = entry.to_remote_image();
        self.entries.put(url, &entry);
        Ok(Some(image))
    }

    /// Bytes for an image the caller cannot proceed without — a cover the user
    /// picked, or one a release document says the archive holds. An address
    /// that serves nothing is a failure here, named as one.
    pub(crate) async fn fetch_required(&self, url: &str) -> Result<RemoteImage, ImportError> {
        self.fetch(url).await?.ok_or_else(|| ImportError::CoverArt {
            detail: format!("no image is served at {url}"),
        })
    }
}

/// Download an image with no caching in front — the artist-image path, whose
/// bytes are stored in the library on first fetch and never re-read from the URL.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) async fn download_image_bytes(
    image_url: &str,
    operation: &str,
) -> Result<(Vec<u8>, ContentType), ImportError> {
    match send_image_request(image_url, None, operation, RETRY_BASE_DELAY).await? {
        ImageResponse::Body {
            bytes,
            content_type,
            freshness: _,
        } => Ok((bytes, content_type)),
        ImageResponse::Nothing => Err(ImportError::CoverArt {
            detail: format!("no image is served at {image_url}"),
        }),
        ImageResponse::NotModified { .. } => Err(ImportError::CoverArt {
            detail: format!("{image_url} answered 304 to an unconditional request"),
        }),
    }
}

/// GET an image URL, conditionally when `revalidate` supplies validators.
/// Retries transient failures (network errors, 5xx) up to `MAX_RETRIES` times.
async fn send_image_request(
    image_url: &str,
    revalidate: Option<&Freshness>,
    operation: &str,
    base_delay: Duration,
) -> Result<ImageResponse, ImportError> {
    let client = image_download_client()?;
    retry_classified(
        MAX_RETRIES + 1,
        operation,
        |attempt| exponential_backoff(base_delay, attempt),
        || async {
            let mut request = client.get(image_url);
            if let Some(freshness) = revalidate {
                if let Some(etag) = &freshness.etag {
                    request = request.header(IF_NONE_MATCH, etag);
                }
                if let Some(last_modified) = &freshness.last_modified {
                    request = request.header(IF_MODIFIED_SINCE, last_modified);
                }
            }

            let response = match request.send().await {
                Ok(r) => r,
                Err(e) if is_permanent_request_error(&e) => {
                    return ClassifiedAttempt::Permanent(ImportError::CoverArt {
                        detail: format!("Failed to fetch image: {}", e),
                    });
                }
                Err(e) => {
                    return ClassifiedAttempt::Retry(ImportError::CoverArt {
                        detail: format!("Failed to fetch image: {}", e),
                    });
                }
            };

            if response.status() == reqwest::StatusCode::NOT_MODIFIED && revalidate.is_some() {
                return ClassifiedAttempt::Done(ImageResponse::NotModified {
                    freshness: Freshness::from_headers(response.headers()),
                });
            }
            if response.status().is_success() {
                match read_image_response(response, image_url).await {
                    Ok(body) => ClassifiedAttempt::Done(body),
                    Err(e) => ClassifiedAttempt::Permanent(e),
                }
            } else if response.status() == reqwest::StatusCode::NOT_FOUND {
                ClassifiedAttempt::Done(ImageResponse::Nothing)
            } else if is_transient_status(response.status()) {
                ClassifiedAttempt::Retry(ImportError::CoverArt {
                    detail: format!("Image download failed with status {}", response.status()),
                })
            } else {
                ClassifiedAttempt::Permanent(ImportError::CoverArt {
                    detail: format!("Image download failed with status {}", response.status()),
                })
            }
        },
    )
    .await
}

/// Errors that fail the same way every attempt — a URL-parse / request-builder
/// failure, or a redirect loop bottoming out. Retrying only burns the backoff
/// budget. Network/timeout/connection errors are transient and do keep retrying.
fn is_permanent_request_error(e: &reqwest::Error) -> bool {
    e.is_builder() || e.is_redirect()
}

/// Read bytes, content type, and freshness terms from a successful image response.
async fn read_image_response(
    response: reqwest::Response,
    image_url: &str,
) -> Result<ImageResponse, ImportError> {
    let content_type =
        super::image_response::image_content_type_from_response(response.headers(), image_url);
    let freshness = Freshness::from_headers(response.headers());

    let bytes = crate::util::http::read_body_capped(response, crate::util::http::MAX_IMAGE_BYTES)
        .await
        .map_err(|e| ImportError::CoverArt {
            detail: format!("Failed to read image response: {}", e),
        })?;
    if bytes.len() < 100 {
        return Err(ImportError::CoverArt {
            detail: "Downloaded file too small to be a valid image".to_string(),
        });
    }

    debug!("Downloaded cover art ({} bytes)", bytes.len());
    Ok(ImageResponse::Body {
        bytes,
        content_type,
        freshness,
    })
}

#[cfg(test)]
#[path = "cover_art_tests.rs"]
mod tests;
