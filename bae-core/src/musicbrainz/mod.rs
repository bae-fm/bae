//! MusicBrainz API client.
//!
//! [`MusicBrainz`] builds, rate-limits, times out, and retries the MusicBrainz
//! web-service requests, caches their results, and exposes the lookup/search
//! entry points plus `MusicBrainzError`. The app builds one when it starts and
//! hands it down; its rate limit and response cache are that object's, so two
//! of them — two tests, say — share nothing. A caller just awaits a lookup; the
//! retry policy is this module's, not theirs.
//!
//! The response shapes these requests deserialize into live in `types`,
//! re-exported here so callers keep using `crate::musicbrainz::Mb…` paths.

use std::time::Duration;

use crate::import::CatalogPage;
use crate::retry::RetryPolicy;
use crate::util::http::{is_cacheable, CachedResponse, Http};
use crate::util::rate_limiter::{CallPriority, RateLimiter};
use crate::util::session_cache::{SessionCache, PROVIDER_RESPONSE_CAPACITY};
use thiserror::Error;
use tracing::{debug, warn};

mod types;
pub use types::*;

/// Where every MusicBrainz web-service request goes.
const BASE_URL: &str = "https://musicbrainz.org/ws/2";

/// MusicBrainz's published rate: one request a second.
const REQUEST_INTERVAL: Duration = Duration::from_secs(1);

/// The MusicBrainz web service, as bae asks it: the transport requests go out
/// on, the rate limit they wait for, and the answers already had.
pub struct MusicBrainz {
    http: Http,
    limiter: RateLimiter,
    /// Every answer kept, keyed by the full request URL.
    responses: SessionCache<CachedResponse>,
}

/// One web-service URL. `path` is everything after `ws/2/`, query string
/// included.
fn ws2(path: &str) -> String {
    format!("{BASE_URL}/{path}")
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
        "release-group/{release_group_id}?inc=artist-credits+url-rels&fmt=json"
    ))
}

/// How many of a release group's releases one browse page answers — the
/// most MusicBrainz serves a page.
pub const GROUP_RELEASES_PAGE: usize = 100;

/// One page of a release group's releases, from `offset`, each with its own
/// links and the group's — one request that states both what the group's
/// page links and what each of those releases links.
fn group_releases_url(release_group_id: &str, offset: usize) -> String {
    ws2(&format!(
        "release?release-group={release_group_id}&inc=url-rels+release-groups+release-group-level-rels&limit={GROUP_RELEASES_PAGE}&offset={offset}&fmt=json"
    ))
}

fn discid_url(discid: &str) -> String {
    ws2(&format!(
        "discid/{discid}?inc=recordings+artist-credits+release-groups+url-rels+labels"
    ))
}

/// MusicBrainz's URL document for a catalog page, with only the requested
/// relationship kind included. Query encoding preserves the complete resource.
fn discogs_url_lookup_url(discogs_release_id: &str) -> String {
    url_lookup_url(
        &format!("https://www.discogs.com/release/{discogs_release_id}"),
        "release-rels",
    )
}

fn discogs_master_lookup_url(discogs_master_id: &str) -> String {
    url_lookup_url(
        &format!("https://www.discogs.com/master/{discogs_master_id}"),
        "release-group-rels",
    )
}

fn url_lookup_url(resource: &str, include: &str) -> String {
    let mut url = reqwest::Url::parse(&ws2("url")).expect("MusicBrainz base URL is valid");
    url.query_pairs_mut()
        .append_pair("resource", resource)
        .append_pair("inc", include)
        .append_pair("fmt", "json");
    url.to_string()
}

/// Retry only what a retry can fix. `NotFound` is MusicBrainz's answer, not a
/// fault — and it's the ordinary answer for a disc it doesn't have, so retrying
/// buys a round trip and a rate-limit wait per try to learn it again. `Other`
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

/// How MusicBrainz is asked again. Its rate-limiting page
/// (<https://musicbrainz.org/doc/MusicBrainz_API/Rate_Limiting>) says every
/// request it turns away — this address over one request a second, or the
/// servers as a whole over their 300 a second — is declined with a 503, and
/// that an address over its rate is declined outright "until the rate drops".
/// The page names no wait. So a 503 says to ask less, not to ask again at
/// once: the waits double from one second, which lets the address's own rate
/// fall back under one a second at the first repeat and gives a busy server
/// some fifteen seconds in all to recover, and each is jittered, since a
/// server shedding everyone's load at once hears every client's repeats
/// together otherwise.
const RETRY: RetryPolicy =
    RetryPolicy::exponential(5, Duration::from_secs(1), Duration::from_secs(10));

/// Wrap one request in the client's own retry policy — a caller shouldn't have to
/// know which of these failures are worth repeating. (Discogs does the same.)
async fn mb_retry<F, Fut, T>(label: &str, f: F) -> Result<T, MusicBrainzError>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, MusicBrainzError>>,
{
    crate::retry::retry_with_backoff_if(RETRY, label, should_retry_mb, f).await
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
    /// Keep invalid request construction and redirect failures diagnostic;
    /// actual transport, timeout, and HTTP status failures remain typed.
    fn from_reqwest(e: reqwest::Error) -> Self {
        if e.is_builder() || e.is_redirect() {
            MusicBrainzError::Other(format!("{e:?}"))
        } else if e.is_timeout() {
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

impl MusicBrainz {
    pub fn new(http: Http) -> Self {
        Self::with_interval(http, REQUEST_INTERVAL)
    }

    /// One whose requests are not spaced: a test's fake service has no rate
    /// to keep to, and waiting a second between its answers only slows the
    /// test.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn for_test(http: Http) -> Self {
        Self::with_interval(http, Duration::ZERO)
    }

    fn with_interval(http: Http, interval: Duration) -> Self {
        Self {
            http,
            limiter: RateLimiter::new(interval),
            responses: SessionCache::new("MusicBrainz response cache", PROVIDER_RESPONSE_CAPACITY),
        }
    }

    /// One MusicBrainz GET, answered from the cache when this URL already has an
    /// answer — without waiting for a rate-limit slot — and otherwise sent, kept
    /// when the answer is stable, and returned. The body is the whole response;
    /// each caller parses what it needs out of it.
    async fn get(&self, url: &str, priority: CallPriority) -> Result<String, MusicBrainzError> {
        self.get_request(self.http.get(url), priority).await
    }

    async fn get_request(
        &self,
        request: reqwest::RequestBuilder,
        priority: CallPriority,
    ) -> Result<String, MusicBrainzError> {
        let request = request
            .header("Accept", "application/json")
            .timeout(crate::util::http::API_TIMEOUT)
            .build()
            .map_err(MusicBrainzError::from_reqwest)?;
        let key = request.url().to_string();

        if let Some(cached) = self.responses.get_cloned(&key) {
            debug!("MusicBrainz response cache hit for {}", key);
            return mb_body(cached);
        }

        self.limiter.wait(priority).await;
        let response = self
            .http
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
            self.responses.put(key, response.clone());
        }
        mb_body(response)
    }

    /// Lookup releases by MusicBrainz DiscID.
    pub async fn lookup_by_discid(
        &self,
        discid: &str,
        priority: CallPriority,
    ) -> Result<Vec<MbReleaseResponse>, MusicBrainzError> {
        mb_retry("MusicBrainz DiscID lookup", || {
            self.lookup_by_discid_once(discid, priority)
        })
        .await
    }

    async fn lookup_by_discid_once(
        &self,
        discid: &str,
        priority: CallPriority,
    ) -> Result<Vec<MbReleaseResponse>, MusicBrainzError> {
        debug!("MusicBrainz: Looking up DiscID '{}'", discid);
        let url = discid_url(discid);
        debug!("MusicBrainz API request: {}", url);

        let body = match self.get(&url, priority).await {
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

    /// Look up only the requested release, returning its parsed and raw document.
    /// Its parent and linked documents are fetched independently by the caller.
    pub async fn lookup_release_by_id(
        &self,
        release_id: &str,
        priority: CallPriority,
    ) -> Result<(MbReleaseResponse, String), MusicBrainzError> {
        mb_retry("MusicBrainz release fetch", || async {
            let raw_json = match self.get(&release_url(release_id), priority).await {
                Ok(body) => body,
                Err(MusicBrainzError::Provider { status: Some(404) }) => {
                    return Err(MusicBrainzError::NotFound(release_id.to_string()));
                }
                Err(error) => return Err(error),
            };
            let response: MbReleaseResponse = serde_json::from_str(&raw_json).map_err(|error| {
                MusicBrainzError::Other(format!("Failed to parse release JSON: {error}"))
            })?;
            Ok((response, raw_json))
        })
        .await
    }

    /// A release-group's raw JSON, for archival.
    pub async fn fetch_release_group_json(
        &self,
        release_group_id: &str,
        priority: CallPriority,
    ) -> Result<String, MusicBrainzError> {
        let url = release_group_url(release_group_id);
        debug!("Fetching release-group JSON: {}", url);

        mb_retry("MusicBrainz release-group fetch", || async {
            let json = self.get(&url, priority).await?;
            parse_release_group(&json).map_err(|error| {
                MusicBrainzError::Other(format!("Failed to parse release-group JSON: {error}"))
            })?;
            Ok(json)
        })
        .await
    }

    /// The page of a release group's releases starting at `offset`, each with
    /// its own links and the group's.
    pub async fn browse_group_releases(
        &self,
        release_group_id: &str,
        offset: usize,
        priority: CallPriority,
    ) -> Result<GroupReleases, MusicBrainzError> {
        let url = group_releases_url(release_group_id, offset);
        debug!("Browsing release-group releases: {}", url);
        mb_retry("MusicBrainz release-group browse", || async {
            let json = match self.get(&url, priority).await {
                Ok(json) => json,
                Err(MusicBrainzError::Provider { status: Some(404) }) => {
                    return Err(MusicBrainzError::NotFound(release_group_id.to_string()));
                }
                Err(error) => return Err(error),
            };
            serde_json::from_str(&json).map_err(|error| {
                MusicBrainzError::Other(format!("Failed to parse release browse JSON: {error}"))
            })
        })
        .await
    }

    /// Every MusicBrainz release explicitly related to this Discogs release URL.
    /// A missing URL resource is `None`; a found document retains its raw answer,
    /// including an empty or ambiguous set of matching targets.
    pub async fn lookup_releases_by_discogs_release(
        &self,
        discogs_release_id: &str,
        priority: CallPriority,
    ) -> Result<Option<(Vec<CatalogPage>, String)>, MusicBrainzError> {
        self.lookup_discogs_url(
            discogs_url_lookup_url(discogs_release_id),
            parse_discogs_release_lookup,
            priority,
        )
        .await
    }

    /// Every MusicBrainz release group explicitly related to this Discogs master URL.
    pub async fn lookup_groups_by_discogs_master(
        &self,
        discogs_master_id: &str,
        priority: CallPriority,
    ) -> Result<Option<(Vec<CatalogPage>, String)>, MusicBrainzError> {
        self.lookup_discogs_url(
            discogs_master_lookup_url(discogs_master_id),
            parse_discogs_master_lookup,
            priority,
        )
        .await
    }

    async fn lookup_discogs_url(
        &self,
        url: String,
        parse: fn(&str) -> Result<Vec<CatalogPage>, serde_json::Error>,
        priority: CallPriority,
    ) -> Result<Option<(Vec<CatalogPage>, String)>, MusicBrainzError> {
        mb_retry("MusicBrainz URL lookup", || async {
            let body = match self.get(&url, priority).await {
                Ok(body) => body,
                // The URL endpoint documents 404 as an unknown resource URL.
                Err(MusicBrainzError::Provider { status: Some(404) }) => return Ok(None),
                Err(error) => return Err(error),
            };
            let targets = parse(&body).map_err(|error| {
                MusicBrainzError::Other(format!("Failed to parse URL lookup JSON: {error}"))
            })?;
            Ok(Some((targets, body)))
        })
        .await
    }

    pub async fn search_releases_with_params(
        &self,
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
            self.search_releases_with_params_once(params, priority)
        })
        .await
    }

    async fn search_releases_with_params_once(
        &self,
        params: &ReleaseSearchParams,
        priority: CallPriority,
    ) -> Result<Vec<SearchRelease>, MusicBrainzError> {
        let query = params.build_query();
        debug!("MusicBrainz: Searching with params: {:?}", params);
        debug!("   Query: {}", query);
        let url = ws2("release");
        debug!("MusicBrainz API request: {}?query={}&limit=25", url, query);

        let request = self
            .http
            .get(&url)
            .query(&[("query", query.as_str()), ("limit", "25")]);
        let body = match self.get_request(request, priority).await {
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

    /// Put `body` where a request built right now for `url` would look for it.
    #[cfg(any(test, feature = "test-utils"))]
    fn seed_response(&self, url: &str, status: u16, body: String) {
        self.responses.put(
            crate::util::http::response_key(url),
            CachedResponse { status, body },
        );
    }

    /// Pre-populate the answer to the Discogs-URL lookup, so a test can drive the
    /// cross-reference path without the network. `None` means "no MB release
    /// linked" — the natural answer for a synthetic test release, and the 404 the
    /// endpoint gives for a URL it has never seen.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn seed_discogs_url_lookup(&self, discogs_release_id: &str, mb_release_id: Option<String>) {
        let url = discogs_url_lookup_url(discogs_release_id);
        match mb_release_id {
            Some(id) => self.seed_response(
                &url,
                200,
                serde_json::json!({
                    "relations": [{ "type": "discogs", "target-type": "release", "release": { "id": id } }],
                })
                .to_string(),
            ),
            None => self.seed_response(&url, 404, String::new()),
        }
    }

    /// Seed the MusicBrainz URL answer for a Discogs master. `None` is an unknown URL.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn seed_discogs_master_url_lookup(&self, master_id: &str, mb_group_id: Option<String>) {
        let url = discogs_master_lookup_url(master_id);
        match mb_group_id {
            Some(id) => self.seed_response(
                &url,
                200,
                serde_json::json!({"relations": [
                    {"type": "discogs", "target-type": "release_group", "release_group": {"id": id}}
                ]})
                .to_string(),
            ),
            None => self.seed_response(&url, 404, String::new()),
        }
    }

    /// Pre-populate a release document, so a test can drive release lookup without
    /// an HTTP call. `raw_json` is the endpoint's own answer: it is what gets
    /// archived, what a later projection replays from, and what the client parses
    /// here, so those three cannot disagree.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn seed_release_cache(&self, release_id: &str, raw_json: String) {
        self.seed_response(&release_url(release_id), 200, raw_json);
    }

    /// Pre-populate the page of a release group's browsed releases starting
    /// at `offset`, as [`Self::browse_group_releases`] asks for it.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn seed_group_releases(&self, release_group_id: &str, offset: usize, raw_json: String) {
        self.seed_response(&group_releases_url(release_group_id, offset), 200, raw_json);
    }

    /// Pre-populate a release-group document. Pairs with `seed_release_cache`.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn seed_release_group_json_cache(&self, release_group_id: &str, raw_json: String) {
        self.seed_response(&release_group_url(release_group_id), 200, raw_json);
    }
}

#[cfg(test)]
mod tests;
