use crate::discogs::models::{DiscogsArtist, DiscogsRelease, DiscogsRoleArtist, DiscogsTrack};
use crate::discogs::remote_cover_from_urls;
use crate::import::cover_art::RemoteCover;
use crate::retry::retry_with_backoff_if;
use crate::util::rate_limiter::{CallPriority, RateLimiter};
use crate::util::session_cache::{SessionCache, PROVIDER_LOOKUP_CAPACITY};
use reqwest::{Client, Error as ReqwestError, Response, StatusCode};
use serde::Deserialize;
use std::time::Duration;
use thiserror::Error;
use tracing::{debug, warn};

const DISCOGS_REQUEST_INTERVAL: Duration = Duration::from_secs(1);
const DISCOGS_RETRY_ATTEMPTS: u32 = 3;

type ReleaseCacheValue = (DiscogsRelease, String);
type MasterCacheValue = (Option<u32>, String);

/// In-memory cache for `releases/{id}` lookups: the parsed release plus its raw JSON
/// for archival, keyed by release ID. A module-level static, so it survives across
/// `DiscogsClient::new` calls — release content doesn't vary with the API token.
static RELEASE_CACHE: SessionCache<ReleaseCacheValue> =
    SessionCache::new("Discogs release cache", PROVIDER_LOOKUP_CAPACITY);

/// The same, for `masters/{id}`.
static MASTER_CACHE: SessionCache<MasterCacheValue> =
    SessionCache::new("Discogs master cache", PROVIDER_LOOKUP_CAPACITY);

static RATE_LIMITER: RateLimiter = RateLimiter::new(DISCOGS_REQUEST_INTERVAL);

/// Pre-populate the release cache, so a test can drive `prepare_release` without an
/// HTTP call. The raw JSON is what gets archived, and what a later projection
/// replays from, so it has to be the release handed over with it.
#[cfg(any(test, feature = "test-utils"))]
pub fn seed_release_cache(id: &str, value: (DiscogsRelease, String)) {
    RELEASE_CACHE.put(id, value);
}

/// Pre-populate the master cache, for a synthetic `DiscogsRelease` that carries a
/// `master_id` — the worker's cross-reference fetch then resolves through it.
#[cfg(any(test, feature = "test-utils"))]
pub fn seed_master_cache(master_id: &str, year: Option<u32>, raw_json: String) {
    MASTER_CACHE.put(master_id, (year, raw_json));
}
#[derive(Error, Debug)]
pub enum DiscogsError {
    /// The request never reached a usable response — connection, DNS, timeout, a
    /// dropped or unreadable body. Transport-level and worth retrying.
    #[error("Discogs transport error: {0}")]
    Transport(#[from] ReqwestError),
    /// Discogs returned an HTTP error status not otherwise carved out below (not
    /// 404 / 401 / 429). Distinct from `Transport` so the retry policy can repeat a
    /// 5xx but not a 4xx, which is the server's permanent answer to this request.
    #[error("Discogs returned an error response (status {0})")]
    Provider(StatusCode),
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
    /// An HTTP 401 — the configured token is bad. Callers mark the stored key
    /// `Rejected` on this, so the UI can prompt for a new one.
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
        other => Err(DiscogsError::Provider(other)),
    }
}

/// Retry only what a retry can fix: transport failures, an explicit rate-limit,
/// and Discogs server errors. A 4xx `Provider` status is the server's permanent
/// answer to this exact request — retrying it burns three round trips and three
/// rate-limit waits to hear the same 4xx again.
fn should_retry_discogs(error: &DiscogsError) -> bool {
    match error {
        DiscogsError::Transport(_) | DiscogsError::RateLimit => true,
        DiscogsError::Provider(status) => crate::retry::is_transient_status(*status),
        DiscogsError::InvalidApiKey | DiscogsError::NotFound | DiscogsError::Serialization(_) => {
            false
        }
    }
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    results: Vec<DiscogsSearchResult>,
}
#[derive(Debug, Clone, Default)]
pub struct DiscogsSearchParams {
    pub artist: Option<String>,
    pub release_title: Option<String>,
    pub year: Option<String>,
    pub label: Option<String>,
    pub catno: Option<String>,
    pub barcode: Option<String>,
}
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct DiscogsSearchResult {
    pub id: u64,
    pub title: String,
    pub year: Option<String>,
    pub format: Option<Vec<String>>,
    pub country: Option<String>,
    pub label: Option<Vec<String>>,
    pub catno: Option<String>,
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
    /// The best cover image the search result offers.
    pub fn remote_cover(&self) -> Option<RemoteCover> {
        remote_cover_from_urls(
            self.cover_image.as_deref(),
            self.thumb.as_deref(),
            "search result",
            self.id,
        )
    }
}

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
#[derive(Debug, Deserialize)]
struct ReleaseResponse {
    id: u64,
    title: String,
    year: Option<u32>,
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

/// Raw Discogs release JSON to the public `DiscogsRelease`. The same projection
/// `get_release` applies to a fresh response, exposed as a free function so an
/// archived `source_release_payloads` row can be replayed without re-fetching.
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

/// The original release year out of an archived Discogs master payload, which
/// carries `{"year": …}` at the top level. Callers archive that JSON in
/// `source_release_payloads` and call this to replay the year on reset.
pub fn parse_discogs_master_year(raw_json: &str) -> Result<Option<u32>, DiscogsError> {
    let parsed: serde_json::Value = serde_json::from_str(raw_json)?;
    Ok(parsed
        .get("year")
        .and_then(|y| y.as_u64())
        .map(|y| y as u32))
}

#[derive(Clone)]
/// What a Discogs call revealed about the stored key. Only a 401 or a success says
/// anything — a network or rate-limit error tells us nothing about the key itself.
pub enum DiscogsKeySignal {
    Rejected,
    Accepted,
}

/// Invoked after every call a [`DiscogsClient`] makes, so the stored key's persisted
/// validation state tracks reality without each call site recording it. Injected by
/// `LibraryManager::discogs_client`.
pub type DiscogsValidationObserver = std::sync::Arc<dyn Fn(DiscogsKeySignal) + Send + Sync>;

/// Where every Discogs request goes.
const API_BASE_URL: &str = "https://api.discogs.com";

#[cfg(not(any(test, feature = "test-utils")))]
fn api_base_url() -> String {
    API_BASE_URL.to_string()
}

/// The redirectable form of [`API_BASE_URL`], compiled only into test builds —
/// the same seam `musicbrainz::set_base_url_for_test` gives that client. A
/// client is built per call site rather than held as one static, so the
/// override is read at construction.
#[cfg(any(test, feature = "test-utils"))]
fn api_base_url() -> String {
    BASE_URL_OVERRIDE
        .lock()
        .expect("Discogs base URL mutex poisoned")
        .clone()
        .unwrap_or_else(|| API_BASE_URL.to_string())
}

#[cfg(any(test, feature = "test-utils"))]
static BASE_URL_OVERRIDE: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Point every Discogs request built after this call at `url` (`None` restores
/// the live API), so a test never reaches the real service — and never spends a
/// fixture's fake key on a real auth check, which would come back 401 and mark
/// the stored key rejected for everything after it. Process-wide, like the
/// caches it sits next to.
#[cfg(any(test, feature = "test-utils"))]
pub fn set_base_url_for_test(url: Option<String>) {
    *BASE_URL_OVERRIDE
        .lock()
        .expect("Discogs base URL mutex poisoned") = url;
}

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

    /// A client that reports each call's outcome to `observer`, so a stored key
    /// re-validates as it's used.
    pub fn with_observer(api_key: String, observer: DiscogsValidationObserver) -> Self {
        Self::build(api_key, Some(observer))
    }

    fn build(api_key: String, observer: Option<DiscogsValidationObserver>) -> Self {
        Self {
            client: crate::util::http::client_builder()
                .build()
                .expect("Failed to build HTTP client"),
            api_key,
            base_url: api_base_url(),
            observer,
        }
    }

    /// Requests target `base_url` instead of the live API, so a test can point at a
    /// local mock or a refused port. Production always goes through `new` /
    /// `with_observer`.
    #[cfg(test)]
    pub(crate) fn with_base_url(api_key: String, base_url: String) -> Self {
        Self {
            base_url,
            ..Self::build(api_key, None)
        }
    }

    /// A 401 is the only error that proves the key is bad, and a success confirms it.
    /// A network or rate-limit error must NOT reject a good key.
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

    /// Every public request method routes through here, so an outcome folds into the
    /// key's validation state in exactly one place.
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

    async fn send(
        &self,
        request: reqwest::RequestBuilder,
        priority: CallPriority,
    ) -> Result<Response, DiscogsError> {
        RATE_LIMITER.wait(priority).await;
        let response = request.send().await?;
        classify_discogs_response(response)
    }

    /// Check the API token with a request cheap enough to throw away.
    pub async fn validate_token(&self, priority: CallPriority) -> Result<(), DiscogsError> {
        let url = format!("{}/database/search", self.base_url);
        let query_params = [("per_page", "1")];

        retry_with_backoff_if(
            DISCOGS_RETRY_ATTEMPTS,
            "Discogs token validation",
            should_retry_discogs,
            crate::retry::linear_backoff,
            || async {
                self.send(self.get(&url).query(&query_params), priority)
                    .await
                    .map(|_| ())
            },
        )
        .await
    }

    /// Search on any combination of the supported parameters.
    pub async fn search_with_params(
        &self,
        params: &DiscogsSearchParams,
        priority: CallPriority,
    ) -> Result<Vec<DiscogsSearchResult>, DiscogsError> {
        self.observed(self.search_with_params_inner(params, priority))
            .await
    }

    async fn search_with_params_inner(
        &self,
        params: &DiscogsSearchParams,
        priority: CallPriority,
    ) -> Result<Vec<DiscogsSearchResult>, DiscogsError> {
        use tracing::{debug, warn};
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
        debug!("Discogs API: GET {} with params: {:?}", url, params);
        let search_response: SearchResponse = retry_with_backoff_if(
            DISCOGS_RETRY_ATTEMPTS,
            "Discogs search",
            should_retry_discogs,
            crate::retry::linear_backoff,
            || async {
                let response = self
                    .send(self.get(&url).query(&query_params), priority)
                    .await
                    .inspect_err(|error| match error {
                        DiscogsError::RateLimit => warn!("Discogs rate limit exceeded"),
                        DiscogsError::InvalidApiKey => warn!("Discogs invalid API key"),
                        DiscogsError::NotFound => warn!("Discogs API returned not found"),
                        DiscogsError::Transport(_) => warn!("Discogs API request failed"),
                        DiscogsError::Provider(status) => {
                            warn!("Discogs API error response (status {status})")
                        }
                        DiscogsError::Serialization(_) => {}
                    })?;
                debug!("Response status: {}", response.status());
                response.json().await.map_err(DiscogsError::Transport)
            },
        )
        .await?;
        debug!(
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
        debug!("  → {} release(s) after filtering", releases.len());
        Ok(releases)
    }
    /// A release, parsed, plus the raw JSON the API returned.
    pub async fn get_release(
        &self,
        id: &str,
        priority: CallPriority,
    ) -> Result<(DiscogsRelease, String), DiscogsError> {
        self.observed(self.get_release_inner(id, priority)).await
    }

    async fn get_release_inner(
        &self,
        id: &str,
        priority: CallPriority,
    ) -> Result<(DiscogsRelease, String), DiscogsError> {
        if let Some(hit) = RELEASE_CACHE.get_cloned(id) {
            debug!("Discogs release cache hit for {}", id);
            return Ok(hit);
        }

        let url = format!("{}/releases/{}", self.base_url, id);
        let value = retry_with_backoff_if(
            DISCOGS_RETRY_ATTEMPTS,
            "Discogs release fetch",
            should_retry_discogs,
            crate::retry::linear_backoff,
            || async {
                let response = self.send(self.get(&url), priority).await?;
                let raw_json = response.text().await.map_err(DiscogsError::Transport)?;
                let release = parse_discogs_release_json(&raw_json)?;
                Ok((release, raw_json))
            },
        )
        .await?;
        RELEASE_CACHE.put(id, value.clone());
        Ok(value)
    }

    /// The master's year — the *original* release year, so 1967 for a 1985 reissue —
    /// plus the raw JSON.
    pub async fn get_master(
        &self,
        master_id: &str,
        priority: CallPriority,
    ) -> Result<(Option<u32>, String), DiscogsError> {
        self.observed(self.get_master_inner(master_id, priority))
            .await
    }

    async fn get_master_inner(
        &self,
        master_id: &str,
        priority: CallPriority,
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
            crate::retry::linear_backoff,
            || async {
                let response = self.send(self.get(&url), priority).await?;
                let raw_json = response.text().await.map_err(DiscogsError::Transport)?;
                let year = parse_discogs_master_year(&raw_json)?;
                Ok((year, raw_json))
            },
        )
        .await?;
        MASTER_CACHE.put(master_id, value.clone());
        Ok(value)
    }

    pub async fn get_artist_image(
        &self,
        artist_id: &str,
        priority: CallPriority,
    ) -> Result<Option<String>, DiscogsError> {
        self.observed(self.get_artist_image_inner(artist_id, priority))
            .await
    }

    async fn get_artist_image_inner(
        &self,
        artist_id: &str,
        priority: CallPriority,
    ) -> Result<Option<String>, DiscogsError> {
        let url = format!("{}/artists/{}", self.base_url, artist_id);
        let Some(json) = retry_with_backoff_if(
            DISCOGS_RETRY_ATTEMPTS,
            "Discogs artist fetch",
            should_retry_discogs,
            crate::retry::linear_backoff,
            || async {
                let response = match self.send(self.get(&url), priority).await {
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
                    response.json().await.map_err(DiscogsError::Transport)?;
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

/// The Discogs cross-reference for an MB release: given the Discogs URL from MB's
/// url-rels, fetch the linked release and, if it has one, its master. Returns the
/// `DiscogsRelease` plus the documents to store, each keyed by its own entity —
/// so the release a reader reaches through the MB document's url-rels is the same
/// row a Discogs-seeded import would have written.
///
/// `None` when the URL holds no numeric release ID, or the release fetch fails — an
/// auth failure included, which marks the stored key `Rejected` through the client's
/// observer and surfaces in the UI rather than aborting the import. A successful
/// release fetch with a failing master fetch still returns `Some` with just the
/// release pair; the master is best-effort.
pub async fn fetch_discogs_xref(
    client: &DiscogsClient,
    discogs_url: &str,
    priority: CallPriority,
) -> Option<(DiscogsRelease, Vec<crate::import::SourcePayload>)> {
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

    tracing::debug!(
        "Found Discogs release URL: {}, fetching release {}",
        discogs_url,
        id
    );
    let mut payloads: Vec<crate::import::SourcePayload> = Vec::new();
    let discogs_release = match client.get_release(&id, priority).await {
        Ok((release, raw)) => {
            payloads.push(crate::import::SourcePayload::new(
                crate::import::PayloadSource::Discogs,
                &id,
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
        match client.get_master(master_id, priority).await {
            Ok((_year, master_json)) => {
                payloads.push(crate::import::SourcePayload::new(
                    crate::import::PayloadSource::DiscogsMaster,
                    master_id,
                    master_json,
                ));
            }
            Err(e) => {
                tracing::warn!("Failed to fetch Discogs master {}: {}", master_id, e);
            }
        }
    }

    Some((discogs_release, payloads))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::MetadataSource;
    use serial_test::serial;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex, OnceLock};
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
    const BAD_REQUEST: &str = "HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n";

    fn discogs_test_guard() -> &'static tokio::sync::Mutex<()> {
        static GUARD: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
        GUARD.get_or_init(|| tokio::sync::Mutex::new(()))
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

    fn search_result_with_cover_fields(
        cover_image: Option<&str>,
        thumb: Option<&str>,
    ) -> DiscogsSearchResult {
        DiscogsSearchResult {
            id: 1,
            title: "Artist Name - Album Title".to_string(),
            year: None,
            format: None,
            country: None,
            label: None,
            catno: None,
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
        // A rate limit says nothing about the key, so it must not signal — a
        // transient blip cannot be allowed to reject a good key.
        client.observe::<()>(&Err(DiscogsError::RateLimit));

        assert_eq!(*signals.lock().unwrap(), vec!["accepted", "rejected"]);
    }

    #[tokio::test]
    #[serial(discogs_rate_limiter)]
    async fn transport_error_display_does_not_include_discogs_token() {
        let _guard = discogs_test_guard().lock().await;
        RATE_LIMITER.reset();
        let token = "secret-discogs-token";
        let mut client = DiscogsClient::new(token.to_string());
        client.base_url = "http://127.0.0.1:1".to_string();

        let error = client
            .validate_token(CallPriority::Interactive)
            .await
            .unwrap_err();

        assert!(!error.to_string().contains(token));
    }

    #[tokio::test]
    #[serial(discogs_rate_limiter)]
    async fn validate_token_sends_token_in_authorization_header() {
        let _guard = discogs_test_guard().lock().await;
        RATE_LIMITER.reset();
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
            .validate_token(CallPriority::Interactive)
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
    #[serial(discogs_rate_limiter)]
    async fn search_retries_rate_limit_then_returns_success() {
        let _guard = discogs_test_guard().lock().await;
        RATE_LIMITER.reset();
        let (url, request_count) =
            discogs_response_server(vec![RATE_LIMITED, SEARCH_OK_EMPTY]).await;
        let mut client = DiscogsClient::new("token".to_string());
        client.base_url = url;

        let releases = client
            .search_with_params(&DiscogsSearchParams::default(), CallPriority::Interactive)
            .await
            .expect("retry should return the successful search response");

        assert!(releases.is_empty());
        assert_eq!(request_count.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    #[serial(discogs_rate_limiter)]
    async fn search_returns_persistent_rate_limit_after_retry_attempts() {
        let _guard = discogs_test_guard().lock().await;
        RATE_LIMITER.reset();
        let (url, request_count) =
            discogs_response_server(vec![RATE_LIMITED, RATE_LIMITED, RATE_LIMITED]).await;
        let mut client = DiscogsClient::new("token".to_string());
        client.base_url = url;

        let error = client
            .search_with_params(&DiscogsSearchParams::default(), CallPriority::Interactive)
            .await
            .expect_err("persistent rate limit should fail after retry attempts");

        assert!(matches!(error, DiscogsError::RateLimit));
        assert_eq!(request_count.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    #[serial(discogs_rate_limiter)]
    async fn search_does_not_retry_invalid_api_key() {
        let _guard = discogs_test_guard().lock().await;
        RATE_LIMITER.reset();
        let (url, request_count) = discogs_response_server(vec![UNAUTHORIZED]).await;
        let mut client = DiscogsClient::new("token".to_string());
        client.base_url = url;

        let error = client
            .search_with_params(&DiscogsSearchParams::default(), CallPriority::Interactive)
            .await
            .expect_err("invalid API key should fail without retry");

        assert!(matches!(error, DiscogsError::InvalidApiKey));
        assert_eq!(request_count.load(Ordering::SeqCst), 1);
    }

    /// A 4xx that isn't one of the carved-out statuses is the server's permanent
    /// answer to this request — it must be tried once, not retried. Before the
    /// error split it landed in `Request` and was retried like a transport failure.
    #[tokio::test]
    #[serial(discogs_rate_limiter)]
    async fn search_does_not_retry_client_error() {
        let _guard = discogs_test_guard().lock().await;
        RATE_LIMITER.reset();
        let (url, request_count) = discogs_response_server(vec![BAD_REQUEST]).await;
        let mut client = DiscogsClient::new("token".to_string());
        client.base_url = url;

        let error = client
            .search_with_params(&DiscogsSearchParams::default(), CallPriority::Interactive)
            .await
            .expect_err("a 400 should fail without retry");

        assert!(
            matches!(error, DiscogsError::Provider(StatusCode::BAD_REQUEST)),
            "expected Provider(400), got {error:?}",
        );
        assert_eq!(request_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn retry_policy_repeats_only_transient_failures() {
        assert!(should_retry_discogs(&DiscogsError::RateLimit));
        assert!(should_retry_discogs(&DiscogsError::Provider(
            StatusCode::INTERNAL_SERVER_ERROR
        )));
        assert!(should_retry_discogs(&DiscogsError::Provider(
            StatusCode::SERVICE_UNAVAILABLE
        )));
        assert!(!should_retry_discogs(&DiscogsError::Provider(
            StatusCode::BAD_REQUEST
        )));
        assert!(!should_retry_discogs(&DiscogsError::Provider(
            StatusCode::FORBIDDEN
        )));
        assert!(!should_retry_discogs(&DiscogsError::Provider(
            StatusCode::UNPROCESSABLE_ENTITY
        )));
        assert!(!should_retry_discogs(&DiscogsError::InvalidApiKey));
        assert!(!should_retry_discogs(&DiscogsError::NotFound));
    }

    #[tokio::test]
    #[serial(discogs_rate_limiter)]
    async fn search_does_not_retry_not_found() {
        let _guard = discogs_test_guard().lock().await;
        RATE_LIMITER.reset();
        let (url, request_count) = discogs_response_server(vec![NOT_FOUND]).await;
        let mut client = DiscogsClient::new("token".to_string());
        client.base_url = url;

        let error = client
            .search_with_params(&DiscogsSearchParams::default(), CallPriority::Interactive)
            .await
            .expect_err("not found should fail without retry");

        assert!(matches!(error, DiscogsError::NotFound));
        assert_eq!(request_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    #[serial(discogs_rate_limiter)]
    async fn search_observer_records_one_signal_after_internal_retry() {
        let _guard = discogs_test_guard().lock().await;
        RATE_LIMITER.reset();
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
            .search_with_params(&DiscogsSearchParams::default(), CallPriority::Interactive)
            .await
            .expect("retry should return the successful search response");

        assert_eq!(request_count.load(Ordering::SeqCst), 2);
        assert_eq!(*signals.lock().unwrap(), vec!["accepted"]);
    }
}
