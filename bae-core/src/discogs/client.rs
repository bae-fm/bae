use crate::discogs::models::{DiscogsArtist, DiscogsRelease, DiscogsRoleArtist, DiscogsTrack};
use crate::discogs::remote_cover_from_urls;
use crate::import::cover_art::RemoteCover;
use crate::retry::retry_with_backoff_if;
use crate::util::session_cache::SessionCache;
use reqwest::{Client, Error as ReqwestError, Response, StatusCode};
use serde::Deserialize;
use std::sync::OnceLock;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::Mutex;
use tokio::time::Instant;
use tracing::{debug, warn};

const DISCOGS_REQUEST_INTERVAL: Duration = Duration::from_secs(1);
const DISCOGS_RETRY_ATTEMPTS: u32 = 3;

/// In-memory cache for `releases/{id}` lookups. Cache key is the
/// release ID; value is the parsed release plus the raw JSON for
/// archival. Module-level static so the cache survives across
/// `DiscogsClient::new` calls — release content doesn't change with
/// the API token.
type ReleaseCacheValue = (DiscogsRelease, String);
type MasterCacheValue = (Option<u32>, String);

static RELEASE_CACHE: SessionCache<ReleaseCacheValue> = SessionCache::new("Discogs release cache");

/// In-memory cache for `masters/{id}` lookups. Same structure as the
/// release cache, keyed by master ID.
static MASTER_CACHE: SessionCache<MasterCacheValue> = SessionCache::new("Discogs master cache");

fn rate_limiter() -> &'static Mutex<Instant> {
    static LIMITER: OnceLock<Mutex<Instant>> = OnceLock::new();
    LIMITER.get_or_init(|| Mutex::new(Instant::now() - DISCOGS_REQUEST_INTERVAL))
}

async fn wait_for_rate_limit() {
    let mut last_request = rate_limiter().lock().await;
    let elapsed = last_request.elapsed();
    if elapsed < DISCOGS_REQUEST_INTERVAL {
        tokio::time::sleep(DISCOGS_REQUEST_INTERVAL - elapsed).await;
    }
    *last_request = Instant::now();
}

/// Pre-populate the release cache. Tests use this to drive
/// `prepare_release` without making an HTTP call. The raw JSON is
/// stored in `release_metadata` for archival; tests can pass an empty
/// string when the archival content isn't being asserted on.
#[cfg(any(test, feature = "test-utils"))]
pub fn seed_release_cache(id: &str, value: (DiscogsRelease, String)) {
    RELEASE_CACHE.put(id, value);
}

/// Pre-populate the master cache. Tests use this when a synthetic
/// `DiscogsRelease` carries a `master_id`; the worker's cross-reference
/// fetch then resolves through the cache.
#[cfg(any(test, feature = "test-utils"))]
pub fn seed_master_cache(master_id: &str, year: Option<u32>, raw_json: String) {
    MASTER_CACHE.put(master_id, (year, raw_json));
}
#[derive(Error, Debug)]
pub enum DiscogsError {
    #[error("HTTP request failed: {0}")]
    Request(#[from] ReqwestError),
    #[error("API rate limit exceeded")]
    RateLimit,
    #[error("Invalid API key")]
    InvalidApiKey,
    #[error("Release not found")]
    NotFound,
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

impl DiscogsError {
    /// Whether this error indicates the configured Discogs token is invalid
    /// (HTTP 401). Callers use this to mark the stored key `Rejected` so the UI
    /// can prompt the user to re-enter it.
    pub fn is_invalid_api_key(&self) -> bool {
        matches!(self, DiscogsError::InvalidApiKey)
    }
}

fn classify_discogs_response(response: Response) -> Result<Response, DiscogsError> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }

    match status {
        StatusCode::NOT_FOUND => Err(DiscogsError::NotFound),
        StatusCode::TOO_MANY_REQUESTS => Err(DiscogsError::RateLimit),
        StatusCode::UNAUTHORIZED => Err(DiscogsError::InvalidApiKey),
        _ => Err(DiscogsError::Request(
            response.error_for_status().unwrap_err(),
        )),
    }
}

fn should_retry_discogs(error: &DiscogsError) -> bool {
    matches!(error, DiscogsError::RateLimit | DiscogsError::Request(_))
}

fn discogs_retry_delay(attempt: u32) -> Duration {
    Duration::from_millis(500 * u64::from(attempt))
}

/// Discogs search response wrapper
#[derive(Debug, Deserialize)]
struct SearchResponse {
    results: Vec<DiscogsSearchResult>,
}
/// Search parameters for flexible Discogs queries
#[derive(Debug, Clone, Default)]
pub struct DiscogsSearchParams {
    pub artist: Option<String>,
    pub release_title: Option<String>,
    pub year: Option<String>,
    pub label: Option<String>,
    pub catno: Option<String>,
    pub barcode: Option<String>,
}
/// Individual search result
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct DiscogsSearchResult {
    pub id: u64,
    pub title: String,
    pub year: Option<String>,
    pub genre: Option<Vec<String>>,
    pub style: Option<Vec<String>>,
    pub format: Option<Vec<String>>,
    pub country: Option<String>,
    pub label: Option<Vec<String>>,
    pub catno: Option<String>,
    pub barcode: Option<Vec<String>>,
    #[serde(
        default,
        deserialize_with = "crate::serde_helpers::empty_string_as_none"
    )]
    pub cover_image: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::serde_helpers::empty_string_as_none"
    )]
    pub thumb: Option<String>,
    pub master_id: Option<u64>,
    #[serde(rename = "type")]
    pub result_type: String,
}

impl DiscogsSearchResult {
    /// Best available cover image from Discogs search result fields.
    pub fn remote_cover(&self) -> Option<RemoteCover> {
        remote_cover_from_urls(
            self.cover_image.as_deref(),
            self.thumb.as_deref(),
            "search result",
            self.id,
        )
    }
}

/// Artist credit in Discogs API responses
#[derive(Debug, Deserialize, Clone)]
struct ArtistCredit {
    id: u64,
    name: String,
}

#[derive(Debug, Deserialize, Clone)]
struct ExtraArtistCredit {
    id: Option<u64>,
    name: String,
    #[serde(
        default,
        deserialize_with = "crate::serde_helpers::empty_string_as_none"
    )]
    role: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::serde_helpers::empty_string_as_none"
    )]
    anv: Option<String>,
}
/// Detailed release response from Discogs
#[derive(Debug, Deserialize)]
struct ReleaseResponse {
    id: u64,
    title: String,
    year: Option<u32>,
    genres: Option<Vec<String>>,
    styles: Option<Vec<String>>,
    formats: Option<Vec<Format>>,
    country: Option<String>,
    labels: Option<Vec<LabelResponse>>,
    images: Option<Vec<Image>>,
    artists: Option<Vec<ArtistCredit>>,
    extraartists: Option<Vec<ExtraArtistCredit>>,
    tracklist: Option<Vec<TrackResponse>>,
    master_id: Option<u64>,
}
#[derive(Debug, Deserialize)]
struct Format {
    name: String,
}
#[derive(Debug, Deserialize)]
struct Image {
    #[serde(rename = "type")]
    image_type: String,
    uri: String,
    uri150: Option<String>,
}
#[derive(Debug, Deserialize)]
struct TrackResponse {
    position: String,
    title: String,
    duration: Option<String>,
    #[serde(default)]
    artists: Vec<ArtistCredit>,
    extraartists: Option<Vec<ExtraArtistCredit>>,
    #[serde(default)]
    type_: String,
}
#[derive(Debug, Deserialize)]
struct LabelResponse {
    name: String,
    catno: Option<String>,
}

fn extra_artist_to_model(a: ExtraArtistCredit) -> Option<DiscogsRoleArtist> {
    let Some(role) = a.role else {
        warn!(
            discogs_artist_id = ?a.id,
            artist_name = %a.name,
            "Skipping Discogs extraartist without role"
        );
        return None;
    };
    Some(DiscogsRoleArtist {
        id: a.id.map(|id| id.to_string()),
        name: a.name,
        role,
        credited_name: a.anv,
    })
}

/// Convert raw Discogs release JSON into the public `DiscogsRelease` shape.
///
/// Same projection `get_release` applies to fresh API responses — exposed as
/// a free function so cached `release_metadata.json` rows can be replayed
/// without re-fetching.
pub fn parse_discogs_release_json(raw_json: &str) -> Result<DiscogsRelease, DiscogsError> {
    let release: ReleaseResponse = serde_json::from_str(raw_json)?;
    let tracklist = release
        .tracklist
        .unwrap_or_default()
        .into_iter()
        .map(|t| DiscogsTrack {
            position: t.position,
            title: t.title,
            duration: t.duration,
            artists: t
                .artists
                .into_iter()
                .map(|a| DiscogsArtist {
                    id: a.id.to_string(),
                    name: a.name,
                })
                .collect(),
            extraartists: t.extraartists.map(|extraartists| {
                extraartists
                    .into_iter()
                    .filter_map(extra_artist_to_model)
                    .collect()
            }),
            type_: if t.type_.is_empty() {
                "track".to_string()
            } else {
                t.type_
            },
        })
        .collect();
    let artists = release
        .artists
        .unwrap_or_default()
        .into_iter()
        .map(|a| DiscogsArtist {
            id: a.id.to_string(),
            name: a.name,
        })
        .collect();
    let extraartists = release.extraartists.map(|extraartists| {
        extraartists
            .into_iter()
            .filter_map(extra_artist_to_model)
            .collect()
    });
    let primary_image = release.images.as_ref().and_then(|images| {
        images
            .iter()
            .find(|img| img.image_type == "primary")
            .or_else(|| images.first())
    });
    let cover_image = primary_image.map(|img| img.uri.clone());
    let thumb = primary_image.and_then(|img| img.uri150.clone().or_else(|| Some(img.uri.clone())));
    let master_id = release.master_id.map(|id| id.to_string());
    let labels = release.labels.unwrap_or_default();
    let label_names: Vec<String> = labels.iter().map(|l| l.name.clone()).collect();
    let catno = labels.first().and_then(|l| l.catno.clone());
    let formats = release.formats.unwrap_or_default();

    Ok(DiscogsRelease {
        id: release.id.to_string(),
        title: release.title,
        year: release.year,
        genre: release.genres.unwrap_or_default(),
        style: release.styles.unwrap_or_default(),
        format: formats.into_iter().map(|f| f.name).collect(),
        country: release.country,
        label: label_names,
        catno,
        cover_image,
        thumb,
        artists,
        extraartists,
        tracklist,
        master_id,
    })
}

/// Extract the original release year from a cached Discogs master JSON
/// payload. The master endpoint stores `{"year": ...}` at the top level;
/// callers archive the raw JSON in `release_metadata` (source =
/// `'discogs_master'`) and call this to replay the year on reset.
pub fn parse_discogs_master_year(raw_json: &str) -> Result<Option<u32>, DiscogsError> {
    let parsed: serde_json::Value = serde_json::from_str(raw_json)?;
    Ok(parsed
        .get("year")
        .and_then(|y| y.as_u64())
        .map(|y| y as u32))
}

#[derive(Clone)]
/// What a Discogs API call revealed about the stored key, for the validation
/// observer. Only a 401 (rejected) or a success carries a signal — network and
/// rate-limit errors say nothing about the key itself.
pub enum DiscogsKeySignal {
    Rejected,
    Accepted,
}

/// Invoked after every Discogs API call a [`DiscogsClient`] makes, so the stored
/// key's persisted validation state tracks reality without each call site having
/// to record it. Injected by `LibraryManager::discogs_client`.
pub type DiscogsValidationObserver = std::sync::Arc<dyn Fn(DiscogsKeySignal) + Send + Sync>;

pub struct DiscogsClient {
    client: Client,
    api_key: String,
    base_url: String,
    observer: Option<DiscogsValidationObserver>,
}
impl DiscogsClient {
    /// A client with no validation observer — for validating a candidate key
    /// before it's stored (the save path interprets that result directly).
    pub fn new(api_key: String) -> Self {
        Self::build(api_key, None)
    }

    /// A client that reports each call's outcome to `observer`, so a stored
    /// key re-validates at use time: a 401 marks it rejected, a success confirms
    /// a previously-unvalidated key.
    pub fn with_observer(api_key: String, observer: DiscogsValidationObserver) -> Self {
        Self::build(api_key, Some(observer))
    }

    fn build(api_key: String, observer: Option<DiscogsValidationObserver>) -> Self {
        Self {
            client: crate::util::http::client_builder()
                .build()
                .expect("Failed to build HTTP client"),
            api_key,
            base_url: "https://api.discogs.com".to_string(),
            observer,
        }
    }

    /// A client whose requests target `base_url` instead of the live API — used
    /// by tests to point at a local mock or a refused port. Public API is
    /// unchanged; production always goes through `new` / `with_observer`.
    #[cfg(test)]
    pub(crate) fn with_base_url(api_key: String, base_url: String) -> Self {
        Self {
            base_url,
            ..Self::build(api_key, None)
        }
    }

    /// Report a call's outcome to the observer (if any). A 401 is the only error
    /// that reveals the key is bad; a success confirms it; network/rate-limit
    /// errors carry no signal.
    fn observe<T>(&self, result: &Result<T, DiscogsError>) {
        let Some(observer) = &self.observer else {
            return;
        };
        match result {
            Err(e) if e.is_invalid_api_key() => observer(DiscogsKeySignal::Rejected),
            Ok(_) => observer(DiscogsKeySignal::Accepted),
            _ => {}
        }
    }

    /// Drive a request future to completion and report its outcome to the
    /// observer before handing the result back. Every public request method
    /// routes through here so the outcome is folded into the key's validation in
    /// exactly one place.
    async fn observed<T>(
        &self,
        fut: impl std::future::Future<Output = Result<T, DiscogsError>>,
    ) -> Result<T, DiscogsError> {
        let result = fut.await;
        self.observe(&result);
        result
    }

    fn get(&self, url: &str) -> reqwest::RequestBuilder {
        self.client
            .get(url)
            .header("Authorization", format!("Discogs token={}", self.api_key))
            .timeout(crate::util::http::API_TIMEOUT)
    }

    async fn send(&self, request: reqwest::RequestBuilder) -> Result<Response, DiscogsError> {
        wait_for_rate_limit().await;
        let response = request.send().await?;
        classify_discogs_response(response)
    }

    /// Validate the API token by making a lightweight request.
    pub async fn validate_token(&self) -> Result<(), DiscogsError> {
        let url = format!("{}/database/search", self.base_url);
        let query_params = [("per_page", "1")];

        retry_with_backoff_if(
            DISCOGS_RETRY_ATTEMPTS,
            "Discogs token validation",
            should_retry_discogs,
            discogs_retry_delay,
            || async {
                self.send(self.get(&url).query(&query_params))
                    .await
                    .map(|_| ())
            },
        )
        .await
    }

    /// Flexible search using any combination of supported parameters
    pub async fn search_with_params(
        &self,
        params: &DiscogsSearchParams,
    ) -> Result<Vec<DiscogsSearchResult>, DiscogsError> {
        self.observed(self.search_with_params_inner(params)).await
    }

    async fn search_with_params_inner(
        &self,
        params: &DiscogsSearchParams,
    ) -> Result<Vec<DiscogsSearchResult>, DiscogsError> {
        use tracing::{debug, info, warn};
        let url = format!("{}/database/search", self.base_url);
        let mut query_params: Vec<(&str, &str)> = vec![("type", "release")];
        if let Some(ref artist) = params.artist {
            query_params.push(("artist", artist));
        }
        if let Some(ref title) = params.release_title {
            query_params.push(("release_title", title));
        }
        if let Some(ref year) = params.year {
            query_params.push(("year", year));
        }
        if let Some(ref label) = params.label {
            query_params.push(("label", label));
        }
        if let Some(ref catno) = params.catno {
            query_params.push(("catno", catno));
        }
        if let Some(ref barcode) = params.barcode {
            query_params.push(("barcode", barcode));
        }
        info!("Discogs API: GET {} with params: {:?}", url, params);
        let search_response: SearchResponse = retry_with_backoff_if(
            DISCOGS_RETRY_ATTEMPTS,
            "Discogs search",
            should_retry_discogs,
            discogs_retry_delay,
            || async {
                let response = self
                    .send(self.get(&url).query(&query_params))
                    .await
                    .inspect_err(|error| match error {
                        DiscogsError::RateLimit => warn!("Discogs rate limit exceeded"),
                        DiscogsError::InvalidApiKey => warn!("Discogs invalid API key"),
                        DiscogsError::NotFound => warn!("Discogs API returned not found"),
                        DiscogsError::Request(_) => warn!("Discogs API request failed"),
                        DiscogsError::Serialization(_) => {}
                    })?;
                debug!("Response status: {}", response.status());
                response.json().await.map_err(DiscogsError::Request)
            },
        )
        .await?;
        info!(
            "Discogs search returned {} total result(s)",
            search_response.results.len()
        );
        for (i, result) in search_response.results.iter().enumerate().take(3) {
            debug!(
                "  Raw result {}: {} (type: {}, master_id: {:?})",
                i + 1,
                result.title,
                result.result_type,
                result.master_id
            );
        }
        let releases: Vec<_> = search_response
            .results
            .into_iter()
            .filter(|r| r.result_type == "release")
            .collect();
        info!("  → {} release(s) after filtering", releases.len());
        Ok(releases)
    }
    /// Get detailed information about a specific release.
    ///
    /// Returns the parsed release and the raw JSON text from the API.
    /// Hits a session-wide LRU cache (size 25) keyed by release ID.
    pub async fn get_release(&self, id: &str) -> Result<(DiscogsRelease, String), DiscogsError> {
        self.observed(self.get_release_inner(id)).await
    }

    async fn get_release_inner(&self, id: &str) -> Result<(DiscogsRelease, String), DiscogsError> {
        if let Some(hit) = RELEASE_CACHE.get_cloned(id) {
            debug!("Discogs release cache hit for {}", id);
            return Ok(hit);
        }

        let url = format!("{}/releases/{}", self.base_url, id);
        let value = retry_with_backoff_if(
            DISCOGS_RETRY_ATTEMPTS,
            "Discogs release fetch",
            should_retry_discogs,
            discogs_retry_delay,
            || async {
                let response = self.send(self.get(&url)).await?;
                let raw_json = response.text().await.map_err(DiscogsError::Request)?;
                let release = parse_discogs_release_json(&raw_json)?;
                Ok((release, raw_json))
            },
        )
        .await?;
        RELEASE_CACHE.put(id, value.clone());
        Ok(value)
    }

    /// Fetch a Discogs master release to get the original release year.
    ///
    /// Returns (year, raw_json). The year is the original release year
    /// (e.g., 1967 for a 1985 reissue). Hits a session-wide LRU cache.
    pub async fn get_master(&self, master_id: &str) -> Result<(Option<u32>, String), DiscogsError> {
        self.observed(self.get_master_inner(master_id)).await
    }

    async fn get_master_inner(
        &self,
        master_id: &str,
    ) -> Result<(Option<u32>, String), DiscogsError> {
        if let Some(hit) = MASTER_CACHE.get_cloned(master_id) {
            debug!("Discogs master cache hit for {}", master_id);
            return Ok(hit);
        }

        let url = format!("{}/masters/{}", self.base_url, master_id);
        let value = retry_with_backoff_if(
            DISCOGS_RETRY_ATTEMPTS,
            "Discogs master fetch",
            should_retry_discogs,
            discogs_retry_delay,
            || async {
                let response = self.send(self.get(&url)).await?;
                let raw_json = response.text().await.map_err(DiscogsError::Request)?;
                let year = parse_discogs_master_year(&raw_json)?;
                Ok((year, raw_json))
            },
        )
        .await?;
        MASTER_CACHE.put(master_id, value.clone());
        Ok(value)
    }

    /// Get the primary image URL for a Discogs artist
    pub async fn get_artist_image(&self, artist_id: &str) -> Result<Option<String>, DiscogsError> {
        self.observed(self.get_artist_image_inner(artist_id)).await
    }

    async fn get_artist_image_inner(
        &self,
        artist_id: &str,
    ) -> Result<Option<String>, DiscogsError> {
        let url = format!("{}/artists/{}", self.base_url, artist_id);
        let Some(json) = retry_with_backoff_if(
            DISCOGS_RETRY_ATTEMPTS,
            "Discogs artist fetch",
            should_retry_discogs,
            discogs_retry_delay,
            || async {
                let response = match self.send(self.get(&url)).await {
                    Ok(response) => response,
                    Err(DiscogsError::NotFound) => {
                        warn!(
                            discogs_artist_id = %artist_id,
                            "Discogs artist image lookup returned not found"
                        );
                        return Ok(None);
                    }
                    Err(error) => return Err(error),
                };
                let json: serde_json::Value =
                    response.json().await.map_err(DiscogsError::Request)?;
                Ok(Some(json))
            },
        )
        .await?
        else {
            return Ok(None);
        };
        let image_url = json
            .get("images")
            .and_then(|images| images.as_array())
            .and_then(|images| {
                images
                    .iter()
                    .find(|img| {
                        img.get("type")
                            .and_then(|t| t.as_str())
                            .map(|t| t == "primary")
                            .unwrap_or(false)
                    })
                    .or_else(|| images.first())
            })
            .and_then(|img| img.get("uri").and_then(|u| u.as_str()))
            .map(|s| s.to_string());

        Ok(image_url)
    }
}

/// Fetch the Discogs cross-reference for an MB release. Given the Discogs
/// release URL parsed from MB's url-rels, fetches the linked Discogs release
/// and (if it has a master) the master too. Returns the `DiscogsRelease` plus
/// the raw JSON pairs to append to the caller's `release_metadata`
/// collection.
///
/// A fetch that fails with `InvalidApiKey` marks the stored key `Rejected` via
/// the client's validation observer; the cross-ref doesn't abort the import on
/// auth failure — it returns `None` and the UI surfaces the bad-key state.
///
/// Returns `None` if the URL doesn't parse to a numeric release ID, or the
/// Discogs release fetch fails. A successful release fetch with a failing
/// master fetch returns `Some` with just the release pair — the master is
/// best-effort.
pub async fn fetch_discogs_xref(
    client: &DiscogsClient,
    discogs_url: &str,
) -> Option<(DiscogsRelease, Vec<(String, String)>)> {
    let id = match crate::import::musicbrainz_mapper::extract_discogs_release_id(discogs_url) {
        Some(id) => id,
        None => {
            tracing::warn!(
                "Could not extract Discogs release ID from URL: {}",
                discogs_url
            );
            return None;
        }
    };

    tracing::info!(
        "Found Discogs release URL: {}, fetching release {}",
        discogs_url,
        id
    );
    let mut pairs: Vec<(String, String)> = Vec::new();
    let discogs_release = match client.get_release(&id).await {
        Ok((release, raw)) => {
            pairs.push((
                crate::import::MetadataSource::Discogs.as_str().to_string(),
                raw,
            ));
            release
        }
        Err(e) => {
            tracing::warn!("Failed to fetch Discogs release {}: {}", id, e);
            return None;
        }
    };

    if let Some(ref master_id) = discogs_release.master_id {
        match client.get_master(master_id).await {
            Ok((_year, master_json)) => {
                pairs.push(("discogs_master".to_string(), master_json));
            }
            Err(e) => {
                tracing::warn!("Failed to fetch Discogs master {}: {}", master_id, e);
            }
        }
    }

    Some((discogs_release, pairs))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::MetadataSource;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    const SEARCH_OK_EMPTY: &str = concat!(
        "HTTP/1.1 200 OK\r\n",
        "Content-Type: application/json\r\n",
        "Content-Length: 14\r\n",
        "\r\n",
        "{\"results\":[]}",
    );
    const RATE_LIMITED: &str = "HTTP/1.1 429 Too Many Requests\r\nContent-Length: 0\r\n\r\n";
    const UNAUTHORIZED: &str = "HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\n\r\n";
    const NOT_FOUND: &str = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n";

    fn discogs_test_guard() -> &'static tokio::sync::Mutex<()> {
        static GUARD: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
        GUARD.get_or_init(|| tokio::sync::Mutex::new(()))
    }

    async fn reset_rate_limiter_for_test() {
        let mut last_request = rate_limiter().lock().await;
        *last_request = Instant::now() - DISCOGS_REQUEST_INTERVAL;
    }

    async fn discogs_response_server(responses: Vec<&'static str>) -> (String, Arc<AtomicUsize>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("test listener should bind");
        let url = format!(
            "http://{}",
            listener
                .local_addr()
                .expect("test listener should have an address")
        );
        let request_count = Arc::new(AtomicUsize::new(0));
        let counted_requests = request_count.clone();
        tokio::spawn(async move {
            for response in responses {
                let (mut stream, _) = listener
                    .accept()
                    .await
                    .expect("test request should connect");
                counted_requests.fetch_add(1, Ordering::SeqCst);
                let mut buffer = [0; 4096];
                let _ = stream
                    .read(&mut buffer)
                    .await
                    .expect("test request should be readable");
                stream
                    .write_all(response.as_bytes())
                    .await
                    .expect("test response should write");
            }
        });
        (url, request_count)
    }

    #[tokio::test(start_paused = true)]
    async fn rate_limiter_enforces_discogs_spacing() {
        let _guard = discogs_test_guard().lock().await;
        reset_rate_limiter_for_test().await;

        let start = Instant::now();
        wait_for_rate_limit().await;
        assert!(start.elapsed() < Duration::from_millis(100));

        let start = Instant::now();
        wait_for_rate_limit().await;
        assert!(start.elapsed() >= Duration::from_millis(900));
    }

    fn search_result_with_cover_fields(
        cover_image: Option<&str>,
        thumb: Option<&str>,
    ) -> DiscogsSearchResult {
        DiscogsSearchResult {
            id: 1,
            title: "Artist Name - Album Title".to_string(),
            year: None,
            genre: None,
            style: None,
            format: None,
            country: None,
            label: None,
            catno: None,
            barcode: None,
            cover_image: cover_image.map(str::to_string),
            thumb: thumb.map(str::to_string),
            master_id: None,
            result_type: "release".to_string(),
        }
    }

    #[test]
    fn search_result_remote_cover_uses_thumb_as_cover_when_cover_image_is_absent() {
        let result =
            search_result_with_cover_fields(None, Some("https://discogs.example/thumb.jpg"));

        let cover = result.remote_cover().unwrap();

        assert_eq!(cover.url, "https://discogs.example/thumb.jpg");
        assert_eq!(cover.thumbnail_url, "https://discogs.example/thumb.jpg");
        assert_eq!(cover.label, MetadataSource::Discogs.cover_source_label());
        assert_eq!(cover.source, MetadataSource::Discogs);
    }

    #[test]
    fn search_result_remote_cover_uses_cover_as_thumbnail_when_thumb_is_absent() {
        let result =
            search_result_with_cover_fields(Some("https://discogs.example/full.jpg"), None);

        let cover = result.remote_cover().unwrap();

        assert_eq!(cover.url, "https://discogs.example/full.jpg");
        assert_eq!(cover.thumbnail_url, "https://discogs.example/full.jpg");
        assert_eq!(cover.label, MetadataSource::Discogs.cover_source_label());
        assert_eq!(cover.source, MetadataSource::Discogs);
    }

    #[test]
    fn search_result_remote_cover_is_absent_without_cover_fields() {
        let result = search_result_with_cover_fields(None, None);

        assert!(result.remote_cover().is_none());
    }

    #[test]
    fn observe_signals_only_on_rejection_or_success() {
        let signals = Arc::new(Mutex::new(Vec::<&'static str>::new()));
        let recorded = signals.clone();
        let observer: DiscogsValidationObserver = Arc::new(move |sig| {
            recorded.lock().unwrap().push(match sig {
                DiscogsKeySignal::Rejected => "rejected",
                DiscogsKeySignal::Accepted => "accepted",
            });
        });
        let client = DiscogsClient::with_observer("token".to_string(), observer);

        client.observe::<()>(&Ok(()));
        client.observe::<()>(&Err(DiscogsError::InvalidApiKey));
        // A rate-limit (like a network error) reveals nothing about the key, so
        // it must NOT signal — a transient blip can't reject a good key.
        client.observe::<()>(&Err(DiscogsError::RateLimit));

        assert_eq!(*signals.lock().unwrap(), vec!["accepted", "rejected"]);
    }

    #[tokio::test]
    async fn transport_error_display_does_not_include_discogs_token() {
        let _guard = discogs_test_guard().lock().await;
        reset_rate_limiter_for_test().await;
        let token = "secret-discogs-token";
        let mut client = DiscogsClient::new(token.to_string());
        client.base_url = "http://127.0.0.1:1".to_string();

        let error = client.validate_token().await.unwrap_err();

        assert!(!error.to_string().contains(token));
    }

    #[tokio::test]
    async fn validate_token_sends_token_in_authorization_header() {
        let _guard = discogs_test_guard().lock().await;
        reset_rate_limiter_for_test().await;
        let token = "secret-discogs-token";
        let listener = TcpListener::bind("127.0.0.1:0").expect("test listener should bind");
        let url = format!(
            "http://{}",
            listener
                .local_addr()
                .expect("test listener should have an address")
        );
        let request = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("test request should connect");
            let mut buffer = [0; 4096];
            let read = stream
                .read(&mut buffer)
                .expect("test request should be readable");
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
                .expect("test response should write");
            String::from_utf8(buffer[..read].to_vec()).expect("request should be UTF-8")
        });
        let mut client = DiscogsClient::new(token.to_string());
        client.base_url = url;

        client
            .validate_token()
            .await
            .expect("token validation should accept 200 response");

        let request = request.join().expect("test listener should finish");
        let request_line = request
            .lines()
            .next()
            .expect("test request should include a request line");
        assert_eq!(request_line, "GET /database/search?per_page=1 HTTP/1.1");
        assert!(request.contains("authorization: Discogs token=secret-discogs-token\r\n"));
        assert!(!request_line.contains("token=secret-discogs-token"));
    }

    #[tokio::test]
    async fn search_retries_rate_limit_then_returns_success() {
        let _guard = discogs_test_guard().lock().await;
        reset_rate_limiter_for_test().await;
        let (url, request_count) =
            discogs_response_server(vec![RATE_LIMITED, SEARCH_OK_EMPTY]).await;
        let mut client = DiscogsClient::new("token".to_string());
        client.base_url = url;

        let releases = client
            .search_with_params(&DiscogsSearchParams::default())
            .await
            .expect("retry should return the successful search response");

        assert!(releases.is_empty());
        assert_eq!(request_count.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn search_returns_persistent_rate_limit_after_retry_attempts() {
        let _guard = discogs_test_guard().lock().await;
        reset_rate_limiter_for_test().await;
        let (url, request_count) =
            discogs_response_server(vec![RATE_LIMITED, RATE_LIMITED, RATE_LIMITED]).await;
        let mut client = DiscogsClient::new("token".to_string());
        client.base_url = url;

        let error = client
            .search_with_params(&DiscogsSearchParams::default())
            .await
            .expect_err("persistent rate limit should fail after retry attempts");

        assert!(matches!(error, DiscogsError::RateLimit));
        assert_eq!(request_count.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn search_does_not_retry_invalid_api_key() {
        let _guard = discogs_test_guard().lock().await;
        reset_rate_limiter_for_test().await;
        let (url, request_count) = discogs_response_server(vec![UNAUTHORIZED]).await;
        let mut client = DiscogsClient::new("token".to_string());
        client.base_url = url;

        let error = client
            .search_with_params(&DiscogsSearchParams::default())
            .await
            .expect_err("invalid API key should fail without retry");

        assert!(matches!(error, DiscogsError::InvalidApiKey));
        assert_eq!(request_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn search_does_not_retry_not_found() {
        let _guard = discogs_test_guard().lock().await;
        reset_rate_limiter_for_test().await;
        let (url, request_count) = discogs_response_server(vec![NOT_FOUND]).await;
        let mut client = DiscogsClient::new("token".to_string());
        client.base_url = url;

        let error = client
            .search_with_params(&DiscogsSearchParams::default())
            .await
            .expect_err("not found should fail without retry");

        assert!(matches!(error, DiscogsError::NotFound));
        assert_eq!(request_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn search_observer_records_one_signal_after_internal_retry() {
        let _guard = discogs_test_guard().lock().await;
        reset_rate_limiter_for_test().await;
        let (url, request_count) =
            discogs_response_server(vec![RATE_LIMITED, SEARCH_OK_EMPTY]).await;
        let signals = Arc::new(Mutex::new(Vec::<&'static str>::new()));
        let recorded = signals.clone();
        let observer: DiscogsValidationObserver = Arc::new(move |sig| {
            recorded.lock().unwrap().push(match sig {
                DiscogsKeySignal::Rejected => "rejected",
                DiscogsKeySignal::Accepted => "accepted",
            });
        });
        let mut client = DiscogsClient::with_observer("token".to_string(), observer);
        client.base_url = url;

        client
            .search_with_params(&DiscogsSearchParams::default())
            .await
            .expect("retry should return the successful search response");

        assert_eq!(request_count.load(Ordering::SeqCst), 2);
        assert_eq!(*signals.lock().unwrap(), vec!["accepted"]);
    }
}
