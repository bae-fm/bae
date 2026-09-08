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

use crate::import::{PayloadSource, SourcePayload};
use crate::util::http::{is_cacheable, CachedResponse};
use crate::util::rate_limiter::{CallPriority, RateLimiter};
use crate::util::session_cache::{SessionCache, PROVIDER_RESPONSE_CAPACITY};
use crate::util::test_base_url::TestBaseUrl;
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

/// Restore the shared limiter, so one test's requests don't delay the next's.
/// Serialize the tests that call it — the limiter is process-wide.
#[cfg(test)]
pub(crate) fn reset_rate_limiter_for_test() {
    RATE_LIMITER.reset();
}

/// Where every MusicBrainz web-service request goes.
pub(crate) static BASE_URL: TestBaseUrl = TestBaseUrl::new("https://musicbrainz.org/ws/2");

/// One web-service URL. `path` is everything after `ws/2/`, query string
/// included.
fn ws2(path: &str) -> String {
    format!("{}/{path}", BASE_URL.get())
}

/// Where a release's own document is fetched from. Named once, because the
/// response cache is keyed by URL: the request and anything putting an answer
/// where the request will look for it have to agree on it.
fn release_url(release_id: &str) -> String {
    ws2(&format!(
        "release/{release_id}?inc=recordings+artist-credits+release-groups+release-group-rels+url-rels+labels+media+recording-level-rels+work-level-rels+work-rels+artist-rels"
    ))
}

fn release_group_url(release_group_id: &str) -> String {
    ws2(&format!(
        "release-group/{release_group_id}?inc=url-rels&fmt=json"
    ))
}

fn discid_url(discid: &str) -> String {
    ws2(&format!(
        "discid/{discid}?inc=recordings+artist-credits+release-groups+url-rels+labels"
    ))
}

/// The URL endpoint asked which MusicBrainz release links back to a Discogs
/// release.
fn discogs_url_lookup_url(discogs_release_id: &str) -> String {
    let discogs_url = format!("https://www.discogs.com/release/{discogs_release_id}");
    ws2(&format!(
        "url?resource={discogs_url}&inc=release-rels&fmt=json"
    ))
}

/// Retry only what a retry can fix. `NotFound` is MusicBrainz's answer, not a
/// fault — and it's the ordinary answer for a disc it doesn't have, so retrying
/// buys three round trips and three rate-limit waits to learn it again. `Other`
/// is local (URL construction, JSON parse, a missing search field): either no
/// request was made, or the same bytes will parse the same way.
fn should_retry_mb(error: &MusicBrainzError) -> bool {
    match error {
        MusicBrainzError::Network(_) | MusicBrainzError::Timeout => true,
        // No readable status means reqwest classified a send error as carrying
        // one it couldn't produce — repeat it like any transport failure.
        MusicBrainzError::Provider { status } => status.is_none_or(|status| {
            reqwest::StatusCode::from_u16(status).is_ok_and(crate::retry::is_transient_status)
        }),
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

/// The response cache every MusicBrainz request is answered from, keyed by the
/// full request URL — base address included, so two addresses for one path are
/// two answers.
static MUSICBRAINZ_RESPONSES: SessionCache<CachedResponse> =
    SessionCache::new("MusicBrainz response cache", PROVIDER_RESPONSE_CAPACITY);

/// One MusicBrainz GET, answered from the cache when this URL already has an
/// answer — without waiting for a rate-limit slot — and otherwise sent, kept
/// when the answer is stable, and returned. The body is the whole response;
/// each caller parses what it needs out of it.
async fn mb_get(
    request: reqwest::RequestBuilder,
    priority: CallPriority,
) -> Result<String, MusicBrainzError> {
    let request = request
        .header("Accept", "application/json")
        .timeout(crate::util::http::API_TIMEOUT)
        .build()
        .map_err(MusicBrainzError::from_reqwest)?;
    let key = request.url().to_string();

    if let Some(cached) = MUSICBRAINZ_RESPONSES.get_cloned(&key) {
        debug!("MusicBrainz response cache hit for {}", key);
        return mb_body(cached);
    }

    RATE_LIMITER.wait(priority).await;
    let response = http_client()
        .execute(request)
        .await
        .map_err(MusicBrainzError::from_reqwest)?;
    let status = response.status().as_u16();
    let body = response
        .text()
        .await
        .map_err(MusicBrainzError::from_reqwest)?;
    let response = CachedResponse { status, body };

    if !response.is_success() {
        warn!(
            "MusicBrainz API error response ({} from {}): {}",
            status, key, response.body
        );
    }
    if is_cacheable(status) {
        MUSICBRAINZ_RESPONSES.put(key, response.clone());
    }
    mb_body(response)
}

fn mb_body(response: CachedResponse) -> Result<String, MusicBrainzError> {
    if response.is_success() {
        Ok(response.body)
    } else {
        Err(MusicBrainzError::Provider {
            status: Some(response.status),
        })
    }
}

/// Put `body` where a request built right now for `url` would look for it.
#[cfg(any(test, feature = "test-utils"))]
fn seed_response(url: &str, status: u16, body: String) {
    MUSICBRAINZ_RESPONSES.put(
        crate::util::http::response_key(url),
        CachedResponse { status, body },
    );
}

/// Pre-populate the answer to the Discogs-URL lookup, so a test can drive the
/// cross-reference path without the network. `None` means "no MB release
/// linked" — the natural answer for a synthetic test release, and the 404 the
/// endpoint gives for a URL it has never seen.
#[cfg(any(test, feature = "test-utils"))]
pub fn seed_discogs_url_lookup(discogs_release_id: &str, mb_release_id: Option<String>) {
    let url = discogs_url_lookup_url(discogs_release_id);
    match mb_release_id {
        Some(id) => seed_response(
            &url,
            200,
            serde_json::json!({
                "relations": [{ "type": "discogs", "release": { "id": id } }],
            })
            .to_string(),
        ),
        None => seed_response(&url, 404, String::new()),
    }
}

/// Pre-populate a release document, so a test can drive `fetch_mb_xref` without
/// an HTTP call. `raw_json` is the endpoint's own answer: it is what gets
/// archived, what a later projection replays from, and what the client parses
/// here, so those three cannot disagree.
#[cfg(any(test, feature = "test-utils"))]
pub fn seed_release_cache(release_id: &str, raw_json: String) {
    seed_response(&release_url(release_id), 200, raw_json);
}

/// Pre-populate a release-group document. Pairs with `seed_release_cache`.
#[cfg(any(test, feature = "test-utils"))]
pub fn seed_release_group_json_cache(release_group_id: &str, raw_json: String) {
    seed_response(&release_group_url(release_group_id), 200, raw_json);
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
pub async fn lookup_by_discid(
    discid: &str,
    priority: CallPriority,
) -> Result<Vec<MbReleaseResponse>, MusicBrainzError> {
    mb_retry("MusicBrainz DiscID lookup", || {
        lookup_by_discid_once(discid, priority)
    })
    .await
}

async fn lookup_by_discid_once(
    discid: &str,
    priority: CallPriority,
) -> Result<Vec<MbReleaseResponse>, MusicBrainzError> {
    debug!("MusicBrainz: Looking up DiscID '{}'", discid);
    let url = discid_url(discid);
    debug!("MusicBrainz API request: {}", url);

    let body = match mb_get(http_client().get(&url), priority).await {
        Ok(body) => body,
        Err(MusicBrainzError::Provider { status: Some(404) }) => {
            return Err(MusicBrainzError::NotFound(discid.to_string()));
        }
        Err(error) => return Err(error),
    };

    let disc_response: DiscIdResponse = serde_json::from_str(&body)
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
/// JSON that gets archived. The Discogs URL is derived from the two documents
/// each time: the release's own url-rels, and — when those carry none — the
/// release group's, which is a second request. Both documents are answered from
/// the response cache, so re-deriving costs a parse, not a round trip.
pub async fn lookup_release_by_id(
    release_id: &str,
    priority: CallPriority,
) -> Result<(MbReleaseResponse, Option<String>, String), MusicBrainzError> {
    mb_retry("MusicBrainz release fetch", || {
        lookup_release_by_id_once(release_id, priority)
    })
    .await
}

async fn lookup_release_by_id_once(
    release_id: &str,
    priority: CallPriority,
) -> Result<(MbReleaseResponse, Option<String>, String), MusicBrainzError> {
    debug!("MusicBrainz: Looking up release ID '{}'", release_id);
    let url = release_url(release_id);
    debug!("MusicBrainz API request: {}", url);

    let raw_json = match mb_get(http_client().get(&url), priority).await {
        Ok(body) => body,
        Err(MusicBrainzError::Provider { status: Some(404) }) => {
            return Err(MusicBrainzError::NotFound(release_id.to_string()));
        }
        Err(error) => return Err(error),
    };

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

                discogs_url =
                    release_group_discogs_url(rg_id, fetch_release_group(rg_id, priority).await);
            }
        }
    }

    Ok((mb_response, discogs_url, raw_json))
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

/// The release-group, parsed. The same URL `fetch_release_group_json` asks for,
/// so a release whose own url-rels carry no Discogs link and the later
/// cross-reference archival of that same group cost one round trip between them
/// rather than two.
async fn fetch_release_group(
    release_group_id: &str,
    priority: CallPriority,
) -> Result<ReleaseGroupResponse, MusicBrainzError> {
    let json = fetch_release_group_json(release_group_id, priority).await?;
    serde_json::from_str(&json)
        .map_err(|e| MusicBrainzError::Other(format!("Failed to parse JSON: {}", e)))
}

/// A release-group's raw JSON, for archival.
pub async fn fetch_release_group_json(
    release_group_id: &str,
    priority: CallPriority,
) -> Result<String, MusicBrainzError> {
    let url = release_group_url(release_group_id);
    debug!("Fetching release-group JSON: {}", url);

    mb_get(http_client().get(&url), priority).await
}

/// A release and everything the import pipeline archives with it.
///
/// `raw_json` is the release's own document — the anchor of its archived set —
/// and `release_group` is the group's, keyed by the group. The group fetch is
/// best-effort: a release that archives without its group is still a complete
/// import, and failing the whole fetch over the group would turn a metadata
/// nicety into an import failure.
pub struct FetchedRelease {
    pub response: MbReleaseResponse,
    /// The Discogs release URL from the release's url-rels, if an editor linked
    /// one.
    pub discogs_url: Option<String>,
    pub raw_json: String,
    pub release_group: Option<SourcePayload>,
}

/// Fetch a release and its release-group. Both MB entry points (a direct import
/// and a Discogs cross-reference) archive through here, so the rows they write
/// cannot differ by which path ran.
pub async fn fetch_release_with_metadata(
    release_id: &str,
    priority: CallPriority,
) -> Result<FetchedRelease, MusicBrainzError> {
    let (response, discogs_url, raw_json) = lookup_release_by_id(release_id, priority).await?;

    let mut release_group = None;
    if let Some(rg_id) = response.release_group.as_ref().map(|rg| rg.id.as_str()) {
        match fetch_release_group_json(rg_id, priority).await {
            Ok(rg_json) => {
                release_group = Some(SourcePayload::new(
                    PayloadSource::MusicBrainzReleaseGroup,
                    rg_id,
                    rg_json,
                ))
            }
            Err(e) => warn!("Failed to fetch MB release-group: {e}"),
        }
    }

    Ok(FetchedRelease {
        response,
        discogs_url,
        raw_json,
        release_group,
    })
}

/// The MB release ID linked to a Discogs release, via MB's URL lookup endpoint;
/// `None` when no MB editor has linked one.
pub async fn lookup_release_id_by_discogs_url(
    discogs_release_id: &str,
    priority: CallPriority,
) -> Result<Option<String>, MusicBrainzError> {
    let url = discogs_url_lookup_url(discogs_release_id);
    debug!("MusicBrainz URL lookup: {}", url);

    let body = match mb_get(http_client().get(&url), priority).await {
        Ok(body) => body,
        // MusicBrainz has never seen this URL, which is the same answer as
        // seeing it with no release linked.
        Err(MusicBrainzError::Provider { status: Some(404) }) => return Ok(None),
        Err(error) => return Err(error),
    };

    let lookup: UrlLookupResponse = serde_json::from_str(&body)
        .map_err(|e| MusicBrainzError::Other(format!("Failed to parse JSON: {}", e)))?;

    Ok(lookup
        .relations
        .iter()
        .filter(|r| r.relation_type.as_deref() == Some("discogs"))
        .find_map(|r| r.release.as_ref().and_then(|rel| rel.id.clone())))
}

/// The MB cross-reference for a Discogs release — the reverse of
/// `crate::discogs::client::fetch_discogs_xref`.
///
/// Asks MB's URL endpoint for a release linking back to this Discogs release and,
/// when there is one, fetches its release and release-group JSON. Returns the
/// parsed `MbReleaseResponse` (the caller pulls the release and group IDs out for
/// `release_identities`) plus the documents to store with it.
///
/// The release document is keyed by the *Discogs* release id it was found from:
/// MusicBrainz's URL endpoint is what turned one into the other, and nothing in
/// the Discogs document names the MusicBrainz release back, so the Discogs id is
/// the only key a reader can start from.
///
/// `None` when MB has no linked release, or either lookup fails. A successful
/// release fetch with a failing release-group fetch still returns `Some` with just
/// the release payload — the release-group is best-effort.
///
/// Depends on an MB editor having linked the Discogs URL, as the forward
/// direction does.
pub async fn fetch_mb_xref(
    discogs_release_id: &str,
    priority: CallPriority,
) -> Option<(MbReleaseResponse, Vec<SourcePayload>)> {
    let mb_release_id = match lookup_release_id_by_discogs_url(discogs_release_id, priority).await {
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

    match fetch_release_with_metadata(&mb_release_id, priority).await {
        Ok(fetched) => {
            // The release document is keyed by the Discogs id the lookup started
            // from; the release-group document keeps its own key.
            let mut payloads = vec![SourcePayload::new(
                PayloadSource::MusicBrainzDiscogsXref,
                discogs_release_id,
                fetched.raw_json,
            )];
            payloads.extend(fetched.release_group);
            Some((fetched.response, payloads))
        }
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
    priority: CallPriority,
) -> Result<Vec<SearchRelease>, MusicBrainzError> {
    // Outside the retry: no request is made, so repeating cannot help.
    if !params.has_any_field() {
        return Err(MusicBrainzError::Other(
            "At least one search field must be provided".to_string(),
        ));
    }
    mb_retry("MusicBrainz search", || {
        search_releases_with_params_once(params, priority)
    })
    .await
}

async fn search_releases_with_params_once(
    params: &ReleaseSearchParams,
    priority: CallPriority,
) -> Result<Vec<SearchRelease>, MusicBrainzError> {
    let query = params.build_query();
    debug!("MusicBrainz: Searching with params: {:?}", params);
    debug!("   Query: {}", query);
    let url = ws2("release");
    debug!("MusicBrainz API request: {}?query={}&limit=25", url, query);

    let request = http_client()
        .get(&url)
        .query(&[("query", query.as_str()), ("limit", "25")]);
    let body = match mb_get(request, priority).await {
        Ok(body) => body,
        Err(MusicBrainzError::Provider { status: Some(404) }) => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };

    let search_response: SearchResponse = serde_json::from_str(&body)
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
