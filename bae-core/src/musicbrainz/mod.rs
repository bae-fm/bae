//! MusicBrainz API client.
//!
//! Builds, rate-limits, times out, and retries the MusicBrainz web-service
//! requests, caches their results for the session, and exposes the lookup/search
//! entry points plus `MusicBrainzError`. A caller just awaits a lookup; the retry
//! policy is this module's, not theirs.
//!
//! The response shapes these requests deserialize into live in `types`,
//! re-exported here so callers keep using `crate::musicbrainz::Mb…` paths.

use std::sync::OnceLock;
use std::time::Duration;

use crate::util::rate_limiter::RateLimiter;
use crate::util::session_cache::SessionCache;
use thiserror::Error;
use tracing::{debug, warn};

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
static RATE_LIMITER: RateLimiter = RateLimiter::new(Duration::from_secs(1));

/// Retry only what a retry can fix. `NotFound` is MusicBrainz's answer, not a
/// fault — and it's the ordinary answer for a disc it doesn't have, so retrying
/// buys three round trips and three rate-limit waits to learn it again. `Other`
/// is local (URL construction, JSON parse, a missing search field): either no
/// request was made, or the same bytes will parse the same way.
fn should_retry_mb(error: &MusicBrainzError) -> bool {
    match error {
        MusicBrainzError::Network(_) | MusicBrainzError::Timeout => true,
        MusicBrainzError::Provider { status } => {
            matches!(status, Some(429) | Some(500..=599) | None)
        }
        MusicBrainzError::NotFound(_) | MusicBrainzError::Other(_) => false,
    }
}

/// Wrap one request in the client's own retry policy — a caller shouldn't have to
/// know which of these failures are worth repeating. (Discogs does the same.)
async fn mb_retry<F, Fut, T>(label: &str, f: F) -> Result<T, MusicBrainzError>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, MusicBrainzError>>,
{
    crate::retry::retry_with_backoff_if(3, label, should_retry_mb, crate::retry::linear_backoff, f)
        .await
}

async fn mb_get(request: reqwest::RequestBuilder) -> Result<reqwest::Response, MusicBrainzError> {
    RATE_LIMITER.wait().await;
    let response = request
        .header("Accept", "application/json")
        .timeout(crate::util::http::API_TIMEOUT)
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

type ReleaseCacheValue = (MbReleaseResponse, Option<String>, String);

/// In-memory cache for `release/{id}` lookups. Holds the parsed response, the
/// Discogs release URL from its url-rels (if any), and the raw JSON, so a caller
/// needing any of the three hits a warm cache.
static RELEASE_CACHE: SessionCache<ReleaseCacheValue> =
    SessionCache::new("MusicBrainz release cache");

/// In-memory cache for `release-group/{id}` JSON. Discogs cross-reference
/// archival keeps the raw JSON; we cache that string directly.
static RELEASE_GROUP_JSON_CACHE: SessionCache<String> =
    SessionCache::new("MusicBrainz release-group JSON cache");

/// In-memory cache for `url?resource=https://www.discogs.com/release/{id}`
/// lookups, keyed by Discogs release ID; the value is the linked MB release ID,
/// or `None` when no link exists. Caching means a confirmed Discogs import warms
/// the lookup for the worker's later commit-time call.
static DISCOGS_URL_LOOKUP_CACHE: SessionCache<Option<String>> =
    SessionCache::new("Discogs URL lookup cache");

/// Pre-populate the Discogs-URL → MB-release-ID lookup cache, so a test can drive
/// the cross-reference path without the network. `None` means "no MB release
/// linked" — the natural answer for a synthetic test release.
#[cfg(any(test, feature = "test-utils"))]
pub fn seed_discogs_url_lookup(discogs_release_id: &str, mb_release_id: Option<String>) {
    DISCOGS_URL_LOOKUP_CACHE.put(discogs_release_id, mb_release_id);
}

/// Pre-populate the MB release cache, so a test can drive `fetch_mb_xref` without
/// an HTTP call. The raw JSON is what gets archived in `release_metadata`; pass an
/// empty string when the test doesn't assert on it.
#[cfg(any(test, feature = "test-utils"))]
pub fn seed_release_cache(release_id: &str, value: (MbReleaseResponse, Option<String>, String)) {
    RELEASE_CACHE.put(release_id, value);
}

/// Pre-populate the MB release-group JSON cache. Pairs with `seed_release_cache`.
#[cfg(any(test, feature = "test-utils"))]
pub fn seed_release_group_json_cache(release_group_id: &str, raw_json: String) {
    RELEASE_GROUP_JSON_CACHE.put(release_group_id, raw_json);
}

/// A MusicBrainz lookup failure, keeping the wire-level distinction the caller
/// needs in order to localize: a transport failure that produced no HTTP
/// response, a timeout, or an HTTP error *response* carrying a status. The status
/// is kept structured — flattening it into a formatted string here would destroy
/// it for every consumer. `Other` is local/internal detail, never a provider
/// verdict.
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
    /// Classify a `reqwest::Error` from a send or body read. A timeout is its own
    /// variant; an error carrying an HTTP status is a `Provider` response;
    /// everything else (connection, DNS, dropped body) is transport `Network`.
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

/// Lookup releases by MusicBrainz DiscID.
pub async fn lookup_by_discid(discid: &str) -> Result<Vec<MbReleaseResponse>, MusicBrainzError> {
    mb_retry("MusicBrainz DiscID lookup", || {
        lookup_by_discid_once(discid)
    })
    .await
}

async fn lookup_by_discid_once(discid: &str) -> Result<Vec<MbReleaseResponse>, MusicBrainzError> {
    debug!("MusicBrainz: Looking up DiscID '{}'", discid);
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

    if disc_response.releases.is_empty() {
        return Err(MusicBrainzError::NotFound(discid.to_string()));
    }

    let releases = disc_response.releases;

    debug!(
        "MusicBrainz found {} release(s) for DiscID {}",
        releases.len(),
        discid
    );

    Ok(releases)
}

/// Look up a release by MusicBrainz release ID.
///
/// Returns the parsed response, the Discogs release URL (if any), and the raw
/// JSON (archived in `release_metadata`). On a cache miss this does the network
/// round-trip, plus a release-group fetch when the release's own relations carry
/// no Discogs URL.
pub async fn lookup_release_by_id(
    release_id: &str,
) -> Result<(MbReleaseResponse, Option<String>, String), MusicBrainzError> {
    mb_retry("MusicBrainz release fetch", || {
        lookup_release_by_id_once(release_id)
    })
    .await
}

async fn lookup_release_by_id_once(
    release_id: &str,
) -> Result<(MbReleaseResponse, Option<String>, String), MusicBrainzError> {
    if let Some(hit) = RELEASE_CACHE.get_cloned(release_id) {
        debug!("MusicBrainz release cache hit for {}", release_id);
        return Ok(hit);
    }

    debug!("MusicBrainz: Looking up release ID '{}'", release_id);
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

    let mut discogs_url = mb_response.discogs_release_url();

    debug!(
        "MusicBrainz release response: {} ({} relations), release_id: {}",
        mb_response.title,
        mb_response.relations.len(),
        release_id
    );

    if let Some(resource) = &discogs_url {
        debug!("Found Discogs release URL: {}", resource);
    }

    if discogs_url.is_none() {
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

                discogs_url = release_group_discogs_url(rg_id, fetch_release_group(rg_id).await);
            }
        }
    }

    let value = (mb_response, discogs_url, raw_json);
    RELEASE_CACHE.put(release_id, value.clone());

    Ok(value)
}

fn release_group_discogs_url(
    rg_id: &str,
    result: Result<ReleaseGroupResponse, MusicBrainzError>,
) -> Option<String> {
    match result {
        Ok(rg_response) => {
            let url = first_discogs_release_url(&rg_response.relations);
            if let Some(resource) = &url {
                debug!("Found Discogs release URL on release-group: {}", resource);
            }
            url
        }
        Err(e) => {
            warn!("Failed to fetch MusicBrainz release-group {rg_id}: {e}");
            None
        }
    }
}

/// The release-group, parsed. Shares `fetch_release_group_json`'s cache: a
/// release whose own url-rels carry no Discogs link, and the later
/// cross-reference archival of that same group, now cost one round trip between
/// them rather than two.
async fn fetch_release_group(
    release_group_id: &str,
) -> Result<ReleaseGroupResponse, MusicBrainzError> {
    let json = fetch_release_group_json(release_group_id).await?;
    serde_json::from_str(&json)
        .map_err(|e| MusicBrainzError::Other(format!("Failed to parse JSON: {}", e)))
}

/// A release-group's raw JSON, for archival. Cached for the session.
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

/// A release plus everything the import pipeline archives with it: the parsed
/// response, the Discogs release URL from its url-rels (if any), and the raw JSON
/// pairs for the `release_metadata` table — the release itself, and its
/// release-group.
///
/// The release-group fetch is best-effort: a release that archives without its
/// group is still a complete import, and failing the whole fetch over the group
/// would turn a metadata nicety into an import failure. Both MB entry points (a
/// direct import and a Discogs cross-reference) archive through here, so the rows
/// they write cannot differ by which path ran.
pub async fn fetch_release_with_metadata(
    release_id: &str,
) -> Result<(MbReleaseResponse, Option<String>, Vec<(String, String)>), MusicBrainzError> {
    let (response, discogs_url, raw_json) = lookup_release_by_id(release_id).await?;

    let mut pairs = vec![(
        crate::import::MetadataSource::MusicBrainz
            .as_str()
            .to_string(),
        raw_json,
    )];

    if let Some(rg_id) = response.release_group.as_ref().map(|rg| rg.id.as_str()) {
        match fetch_release_group_json(rg_id).await {
            Ok(rg_json) => pairs.push((RELEASE_GROUP_METADATA_SOURCE.to_string(), rg_json)),
            Err(e) => warn!("Failed to fetch MB release-group: {e}"),
        }
    }

    Ok((response, discogs_url, pairs))
}

/// The `release_metadata.source` value the release-group JSON is archived under.
/// The release itself uses `MetadataSource::MusicBrainz`.
const RELEASE_GROUP_METADATA_SOURCE: &str = "musicbrainz_release_group";

/// The MB release ID linked to a Discogs release, via MB's URL lookup endpoint;
/// `None` when no MB editor has linked one.
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

    let release_id = lookup
        .relations
        .iter()
        .filter(|r| r.relation_type.as_deref() == Some("discogs"))
        .find_map(|r| r.release.as_ref().and_then(|rel| rel.id.clone()));

    DISCOGS_URL_LOOKUP_CACHE.put(discogs_release_id, release_id.clone());

    Ok(release_id)
}

/// The MB cross-reference for a Discogs release — the reverse of
/// `crate::discogs::client::fetch_discogs_xref`.
///
/// Asks MB's URL endpoint for a release linking back to this Discogs release and,
/// when there is one, fetches its release and release-group JSON. Returns the
/// parsed `MbReleaseResponse` (the caller pulls the release and group IDs out for
/// `release_identities`) plus the raw JSON pairs for its `release_metadata`.
///
/// `None` when MB has no linked release, or either lookup fails. A successful
/// release fetch with a failing release-group fetch still returns `Some` with just
/// the release pair — the release-group is best-effort.
///
/// Depends on an MB editor having linked the Discogs URL, as the forward
/// direction does.
pub async fn fetch_mb_xref(
    discogs_release_id: &str,
) -> Option<(MbReleaseResponse, Vec<(String, String)>)> {
    let mb_release_id = match lookup_release_id_by_discogs_url(discogs_release_id).await {
        Ok(Some(id)) => {
            debug!("Found linked MB release: {}", id);
            id
        }
        Ok(None) => {
            debug!(
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

    match fetch_release_with_metadata(&mb_release_id).await {
        Ok((response, _discogs_url, pairs)) => Some((response, pairs)),
        Err(e) => {
            warn!("Failed to fetch linked MB release {}: {e}", mb_release_id);
            None
        }
    }
}

// ============================================================================
// Search
// ============================================================================

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

    pub fn has_any_field(&self) -> bool {
        self.query_fields().iter().any(|(value, _, _)| {
            value
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
        })
    }

    /// The filled fields as a Lucene query.
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

pub async fn search_releases_with_params(
    params: &ReleaseSearchParams,
) -> Result<Vec<SearchRelease>, MusicBrainzError> {
    // Outside the retry: no request is made, so repeating cannot help.
    if !params.has_any_field() {
        return Err(MusicBrainzError::Other(
            "At least one search field must be provided".to_string(),
        ));
    }
    mb_retry("MusicBrainz search", || {
        search_releases_with_params_once(params)
    })
    .await
}

async fn search_releases_with_params_once(
    params: &ReleaseSearchParams,
) -> Result<Vec<SearchRelease>, MusicBrainzError> {
    let query = params.build_query();
    debug!("MusicBrainz: Searching with params: {:?}", params);
    debug!("   Query: {}", query);
    let url = "https://musicbrainz.org/ws/2/release";
    debug!("MusicBrainz API request: {}?query={}&limit=25", url, query);

    let request = http_client()
        .get(url)
        .query(&[("query", query.as_str()), ("limit", "25")]);
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

    debug!("Found {} release(s)", releases.len());
    Ok(releases)
}

#[cfg(test)]
mod tests;
