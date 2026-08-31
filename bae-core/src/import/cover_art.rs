use crate::import::{ImportError, MetadataSource};
use crate::retry::{exponential_backoff, is_transient_status, retry_classified, ClassifiedAttempt};
use crate::util::content_type::ContentType;
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
#[cfg(not(any(target_os = "ios", target_os = "android")))]
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

/// Where a cover's bytes are read from — a remote address, a file the folder
/// holds, or the candidate's stored File Tags snapshot.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoverImageSource {
    Remote { url: String },
    Local { path: std::path::PathBuf },
    Bytes { data: Vec<u8> },
}

/// The cover a candidate will be committed with, and where to draw it from.
///
/// The selection is what the commit records; the two addresses are what the
/// picker and the sidebar render, at the two sizes each wants.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverChoice {
    pub selection: crate::import::CoverSelection,
    pub preview: CoverImageSource,
    pub thumbnail: CoverImageSource,
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl CoverChoice {
    /// One of the release's remote covers. Its thumbnail address is the
    /// archive's own, so a picker row costs no full-size fetch.
    pub fn remote(cover: &RemoteCover) -> Self {
        Self {
            selection: crate::import::CoverSelection::Remote(cover.url.clone(), cover.source),
            preview: CoverImageSource::Remote {
                url: cover.url.clone(),
            },
            thumbnail: CoverImageSource::Remote {
                url: cover.thumbnail_url.clone(),
            },
        }
    }

    /// One of the folder's own images, named by its relative path and drawn
    /// from where it sits on disk.
    pub fn local(file_id: String, path: std::path::PathBuf) -> Self {
        Self {
            selection: crate::import::CoverSelection::Local(file_id),
            preview: CoverImageSource::Local { path: path.clone() },
            thumbnail: CoverImageSource::Local { path },
        }
    }

    /// Artwork stored in the candidate's File Tags snapshot. The source file
    /// identifies the selection; the snapshot owns the exact bytes rendered
    /// by both the pane and the sidebar.
    pub fn embedded(source_file_id: String, data: Vec<u8>) -> Self {
        Self {
            selection: crate::import::CoverSelection::Embedded(source_file_id),
            preview: CoverImageSource::Bytes { data: data.clone() },
            thumbnail: CoverImageSource::Bytes { data },
        }
    }
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

/// Encoded provider images retained across launches.
const REMOTE_IMAGE_DISK_BUDGET: u64 = 128 * 1024 * 1024;

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

/// One remote image's bytes and their declared content type.
#[derive(Debug, Clone)]
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
}

struct DiskImageAccess {
    next: u64,
    current_session: HashMap<std::path::PathBuf, u64>,
}

impl DiskImageCache {
    fn new(dir: std::path::PathBuf, budget: u64) -> Self {
        Self {
            dir,
            budget,
            access: Mutex::new(DiskImageAccess {
                next: 0,
                current_session: HashMap::new(),
            }),
        }
    }

    fn path_for(&self, url: &str) -> std::path::PathBuf {
        self.dir.join(crate::util::fs::hash_bytes(url.as_bytes()))
    }

    fn get(&self, url: &str) -> Option<DiskImageEntry> {
        let path = self.path_for(url);
        let mut raw = match std::fs::read(&path) {
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
        let Some(header_end) = raw.iter().position(|byte| *byte == b'\n') else {
            warn!(
                "Discarding the cached image at {}: it has no header",
                path.display()
            );
            self.remove(&path);
            return None;
        };
        let header = match std::str::from_utf8(&raw[..header_end]) {
            Ok(header) => header,
            Err(error) => {
                warn!(
                    "Discarding the cached image at {}: its header is invalid: {error}",
                    path.display()
                );
                self.remove(&path);
                return None;
            }
        };
        let entry = if header == "none" {
            DiskImageEntry::Nothing
        } else if let Some(content_type) = header.strip_prefix("image ") {
            let content_type = ContentType::from_mime(content_type);
            raw.drain(..=header_end);
            DiskImageEntry::Image(RemoteImage {
                bytes: raw,
                content_type,
            })
        } else {
            warn!(
                "Discarding the cached image at {}: its entry kind is invalid",
                path.display()
            );
            self.remove(&path);
            return None;
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
                warn!(
                    "Could not measure the image cache at {}: {error}",
                    self.dir.display()
                );
                return;
            }
        };
        let mut held: u64 = entries.iter().map(|(_, _, size)| size).sum();
        if held <= self.budget {
            return;
        }

        {
            let access = self
                .access
                .lock()
                .expect("disk image cache access mutex poisoned");
            entries.sort_by_key(
                |(path, modified, _)| match access.current_session.get(path) {
                    Some(sequence) => CacheAccess::Current(*sequence),
                    None => CacheAccess::Prior(*modified),
                },
            );
        }

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
            let mut access = self
                .access
                .lock()
                .expect("disk image cache access mutex poisoned");
            for path in removed {
                access.current_session.remove(&path);
            }
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

    fn remove(&self, path: &std::path::Path) -> bool {
        match std::fs::remove_file(path) {
            Ok(()) => true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
            Err(error) => {
                warn!(
                    "Could not remove the cached image at {}: {error}",
                    path.display()
                );
                false
            }
        }
    }

    fn record_access(&self, path: std::path::PathBuf) {
        let mut access = self
            .access
            .lock()
            .expect("disk image cache access mutex poisoned");
        access.next = access
            .next
            .checked_add(1)
            .expect("disk image cache access counter overflow");
        let sequence = access.next;
        access.current_session.insert(path, sequence);
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
    in_flight: Arc<Mutex<InFlightImages>>,
    disk: Arc<DiskImageCache>,
    retry_base_delay: Duration,
    #[cfg(any(test, feature = "test-utils"))]
    _owned_dir: Option<Arc<tempfile::TempDir>>,
}

impl RemoteImageCache {
    pub fn new(library_path: &std::path::Path) -> Self {
        Self::in_dir(
            library_path.join("cache").join("remote-images"),
            REMOTE_IMAGE_DISK_BUDGET,
            RETRY_BASE_DELAY,
        )
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn for_test() -> Self {
        let directory =
            tempfile::TempDir::new().expect("a temp directory for the remote image cache");
        let mut cache = Self::in_dir(
            directory.path().to_path_buf(),
            REMOTE_IMAGE_DISK_BUDGET,
            Duration::from_millis(1),
        );
        cache._owned_dir = Some(Arc::new(directory));
        cache
    }

    fn in_dir(dir: std::path::PathBuf, budget: u64, retry_base_delay: Duration) -> Self {
        Self {
            in_flight: Arc::new(Mutex::new(HashMap::new())),
            disk: Arc::new(DiskImageCache::new(dir, budget)),
            retry_base_delay,
            #[cfg(any(test, feature = "test-utils"))]
            _owned_dir: None,
        }
    }

    /// Fetch a provider URL from the bounded disk cache or its host.
    pub async fn fetch(&self, url: &str) -> Result<Option<RemoteImage>, ImportError> {
        if let Some(entry) = self.read_disk(url).await? {
            return Ok(entry.into_image());
        }

        let active = {
            let mut in_flight = self
                .in_flight
                .lock()
                .expect("remote image in-flight mutex poisoned");
            in_flight
                .entry(url.to_string())
                .or_insert_with(|| Arc::new(tokio::sync::OnceCell::new()))
                .clone()
        };
        let retry_base_delay = self.retry_base_delay;
        let disk = Arc::clone(&self.disk);
        let owned_url = url.to_string();

        let result = active
            .get_or_try_init(|| async {
                if let Some(entry) = read_disk(Arc::clone(&disk), owned_url.clone()).await? {
                    return Ok::<Option<RemoteImage>, ImportError>(entry.into_image());
                }

                debug!("Downloading remote image from {owned_url}");
                let entry =
                    match send_image_request(&owned_url, "Cover art download", retry_base_delay)
                        .await?
                    {
                        ImageResponse::Body {
                            bytes,
                            content_type,
                        } => DiskImageEntry::Image(RemoteImage {
                            bytes,
                            content_type,
                        }),
                        ImageResponse::Nothing => {
                            debug!("No image is served at {owned_url}");
                            DiskImageEntry::Nothing
                        }
                    };
                let image = entry.clone().into_image();
                write_disk(Arc::clone(&disk), owned_url.clone(), entry).await?;
                Ok(image)
            })
            .await
            .cloned();

        let mut in_flight = self
            .in_flight
            .lock()
            .expect("remote image in-flight mutex poisoned");
        if in_flight
            .get(url)
            .is_some_and(|current| Arc::ptr_eq(current, &active))
        {
            in_flight.remove(url);
        }
        result
    }

    async fn read_disk(&self, url: &str) -> Result<Option<DiskImageEntry>, ImportError> {
        read_disk(Arc::clone(&self.disk), url.to_string()).await
    }

    pub(crate) async fn fetch_required(&self, url: &str) -> Result<RemoteImage, ImportError> {
        self.fetch(url).await?.ok_or_else(|| ImportError::CoverArt {
            detail: format!("no image is served at {url}"),
        })
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
    disk: Arc<DiskImageCache>,
    url: String,
) -> Result<Option<DiskImageEntry>, ImportError> {
    tokio::task::spawn_blocking(move || disk.get(&url))
        .await
        .map_err(|error| ImportError::CoverArt {
            detail: format!("Remote image cache read task failed: {error}"),
        })
}

async fn write_disk(
    disk: Arc<DiskImageCache>,
    url: String,
    entry: DiskImageEntry,
) -> Result<(), ImportError> {
    tokio::task::spawn_blocking(move || disk.put(&url, &entry))
        .await
        .map_err(|error| ImportError::CoverArt {
            detail: format!("Remote image cache write task failed: {error}"),
        })
}

/// Download an image with no caching in front — the artist-image path, whose
/// bytes are stored in the library on first fetch and never re-read from the URL.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) async fn download_image_bytes(
    image_url: &str,
    operation: &str,
) -> Result<(Vec<u8>, ContentType), ImportError> {
    match send_image_request(image_url, operation, RETRY_BASE_DELAY).await? {
        ImageResponse::Body {
            bytes,
            content_type,
        } => Ok((bytes, content_type)),
        ImageResponse::Nothing => Err(ImportError::CoverArt {
            detail: format!("no image is served at {image_url}"),
        }),
    }
}

/// What one image request returned. A 404 is an ordinary answer because cover
/// addresses are derived without knowing whether their host has bytes there.
enum ImageResponse {
    Body {
        bytes: Vec<u8>,
        content_type: ContentType,
    },
    Nothing,
}

/// GET an image URL. Retries transient failures (network errors, 5xx) up to
/// `MAX_RETRIES` times.
async fn send_image_request(
    image_url: &str,
    operation: &str,
    base_delay: Duration,
) -> Result<ImageResponse, ImportError> {
    let client = image_download_client()?;
    retry_classified(
        MAX_RETRIES + 1,
        operation,
        |attempt| exponential_backoff(base_delay, attempt),
        || async {
            let response = match client.get(image_url).send().await {
                Ok(response) => response,
                Err(error) if is_permanent_request_error(&error) => {
                    return ClassifiedAttempt::Permanent(ImportError::CoverArt {
                        detail: format!("Failed to fetch image: {error}"),
                    });
                }
                Err(error) => {
                    return ClassifiedAttempt::Retry(ImportError::CoverArt {
                        detail: format!("Failed to fetch image: {error}"),
                    });
                }
            };

            if response.status().is_success() {
                match read_image_response(response, image_url).await {
                    Ok(body) => ClassifiedAttempt::Done(body),
                    Err(error) => ClassifiedAttempt::Permanent(error),
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
/// failure, or a redirect loop bottoming out. Network, timeout, and connection
/// errors remain retryable.
fn is_permanent_request_error(error: &reqwest::Error) -> bool {
    error.is_builder() || error.is_redirect()
}

/// Read bytes and content type from a successful image response.
async fn read_image_response(
    response: reqwest::Response,
    image_url: &str,
) -> Result<ImageResponse, ImportError> {
    let content_type =
        super::image_response::image_content_type_from_response(response.headers(), image_url);

    let bytes = crate::util::http::read_body_capped(response, crate::util::http::MAX_IMAGE_BYTES)
        .await
        .map_err(|error| ImportError::CoverArt {
            detail: format!("Failed to read image response: {error}"),
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
    })
}

#[cfg(test)]
#[path = "cover_art_tests.rs"]
mod tests;
