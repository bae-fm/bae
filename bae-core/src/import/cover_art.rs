use crate::import::{ImportError, MetadataSource};
use crate::network::upgrade_to_https;
use crate::retry::{exponential_backoff, is_transient_status, retry_classified, ClassifiedAttempt};
use crate::util::content_type::ContentType;
use chrono::{DateTime, Utc};
use coven::ClockRef;
use lru::LruCache;
use reqwest::header::{
    HeaderMap, CACHE_CONTROL, ETAG, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED,
};
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tokio::sync::{OnceCell, Semaphore};
use tracing::{debug, warn};

/// A remote cover art option from an external source.
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

/// Max retries for transient HTTP failures (network errors, 5xx responses).
const MAX_RETRIES: u32 = 3;

/// Base delay between retries (doubles each attempt: 1s, 2s, 4s).
const RETRY_BASE_DELAY: Duration = Duration::from_secs(1);

const CAA_LOOKUP_CACHE_CAPACITY: usize = 128;
const CAA_MAX_CONCURRENT_LOOKUPS: usize = 4;

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

/// Outer `None` means the lookup failed and must not be cached; inner `None`
/// means CAA returned a cacheable no-cover result.
type CaaLookup = Option<Option<RemoteCover>>;
type CaaLookupCell = Arc<OnceCell<CaaLookup>>;

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

/// Cover Art Archive lookup client for release/release-group cover selection.
#[derive(Clone)]
pub struct CoverArtArchiveClient {
    http: reqwest::Client,
    lookup_cache: Arc<Mutex<LruCache<String, Option<RemoteCover>>>>,
    in_flight: Arc<Mutex<HashMap<String, CaaLookupCell>>>,
    lookup_limiter: Arc<Semaphore>,
    retry_base_delay: Duration,
    /// When set, any lookup that misses the cache panics instead of hitting the
    /// network. Hermetic tests inject such a client (via [`hermetic`]) so an
    /// unseeded cover-art lookup fails loud rather than silently reaching the
    /// live Cover Art Archive and returning "no cover".
    #[cfg(feature = "test-utils")]
    forbid_network: bool,
}

impl CoverArtArchiveClient {
    pub fn new() -> Self {
        Self::with_retry_base_delay(RETRY_BASE_DELAY)
    }

    fn with_retry_base_delay(retry_base_delay: Duration) -> Self {
        let http = crate::util::http::client_builder()
            .build()
            .expect("Failed to create HTTP client for Cover Art Archive");
        Self {
            http,
            lookup_cache: Arc::new(Mutex::new(LruCache::new(
                NonZeroUsize::new(CAA_LOOKUP_CACHE_CAPACITY)
                    .expect("CAA_LOOKUP_CACHE_CAPACITY > 0"),
            ))),
            in_flight: Arc::new(Mutex::new(HashMap::new())),
            lookup_limiter: Arc::new(Semaphore::new(CAA_MAX_CONCURRENT_LOOKUPS)),
            retry_base_delay,
            #[cfg(feature = "test-utils")]
            forbid_network: false,
        }
    }

    /// The base backoff this client retries transient failures with (doubling
    /// each attempt). The import service reads it to give the cover-bytes
    /// download the same retry cadence as this client's lookups, so a test that
    /// injects a near-zero delay controls both paths through one seam.
    ///
    /// Gated with `import::service`, its only caller: the import pipeline is
    /// desktop-only, so on mobile this accessor is dead and `dead_code` denies it.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) fn retry_base_delay(&self) -> Duration {
        self.retry_base_delay
    }

    /// A client for hermetic tests: any cover-art lookup that misses the cache
    /// panics rather than reaching the live Cover Art Archive, so an accidental
    /// unseeded fetch is a test failure instead of a silent network call.
    #[cfg(feature = "test-utils")]
    pub fn hermetic() -> Self {
        let mut client = Self::with_retry_base_delay(Duration::from_millis(0));
        client.forbid_network = true;
        client
    }

    /// Pre-populate the lookup cache for a release and its release group, so a
    /// hermetic test can drive the prefetch path without the network. `None` is
    /// "no cover art" — the natural answer for a synthetic test release.
    #[cfg(feature = "test-utils")]
    pub fn seed_lookup(
        &self,
        release_id: Option<&str>,
        release_group_id: Option<&str>,
        cover: Option<RemoteCover>,
    ) {
        let mut cache = self
            .lookup_cache
            .lock()
            .expect("Cover Art Archive lookup cache mutex poisoned");
        if let Some(id) = release_id {
            cache.put(lookup_cache_key("release", id), cover.clone());
        }
        if let Some(id) = release_group_id {
            cache.put(lookup_cache_key("release-group", id), cover);
        }
    }

    /// Fetch cover art candidates from Cover Art Archive for a MusicBrainz
    /// release and release group.
    pub async fn fetch_candidates(
        &self,
        release_id: Option<&str>,
        release_group_id: Option<&str>,
    ) -> Vec<RemoteCover> {
        let mut covers = Vec::new();
        if let Some(rid) = release_id {
            if let Some(cover) = self.fetch_release(rid).await {
                push_unique_cover(&mut covers, cover);
            }
        }
        if let Some(rg_id) = release_group_id {
            if let Some(cover) = self.fetch_release_group(rg_id).await {
                push_unique_cover(&mut covers, cover);
            }
        }
        covers
    }

    /// Cover art from the Cover Art Archive for a MusicBrainz release. Retries
    /// transient failures (network errors, 5xx), but never a 404 (no cover art
    /// exists) or another client error.
    pub async fn fetch_release(&self, release_id: &str) -> Option<RemoteCover> {
        self.fetch_entity(
            "release",
            "release",
            release_id,
            MetadataSource::MusicBrainz.cover_source_label().to_string(),
        )
        .await
    }

    /// Cover art from the Cover Art Archive for a MusicBrainz release group. This
    /// endpoint returns the community-selected cover for the album as a concept,
    /// which may differ from any specific release's cover.
    pub async fn fetch_release_group(&self, release_group_id: &str) -> Option<RemoteCover> {
        self.fetch_entity(
            "release-group",
            "release group",
            release_group_id,
            format!(
                "{} (Album)",
                MetadataSource::MusicBrainz.cover_source_label()
            ),
        )
        .await
    }
}

impl Default for CoverArtArchiveClient {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) fn push_unique_cover(covers: &mut Vec<RemoteCover>, cover: RemoteCover) {
    if !covers.iter().any(|existing| existing.url == cover.url) {
        covers.push(cover);
    }
}

/// Key one CAA entity's cover lookup. The seeding seam and the fetch path both
/// go through it, so a seeded lookup is the one the fetch reads.
fn lookup_cache_key(caa_entity: &str, id: &str) -> String {
    format!("{caa_entity}:{id}")
}

impl CoverArtArchiveClient {
    async fn fetch_entity(
        &self,
        caa_entity: &str,
        log_entity: &str,
        id: &str,
        label: String,
    ) -> Option<RemoteCover> {
        self.fetch_url(
            lookup_cache_key(caa_entity, id),
            format!("https://coverartarchive.org/{caa_entity}/{id}"),
            log_entity,
            id,
            label,
        )
        .await
    }

    async fn fetch_url(
        &self,
        cache_key: String,
        json_url: String,
        entity: &str,
        id: &str,
        label: String,
    ) -> Option<RemoteCover> {
        if let Some(cached) = self
            .lookup_cache
            .lock()
            .expect("Cover Art Archive lookup cache mutex poisoned")
            .get(&cache_key)
            .cloned()
        {
            return cached;
        }

        #[cfg(feature = "test-utils")]
        assert!(
            !self.forbid_network,
            "hermetic test made a live Cover Art Archive request for {json_url} \
             ({entity} {id}); seed the lookup cache instead of hitting the network"
        );

        let cell = {
            let mut inflight = self
                .in_flight
                .lock()
                .expect("Cover Art Archive in-flight map mutex poisoned");
            inflight
                .entry(cache_key.clone())
                .or_insert_with(|| Arc::new(OnceCell::new()))
                .clone()
        };

        let owned_id = id.to_string();
        let entity = entity.to_string();
        let base_delay = self.retry_base_delay;
        cell.get_or_init(|| async move {
            let lookup = match self.lookup_limiter.acquire().await {
                Ok(permit) => {
                    let _permit = permit;
                    fetch_cover_art_from_url(
                        &self.http, json_url, &entity, &owned_id, label, base_delay,
                    )
                    .await
                }
                Err(e) => {
                    warn!("Cover Art Archive lookup limiter closed: {}", e);
                    None
                }
            };
            if let Some(cache_value) = &lookup {
                self.lookup_cache
                    .lock()
                    .expect("Cover Art Archive lookup cache mutex poisoned")
                    .put(cache_key.clone(), cache_value.clone());
            }
            self.in_flight
                .lock()
                .expect("Cover Art Archive in-flight map mutex poisoned")
                .remove(&cache_key);
            lookup
        })
        .await
        .clone()
        .flatten()
    }
}

/// Shared implementation for fetching cover art from a Cover Art Archive URL.
async fn fetch_cover_art_from_url(
    client: &reqwest::Client,
    json_url: String,
    entity: &str,
    id: &str,
    label: String,
    base_delay: Duration,
) -> CaaLookup {
    debug!("Fetching cover art from Cover Art Archive: {}", json_url);

    match retry_classified(
        MAX_RETRIES + 1,
        "Cover Art Archive fetch",
        |attempt| exponential_backoff(base_delay, attempt),
        || async {
            match client.get(&json_url).send().await {
                Ok(response) => {
                    if response.status().is_success() {
                        ClassifiedAttempt::Done(
                            parse_cover_art_response(response, &json_url, &label).await,
                        )
                    } else if response.status() == reqwest::StatusCode::NOT_FOUND {
                        debug!(
                            "No cover art found in Cover Art Archive for {} {}",
                            entity, id
                        );
                        ClassifiedAttempt::Done(Some(None))
                    } else if is_transient_status(response.status()) {
                        ClassifiedAttempt::Retry(format!("status {}", response.status()))
                    } else {
                        // A non-transient client error (400, 403, ...) — don't retry.
                        debug!(
                            "Cover Art Archive returned status {} for {} {}",
                            response.status(),
                            entity,
                            id
                        );
                        ClassifiedAttempt::Done(None)
                    }
                }
                Err(e) if is_permanent_request_error(&e) => {
                    ClassifiedAttempt::Permanent(format!("Cover Art Archive fetch failed: {}", e))
                }
                Err(e) => ClassifiedAttempt::Retry(e.to_string()),
            }
        },
    )
    .await
    {
        Ok(lookup) => lookup,
        Err(e) => {
            warn!("Cover Art Archive fetch failed for {}: {}", json_url, e);
            None
        }
    }
}

/// Parse the Cover Art Archive JSON response, extracting the best image URL.
async fn parse_cover_art_response(
    response: reqwest::Response,
    url: &str,
    label: &str,
) -> CaaLookup {
    let json = match response.json::<serde_json::Value>().await {
        Ok(json) => json,
        Err(e) => {
            warn!(
                "Cover Art Archive returned malformed JSON from {}: {}",
                url, e
            );
            return None;
        }
    };
    match select_cover_candidate(&json, label) {
        Some(cover) => Some(Some(cover)),
        None => {
            debug!(
                "Cover Art Archive JSON from {} had no usable cover image",
                url
            );
            Some(None)
        }
    }
}

/// Pick the best image URL from a Cover Art Archive JSON body: the front cover,
/// else the first image. Pure (no I/O), so the selection rules are testable
/// without a live response.
fn select_cover_candidate(json: &serde_json::Value, label: &str) -> Option<RemoteCover> {
    let images = json.get("images").and_then(|i| i.as_array())?;

    for image in images {
        if image.get("front").and_then(|f| f.as_bool()) == Some(true) {
            if let Some(cover) = extract_cover_candidate(image, label) {
                return Some(cover);
            }
        }
    }

    // Fall back to the first image with a usable URL.
    images
        .iter()
        .find_map(|image| extract_cover_candidate(image, label))
}

/// Extract the selected image URL and thumbnail from a Cover Art Archive image entry.
fn extract_cover_candidate(image: &serde_json::Value, label: &str) -> Option<RemoteCover> {
    let url = extract_image_url(image)?;
    let thumbnail_url = extract_thumbnail_url(image, &url);
    Some(RemoteCover {
        url,
        thumbnail_url,
        label: label.to_string(),
        source: MetadataSource::MusicBrainz,
    })
}

/// Extract the image URL from a Cover Art Archive image entry.
fn extract_image_url(image: &serde_json::Value) -> Option<String> {
    let (key, url) = find_url_in(image, &["image", "thumb", "small"])?;
    debug!("Using cover art ({}): {}", key, url);
    Some(url)
}

/// Extract the thumbnail URL from a Cover Art Archive image entry.
fn extract_thumbnail_url(image: &serde_json::Value, image_url: &str) -> String {
    if let Some(thumbnails) = image.get("thumbnails") {
        if let Some((_key, url)) = find_url_in(thumbnails, &["250", "small", "500", "large"]) {
            return url;
        }
    }
    if let Some((_key, url)) = find_url_in(image, &["thumb", "small"]) {
        return url;
    }
    debug!("CAA image {image_url} has no thumbnail URL; using image URL");
    image_url.to_string()
}

fn find_url_in<'a>(value: &serde_json::Value, keys: &'a [&'a str]) -> Option<(&'a str, String)> {
    for key in keys {
        if let Some(url) = value.get(*key).and_then(|v| v.as_str()) {
            return Some((*key, upgrade_to_https(url)));
        }
    }
    None
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

/// What one image request returned: a body, or a 304 saying the bytes already
/// held are still current.
enum ImageResponse {
    Body {
        bytes: Vec<u8>,
        content_type: ContentType,
        freshness: Freshness,
    },
    NotModified {
        freshness: Freshness,
    },
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
}

impl RemoteImageCache {
    pub fn new(clock: ClockRef) -> Self {
        Self {
            clock,
            entries: Arc::new(Mutex::new(LruCache::new(
                NonZeroUsize::new(REMOTE_IMAGE_CACHE_CAPACITY)
                    .expect("REMOTE_IMAGE_CACHE_CAPACITY > 0"),
            ))),
        }
    }

    /// Bytes for a remote image URL, served from the cache while fresh and
    /// revalidated once stale. Retries transient failures (network errors, 5xx)
    /// up to `MAX_RETRIES` times.
    pub async fn fetch(&self, url: &str) -> Result<RemoteImage, ImportError> {
        self.fetch_with_backoff(url, RETRY_BASE_DELAY).await
    }

    pub(crate) async fn fetch_with_backoff(
        &self,
        url: &str,
        base_delay: Duration,
    ) -> Result<RemoteImage, ImportError> {
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
                return Ok(entry.to_remote_image());
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
        Ok(image)
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
    use serde_json::json;

    #[test]
    fn select_cover_candidate_prefers_front_then_first() {
        // A front cover wins even when it isn't first in the list.
        let body = json!({
            "images": [
                { "front": false, "image": "https://caa.example/back.jpg" },
                {
                    "front": true,
                    "image": "https://caa.example/front.jpg",
                    "thumbnails": {
                        "250": "https://caa.example/front-250.jpg",
                        "500": "https://caa.example/front-500.jpg"
                    }
                },
            ]
        });
        let cover = select_cover_candidate(&body, "Cover Art Archive").unwrap();
        assert_eq!(cover.url, "https://caa.example/front.jpg");
        assert_eq!(cover.thumbnail_url, "https://caa.example/front-250.jpg");
        assert_eq!(cover.label, "Cover Art Archive");
        assert_eq!(cover.source, MetadataSource::MusicBrainz);

        // With no front cover, the first image is the fallback.
        let body = json!({
            "images": [
                { "front": false, "image": "https://caa.example/a.jpg" },
                { "front": false, "image": "https://caa.example/b.jpg" },
            ]
        });
        let cover = select_cover_candidate(&body, "Cover Art Archive").unwrap();
        assert_eq!(cover.url, "https://caa.example/a.jpg");
        assert_eq!(cover.thumbnail_url, "https://caa.example/a.jpg");

        // No images array / empty list means nothing to select.
        assert!(select_cover_candidate(&json!({}), "Cover Art Archive").is_none());
        assert!(select_cover_candidate(&json!({ "images": [] }), "Cover Art Archive").is_none());
    }

    #[test]
    fn extract_cover_candidate_walks_keys_and_upgrades_to_https() {
        // 'image' is preferred over 'thumb'/'small' and http is upgraded.
        let image = json!({
            "image": "http://caa.example/full.jpg",
            "thumbnails": {
                "small": "http://caa.example/thumb.jpg",
            },
        });
        let cover = extract_cover_candidate(&image, "Cover Art Archive").unwrap();
        assert_eq!(cover.url, "https://caa.example/full.jpg");
        assert_eq!(cover.thumbnail_url, "https://caa.example/thumb.jpg");

        // Falls through to 'thumb' then 'small' when 'image' is absent.
        assert_eq!(
            extract_image_url(&json!({ "small": "https://caa.example/s.jpg" })),
            Some("https://caa.example/s.jpg".to_string())
        );

        // An entry with none of the keys yields nothing.
        assert_eq!(extract_image_url(&json!({ "front": true })), None);
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

    #[tokio::test]
    async fn caa_lookup_does_not_cache_transient_failure() {
        let success = br#"{
            "images": [
                {
                    "front": true,
                    "image": "https://caa.example/cover.jpg",
                    "thumbnails": {
                        "250": "https://caa.example/thumb.jpg"
                    }
                }
            ]
        }"#
        .to_vec();
        let url = start_mock(vec![
            (503, vec![]),
            (503, vec![]),
            (503, vec![]),
            (503, vec![]),
            (200, success),
        ])
        .await;
        let cache_key = "release:transient-cache-test".to_string();
        let label = MetadataSource::MusicBrainz.cover_source_label().to_string();
        // Inject a near-zero backoff so the four transient failures don't sleep
        // the real 1s + 2s + 4s.
        let client = CoverArtArchiveClient::with_retry_base_delay(Duration::from_millis(1));

        assert!(client
            .fetch_url(
                cache_key.clone(),
                url.clone(),
                "release",
                "transient-cache-test",
                label.clone(),
            )
            .await
            .is_none());

        let cover = client
            .fetch_url(cache_key, url, "release", "transient-cache-test", label)
            .await
            .expect("transient failure should not be cached");

        assert_eq!(cover.url, "https://caa.example/cover.jpg");
        assert_eq!(cover.thumbnail_url, "https://caa.example/thumb.jpg");
    }

    #[tokio::test]
    async fn caa_lookup_caches_selected_cover() {
        let first = br#"{
            "images": [
                {
                    "front": true,
                    "image": "https://caa.example/cover-a.jpg",
                    "thumbnails": {
                        "250": "https://caa.example/thumb-a.jpg"
                    }
                }
            ]
        }"#
        .to_vec();
        let second = br#"{
            "images": [
                {
                    "front": true,
                    "image": "https://caa.example/cover-b.jpg",
                    "thumbnails": {
                        "250": "https://caa.example/thumb-b.jpg"
                    }
                }
            ]
        }"#
        .to_vec();
        let url = start_mock(vec![(200, first), (200, second)]).await;
        let cache_key = "release:cover-cache-test".to_string();
        let label = MetadataSource::MusicBrainz.cover_source_label().to_string();
        let client = CoverArtArchiveClient::new();

        let first = client
            .fetch_url(
                cache_key.clone(),
                url.clone(),
                "release",
                "cover-cache-test",
                label.clone(),
            )
            .await
            .unwrap();
        let second = client
            .fetch_url(cache_key, url, "release", "cover-cache-test", label)
            .await
            .unwrap();

        assert_eq!(first.url, "https://caa.example/cover-a.jpg");
        assert_eq!(second.url, "https://caa.example/cover-a.jpg");
    }

    #[tokio::test]
    async fn caa_lookup_caches_not_found() {
        let success = br#"{
            "images": [
                {
                    "front": true,
                    "image": "https://caa.example/cover.jpg",
                    "thumbnails": {
                        "250": "https://caa.example/thumb.jpg"
                    }
                }
            ]
        }"#
        .to_vec();
        let url = start_mock(vec![(404, vec![]), (200, success)]).await;
        let cache_key = "release:not-found-cache-test".to_string();
        let label = MetadataSource::MusicBrainz.cover_source_label().to_string();
        let client = CoverArtArchiveClient::new();

        assert!(client
            .fetch_url(
                cache_key.clone(),
                url.clone(),
                "release",
                "not-found-cache-test",
                label.clone(),
            )
            .await
            .is_none());
        assert!(client
            .fetch_url(cache_key, url, "release", "not-found-cache-test", label,)
            .await
            .is_none());
    }

    /// Spawn a localhost server that counts every request and always answers
    /// 200 with `body`. Lets a test assert how many fetches reached the wire.
    async fn start_counting_mock(
        hits: Arc<std::sync::atomic::AtomicUsize>,
        body: Vec<u8>,
    ) -> String {
        use axum::extract::State;
        use axum::http::StatusCode;
        use std::sync::atomic::Ordering;

        #[derive(Clone)]
        struct Mock {
            hits: Arc<std::sync::atomic::AtomicUsize>,
            body: Arc<Vec<u8>>,
        }

        async fn handler(State(m): State<Mock>) -> (StatusCode, Vec<u8>) {
            m.hits.fetch_add(1, Ordering::SeqCst);
            (StatusCode::OK, (*m.body).clone())
        }

        let state = Mock {
            hits,
            body: Arc::new(body),
        };
        let app = axum::Router::new().fallback(handler).with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        format!("http://{addr}/cover.jpg")
    }

    #[tokio::test]
    async fn caa_lookup_coalesces_concurrent_fetches_into_one_request() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let success = br#"{
            "images": [
                {
                    "front": true,
                    "image": "https://caa.example/cover.jpg",
                    "thumbnails": {
                        "250": "https://caa.example/thumb.jpg"
                    }
                }
            ]
        }"#
        .to_vec();
        let hits = Arc::new(AtomicUsize::new(0));
        let url = start_counting_mock(hits.clone(), success).await;
        let cache_key = "release:coalesce-test".to_string();
        let label = MetadataSource::MusicBrainz.cover_source_label().to_string();
        let client = CoverArtArchiveClient::new();

        // Two fetches for the same key, launched together, share the in-flight
        // OnceCell: only one reaches the wire, and both see the same cover.
        let (a, b) = tokio::join!(
            client.fetch_url(
                cache_key.clone(),
                url.clone(),
                "release",
                "coalesce-test",
                label.clone(),
            ),
            client.fetch_url(
                cache_key.clone(),
                url.clone(),
                "release",
                "coalesce-test",
                label.clone(),
            ),
        );

        let a = a.expect("first concurrent fetch resolves a cover");
        let b = b.expect("second concurrent fetch resolves a cover");
        assert_eq!(a.url, "https://caa.example/cover.jpg");
        assert_eq!(a.url, b.url);
        assert_eq!(
            hits.load(Ordering::SeqCst),
            1,
            "concurrent fetches for one key must hit the wire exactly once"
        );
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
        RemoteImageCache::new(clock as ClockRef)
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

        let first = cache.fetch(&url).await.unwrap();
        assert_eq!(first.bytes, body);
        assert_eq!(first.validator, "\"v1\"");

        // Still inside the declared 600s lifetime: served from memory.
        clock.advance(599);
        let second = cache.fetch(&url).await.unwrap();
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

        cache.fetch(&url).await.unwrap();
        clock.advance(DEFAULT_REMOTE_IMAGE_TTL.as_secs() as i64 - 1);
        cache.fetch(&url).await.unwrap();
        assert_eq!(host.hits(), 1, "still fresh under the default TTL");

        // Past the default TTL there is no ETag to revalidate with, so the
        // fetch is an unconditional re-download.
        clock.advance(2);
        let refetched = cache.fetch(&url).await.unwrap();
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

        cache.fetch(&url).await.unwrap();

        // Stale: revalidated with If-None-Match, answered 304, bytes kept.
        clock.advance(11);
        let revalidated = cache.fetch(&url).await.unwrap();
        assert_eq!(revalidated.bytes, body);
        assert_eq!(revalidated.validator, "\"v1\"");
        assert_eq!(host.hits(), 2);
        assert_eq!(host.conditional_hits(), 1);

        // The 304 restarted the entry's lifetime, so the next read inside it
        // serves from memory instead of revalidating again.
        clock.advance(5);
        cache.fetch(&url).await.unwrap();
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

        let first = cache.fetch(&url).await.unwrap();
        assert_eq!(first.validator, "\"v1\"");

        host.serve_version(1);
        clock.advance(11);
        let second = cache.fetch(&url).await.unwrap();
        assert_eq!(second.bytes, second_body);
        assert_eq!(
            second.validator, "\"v2\"",
            "a 200 replaces the stored bytes and their validator"
        );

        // The replacement is what the next fresh read serves.
        clock.advance(1);
        let third = cache.fetch(&url).await.unwrap();
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

        let image = cache.fetch(&url).await.unwrap();
        assert_eq!(image.validator, crate::util::fs::hash_bytes(&body));
    }

    #[tokio::test]
    async fn download_succeeds_and_then_serves_from_cache() {
        let body = vec![0xABu8; 256];
        let other = vec![0x11u8; 256];
        let url = start_mock(vec![(200, body.clone()), (200, other)]).await;
        let cache = cache_with(TestClock::at(1_700_000_000));

        let first = cache.fetch(&url).await.unwrap();
        assert_eq!(first.bytes, body);
        // The second call is served from the session cache: it returns the
        // first body, not the mock's second (different) response.
        let second = cache.fetch(&url).await.unwrap();
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
        let result = tokio::time::timeout(std::time::Duration::from_secs(1), cache.fetch(&url))
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
        // A near-zero backoff keeps the retry from sleeping the real second.
        let url = start_mock(vec![(503, vec![]), (200, body.clone())]).await;
        let cache = cache_with(TestClock::at(1_700_000_000));
        let image = cache
            .fetch_with_backoff(&url, Duration::from_millis(1))
            .await
            .unwrap();
        assert_eq!(image.bytes, body);
    }

    #[tokio::test]
    async fn download_does_not_retry_client_error() {
        // 404 is non-transient: fail without burning the retry budget.
        let url = start_mock(vec![(404, vec![])]).await;
        let cache = cache_with(TestClock::at(1_700_000_000));
        assert!(cache.fetch(&url).await.is_err());
    }
}
