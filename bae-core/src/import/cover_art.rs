use crate::import::{Catalog, ImportError};
use crate::retry::{exponential_backoff, is_transient_status, retry_classified, ClassifiedAttempt};
use crate::signals::LookupFailure;
use crate::util::content_type::ContentType;
use crate::util::test_base_url::TestBaseUrl;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tracing::{debug, warn};

#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[path = "cover_art_archive.rs"]
mod archive;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use archive::{musicbrainz_gallery, musicbrainz_group_gallery};

/// The persisted owner whose external identities supply a cover gallery.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub enum CoverTarget {
    Release(String),
    Candidate(String),
}

/// Whether an owner names an external release, and the artwork it offers.
/// An empty linked gallery is not an unidentified release.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Debug, Clone, PartialEq)]
pub enum RemoteCoverGallery {
    Unlinked,
    Linked(Vec<RemoteCover>),
}

/// Where the Cover Art Archive serves images from. Every path under it is fixed
/// by the entity's MusicBrainz id, so an image's address is knowable without
/// asking the archive anything.
#[cfg(not(any(test, feature = "test-utils")))]
pub(crate) static ARCHIVE: TestBaseUrl = TestBaseUrl::new("https://coverartarchive.org");

/// Test builds start at a port nothing listens on rather than the live archive:
/// a cover address is derived from a release id, so any fixture whose release
/// document says the archive holds a front image would otherwise reach the real
/// service. A test that wants bytes served answers them itself, after pointing
/// this at its own server.
#[cfg(any(test, feature = "test-utils"))]
pub(crate) static ARCHIVE: TestBaseUrl = TestBaseUrl::new("http://127.0.0.1:9");

/// Point every Cover Art Archive address at `url` (`None` restores the unserved
/// default), so a test outside this crate can answer image requests from a local
/// server.
#[cfg(any(test, feature = "test-utils"))]
pub fn set_base_url_for_test(url: Option<String>) {
    ARCHIVE.set_for_test(url);
}

/// Point the archive at a stand-in that holds nothing: every address answers
/// 404, which is what the archive itself answers for a release group it has
/// no image for.
///
/// A test whose subject offers an archive address — the album address a
/// release group's cover is offered at is a guess by construction —
/// otherwise reads a connection failure rather than the archive's answer.
///
/// The stand-in is started once and the archive is pointed at it on every
/// call, because another test's fixture restores the unserved default when it
/// ends. Calling this is serialized with every other test that points the
/// archive somewhere, as the base address requires.
#[cfg(test)]
pub(crate) fn serve_empty_archive_for_test() {
    static STAND_IN: OnceLock<String> = OnceLock::new();
    let address = STAND_IN.get_or_init(|| {
        let (address_tx, address_rx) = std::sync::mpsc::channel();
        // Its own runtime on its own thread: the stand-in outlives each
        // `#[tokio::test]` that reaches it, so it lives on no one test's
        // runtime.
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("the stand-in archive's runtime builds");
            runtime.block_on(async move {
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                    .await
                    .expect("the stand-in archive binds");
                address_tx
                    .send(
                        listener
                            .local_addr()
                            .expect("the stand-in archive has an address"),
                    )
                    .expect("the starting thread is waiting for the address");
                while let Ok((mut stream, _)) = listener.accept().await {
                    tokio::spawn(async move {
                        use tokio::io::{AsyncReadExt, AsyncWriteExt};
                        let mut request = [0u8; 1024];
                        if let Err(error) = stream.read(&mut request).await {
                            debug!("the stand-in archive could not read a request: {error}");
                            return;
                        }
                        if let Err(error) = stream
                            .write_all(
                                b"HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n",
                            )
                            .await
                        {
                            debug!("the stand-in archive could not answer: {error}");
                        }
                    });
                }
            });
        });
        let address = address_rx.recv().expect("the stand-in archive starts");
        format!("http://{address}")
    });
    set_base_url_for_test(Some(address.clone()));
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
    pub source: Catalog,
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
        let base = ARCHIVE.get();
        Self {
            url: format!("{base}/{entity}/{id}/front"),
            thumbnail_url: format!("{base}/{entity}/{id}/front-250"),
            label: label(Catalog::MusicBrainz.cover_source_label()),
            source: Catalog::MusicBrainz,
        }
    }
}

/// This pressing's own front image, offered only when the release document
/// says the archive serves one — that block is the release's own statement,
/// so nothing has to be asked.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub fn musicbrainz_release_cover(
    response: &crate::musicbrainz::MbReleaseResponse,
) -> Option<RemoteCover> {
    response
        .has_front_cover()
        .then(|| RemoteCover::musicbrainz_release(&response.id))
}

/// The album this release belongs to, as the archive addresses it.
///
/// No statement anywhere in MusicBrainz's data says whether the archive holds
/// an image there — a release group document carries no `cover-art-archive`
/// block — so the address is offered and the fetch is what answers. That is
/// why it is an album option and never a pressing's: it may be some other
/// release's cover, and it may be nothing at all.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub fn musicbrainz_album_cover(
    response: &crate::musicbrainz::MbReleaseResponse,
) -> Option<RemoteCover> {
    response
        .release_group
        .as_ref()
        .map(|group| RemoteCover::musicbrainz_release_group(&group.id))
}

/// Where a cover's bytes are read from — a remote address, a file the folder
/// holds, or the candidate's stored file-tag snapshot.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoverImageSource {
    Remote { url: String },
    Local { path: std::path::PathBuf },
    Bytes { data: Vec<u8> },
}

/// The cover a candidate will be committed with, and where to draw it from.
///
/// The selection is the candidate's stored one — what the scan read off the
/// folder, what its identification fetched, or what the person chose. A
/// candidate with none stored commits with no cover, so there is no such
/// thing here as a cover that only a reader knows about.
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

    /// Artwork stored in the candidate's file-tag snapshot. The source file
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
    // re-wraps it in the typed `Internal` error.
    static CLIENT: OnceLock<Result<reqwest::Client, String>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            crate::util::http::client_builder()
                .build()
                .map_err(|e| format!("Failed to create HTTP client: {e:?}"))
        })
        .clone()
        .map_err(|detail| ImportError::Internal { detail })
}

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
            DiskImageEntry::Image(RemoteImage {
                bytes: raw,
                content_type,
            })
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

    /// A cache file that cannot be read back as an entry is deleted; the caller
    /// then has nothing cached and fetches the image again.
    fn discard(&self, path: &std::path::Path, reason: &str) -> Option<DiskImageEntry> {
        warn!(
            "Discarding the cached image at {}: {reason}",
            path.display()
        );
        self.remove(path);
        None
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
            library_path.join("cache").join("remote-images-v2"),
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
        self.fetch(url)
            .await?
            .ok_or_else(|| ImportError::CoverArtRequest {
                failure: LookupFailure::Provider { status: Some(404) },
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
        .map_err(|error| ImportError::Internal {
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
        .map_err(|error| ImportError::Internal {
            detail: format!("Remote image cache write task failed: {error}"),
        })
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
    match send_artwork_request(image_url, operation, base_delay).await? {
        Some(response) => read_image_response(response, image_url).await,
        None => Ok(ImageResponse::Nothing),
    }
}

/// Shared transport policy for image bytes and artwork-list documents.
async fn send_artwork_request(
    url: &str,
    operation: &str,
    base_delay: Duration,
) -> Result<Option<reqwest::Response>, ImportError> {
    let client = image_download_client()?;
    retry_classified(
        MAX_RETRIES + 1,
        operation,
        |attempt| exponential_backoff(base_delay, attempt),
        || async {
            let response = match client
                .get(url)
                .timeout(crate::util::http::READ_TIMEOUT)
                .send()
                .await
            {
                Ok(response) => response,
                Err(error) if is_permanent_request_error(&error) => {
                    return ClassifiedAttempt::Permanent(artwork_request_error(
                        error,
                        "Failed to fetch image",
                    ));
                }
                Err(error) => {
                    return ClassifiedAttempt::Retry(artwork_request_error(
                        error,
                        "Failed to fetch image",
                    ));
                }
            };

            if response.status().is_success() {
                ClassifiedAttempt::Done(Some(response))
            } else if response.status() == reqwest::StatusCode::NOT_FOUND {
                ClassifiedAttempt::Done(None)
            } else if is_transient_status(response.status()) {
                ClassifiedAttempt::Retry(ImportError::CoverArtRequest {
                    failure: LookupFailure::Provider {
                        status: Some(response.status().as_u16()),
                    },
                    detail: format!("Image download failed with status {}", response.status()),
                })
            } else {
                ClassifiedAttempt::Permanent(ImportError::CoverArtRequest {
                    failure: LookupFailure::Provider {
                        status: Some(response.status().as_u16()),
                    },
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

/// Keep transport failures distinct from an invalid request or response content.
fn artwork_request_error(error: reqwest::Error, context: &str) -> ImportError {
    if is_permanent_request_error(&error) {
        return ImportError::Internal {
            detail: format!("{context}: {error:?}"),
        };
    }
    let detail = format!("{context}: {error}");
    let failure = if error.is_timeout() {
        LookupFailure::Timeout
    } else if let Some(status) = error.status() {
        LookupFailure::Provider {
            status: Some(status.as_u16()),
        }
    } else {
        LookupFailure::Network
    };
    ImportError::CoverArtRequest { failure, detail }
}

fn artwork_body_error(error: crate::util::http::HttpBodyError, context: &str) -> ImportError {
    match error {
        crate::util::http::HttpBodyError::Read(error) => artwork_request_error(error, context),
        crate::util::http::HttpBodyError::TooLarge { .. } => ImportError::CoverArt {
            detail: format!("{context}: {error}"),
        },
    }
}

/// Read bytes and content type from a successful image response.
async fn read_image_response(
    response: reqwest::Response,
    _image_url: &str,
) -> Result<ImageResponse, ImportError> {
    let bytes = crate::util::http::read_body_capped(response, crate::util::http::MAX_IMAGE_BYTES)
        .await
        .map_err(|error| artwork_body_error(error, "Failed to read image response"))?;
    let (_, content_type) =
        crate::util::cover::decode_cover(&bytes).map_err(|error| ImportError::CoverArt {
            detail: format!("Downloaded file is not a valid image: {error}"),
        })?;

    debug!("Downloaded cover art ({} bytes)", bytes.len());
    Ok(ImageResponse::Body {
        bytes,
        content_type,
    })
}

#[cfg(test)]
#[path = "cover_art_tests.rs"]
mod tests;
