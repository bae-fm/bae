//! MusicBrainz API client.
//!
//! Builds and rate-limits the MusicBrainz web-service requests, caches
//! their results for the session, and exposes the lookup/search entry
//! points plus `MusicBrainzError`. The response shapes these requests
//! deserialize into live in the `types` submodule, re-exported here so
//! callers keep using `crate::musicbrainz::Mb…` paths.

use std::sync::OnceLock;
use std::time::Duration;

use crate::util::session_cache::SessionCache;
use thiserror::Error;
use tokio::sync::Mutex;
use tokio::time::Instant;
use tracing::{debug, info, warn};

mod types;
pub use types::*;

/// Shared HTTP client for all MusicBrainz requests.
fn http_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        crate::util::http::client_builder()
            .build()
            .expect("Failed to create HTTP client")
    })
}

/// Rate limiter ensuring at least 1 second between MusicBrainz API requests.
fn rate_limiter() -> &'static Mutex<Instant> {
    static LIMITER: OnceLock<Mutex<Instant>> = OnceLock::new();
    LIMITER.get_or_init(|| Mutex::new(Instant::now() - Duration::from_secs(1)))
}

async fn wait_for_rate_limit() {
    let mut last_request = rate_limiter().lock().await;
    let elapsed = last_request.elapsed();
    if elapsed < Duration::from_secs(1) {
        tokio::time::sleep(Duration::from_secs(1) - elapsed).await;
    }
    *last_request = Instant::now();
}

async fn mb_get(request: reqwest::RequestBuilder) -> Result<reqwest::Response, MusicBrainzError> {
    wait_for_rate_limit().await;
    let response = request
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(MusicBrainzError::from_reqwest)?;

    let status = response.status();
    if status.is_success() {
        Ok(response)
    } else {
        let url = response.url().clone();
        match response.text().await {
            Ok(error_text) => warn!(
                "MusicBrainz API error response ({} from {}): {}",
                status, url, error_text
            ),
            Err(error) => warn!(
                "MusicBrainz API error response ({} from {}) with unreadable body: {}",
                status, url, error
            ),
        }
        Err(MusicBrainzError::Provider {
            status: Some(status.as_u16()),
        })
    }
}

type ReleaseCacheValue = (MbReleaseResponse, ExternalUrls, String);

/// In-memory cache for `release/{id}` lookups. Stores the parsed
/// response, extracted URLs, and raw JSON so callers that need any of
/// the three can hit warm cache. Mutex is `std::sync::Mutex` since the
/// guard is dropped before any await.
static RELEASE_CACHE: SessionCache<ReleaseCacheValue> =
    SessionCache::new("MusicBrainz release cache");

/// In-memory cache for `release-group/{id}` JSON. Discogs cross-reference
/// archival keeps the raw JSON; we cache that string directly.
static RELEASE_GROUP_JSON_CACHE: SessionCache<String> =
    SessionCache::new("MusicBrainz release-group JSON cache");

/// In-memory cache for `url?resource=https://www.discogs.com/release/{id}`
/// lookups. Key: Discogs release ID; value: linked MB release ID (or
/// `None` if no link exists). This call is part of the cross-reference
/// archival path; caching means a confirmed Discogs import warms the
/// lookup for the worker's later commit-time call.
static DISCOGS_URL_LOOKUP_CACHE: SessionCache<Option<String>> =
    SessionCache::new("Discogs URL lookup cache");

/// Pre-populate the Discogs-URL → MB-release-ID lookup cache. Tests
/// use this to short-circuit the cross-reference path without hitting
/// the network. `None` means "no MB release linked", which is the
/// natural result for synthetic test releases.
#[cfg(any(test, feature = "test-utils"))]
pub fn seed_discogs_url_lookup(discogs_release_id: &str, mb_release_id: Option<String>) {
    DISCOGS_URL_LOOKUP_CACHE.put(discogs_release_id, mb_release_id);
}

/// Pre-populate the MB release cache. Tests use this to drive the
/// reverse cross-reference path (`fetch_mb_xref`) without making an
/// HTTP call. The raw JSON ends up in `release_metadata` for archival;
/// tests can pass an empty string when the archival content isn't
/// being asserted on.
#[cfg(any(test, feature = "test-utils"))]
pub fn seed_release_cache(release_id: &str, value: (MbReleaseResponse, ExternalUrls, String)) {
    RELEASE_CACHE.put(release_id, value);
}

/// Pre-populate the MB release-group JSON cache. Pairs with
/// `seed_release_cache` for tests that exercise the cross-reference
/// path without HTTP.
#[cfg(any(test, feature = "test-utils"))]
pub fn seed_release_group_json_cache(release_group_id: &str, raw_json: String) {
    RELEASE_GROUP_JSON_CACHE.put(release_group_id, raw_json);
}

/// A MusicBrainz lookup failure, keeping the wire-level distinction the
/// caller needs to localize: a transport failure that produced no HTTP
/// response, a timeout, or an HTTP error *response* carrying a status. The
/// status is preserved at the point of construction — flattening it into a
/// formatted string here would destroy it for every consumer.
///
/// `Other` carries local/internal failures (URL construction, JSON parsing,
/// reading the body) — diagnostic detail, never a provider verdict.
#[derive(Debug, Error)]
pub enum MusicBrainzError {
    /// No release matched the DiscID (404 or an empty result set).
    #[error("No release found for DISCID: {0}")]
    NotFound(String),
    /// The request never reached a response — connection refused, DNS
    /// failure, a dropped body. Carries the underlying error for logging.
    #[error("MusicBrainz network error: {0}")]
    Network(String),
    /// The request timed out before a response arrived.
    #[error("MusicBrainz request timed out")]
    Timeout,
    /// MusicBrainz returned an HTTP error response. `status` is the HTTP
    /// status code when one was observed (`None` when reqwest classified
    /// a send error as carrying a status we couldn't read).
    #[error("MusicBrainz returned an error response (status {status:?})")]
    Provider { status: Option<u16> },
    /// A local/internal failure (URL construction, JSON parsing, body read).
    #[error("MusicBrainz API error: {0}")]
    Other(String),
}

impl MusicBrainzError {
    /// Classify a `reqwest::Error` from a `.send()` / body read into the
    /// wire-level failure it represents. A timeout is distinct; an error
    /// carrying an HTTP status is a `Provider` response; everything else
    /// (connection, DNS, dropped body) is a transport `Network` failure.
    fn from_reqwest(e: reqwest::Error) -> Self {
        if e.is_timeout() {
            MusicBrainzError::Timeout
        } else if let Some(status) = e.status() {
            MusicBrainzError::Provider {
                status: Some(status.as_u16()),
            }
        } else {
            MusicBrainzError::Network(e.to_string())
        }
    }
}

// ============================================================================
// API functions
// ============================================================================

/// Lookup releases by MusicBrainz DiscID
pub async fn lookup_by_discid(
    discid: &str,
) -> Result<(Vec<MbReleaseResponse>, ExternalUrls), MusicBrainzError> {
    info!("MusicBrainz: Looking up DiscID '{}'", discid);
    let base_url = reqwest::Url::parse("https://musicbrainz.org/ws/2/discid/")
        .map_err(|e| MusicBrainzError::Other(format!("Failed to parse base URL: {}", e)))?;
    let url = base_url
        .join(discid)
        .map_err(|e| MusicBrainzError::Other(format!("Failed to construct DiscID URL: {}", e)))?;
    let mut url_with_params = url.clone();
    url_with_params.set_query(Some(
        "inc=recordings+artist-credits+release-groups+url-rels+labels",
    ));
    debug!("MusicBrainz API request: {}", url_with_params);

    let response = match mb_get(http_client().get(url_with_params.as_str())).await {
        Ok(response) => response,
        Err(MusicBrainzError::Provider { status: Some(404) }) => {
            return Err(MusicBrainzError::NotFound(discid.to_string()));
        }
        Err(error) => return Err(error),
    };

    let disc_response: DiscIdResponse = response
        .json()
        .await
        .map_err(|e| MusicBrainzError::Other(format!("Failed to parse JSON: {}", e)))?;

    let mut external_urls = ExternalUrls {
        discogs_release_url: None,
    };

    for release in &disc_response.releases {
        if external_urls.discogs_release_url.is_some() {
            break;
        }
        extract_urls_from_relations(&release.relations, &mut external_urls);
    }

    if disc_response.releases.is_empty() {
        return Err(MusicBrainzError::NotFound(discid.to_string()));
    }

    let releases = disc_response.releases;

    info!(
        "MusicBrainz found {} release(s) for DiscID {}",
        releases.len(),
        discid
    );

    if external_urls.discogs_release_url.is_some() {
        info!("  Found Discogs URL in relationships");
    }

    Ok((releases, external_urls))
}

/// Fetch a release-group with its URL relationships
async fn fetch_release_group_with_relations(
    release_group_id: &str,
) -> Result<ReleaseGroupResponse, MusicBrainzError> {
    let url = format!(
        "https://musicbrainz.org/ws/2/release-group/{}?inc=url-rels",
        release_group_id
    );
    debug!("Fetching release-group with relations: {}", url);

    let response = mb_get(http_client().get(&url)).await?;

    response
        .json()
        .await
        .map_err(|e| MusicBrainzError::Other(format!("Failed to parse JSON: {}", e)))
}

/// Lookup a specific release by MusicBrainz release ID.
///
/// Returns the parsed response, extracted ExternalUrls, and the raw JSON text
/// (for archival in release_metadata). Hits a session-wide LRU cache of size
/// 25; cache miss does the network round-trip + a release-group fallback
/// fetch when the release relations don't carry Discogs URLs.
pub async fn lookup_release_by_id(
    release_id: &str,
) -> Result<(MbReleaseResponse, ExternalUrls, String), MusicBrainzError> {
    if let Some(hit) = RELEASE_CACHE.get_cloned(release_id) {
        debug!("MusicBrainz release cache hit for {}", release_id);
        return Ok(hit);
    }

    info!("MusicBrainz: Looking up release ID '{}'", release_id);
    let url = format!(
        "https://musicbrainz.org/ws/2/release/{}?inc=recordings+artist-credits+release-groups+release-group-rels+url-rels+labels+media+recording-level-rels+work-level-rels+work-rels+artist-rels",
        release_id,
    );
    debug!("MusicBrainz API request: {}", url);

    let response = match mb_get(http_client().get(&url)).await {
        Ok(response) => response,
        Err(MusicBrainzError::Provider { status: Some(404) }) => {
            return Err(MusicBrainzError::NotFound(release_id.to_string()));
        }
        Err(error) => return Err(error),
    };

    let raw_json = response
        .text()
        .await
        .map_err(|e| MusicBrainzError::Other(format!("Failed to read response body: {}", e)))?;

    let mb_response: MbReleaseResponse = serde_json::from_str(&raw_json)
        .map_err(|e| MusicBrainzError::Other(format!("Failed to parse JSON: {}", e)))?;

    let mut external_urls = mb_response.extract_external_urls();

    debug!(
        "MusicBrainz release response: {} ({} relations), release_id: {}",
        mb_response.title,
        mb_response.relations.len(),
        release_id
    );

    if let Some(resource) = &external_urls.discogs_release_url {
        info!("Found Discogs release URL: {}", resource);
    }

    // If release-group relations weren't included inline, fetch them separately
    if external_urls.discogs_release_url.is_none() {
        let has_rg_relations = mb_response
            .release_group
            .as_ref()
            .is_some_and(|rg| rg.relations.is_some());

        if !has_rg_relations {
            if let Some(rg_id) = mb_response.release_group.as_ref().map(|rg| rg.id.as_str()) {
                debug!(
                    "Release-group relations not found, fetching release-group {} separately",
                    rg_id
                );

                merge_release_group_external_urls(
                    rg_id,
                    fetch_release_group_with_relations(rg_id).await,
                    &mut external_urls,
                );
            }
        }
    }

    let value = (mb_response, external_urls, raw_json);
    RELEASE_CACHE.put(release_id, value.clone());

    Ok(value)
}

fn merge_release_group_external_urls(
    rg_id: &str,
    result: Result<ReleaseGroupResponse, MusicBrainzError>,
    external_urls: &mut ExternalUrls,
) {
    match result {
        Ok(rg_response) => {
            extract_urls_from_relations(&rg_response.relations, external_urls);

            if let Some(resource) = &external_urls.discogs_release_url {
                info!("Found Discogs release URL on release-group: {}", resource);
            }
        }
        Err(e) => {
            warn!("Failed to fetch MusicBrainz release-group {rg_id}: {e}");
        }
    }
}

/// Fetch a release-group by ID, returning the raw JSON for metadata storage.
///
/// Uses the same endpoint as `fetch_release_group_with_relations` but returns the
/// raw JSON text instead of a parsed struct. Hits the session-wide LRU cache.
pub async fn fetch_release_group_json(release_group_id: &str) -> Result<String, MusicBrainzError> {
    if let Some(hit) = RELEASE_GROUP_JSON_CACHE.get_cloned(release_group_id) {
        debug!(
            "MusicBrainz release-group JSON cache hit for {}",
            release_group_id
        );
        return Ok(hit);
    }

    let url = format!(
        "https://musicbrainz.org/ws/2/release-group/{}?inc=url-rels&fmt=json",
        release_group_id
    );
    debug!("Fetching release-group JSON: {}", url);

    let response = mb_get(http_client().get(&url)).await?;

    let raw_json = response
        .text()
        .await
        .map_err(|e| MusicBrainzError::Other(format!("Failed to read response body: {}", e)))?;

    RELEASE_GROUP_JSON_CACHE.put(release_group_id, raw_json.clone());

    Ok(raw_json)
}

/// Look up a MusicBrainz release linked to a Discogs release URL.
///
/// Uses the MB URL lookup endpoint to find releases linked to the given Discogs release.
/// Returns the MB release ID if a linked release exists. Hits a session-wide
/// LRU cache keyed by the Discogs release ID.
pub async fn lookup_release_id_by_discogs_url(
    discogs_release_id: &str,
) -> Result<Option<String>, MusicBrainzError> {
    if let Some(hit) = DISCOGS_URL_LOOKUP_CACHE.get_cloned(discogs_release_id) {
        debug!(
            "MusicBrainz URL lookup cache hit for {}",
            discogs_release_id
        );
        return Ok(hit);
    }

    let discogs_url = format!("https://www.discogs.com/release/{}", discogs_release_id);
    let url = format!(
        "https://musicbrainz.org/ws/2/url?resource={}&inc=release-rels&fmt=json",
        discogs_url
    );
    debug!("MusicBrainz URL lookup: {}", url);

    let response = match mb_get(http_client().get(&url)).await {
        Ok(response) => response,
        Err(MusicBrainzError::Provider { status: Some(404) }) => {
            DISCOGS_URL_LOOKUP_CACHE.put(discogs_release_id, None);
            return Ok(None);
        }
        Err(error) => return Err(error),
    };

    let lookup: UrlLookupResponse = response
        .json()
        .await
        .map_err(|e| MusicBrainzError::Other(format!("Failed to parse JSON: {}", e)))?;

    // Find the first relation that has a release with an ID
    let release_id = lookup
        .relations
        .iter()
        .filter(|r| r.relation_type.as_deref() == Some("discogs"))
        .find_map(|r| r.release.as_ref().and_then(|rel| rel.id.clone()));

    DISCOGS_URL_LOOKUP_CACHE.put(discogs_release_id, release_id.clone());

    Ok(release_id)
}

/// Fetch the MB cross-reference for a Discogs release. Mirrors
/// `crate::discogs::client::fetch_discogs_xref` (the forward direction
/// for MB→Discogs).
///
/// Queries MB's URL endpoint for an MB release linking back to the
/// given Discogs release. When found, fetches the full MB release and
/// release-group JSON for archival. Returns the parsed
/// `MbReleaseResponse` (so callers can pull the release ID and group
/// ID for `release_identities`) plus the raw JSON pairs to append to
/// the caller's `release_metadata` collection.
///
/// Returns `None` if MB has no linked release, the MB URL lookup
/// fails, or the linked release fetch fails. A successful release
/// fetch with a failing release-group fetch returns `Some` with just
/// the release pair — the release-group is best-effort.
///
/// Same editor dependence as the forward direction: only works when
/// an MB editor has linked the Discogs URL on the MB release.
pub async fn fetch_mb_xref(
    discogs_release_id: &str,
) -> Option<(MbReleaseResponse, Vec<(String, String)>)> {
    let mb_release_id = match lookup_release_id_by_discogs_url(discogs_release_id).await {
        Ok(Some(id)) => {
            info!("Found linked MB release: {}", id);
            id
        }
        Ok(None) => {
            info!(
                "No MB release linked to Discogs release {}",
                discogs_release_id
            );
            return None;
        }
        Err(e) => {
            warn!(
                "Failed to look up MB release for Discogs {}: {e}",
                discogs_release_id
            );
            return None;
        }
    };

    let (response, _urls, raw_json) = match lookup_release_by_id(&mb_release_id).await {
        Ok(value) => value,
        Err(e) => {
            warn!("Failed to fetch linked MB release {}: {e}", mb_release_id);
            return None;
        }
    };

    let mut pairs = vec![(
        crate::import::MetadataSource::MusicBrainz
            .as_str()
            .to_string(),
        raw_json,
    )];

    if let Some(rg_id) = response.release_group.as_ref().map(|rg| rg.id.as_str()) {
        match fetch_release_group_json(rg_id).await {
            Ok(rg_json) => {
                pairs.push(("musicbrainz_release_group".to_string(), rg_json));
            }
            Err(e) => {
                warn!("Failed to fetch MB release-group: {e}");
            }
        }
    }

    Some((response, pairs))
}

// ============================================================================
// Search
// ============================================================================

/// Parameters for searching MusicBrainz releases
#[derive(Debug, Clone, Default)]
pub struct ReleaseSearchParams {
    pub artist: Option<String>,
    pub album: Option<String>,
    pub year: Option<String>,
    pub label: Option<String>,
    pub catalog_number: Option<String>,
    pub barcode: Option<String>,
}

impl ReleaseSearchParams {
    fn query_fields(&self) -> [(&Option<String>, &'static str, QueryValueFormat); 6] {
        [
            (&self.artist, "artist", QueryValueFormat::Quoted),
            (&self.album, "release", QueryValueFormat::Quoted),
            (&self.year, "date", QueryValueFormat::Bare),
            (&self.label, "label", QueryValueFormat::Quoted),
            (&self.catalog_number, "catno", QueryValueFormat::Quoted),
            (&self.barcode, "barcode", QueryValueFormat::Bare),
        ]
    }

    /// Check if at least one field is filled
    pub fn has_any_field(&self) -> bool {
        self.query_fields().iter().any(|(value, _, _)| {
            value
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
        })
    }

    /// Build Lucene query string from filled fields
    fn build_query(&self) -> String {
        self.query_fields()
            .into_iter()
            .filter_map(|(value, key, format)| {
                let value = value.as_deref()?.trim();
                (!value.is_empty()).then(|| format.render(key, value))
            })
            .collect::<Vec<_>>()
            .join(" AND ")
    }
}

#[derive(Copy, Clone)]
enum QueryValueFormat {
    Bare,
    Quoted,
}

impl QueryValueFormat {
    fn render(self, key: &str, value: &str) -> String {
        match self {
            Self::Bare => format!("{}:{}", key, value),
            Self::Quoted => {
                let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
                format!("{}:\"{}\"", key, escaped)
            }
        }
    }
}

/// Search MusicBrainz for releases using structured parameters
pub async fn search_releases_with_params(
    params: &ReleaseSearchParams,
) -> Result<Vec<SearchRelease>, MusicBrainzError> {
    if !params.has_any_field() {
        return Err(MusicBrainzError::Other(
            "At least one search field must be provided".to_string(),
        ));
    }
    let query = params.build_query();
    info!("MusicBrainz: Searching with params: {:?}", params);
    info!("   Query: {}", query);
    let url = "https://musicbrainz.org/ws/2/release";
    debug!(
        "MusicBrainz API request: {}?query={}&limit=25&inc=recordings+artist-credits+release-groups+labels+media+url-rels+recording-level-rels+work-level-rels+work-rels+artist-rels",
        url, query
    );

    let request = http_client().get(url).query(&[
        ("query", query.as_str()),
        ("limit", "25"),
        (
            "inc",
            "recordings+artist-credits+release-groups+labels+media+url-rels+recording-level-rels+work-level-rels+work-rels+artist-rels",
        ),
    ]);
    let response = match mb_get(request).await {
        Ok(response) => response,
        Err(MusicBrainzError::Provider { status: Some(404) }) => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };

    let search_response: SearchResponse = response
        .json()
        .await
        .map_err(|e| MusicBrainzError::Other(format!("Failed to parse JSON: {}", e)))?;

    if let Some(ref error_msg) = search_response.error {
        warn!("MusicBrainz API returned error: {}", error_msg);
        return Err(MusicBrainzError::Other(format!(
            "MusicBrainz error: {}",
            error_msg
        )));
    }

    let releases = search_response.releases;

    info!("Found {} release(s)", releases.len());
    Ok(releases)
}

#[cfg(test)]
mod tests;
