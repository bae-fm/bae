use crate::import::{ImportError, MetadataSource};
use crate::network::upgrade_to_https;
use crate::util::content_type::ContentType;
use lru::LruCache;
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tokio::sync::{OnceCell, Semaphore};
use tracing::{debug, warn};

/// A remote cover art option from an external source.
#[derive(Debug, Clone, PartialEq)]
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

/// Capacity for the cover-bytes LRU cache. Sized for a typical session;
/// a miss costs one HTTP fetch.
const COVER_CACHE_CAPACITY: usize = 25;

/// In-memory cache for downloaded cover bytes, keyed by URL — covers are direct
/// HTTP fetches against arbitrary hosts (CAA, Discogs CDN), not part of either
/// metadata API client. Both callers of `download_cover_art_bytes` — the UI at
/// cover-pick time and the commit worker at import time — read through it, so a
/// URL hits the wire at most once per session.
type CoverCacheValue = (Vec<u8>, ContentType);
/// Outer `None` means the lookup failed and must not be cached; inner `None`
/// means CAA returned a cacheable no-cover result.
type CaaLookup = Option<Option<RemoteCover>>;
type CaaLookupCell = Arc<OnceCell<CaaLookup>>;

fn cover_bytes_cache() -> &'static Mutex<LruCache<String, CoverCacheValue>> {
    static CACHE: OnceLock<Mutex<LruCache<String, CoverCacheValue>>> = OnceLock::new();
    CACHE.get_or_init(|| {
        Mutex::new(LruCache::new(
            NonZeroUsize::new(COVER_CACHE_CAPACITY).expect("COVER_CACHE_CAPACITY > 0"),
        ))
    })
}

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

/// Whether an HTTP error is transient and worth retrying.
fn is_transient_status(status: reqwest::StatusCode) -> bool {
    status.is_server_error() || status == reqwest::StatusCode::TOO_MANY_REQUESTS
}

enum ClassifiedAttempt<T, E> {
    Done(T),
    Retry(E),
    Permanent(E),
}

async fn retry_classified<T, E, F, Fut>(
    operation: &str,
    base_delay: Duration,
    mut attempt: F,
) -> Result<T, E>
where
    E: std::fmt::Display,
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = ClassifiedAttempt<T, E>>,
{
    let mut last_error: Option<E> = None;
    for attempt_index in 0..=MAX_RETRIES {
        if attempt_index > 0 {
            let delay = base_delay * 2u32.pow(attempt_index - 1);

            warn!(
                "{} failed (attempt {}/{}): {} — retrying in {:?}",
                operation,
                attempt_index,
                MAX_RETRIES + 1,
                last_error
                    .as_ref()
                    .expect("a retry set last_error before this attempt"),
                delay
            );

            tokio::time::sleep(delay).await;
        }

        match attempt().await {
            ClassifiedAttempt::Done(value) => return Ok(value),
            ClassifiedAttempt::Permanent(error) => return Err(error),
            ClassifiedAttempt::Retry(error) => {
                last_error = Some(error);
            }
        }
    }

    let last_error = last_error.expect("the retry loop ran at least once and never succeeded");
    warn!(
        "{} failed after {} attempts: {}",
        operation,
        MAX_RETRIES + 1,
        last_error
    );

    Err(last_error)
}

pub(crate) fn push_unique_cover(covers: &mut Vec<RemoteCover>, cover: RemoteCover) {
    if !covers.iter().any(|existing| existing.url == cover.url) {
        covers.push(cover);
    }
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
            format!("{caa_entity}:{id}"),
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

    match retry_classified("Cover Art Archive fetch", base_delay, || async {
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
    })
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

/// Download cover art from a URL, returning the raw bytes and content type.
/// Retries transient failures (network errors, 5xx) up to `MAX_RETRIES` times.
/// Reads through the session-wide LRU cache, so a URL downloads at most once.
pub async fn download_cover_art_bytes(
    cover_art_url: &str,
) -> Result<(Vec<u8>, ContentType), ImportError> {
    download_cover_art_bytes_with_backoff(cover_art_url, RETRY_BASE_DELAY).await
}

pub(crate) async fn download_cover_art_bytes_with_backoff(
    cover_art_url: &str,
    base_delay: Duration,
) -> Result<(Vec<u8>, ContentType), ImportError> {
    if let Some(hit) = cover_bytes_cache()
        .lock()
        .expect("cover bytes cache mutex poisoned")
        .get(cover_art_url)
        .cloned()
    {
        debug!("Cover bytes cache hit for {}", cover_art_url);
        return Ok(hit);
    }

    debug!("Downloading cover art from {}", cover_art_url);

    let value =
        download_image_bytes_with_backoff(cover_art_url, "Cover art download", base_delay).await?;

    cover_bytes_cache()
        .lock()
        .expect("cover bytes cache mutex poisoned")
        .put(cover_art_url.to_string(), value.clone());
    Ok(value)
}

pub(crate) async fn download_image_bytes(
    image_url: &str,
    operation: &str,
) -> Result<(Vec<u8>, ContentType), ImportError> {
    download_image_bytes_with_backoff(image_url, operation, RETRY_BASE_DELAY).await
}

async fn download_image_bytes_with_backoff(
    image_url: &str,
    operation: &str,
    base_delay: Duration,
) -> Result<(Vec<u8>, ContentType), ImportError> {
    let client = image_download_client()?;
    retry_classified(operation, base_delay, || async {
        let response = match client.get(image_url).send().await {
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

        if response.status().is_success() {
            match read_image_response(response, image_url).await {
                Ok(value) => ClassifiedAttempt::Done(value),
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
    })
    .await
}

/// Errors that fail the same way every attempt — a URL-parse / request-builder
/// failure, or a redirect loop bottoming out. Retrying only burns the backoff
/// budget. Network/timeout/connection errors are transient and do keep retrying.
fn is_permanent_request_error(e: &reqwest::Error) -> bool {
    e.is_builder() || e.is_redirect()
}

/// Read bytes and content type from a successful image response.
async fn read_image_response(
    response: reqwest::Response,
    image_url: &str,
) -> Result<(Vec<u8>, ContentType), ImportError> {
    let content_type =
        super::image_response::image_content_type_from_response(response.headers(), image_url);

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
    Ok((bytes, content_type))
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

    #[tokio::test]
    async fn download_succeeds_and_then_serves_from_cache() {
        let body = vec![0xABu8; 256];
        let other = vec![0x11u8; 256];
        let url = start_mock(vec![(200, body.clone()), (200, other)]).await;

        let (first, _) = download_cover_art_bytes(&url).await.unwrap();
        assert_eq!(first, body);
        // The second call is served from the session cache: it returns the
        // first body, not the mock's second (different) response.
        let (second, _) = download_cover_art_bytes(&url).await.unwrap();
        assert_eq!(second, body, "second call should be a cache hit");
    }

    #[tokio::test]
    async fn download_rejects_too_small_response() {
        let url = start_mock(vec![(200, vec![0u8; 50])]).await; // under the 100-byte floor
        let err = download_cover_art_bytes(&url).await.unwrap_err();
        assert!(
            matches!(&err, ImportError::CoverArt { detail } if detail.contains("too small")),
            "got: {err}"
        );
    }

    #[tokio::test]
    async fn download_rejects_declared_over_cap_response() {
        let url = start_declared_length_response(crate::util::http::MAX_IMAGE_BYTES + 1).await;
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            download_cover_art_bytes(&url),
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
        // A near-zero backoff keeps the retry from sleeping the real second.
        let url = start_mock(vec![(503, vec![]), (200, body.clone())]).await;
        let (bytes, _) = download_cover_art_bytes_with_backoff(&url, Duration::from_millis(1))
            .await
            .unwrap();
        assert_eq!(bytes, body);
    }

    #[tokio::test]
    async fn download_does_not_retry_client_error() {
        // 404 is non-transient: fail without burning the retry budget.
        let url = start_mock(vec![(404, vec![])]).await;
        assert!(download_cover_art_bytes(&url).await.is_err());
    }
}
