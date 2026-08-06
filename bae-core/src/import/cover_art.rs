use crate::import::{ImportError, MetadataSource};
use crate::retry::{exponential_backoff, is_transient_status, retry_classified, ClassifiedAttempt};
use crate::util::content_type::ContentType;
use chrono::{DateTime, Utc};
use coven::ClockRef;
use lru::LruCache;
use reqwest::header::{
    HeaderMap, CACHE_CONTROL, ETAG, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED,
};
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tracing::debug;

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
pub(crate) fn push_unique_cover(covers: &mut Vec<RemoteCover>, cover: RemoteCover) {
    if !covers.iter().any(|existing| existing.url == cover.url) {
        covers.push(cover);
    }
}

/// Max retries for transient HTTP failures (network errors, 5xx responses).
const MAX_RETRIES: u32 = 3;

/// Base delay between retries (doubles each attempt: 1s, 2s, 4s).
const RETRY_BASE_DELAY: Duration = Duration::from_secs(1);

/// Capacity for the remote-image byte LRU. Sized for a typical session; a miss
/// costs one HTTP fetch.
const REMOTE_IMAGE_CACHE_CAPACITY: usize = 25;

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

/// The session cache of remote image bytes, keyed by URL, with HTTP freshness:
/// a fresh entry serves without a request, a stale one revalidates with a
/// conditional GET — a 304 refreshes its clock, a 200 replaces its bytes.
///
/// Provider art is a direct HTTP fetch against arbitrary hosts (CAA, Discogs
/// CDN), not part of either metadata API client, so it caches here. All three
/// readers — the cover picker in the UI, the commit worker, and `change_cover`
/// — share one instance off the library manager, so picking a cover and then
/// importing it hits the wire once.
#[derive(Clone)]
pub struct RemoteImageCache {
    clock: ClockRef,
    entries: Arc<Mutex<LruCache<String, CachedImage>>>,
    /// The base backoff a transient download failure is retried with, doubling
    /// each attempt. A test builds the cache with a near-zero one so a fetch
    /// that is meant to fail does not sleep the real 1s + 2s + 4s.
    retry_base_delay: Duration,
}

impl RemoteImageCache {
    pub fn new(clock: ClockRef) -> Self {
        Self::with_retry_base_delay(clock, RETRY_BASE_DELAY)
    }

    /// A cache whose downloads retry immediately, so a test whose fetch is
    /// meant to fail does not sleep the real 1s + 2s + 4s of backoff. Its clock
    /// is the system one: nothing a test asserts turns on image freshness, and
    /// a test that does exercise it builds the cache through [`Self::new`] with
    /// a clock of its own.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn for_test() -> Self {
        Self::with_retry_base_delay(
            std::sync::Arc::new(coven::SystemClock),
            Duration::from_millis(1),
        )
    }

    fn with_retry_base_delay(clock: ClockRef, retry_base_delay: Duration) -> Self {
        Self {
            clock,
            entries: Arc::new(Mutex::new(LruCache::new(
                NonZeroUsize::new(REMOTE_IMAGE_CACHE_CAPACITY)
                    .expect("REMOTE_IMAGE_CACHE_CAPACITY > 0"),
            ))),
            retry_base_delay,
        }
    }

    /// Bytes for a remote image URL, served from the cache while fresh and
    /// revalidated once stale. Retries transient failures (network errors, 5xx)
    /// up to `MAX_RETRIES` times.
    ///
    /// `Ok(None)` is the host answering that it serves no image at this address
    /// — the reply a derived cover address gets when the archive holds nothing
    /// for that entity. A caller that requires the bytes says so itself with
    /// [`Self::fetch_required`].
    pub async fn fetch(&self, url: &str) -> Result<Option<RemoteImage>, ImportError> {
        let base_delay = self.retry_base_delay;
        let now = self.clock.now();
        let cached = self
            .entries
            .lock()
            .expect("remote image cache mutex poisoned")
            .get(url)
            .cloned();

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
        self.entries
            .lock()
            .expect("remote image cache mutex poisoned")
            .put(url.to_string(), entry);
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
mod tests {
    use super::*;

    /// The archive's addresses are derived from the entity id alone: the
    /// release's own front image, and the album's, at fixed paths with a
    /// 250px thumbnail beside each.
    #[test]
    fn cover_art_archive_addresses_are_derived_from_the_entity_id() {
        let base = archive_base();

        let release = RemoteCover::musicbrainz_release("rel-1");
        assert_eq!(release.url, format!("{base}/release/rel-1/front"));
        assert_eq!(
            release.thumbnail_url,
            format!("{base}/release/rel-1/front-250")
        );
        assert_eq!(release.label, "Cover Art Archive");
        assert_eq!(release.source, MetadataSource::MusicBrainz);

        let group = RemoteCover::musicbrainz_release_group("rg-1");
        assert_eq!(group.url, format!("{base}/release-group/rg-1/front"));
        assert_eq!(
            group.thumbnail_url,
            format!("{base}/release-group/rg-1/front-250")
        );
        assert_eq!(group.label, "Cover Art Archive (Album)");
    }

    #[test]
    fn push_unique_cover_dedupes_by_url() {
        let mut covers = vec![RemoteCover {
            url: "https://caa.example/cover.jpg".to_string(),
            thumbnail_url: "https://caa.example/thumb-a.jpg".to_string(),
            label: "Cover Art Archive".to_string(),
            source: MetadataSource::MusicBrainz,
        }];

        push_unique_cover(
            &mut covers,
            RemoteCover {
                url: "https://caa.example/cover.jpg".to_string(),
                thumbnail_url: "https://caa.example/thumb-b.jpg".to_string(),
                label: "Cover Art Archive (Album)".to_string(),
                source: MetadataSource::MusicBrainz,
            },
        );

        assert_eq!(covers.len(), 1);
        assert_eq!(covers[0].thumbnail_url, "https://caa.example/thumb-a.jpg");
    }

    /// Spawn a localhost HTTP server that returns the given (status, body)
    /// responses in order (clamping to the last once exhausted), and return the
    /// URL to fetch. Lets the download path exercise real HTTP against
    /// controlled responses without reaching the network.
    async fn start_mock(responses: Vec<(u16, Vec<u8>)>) -> String {
        use axum::extract::State;
        use axum::http::StatusCode;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        #[derive(Clone)]
        struct Mock {
            responses: Arc<Vec<(u16, Vec<u8>)>>,
            idx: Arc<AtomicUsize>,
        }

        async fn handler(State(m): State<Mock>) -> (StatusCode, Vec<u8>) {
            let i = m
                .idx
                .fetch_add(1, Ordering::SeqCst)
                .min(m.responses.len() - 1);
            let (code, body) = &m.responses[i];
            (StatusCode::from_u16(*code).unwrap(), body.clone())
        }

        let state = Mock {
            responses: Arc::new(responses),
            idx: Arc::new(AtomicUsize::new(0)),
        };
        let app = axum::Router::new().fallback(handler).with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        format!("http://{addr}/cover.jpg")
    }

    async fn start_declared_length_response(content_length: usize) -> String {
        use tokio::io::AsyncWriteExt;

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("test listener should bind");
        let addr = listener
            .local_addr()
            .expect("test listener should have an address");
        tokio::spawn(async move {
            let (mut stream, _) = listener
                .accept()
                .await
                .expect("test request should connect");
            let response = format!("HTTP/1.1 200 OK\r\nContent-Length: {content_length}\r\n\r\n");
            stream
                .write_all(response.as_bytes())
                .await
                .expect("test response headers should write");
            std::future::pending::<()>().await;
        });
        format!("http://{addr}/cover.jpg")
    }

    /// A clock the test moves by hand, so each fetch sees exactly the instant
    /// the test chooses — coven's `SteppingClock` advances per `now()` call,
    /// which would tie the test to how often the cache reads the clock.
    struct TestClock(Mutex<DateTime<Utc>>);

    impl TestClock {
        fn at(seconds: i64) -> Arc<Self> {
            Arc::new(Self(Mutex::new(
                DateTime::from_timestamp(seconds, 0).expect("a valid test instant"),
            )))
        }

        fn advance(&self, seconds: i64) {
            let mut now = self.0.lock().expect("test clock mutex poisoned");
            *now += chrono::Duration::seconds(seconds);
        }
    }

    impl coven::Clock for TestClock {
        fn now(&self) -> DateTime<Utc> {
            *self.0.lock().expect("test clock mutex poisoned")
        }
    }

    /// One version of the image an [`ImageHost`] serves.
    struct ImageVersion {
        body: Vec<u8>,
        etag: Option<String>,
        cache_control: Option<String>,
    }

    #[derive(Clone)]
    struct ImageHost {
        versions: Arc<Vec<ImageVersion>>,
        current: Arc<std::sync::atomic::AtomicUsize>,
        hits: Arc<std::sync::atomic::AtomicUsize>,
        conditional_hits: Arc<std::sync::atomic::AtomicUsize>,
    }

    impl ImageHost {
        fn hits(&self) -> usize {
            self.hits.load(std::sync::atomic::Ordering::SeqCst)
        }

        fn conditional_hits(&self) -> usize {
            self.conditional_hits
                .load(std::sync::atomic::Ordering::SeqCst)
        }

        fn serve_version(&self, index: usize) {
            self.current
                .store(index, std::sync::atomic::Ordering::SeqCst);
        }
    }

    /// Spawn a localhost image host serving `versions` (the first until the test
    /// switches), answering 304 when the request's `If-None-Match` matches the
    /// served version's ETag, and counting every request that reached it.
    async fn start_image_host(versions: Vec<ImageVersion>) -> (ImageHost, String) {
        use axum::body::Body;
        use axum::extract::State;
        use axum::http::{HeaderMap, Response, StatusCode};
        use std::sync::atomic::{AtomicUsize, Ordering};

        async fn handler(State(host): State<ImageHost>, headers: HeaderMap) -> Response<Body> {
            host.hits.fetch_add(1, Ordering::SeqCst);
            let version = &host.versions[host.current.load(Ordering::SeqCst)];
            let mut builder = Response::builder();
            if let Some(etag) = &version.etag {
                builder = builder.header("etag", etag);
            }
            if let Some(cache_control) = &version.cache_control {
                builder = builder.header("cache-control", cache_control);
            }
            let sent = headers
                .get("if-none-match")
                .and_then(|value| value.to_str().ok());
            if sent.is_some() {
                host.conditional_hits.fetch_add(1, Ordering::SeqCst);
            }
            if sent.is_some() && sent == version.etag.as_deref() {
                return builder
                    .status(StatusCode::NOT_MODIFIED)
                    .body(Body::empty())
                    .expect("test 304 response");
            }
            builder
                .status(StatusCode::OK)
                .body(Body::from(version.body.clone()))
                .expect("test 200 response")
        }

        let host = ImageHost {
            versions: Arc::new(versions),
            current: Arc::new(AtomicUsize::new(0)),
            hits: Arc::new(AtomicUsize::new(0)),
            conditional_hits: Arc::new(AtomicUsize::new(0)),
        };
        let app = axum::Router::new()
            .fallback(handler)
            .with_state(host.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        (host, format!("http://{addr}/cover.jpg"))
    }

    fn cache_with(clock: Arc<TestClock>) -> RemoteImageCache {
        RemoteImageCache::with_retry_base_delay(clock as ClockRef, Duration::from_millis(1))
    }

    #[test]
    fn max_age_parses_from_cache_control_and_clamps() {
        let with = |value: &str| {
            let mut headers = HeaderMap::new();
            headers.insert(CACHE_CONTROL, value.parse().unwrap());
            max_age_from_cache_control(&headers)
        };
        assert_eq!(with("max-age=600"), Some(Duration::from_secs(600)));
        assert_eq!(
            with("public, max-age=120, immutable"),
            Some(Duration::from_secs(120))
        );
        // Directive names are case-insensitive.
        assert_eq!(with("Max-Age=90"), Some(Duration::from_secs(90)));
        // No max-age directive, and an unparsable one, both mean the response
        // stated no lifetime of its own.
        assert_eq!(with("no-transform"), None);
        assert_eq!(with("max-age=soon"), None);
        assert_eq!(max_age_from_cache_control(&HeaderMap::new()), None);
        // An absurd lifetime still revalidates within the clamp.
        assert_eq!(with("max-age=99999999999"), Some(MAX_REMOTE_IMAGE_TTL));
    }

    #[tokio::test]
    async fn fresh_entry_serves_without_a_request() {
        let body = vec![0xABu8; 256];
        let (host, url) = start_image_host(vec![ImageVersion {
            body: body.clone(),
            etag: Some("\"v1\"".to_string()),
            cache_control: Some("max-age=600".to_string()),
        }])
        .await;
        let clock = TestClock::at(1_700_000_000);
        let cache = cache_with(clock.clone());

        let first = cache.fetch_required(&url).await.unwrap();
        assert_eq!(first.bytes, body);
        assert_eq!(first.validator, "\"v1\"");

        // Still inside the declared 600s lifetime: served from memory.
        clock.advance(599);
        let second = cache.fetch_required(&url).await.unwrap();
        assert_eq!(second.bytes, body);
        assert_eq!(host.hits(), 1, "a fresh entry must not reach the wire");
    }

    #[tokio::test]
    async fn absent_cache_control_falls_back_to_the_default_ttl() {
        let body = vec![0x5Au8; 256];
        let (host, url) = start_image_host(vec![ImageVersion {
            body: body.clone(),
            etag: None,
            cache_control: None,
        }])
        .await;
        let clock = TestClock::at(1_700_000_000);
        let cache = cache_with(clock.clone());

        cache.fetch_required(&url).await.unwrap();
        clock.advance(DEFAULT_REMOTE_IMAGE_TTL.as_secs() as i64 - 1);
        cache.fetch_required(&url).await.unwrap();
        assert_eq!(host.hits(), 1, "still fresh under the default TTL");

        // Past the default TTL there is no ETag to revalidate with, so the
        // fetch is an unconditional re-download.
        clock.advance(2);
        let refetched = cache.fetch_required(&url).await.unwrap();
        assert_eq!(refetched.bytes, body);
        assert_eq!(host.hits(), 2);
        assert_eq!(host.conditional_hits(), 0);
    }

    #[tokio::test]
    async fn stale_entry_revalidates_and_not_modified_refreshes_the_clock() {
        let body = vec![0xCDu8; 256];
        let (host, url) = start_image_host(vec![ImageVersion {
            body: body.clone(),
            etag: Some("\"v1\"".to_string()),
            cache_control: Some("max-age=10".to_string()),
        }])
        .await;
        let clock = TestClock::at(1_700_000_000);
        let cache = cache_with(clock.clone());

        cache.fetch_required(&url).await.unwrap();

        // Stale: revalidated with If-None-Match, answered 304, bytes kept.
        clock.advance(11);
        let revalidated = cache.fetch_required(&url).await.unwrap();
        assert_eq!(revalidated.bytes, body);
        assert_eq!(revalidated.validator, "\"v1\"");
        assert_eq!(host.hits(), 2);
        assert_eq!(host.conditional_hits(), 1);

        // The 304 restarted the entry's lifetime, so the next read inside it
        // serves from memory instead of revalidating again.
        clock.advance(5);
        cache.fetch_required(&url).await.unwrap();
        assert_eq!(host.hits(), 2, "a 304 must refresh the entry's clock");
    }

    #[tokio::test]
    async fn revalidation_with_new_bytes_replaces_the_entry() {
        let first_body = vec![0x11u8; 256];
        let second_body = vec![0x22u8; 300];
        let (host, url) = start_image_host(vec![
            ImageVersion {
                body: first_body.clone(),
                etag: Some("\"v1\"".to_string()),
                cache_control: Some("max-age=10".to_string()),
            },
            ImageVersion {
                body: second_body.clone(),
                etag: Some("\"v2\"".to_string()),
                cache_control: Some("max-age=10".to_string()),
            },
        ])
        .await;
        let clock = TestClock::at(1_700_000_000);
        let cache = cache_with(clock.clone());

        let first = cache.fetch_required(&url).await.unwrap();
        assert_eq!(first.validator, "\"v1\"");

        host.serve_version(1);
        clock.advance(11);
        let second = cache.fetch_required(&url).await.unwrap();
        assert_eq!(second.bytes, second_body);
        assert_eq!(
            second.validator, "\"v2\"",
            "a 200 replaces the stored bytes and their validator"
        );

        // The replacement is what the next fresh read serves.
        clock.advance(1);
        let third = cache.fetch_required(&url).await.unwrap();
        assert_eq!(third.bytes, second_body);
        assert_eq!(host.hits(), 2);
    }

    #[tokio::test]
    async fn validator_is_the_content_hash_when_no_etag() {
        let body = vec![0x7Eu8; 256];
        let (_host, url) = start_image_host(vec![ImageVersion {
            body: body.clone(),
            etag: None,
            cache_control: Some("max-age=10".to_string()),
        }])
        .await;
        let cache = cache_with(TestClock::at(1_700_000_000));

        let image = cache.fetch_required(&url).await.unwrap();
        assert_eq!(image.validator, crate::util::fs::hash_bytes(&body));
    }

    #[tokio::test]
    async fn download_succeeds_and_then_serves_from_cache() {
        let body = vec![0xABu8; 256];
        let other = vec![0x11u8; 256];
        let url = start_mock(vec![(200, body.clone()), (200, other)]).await;
        let cache = cache_with(TestClock::at(1_700_000_000));

        let first = cache.fetch_required(&url).await.unwrap();
        assert_eq!(first.bytes, body);
        // The second call is served from the session cache: it returns the
        // first body, not the mock's second (different) response.
        let second = cache.fetch_required(&url).await.unwrap();
        assert_eq!(second.bytes, body, "second call should be a cache hit");
    }

    #[tokio::test]
    async fn download_rejects_too_small_response() {
        let url = start_mock(vec![(200, vec![0u8; 50])]).await; // under the 100-byte floor
        let cache = cache_with(TestClock::at(1_700_000_000));
        let err = cache.fetch(&url).await.unwrap_err();
        assert!(
            matches!(&err, ImportError::CoverArt { detail } if detail.contains("too small")),
            "got: {err}"
        );
    }

    #[tokio::test]
    async fn download_rejects_declared_over_cap_response() {
        let url = start_declared_length_response(crate::util::http::MAX_IMAGE_BYTES + 1).await;
        let cache = cache_with(TestClock::at(1_700_000_000));
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            cache.fetch_required(&url),
        )
        .await
        .expect("oversized response should fail before reading the body");
        let err = result.unwrap_err();
        assert!(
            matches!(&err, ImportError::CoverArt { detail } if detail.contains("too large")),
            "got: {err}"
        );
    }

    #[tokio::test]
    async fn download_retries_transient_then_succeeds() {
        let body = vec![0xCDu8; 256];
        // A 503 is transient: retried after backoff, then the 200 succeeds.
        // `for_test` gives the cache a near-zero backoff, so the retry does not
        // sleep the real second.
        let url = start_mock(vec![(503, vec![]), (200, body.clone())]).await;
        let cache = cache_with(TestClock::at(1_700_000_000));
        let image = cache.fetch_required(&url).await.unwrap();
        assert_eq!(image.bytes, body);
    }

    #[tokio::test]
    async fn a_404_is_no_image_rather_than_a_failed_download() {
        // 404 is non-transient: answered without burning the retry budget.
        let url = start_mock(vec![(404, vec![])]).await;
        let cache = cache_with(TestClock::at(1_700_000_000));
        // A 404 is the host answering that it serves no image at this address —
        // an answer, and the one a derived cover address gets when the archive
        // holds nothing. A caller that needs the bytes names that a failure.
        assert!(cache.fetch(&url).await.unwrap().is_none());
        assert!(cache.fetch_required(&url).await.is_err());
    }
}
