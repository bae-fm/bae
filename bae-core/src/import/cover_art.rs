use crate::import::MetadataSource;
use crate::network::upgrade_to_https;
use crate::util::content_type::ContentType;
use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tracing::{debug, info, warn};

/// A remote cover art option from an external source.
#[derive(Debug, Clone)]
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

/// Capacity for the cover-bytes LRU cache. Sized for a typical session;
/// a miss costs one HTTP fetch.
const COVER_CACHE_CAPACITY: usize = 25;

/// In-memory cache for downloaded cover bytes. Keyed by URL — covers
/// are direct HTTP fetches against arbitrary URLs (CAA, Discogs CDN,
/// etc.), not part of either metadata API client. The UI calls
/// `download_cover_art_bytes` once when the user picks a cover; the
/// commit worker calls it again at import time. Both go through this
/// cache, so the bytes hit the wire at most once per URL per session.
type CoverCacheValue = (Vec<u8>, ContentType);

fn cover_bytes_cache() -> &'static Mutex<LruCache<String, CoverCacheValue>> {
    static CACHE: OnceLock<Mutex<LruCache<String, CoverCacheValue>>> = OnceLock::new();
    CACHE.get_or_init(|| {
        Mutex::new(LruCache::new(
            NonZeroUsize::new(COVER_CACHE_CAPACITY).expect("COVER_CACHE_CAPACITY > 0"),
        ))
    })
}

/// Whether an HTTP error is transient and worth retrying.
fn is_transient_status(status: reqwest::StatusCode) -> bool {
    status.is_server_error() || status == reqwest::StatusCode::TOO_MANY_REQUESTS
}

/// Fetch cover art URL from Cover Art Archive for a MusicBrainz release.
///
/// Retries up to 3 times on transient failures (network errors, 5xx).
/// Does not retry on 404 (no cover art exists) or other client errors.
pub async fn fetch_cover_art_from_archive(release_id: &str) -> Option<String> {
    let url = format!("https://coverartarchive.org/release/{}", release_id);
    fetch_cover_art_from_url(&url, "release", release_id).await
}

/// Fetch cover art URL from Cover Art Archive for a MusicBrainz release group.
///
/// The release-group endpoint returns the community-selected best cover for the
/// album concept, which may differ from any specific release's cover.
pub async fn fetch_cover_art_from_release_group(release_group_id: &str) -> Option<String> {
    let url = format!(
        "https://coverartarchive.org/release-group/{}",
        release_group_id
    );
    fetch_cover_art_from_url(&url, "release group", release_group_id).await
}

/// Shared implementation for fetching cover art from a Cover Art Archive URL.
async fn fetch_cover_art_from_url(json_url: &str, entity: &str, id: &str) -> Option<String> {
    debug!("Fetching cover art from Cover Art Archive: {}", json_url);

    let client = match crate::util::http::client_builder().build() {
        Ok(client) => client,
        Err(e) => {
            warn!("Failed to create HTTP client for Cover Art Archive: {}", e);
            return None;
        }
    };

    let mut last_error = String::new();
    for attempt in 0..=MAX_RETRIES {
        if attempt > 0 {
            let delay = RETRY_BASE_DELAY * 2u32.pow(attempt - 1);

            warn!(
                "Cover Art Archive fetch failed (attempt {}/{}): {} — retrying in {:?}",
                attempt,
                MAX_RETRIES + 1,
                last_error,
                delay
            );

            tokio::time::sleep(delay).await;
        }

        match client.get(json_url).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    return parse_cover_art_response(response, json_url).await;
                } else if response.status() == reqwest::StatusCode::NOT_FOUND {
                    debug!(
                        "No cover art found in Cover Art Archive for {} {}",
                        entity, id
                    );
                    return None;
                } else if is_transient_status(response.status()) {
                    last_error = format!("status {}", response.status());
                    continue;
                } else {
                    // Non-transient client error (400, 403, etc.) — don't retry
                    debug!(
                        "Cover Art Archive returned status {} for {} {}",
                        response.status(),
                        entity,
                        id
                    );
                    return None;
                }
            }
            Err(e) => {
                last_error = e.to_string();
                continue;
            }
        }
    }

    warn!(
        "Cover Art Archive fetch failed after {} attempts: {}",
        MAX_RETRIES + 1,
        last_error
    );

    None
}

/// Parse the Cover Art Archive JSON response, extracting the best image URL.
async fn parse_cover_art_response(response: reqwest::Response, url: &str) -> Option<String> {
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
    select_cover_url(&json)
}

/// Pick the best image URL from a Cover Art Archive JSON body: prefer the
/// front cover, else the first image. Pure (no I/O) so the selection rules are
/// testable without a live response.
fn select_cover_url(json: &serde_json::Value) -> Option<String> {
    let images = json.get("images").and_then(|i| i.as_array())?;

    // Prefer the front cover
    for image in images {
        if image.get("front").and_then(|f| f.as_bool()) == Some(true) {
            if let Some(url) = extract_image_url(image) {
                return Some(url);
            }
        }
    }

    // Fall back to first available image
    images.first().and_then(extract_image_url)
}

/// Extract the best available image URL from a Cover Art Archive image entry.
fn extract_image_url(image: &serde_json::Value) -> Option<String> {
    for key in ["image", "thumb", "small"] {
        if let Some(url) = image.get(key).and_then(|v| v.as_str()) {
            debug!("Using cover art ({}): {}", key, url);
            return Some(upgrade_to_https(url));
        }
    }
    None
}

/// Download cover art from a URL and return the raw bytes and content type.
///
/// Retries up to 3 times on transient failures (network errors, 5xx).
/// Hits a session-wide LRU cache keyed by URL — the UI fetches once at
/// cover-select time and the commit worker re-reads through the same
/// cache at import time, so each URL is downloaded at most once per
/// session.
pub async fn download_cover_art_bytes(
    cover_art_url: &str,
) -> Result<(Vec<u8>, ContentType), String> {
    if let Some(hit) = cover_bytes_cache()
        .lock()
        .expect("cover bytes cache mutex poisoned")
        .get(cover_art_url)
        .cloned()
    {
        debug!("Cover bytes cache hit for {}", cover_art_url);
        return Ok(hit);
    }

    info!("Downloading cover art from {}", cover_art_url);

    let client = crate::util::http::client_builder()
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let mut last_error = String::new();
    for attempt in 0..=MAX_RETRIES {
        if attempt > 0 {
            let delay = RETRY_BASE_DELAY * 2u32.pow(attempt - 1);

            warn!(
                "Cover art download failed (attempt {}/{}): {} — retrying in {:?}",
                attempt,
                MAX_RETRIES + 1,
                last_error,
                delay
            );

            tokio::time::sleep(delay).await;
        }

        let response = match client.get(cover_art_url).send().await {
            Ok(r) => r,
            Err(e) if is_permanent_request_error(&e) => {
                // URL parse / RequestBuilder construction failure or
                // redirect-loop bottoming out — same failure every
                // attempt. Don't burn 1+2+4s of backoff on a
                // deterministic error.
                return Err(format!("Failed to fetch cover art: {}", e));
            }
            Err(e) => {
                last_error = format!("Failed to fetch cover art: {}", e);
                continue;
            }
        };

        if response.status().is_success() {
            let value = read_cover_art_response(response, cover_art_url).await?;
            cover_bytes_cache()
                .lock()
                .expect("cover bytes cache mutex poisoned")
                .put(cover_art_url.to_string(), value.clone());
            return Ok(value);
        } else if is_transient_status(response.status()) {
            last_error = format!(
                "Cover art download failed with status {}",
                response.status()
            );
            continue;
        } else {
            // Non-transient error — don't retry
            return Err(format!(
                "Cover art download failed with status {}",
                response.status()
            ));
        }
    }

    Err(last_error)
}

/// Errors that fail the same way every attempt — retrying just burns
/// the backoff budget. Network/timeout/connection-class errors are
/// transient and continue to retry.
fn is_permanent_request_error(e: &reqwest::Error) -> bool {
    e.is_builder() || e.is_redirect()
}

/// Read bytes and content type from a successful cover art response.
async fn read_cover_art_response(
    response: reqwest::Response,
    cover_art_url: &str,
) -> Result<(Vec<u8>, ContentType), String> {
    let content_type =
        super::image_response::image_content_type_from_response(response.headers(), cover_art_url);

    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read cover art response: {}", e))?;
    if bytes.len() < 100 {
        return Err("Downloaded file too small to be a valid image".to_string());
    }

    info!("Downloaded cover art ({} bytes)", bytes.len());
    Ok((bytes.to_vec(), content_type))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn select_cover_url_prefers_front_then_first() {
        // A front cover wins even when it isn't first in the list.
        let body = json!({
            "images": [
                { "front": false, "image": "https://caa.example/back.jpg" },
                { "front": true, "image": "https://caa.example/front.jpg" },
            ]
        });
        assert_eq!(
            select_cover_url(&body),
            Some("https://caa.example/front.jpg".to_string())
        );

        // With no front cover, the first image is the fallback.
        let body = json!({
            "images": [
                { "front": false, "image": "https://caa.example/a.jpg" },
                { "front": false, "image": "https://caa.example/b.jpg" },
            ]
        });
        assert_eq!(
            select_cover_url(&body),
            Some("https://caa.example/a.jpg".to_string())
        );

        // No images array / empty list → nothing to select.
        assert_eq!(select_cover_url(&json!({})), None);
        assert_eq!(select_cover_url(&json!({ "images": [] })), None);
    }

    #[test]
    fn extract_image_url_walks_keys_and_upgrades_to_https() {
        // 'image' is preferred over 'thumb'/'small' and http is upgraded.
        let image = json!({
            "image": "http://caa.example/full.jpg",
            "thumb": "http://caa.example/thumb.jpg",
        });
        assert_eq!(
            extract_image_url(&image),
            Some("https://caa.example/full.jpg".to_string())
        );

        // Falls through to 'thumb' then 'small' when 'image' is absent.
        assert_eq!(
            extract_image_url(&json!({ "small": "https://caa.example/s.jpg" })),
            Some("https://caa.example/s.jpg".to_string())
        );

        // An entry with none of the keys yields nothing.
        assert_eq!(extract_image_url(&json!({ "front": true })), None);
    }

    #[test]
    fn is_transient_status_is_5xx_or_429_only() {
        use reqwest::StatusCode;
        assert!(is_transient_status(StatusCode::INTERNAL_SERVER_ERROR));
        assert!(is_transient_status(StatusCode::BAD_GATEWAY));
        assert!(is_transient_status(StatusCode::SERVICE_UNAVAILABLE));
        assert!(is_transient_status(StatusCode::TOO_MANY_REQUESTS));
        // Client errors (other than 429) and successes are not retried.
        assert!(!is_transient_status(StatusCode::NOT_FOUND));
        assert!(!is_transient_status(StatusCode::BAD_REQUEST));
        assert!(!is_transient_status(StatusCode::FORBIDDEN));
        assert!(!is_transient_status(StatusCode::OK));
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
        assert!(err.contains("too small"), "got: {err}");
    }

    #[tokio::test]
    async fn download_retries_transient_then_succeeds() {
        let body = vec![0xCDu8; 256];
        // A 503 is transient: retried after backoff, then the 200 succeeds.
        let url = start_mock(vec![(503, vec![]), (200, body.clone())]).await;
        let (bytes, _) = download_cover_art_bytes(&url).await.unwrap();
        assert_eq!(bytes, body);
    }

    #[tokio::test]
    async fn download_does_not_retry_client_error() {
        // 404 is non-transient: fail without burning the retry budget.
        let url = start_mock(vec![(404, vec![])]).await;
        assert!(download_cover_art_bytes(&url).await.is_err());
    }
}
