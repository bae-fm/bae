use std::num::NonZeroUsize;
use std::sync::{Mutex as StdMutex, OnceLock};
use std::time::Duration;

use lru::LruCache;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::Mutex;
use tokio::time::Instant;
use tracing::{debug, info, warn};

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

/// Capacity for each request-kind cache. Sized for a typical session
/// (a few imports, each touching 1-3 releases). Eviction costs one
/// network round-trip — same as a cold start.
const CACHE_CAPACITY: usize = 25;

type ReleaseCacheValue = (MbReleaseResponse, ExternalUrls, String);

/// In-memory cache for `release/{id}` lookups. Stores the parsed
/// response, extracted URLs, and raw JSON so callers that need any of
/// the three can hit warm cache. Mutex is `std::sync::Mutex` since the
/// guard is dropped before any await.
fn release_cache() -> &'static StdMutex<LruCache<String, ReleaseCacheValue>> {
    static CACHE: OnceLock<StdMutex<LruCache<String, ReleaseCacheValue>>> = OnceLock::new();
    CACHE.get_or_init(|| {
        StdMutex::new(LruCache::new(
            NonZeroUsize::new(CACHE_CAPACITY).expect("CACHE_CAPACITY > 0"),
        ))
    })
}

/// In-memory cache for `release-group/{id}` JSON. Discogs cross-reference
/// archival keeps the raw JSON; we cache that string directly.
fn release_group_json_cache() -> &'static StdMutex<LruCache<String, String>> {
    static CACHE: OnceLock<StdMutex<LruCache<String, String>>> = OnceLock::new();
    CACHE.get_or_init(|| {
        StdMutex::new(LruCache::new(
            NonZeroUsize::new(CACHE_CAPACITY).expect("CACHE_CAPACITY > 0"),
        ))
    })
}

/// In-memory cache for `url?resource=https://www.discogs.com/release/{id}`
/// lookups. Key: Discogs release ID; value: linked MB release ID (or
/// `None` if no link exists). This call is part of the cross-reference
/// archival path; caching means a confirmed Discogs import warms the
/// lookup for the worker's later commit-time call.
fn discogs_url_lookup_cache() -> &'static StdMutex<LruCache<String, Option<String>>> {
    static CACHE: OnceLock<StdMutex<LruCache<String, Option<String>>>> = OnceLock::new();
    CACHE.get_or_init(|| {
        StdMutex::new(LruCache::new(
            NonZeroUsize::new(CACHE_CAPACITY).expect("CACHE_CAPACITY > 0"),
        ))
    })
}

/// Pre-populate the Discogs-URL → MB-release-ID lookup cache. Tests
/// use this to short-circuit the cross-reference path without hitting
/// the network. `None` means "no MB release linked", which is the
/// natural result for synthetic test releases.
#[cfg(any(test, feature = "test-utils"))]
pub fn seed_discogs_url_lookup(discogs_release_id: &str, mb_release_id: Option<String>) {
    discogs_url_lookup_cache()
        .lock()
        .expect("Discogs URL lookup cache mutex poisoned")
        .put(discogs_release_id.to_string(), mb_release_id);
}

/// Pre-populate the MB release cache. Tests use this to drive the
/// reverse cross-reference path (`fetch_mb_xref`) without making an
/// HTTP call. The raw JSON ends up in `release_metadata` for archival;
/// tests can pass an empty string when the archival content isn't
/// being asserted on.
#[cfg(any(test, feature = "test-utils"))]
pub fn seed_release_cache(release_id: &str, value: (MbReleaseResponse, ExternalUrls, String)) {
    release_cache()
        .lock()
        .expect("release cache mutex poisoned")
        .put(release_id.to_string(), value);
}

/// Pre-populate the MB release-group JSON cache. Pairs with
/// `seed_release_cache` for tests that exercise the cross-reference
/// path without HTTP.
#[cfg(any(test, feature = "test-utils"))]
pub fn seed_release_group_json_cache(release_group_id: &str, raw_json: String) {
    release_group_json_cache()
        .lock()
        .expect("release-group json cache mutex poisoned")
        .put(release_group_id.to_string(), raw_json);
}

// ============================================================================
// Serde response types for MusicBrainz API
// ============================================================================

/// A URL relation from MusicBrainz (used across release and release-group responses)
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MbRelation {
    pub url: Option<MbUrlResource>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MbUrlResource {
    pub resource: Option<String>,
}

/// Artist credit entry
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MbArtistCredit {
    pub name: String,
    pub artist: Option<MbArtistRef>,
}

/// Reference to a MusicBrainz artist within an artist-credit
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MbArtistRef {
    pub id: Option<String>,
    pub name: Option<String>,
    #[serde(rename = "sort-name")]
    pub sort_name: Option<String>,
}

/// Label info entry
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MbLabelInfo {
    pub label: Option<MbLabel>,
    #[serde(rename = "catalog-number")]
    pub catalog_number: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MbLabel {
    pub name: Option<String>,
}

/// Release group as embedded in a release response
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MbReleaseGroupRef {
    pub id: String,
    #[serde(rename = "first-release-date")]
    pub first_release_date: Option<String>,
    #[serde(default)]
    pub relations: Option<Vec<MbRelation>>,
}

/// A recording within a track
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MbRecording {
    pub title: Option<String>,
}

/// A track within a medium
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MbTrack {
    pub position: Option<i64>,
    pub number: Option<String>,
    pub title: Option<String>,
    pub length: Option<u64>,
    pub recording: Option<MbRecording>,
    #[serde(rename = "artist-credit", default)]
    pub artist_credit: Vec<MbArtistCredit>,
}

/// A medium (disc) within a release
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MbMedium {
    pub format: Option<String>,
    #[serde(default)]
    pub tracks: Vec<MbTrack>,
}

/// A full release as returned by the MB release lookup API
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MbReleaseResponse {
    pub id: String,
    pub title: String,
    pub date: Option<String>,
    pub country: Option<String>,
    pub barcode: Option<String>,
    #[serde(rename = "artist-credit", default)]
    pub artist_credit: Vec<MbArtistCredit>,
    #[serde(rename = "release-group")]
    pub release_group: Option<MbReleaseGroupRef>,
    #[serde(rename = "label-info", default)]
    pub label_info: Vec<MbLabelInfo>,
    #[serde(default)]
    pub media: Vec<MbMedium>,
    #[serde(default)]
    pub relations: Vec<MbRelation>,
}

impl MbReleaseResponse {
    /// Extract ExternalUrls from release relations and release-group relations
    fn extract_external_urls(&self) -> ExternalUrls {
        let mut urls = ExternalUrls {
            discogs_master_url: None,
            discogs_release_url: None,
        };

        // Extract from release-level relations
        extract_urls_from_relations(&self.relations, &mut urls);

        // Extract from release-group relations (if present inline)
        if let Some(rg) = &self.release_group {
            if let Some(rg_relations) = &rg.relations {
                extract_urls_from_relations(rg_relations, &mut urls);
            }
        }

        urls
    }

    /// Count total tracks across all media
    pub fn track_count(&self) -> usize {
        self.media.iter().map(|m| m.tracks.len()).sum()
    }
}

/// Response from the disc ID lookup endpoint
#[derive(Debug, Clone, Deserialize)]
struct DiscIdResponse {
    #[serde(default)]
    releases: Vec<DiscIdRelease>,
}

/// A release within a disc ID lookup response (has slightly different shape from full release)
#[derive(Debug, Clone, Deserialize)]
pub struct DiscIdRelease {
    pub id: String,
    pub title: String,
    pub date: Option<String>,
    pub country: Option<String>,
    #[serde(rename = "artist-credit", default)]
    pub artist_credit: Vec<MbArtistCredit>,
    #[serde(rename = "release-group")]
    pub release_group: Option<MbReleaseGroupRef>,
    #[serde(rename = "label-info", default)]
    pub label_info: Vec<MbLabelInfo>,
    #[serde(default)]
    pub media: Vec<MbMedium>,
    #[serde(default)]
    pub relations: Vec<MbRelation>,
}

/// Response from the release search endpoint
#[derive(Debug, Clone, Deserialize, Serialize)]
struct SearchResponse {
    #[serde(default)]
    releases: Vec<SearchRelease>,
    error: Option<String>,
}

/// A release in search results (less data than full lookup)
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SearchRelease {
    pub id: String,
    pub title: String,
    pub date: Option<String>,
    pub country: Option<String>,
    pub barcode: Option<String>,
    #[serde(rename = "artist-credit", default)]
    pub artist_credit: Vec<MbArtistCredit>,
    #[serde(rename = "release-group")]
    pub release_group: Option<MbReleaseGroupRef>,
    #[serde(rename = "label-info", default)]
    pub label_info: Vec<MbLabelInfo>,
}

/// Release group response (for separate fetch with url-rels)
#[derive(Debug, Clone, Deserialize)]
struct ReleaseGroupResponse {
    #[serde(default)]
    relations: Vec<MbRelation>,
}

/// Response from the MB URL lookup endpoint (used for Discogs -> MB cross-reference)
#[derive(Debug, Deserialize)]
struct UrlLookupResponse {
    #[serde(default)]
    relations: Vec<UrlLookupRelation>,
}

#[derive(Debug, Deserialize)]
struct UrlLookupRelation {
    #[serde(rename = "type")]
    relation_type: Option<String>,
    release: Option<UrlLookupRelease>,
}

#[derive(Debug, Deserialize)]
struct UrlLookupRelease {
    id: Option<String>,
}

/// Extract external URLs from a list of relations into the target struct
fn extract_urls_from_relations(relations: &[MbRelation], urls: &mut ExternalUrls) {
    for relation in relations {
        let Some(url_obj) = &relation.url else {
            continue;
        };
        let Some(resource) = &url_obj.resource else {
            continue;
        };

        if resource.contains("discogs.com/master/") && urls.discogs_master_url.is_none() {
            urls.discogs_master_url = Some(resource.clone());
        } else if resource.contains("discogs.com/release/") && urls.discogs_release_url.is_none() {
            urls.discogs_release_url = Some(resource.clone());
        }
    }
}

// ============================================================================
// Domain types (public API, unchanged)
// ============================================================================

/// External URLs extracted from MusicBrainz relationships
#[derive(Debug, Clone)]
pub struct ExternalUrls {
    pub discogs_master_url: Option<String>,
    pub discogs_release_url: Option<String>,
}

#[derive(Debug, Error)]
pub enum MusicBrainzError {
    #[error("MusicBrainz API error: {0}")]
    Api(String),
    #[error("No release found for DISCID: {0}")]
    NotFound(String),
}

// ============================================================================
// API functions
// ============================================================================

/// Lookup releases by MusicBrainz DiscID
pub async fn lookup_by_discid(
    discid: &str,
) -> Result<(Vec<DiscIdRelease>, ExternalUrls), MusicBrainzError> {
    info!("MusicBrainz: Looking up DiscID '{}'", discid);
    let base_url = reqwest::Url::parse("https://musicbrainz.org/ws/2/discid/")
        .map_err(|e| MusicBrainzError::Api(format!("Failed to parse base URL: {}", e)))?;
    let url = base_url
        .join(discid)
        .map_err(|e| MusicBrainzError::Api(format!("Failed to construct DiscID URL: {}", e)))?;
    let mut url_with_params = url.clone();
    url_with_params.set_query(Some(
        "inc=recordings+artist-credits+release-groups+url-rels+labels",
    ));
    debug!("MusicBrainz API request: {}", url_with_params);

    wait_for_rate_limit().await;

    let response = http_client()
        .get(url_with_params.as_str())
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| MusicBrainzError::Api(format!("HTTP request failed: {}", e)))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());

        warn!(
            "MusicBrainz API error response ({}): {}",
            status, error_text
        );

        if status == 404 {
            return Err(MusicBrainzError::NotFound(discid.to_string()));
        }
        return Err(MusicBrainzError::Api(format!(
            "MusicBrainz API returned status {}: {}",
            status, error_text
        )));
    }

    let disc_response: DiscIdResponse = response
        .json()
        .await
        .map_err(|e| MusicBrainzError::Api(format!("Failed to parse JSON: {}", e)))?;

    let mut external_urls = ExternalUrls {
        discogs_master_url: None,
        discogs_release_url: None,
    };

    for release in &disc_response.releases {
        if external_urls.discogs_master_url.is_none() {
            extract_urls_from_relations(&release.relations, &mut external_urls);
        }
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

    if external_urls.discogs_master_url.is_some() || external_urls.discogs_release_url.is_some() {
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

    wait_for_rate_limit().await;

    let response = http_client()
        .get(&url)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| MusicBrainzError::Api(format!("HTTP request failed: {}", e)))?;

    if !response.status().is_success() {
        return Err(MusicBrainzError::Api(format!(
            "MusicBrainz API returned status: {}",
            response.status()
        )));
    }

    response
        .json()
        .await
        .map_err(|e| MusicBrainzError::Api(format!("Failed to parse JSON: {}", e)))
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
    if let Some(hit) = release_cache()
        .lock()
        .expect("release cache mutex poisoned")
        .get(release_id)
        .cloned()
    {
        debug!("MusicBrainz release cache hit for {}", release_id);
        return Ok(hit);
    }

    info!("MusicBrainz: Looking up release ID '{}'", release_id);
    let url = format!(
        "https://musicbrainz.org/ws/2/release/{}?inc=recordings+artist-credits+release-groups+release-group-rels+url-rels+labels+media",
        release_id,
    );
    debug!("MusicBrainz API request: {}", url);

    wait_for_rate_limit().await;

    let response = http_client()
        .get(&url)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| MusicBrainzError::Api(format!("HTTP request failed: {}", e)))?;

    if !response.status().is_success() {
        if response.status() == 404 {
            return Err(MusicBrainzError::NotFound(release_id.to_string()));
        }
        return Err(MusicBrainzError::Api(format!(
            "MusicBrainz API returned status: {}",
            response.status()
        )));
    }

    let raw_json = response
        .text()
        .await
        .map_err(|e| MusicBrainzError::Api(format!("Failed to read response body: {}", e)))?;

    let mb_response: MbReleaseResponse = serde_json::from_str(&raw_json)
        .map_err(|e| MusicBrainzError::Api(format!("Failed to parse JSON: {}", e)))?;

    #[cfg(debug_assertions)]
    {
        let temp_path = std::env::temp_dir().join("musicbrainz_release_response.json");
        match std::fs::write(&temp_path, &raw_json) {
            Ok(()) => debug!("MusicBrainz release response written to {:?}", temp_path),
            Err(e) => debug!(
                "MusicBrainz release response cache write to {:?} failed: {}",
                temp_path, e
            ),
        }
    }

    let mut external_urls = mb_response.extract_external_urls();

    debug!(
        "MusicBrainz release response: {} ({} relations), release_id: {}",
        mb_response.title,
        mb_response.relations.len(),
        release_id
    );

    if let Some(resource) = &external_urls.discogs_master_url {
        info!("Found Discogs master URL: {}", resource);
    }
    if let Some(resource) = &external_urls.discogs_release_url {
        info!("Found Discogs release URL: {}", resource);
    }

    // If release-group relations weren't included inline, fetch them separately
    if external_urls.discogs_master_url.is_none() && external_urls.discogs_release_url.is_none() {
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

                if let Ok(rg_response) = fetch_release_group_with_relations(rg_id).await {
                    extract_urls_from_relations(&rg_response.relations, &mut external_urls);

                    if let Some(resource) = &external_urls.discogs_master_url {
                        info!("Found Discogs master URL on release-group: {}", resource);
                    }
                    if let Some(resource) = &external_urls.discogs_release_url {
                        info!("Found Discogs release URL on release-group: {}", resource);
                    }
                }
            }
        }
    }

    let value = (mb_response, external_urls, raw_json);
    release_cache()
        .lock()
        .expect("release cache mutex poisoned")
        .put(release_id.to_string(), value.clone());

    Ok(value)
}

/// Fetch a release-group by ID, returning the raw JSON for metadata storage.
///
/// Uses the same endpoint as `fetch_release_group_with_relations` but returns the
/// raw JSON text instead of a parsed struct. Hits the session-wide LRU cache.
pub async fn fetch_release_group_json(release_group_id: &str) -> Result<String, MusicBrainzError> {
    if let Some(hit) = release_group_json_cache()
        .lock()
        .expect("release-group json cache mutex poisoned")
        .get(release_group_id)
        .cloned()
    {
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

    wait_for_rate_limit().await;

    let response = http_client()
        .get(&url)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| MusicBrainzError::Api(format!("HTTP request failed: {}", e)))?;

    if !response.status().is_success() {
        return Err(MusicBrainzError::Api(format!(
            "MusicBrainz API returned status: {}",
            response.status()
        )));
    }

    let raw_json = response
        .text()
        .await
        .map_err(|e| MusicBrainzError::Api(format!("Failed to read response body: {}", e)))?;

    release_group_json_cache()
        .lock()
        .expect("release-group json cache mutex poisoned")
        .put(release_group_id.to_string(), raw_json.clone());

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
    if let Some(hit) = discogs_url_lookup_cache()
        .lock()
        .expect("Discogs URL lookup cache mutex poisoned")
        .get(discogs_release_id)
        .cloned()
    {
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

    wait_for_rate_limit().await;

    let response = http_client()
        .get(&url)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| MusicBrainzError::Api(format!("HTTP request failed: {}", e)))?;

    if response.status() == 404 {
        discogs_url_lookup_cache()
            .lock()
            .expect("Discogs URL lookup cache mutex poisoned")
            .put(discogs_release_id.to_string(), None);
        return Ok(None);
    }

    if !response.status().is_success() {
        return Err(MusicBrainzError::Api(format!(
            "MusicBrainz API returned status: {}",
            response.status()
        )));
    }

    let lookup: UrlLookupResponse = response
        .json()
        .await
        .map_err(|e| MusicBrainzError::Api(format!("Failed to parse JSON: {}", e)))?;

    // Find the first relation that has a release with an ID
    let release_id = lookup
        .relations
        .iter()
        .filter(|r| r.relation_type.as_deref() == Some("discogs"))
        .find_map(|r| r.release.as_ref().and_then(|rel| rel.id.clone()));

    discogs_url_lookup_cache()
        .lock()
        .expect("Discogs URL lookup cache mutex poisoned")
        .put(discogs_release_id.to_string(), release_id.clone());

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
    pub format: Option<String>,
    pub country: Option<String>,
}

impl ReleaseSearchParams {
    /// Check if at least one field is filled
    pub fn has_any_field(&self) -> bool {
        self.artist.is_some()
            || self.album.is_some()
            || self.year.is_some()
            || self.label.is_some()
            || self.catalog_number.is_some()
            || self.barcode.is_some()
            || self.format.is_some()
            || self.country.is_some()
    }

    /// Build Lucene query string from filled fields
    fn build_query(&self) -> String {
        let mut parts = Vec::new();
        if let Some(ref artist) = self.artist {
            if !artist.trim().is_empty() {
                parts.push(format!("artist:\"{}\"", artist.trim()));
            }
        }
        if let Some(ref album) = self.album {
            if !album.trim().is_empty() {
                parts.push(format!("release:\"{}\"", album.trim()));
            }
        }
        if let Some(ref year) = self.year {
            if !year.trim().is_empty() {
                parts.push(format!("date:{}", year.trim()));
            }
        }
        if let Some(ref label) = self.label {
            if !label.trim().is_empty() {
                parts.push(format!("label:\"{}\"", label.trim()));
            }
        }
        if let Some(ref catno) = self.catalog_number {
            if !catno.trim().is_empty() {
                parts.push(format!("catno:\"{}\"", catno.trim()));
            }
        }
        if let Some(ref barcode) = self.barcode {
            if !barcode.trim().is_empty() {
                parts.push(format!("barcode:{}", barcode.trim()));
            }
        }
        if let Some(ref format) = self.format {
            if !format.trim().is_empty() {
                parts.push(format!("format:\"{}\"", format.trim()));
            }
        }
        if let Some(ref country) = self.country {
            if !country.trim().is_empty() {
                parts.push(format!("country:\"{}\"", country.trim()));
            }
        }
        parts.join(" AND ")
    }
}

/// Search MusicBrainz for releases using structured parameters
pub async fn search_releases_with_params(
    params: &ReleaseSearchParams,
) -> Result<Vec<SearchRelease>, MusicBrainzError> {
    if !params.has_any_field() {
        return Err(MusicBrainzError::Api(
            "At least one search field must be provided".to_string(),
        ));
    }
    let query = params.build_query();
    info!("MusicBrainz: Searching with params: {:?}", params);
    info!("   Query: {}", query);
    let url = "https://musicbrainz.org/ws/2/release";
    debug!(
        "MusicBrainz API request: {}?query={}&limit=25&inc=recordings+artist-credits+release-groups+labels+media+url-rels",
        url, query
    );

    wait_for_rate_limit().await;

    let response = http_client()
        .get(url)
        .query(&[
            ("query", query.as_str()),
            ("limit", "25"),
            (
                "inc",
                "recordings+artist-credits+release-groups+labels+media+url-rels",
            ),
        ])
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| MusicBrainzError::Api(format!("HTTP request failed: {}", e)))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());

        warn!(
            "MusicBrainz API error response ({}): {}",
            status, error_text
        );

        if status == 404 {
            return Ok(Vec::new());
        }
        return Err(MusicBrainzError::Api(format!(
            "MusicBrainz API returned status {}: {}",
            status, error_text
        )));
    }

    let search_response: SearchResponse = response
        .json()
        .await
        .map_err(|e| MusicBrainzError::Api(format!("Failed to parse JSON: {}", e)))?;

    #[cfg(debug_assertions)]
    {
        let temp_path = std::env::temp_dir().join("musicbrainz_search_response.json");
        match serde_json::to_string_pretty(&search_response) {
            Ok(json_str) => match std::fs::write(&temp_path, json_str) {
                Ok(()) => debug!("MusicBrainz search response written to {:?}", temp_path),
                Err(e) => debug!(
                    "MusicBrainz search response cache write to {:?} failed: {}",
                    temp_path, e
                ),
            },
            Err(e) => debug!("MusicBrainz search response serialize failed: {}", e),
        }
    }

    if let Some(ref error_msg) = search_response.error {
        warn!("MusicBrainz API returned error: {}", error_msg);
        return Err(MusicBrainzError::Api(format!(
            "MusicBrainz error: {}",
            error_msg
        )));
    }

    let releases = search_response.releases;

    info!("Found {} release(s)", releases.len());
    Ok(releases)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_release_search_params_build_query() {
        let params = ReleaseSearchParams {
            artist: Some("Test Artist".to_string()),
            album: Some("Test Album".to_string()),
            year: Some("2000".to_string()),
            ..Default::default()
        };
        assert_eq!(
            params.build_query(),
            "artist:\"Test Artist\" AND release:\"Test Album\" AND date:2000",
        );
        let params2 = ReleaseSearchParams {
            artist: Some("Another Artist".to_string()),
            catalog_number: Some("TL-1234".to_string()),
            ..Default::default()
        };
        assert_eq!(
            params2.build_query(),
            "artist:\"Another Artist\" AND catno:\"TL-1234\""
        );
    }

    #[tokio::test(start_paused = true)]
    async fn test_rate_limiter_enforces_spacing() {
        // First call should return immediately
        let start = Instant::now();
        wait_for_rate_limit().await;
        let first_elapsed = start.elapsed();
        assert!(
            first_elapsed < Duration::from_millis(100),
            "First call should be near-instant, took {:?}",
            first_elapsed
        );

        // Second call should wait ~1 second
        let start = Instant::now();
        wait_for_rate_limit().await;
        let second_elapsed = start.elapsed();
        assert!(
            second_elapsed >= Duration::from_millis(900),
            "Second call should wait ~1s, only waited {:?}",
            second_elapsed
        );
    }

    #[test]
    fn test_mb_release_response_track_count() {
        let response = MbReleaseResponse {
            id: "r1".to_string(),
            title: "T".to_string(),
            date: None,
            country: None,
            barcode: None,
            artist_credit: vec![],
            release_group: None,
            label_info: vec![],
            media: vec![
                MbMedium {
                    format: None,
                    tracks: vec![
                        MbTrack {
                            position: Some(1),
                            number: None,
                            title: Some("Track 1".to_string()),
                            length: None,
                            recording: None,
                            artist_credit: vec![],
                        },
                        MbTrack {
                            position: Some(2),
                            number: None,
                            title: Some("Track 2".to_string()),
                            length: None,
                            recording: None,
                            artist_credit: vec![],
                        },
                    ],
                },
                MbMedium {
                    format: None,
                    tracks: vec![MbTrack {
                        position: Some(1),
                        number: None,
                        title: Some("Track 1 Disc 2".to_string()),
                        length: None,
                        recording: None,
                        artist_credit: vec![],
                    }],
                },
            ],
            relations: vec![],
        };

        assert_eq!(response.track_count(), 3);
    }

    #[test]
    fn test_extract_urls_from_relations() {
        let relations = vec![
            MbRelation {
                url: Some(MbUrlResource {
                    resource: Some("https://www.discogs.com/master/12345".to_string()),
                }),
            },
            MbRelation {
                url: Some(MbUrlResource {
                    resource: Some("https://www.discogs.com/release/67890".to_string()),
                }),
            },
            MbRelation { url: None },
        ];

        let mut urls = ExternalUrls {
            discogs_master_url: None,
            discogs_release_url: None,
        };

        extract_urls_from_relations(&relations, &mut urls);

        assert_eq!(
            urls.discogs_master_url.as_deref(),
            Some("https://www.discogs.com/master/12345")
        );
        assert_eq!(
            urls.discogs_release_url.as_deref(),
            Some("https://www.discogs.com/release/67890")
        );
    }

    #[test]
    fn test_deserialize_mb_release_response() {
        let json = r#"{
            "id": "f9469bd8-a413-43f1-bee3-e3baabfb91cc",
            "title": "Super Hits of the 70s",
            "date": "2002",
            "country": null,
            "barcode": "8711638222024",
            "artist-credit": [{
                "name": "All Star Cover Band",
                "artist": {
                    "id": "53ebb100-5cfb-42e7-9ae3-453464420840",
                    "name": "All Star Cover Band",
                    "sort-name": "All Star Cover Band"
                }
            }],
            "release-group": {
                "id": "ded0036e-243a-4ae4-8c65-7ec37aae4bd9",
                "first-release-date": "2002",
                "secondary-types": [],
                "secondary-type-ids": []
            },
            "label-info": [{
                "catalog-number": "3822202",
                "label": { "name": "Galaxy Music" }
            }],
            "media": [{
                "format": "CD",
                "tracks": [
                    { "position": 1, "title": "Track One Title", "length": 216000 },
                    { "position": 2, "title": "Track Two Title", "length": 241000 }
                ]
            }],
            "relations": [{
                "url": { "resource": "https://www.discogs.com/release/67890" }
            }]
        }"#;

        let response: MbReleaseResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.id, "f9469bd8-a413-43f1-bee3-e3baabfb91cc");
        assert_eq!(response.title, "Super Hits of the 70s");
        assert_eq!(response.date.as_deref(), Some("2002"));
        assert!(response.country.is_none());
        assert_eq!(response.barcode.as_deref(), Some("8711638222024"));
        assert_eq!(response.artist_credit.len(), 1);
        assert_eq!(response.artist_credit[0].name, "All Star Cover Band");
        assert_eq!(response.media.len(), 1);
        assert_eq!(response.media[0].tracks.len(), 2);
        assert_eq!(
            response.media[0].tracks[0].title.as_deref(),
            Some("Track One Title")
        );
        assert_eq!(response.track_count(), 2);
        assert_eq!(response.label_info.len(), 1);
        assert_eq!(
            response.label_info[0].catalog_number.as_deref(),
            Some("3822202")
        );
        assert_eq!(response.relations.len(), 1);
    }

    #[test]
    fn test_deserialize_mb_release_response_minimal() {
        // Minimal response with only required fields — all optional arrays absent
        let json = r#"{
            "id": "abc-123",
            "title": "Minimal Release"
        }"#;

        let response: MbReleaseResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.id, "abc-123");
        assert_eq!(response.title, "Minimal Release");
        assert!(response.date.is_none());
        assert!(response.artist_credit.is_empty());
        assert!(response.media.is_empty());
        assert!(response.relations.is_empty());
        assert_eq!(response.track_count(), 0);
    }

    // ── fetch_mb_xref ────────────────────────────────────────────────
    //
    // Drives the cross-reference path through cache seeding so the
    // tests don't hit the network. Each test uses a unique Discogs
    // release ID so other tests' cache seeds don't bleed in (the
    // caches are process-global LRUs).

    fn make_mb_response(id: &str, release_group_id: Option<&str>) -> MbReleaseResponse {
        MbReleaseResponse {
            id: id.to_string(),
            title: "Test Album".to_string(),
            date: None,
            country: None,
            barcode: None,
            artist_credit: vec![],
            release_group: release_group_id.map(|rg| MbReleaseGroupRef {
                id: rg.to_string(),
                first_release_date: None,
                relations: None,
            }),
            label_info: vec![],
            media: vec![],
            relations: vec![],
        }
    }

    fn empty_external_urls() -> ExternalUrls {
        ExternalUrls {
            discogs_master_url: None,
            discogs_release_url: None,
        }
    }

    #[tokio::test]
    async fn test_fetch_mb_xref_with_backlink_returns_response_and_metadata() {
        let discogs_id = "fetch-mb-xref-hit-1";
        let mb_release_id = "mb-release-hit-1";
        let mb_group_id = "mb-group-hit-1";

        seed_discogs_url_lookup(discogs_id, Some(mb_release_id.to_string()));
        seed_release_cache(
            mb_release_id,
            (
                make_mb_response(mb_release_id, Some(mb_group_id)),
                empty_external_urls(),
                r#"{"id":"mb-release-hit-1"}"#.to_string(),
            ),
        );
        seed_release_group_json_cache(mb_group_id, r#"{"id":"mb-group-hit-1"}"#.to_string());

        let result = fetch_mb_xref(discogs_id).await;

        let (response, pairs) = result.expect("expected cross-link to be found");
        assert_eq!(response.id, mb_release_id);
        assert_eq!(
            response.release_group.as_ref().map(|rg| rg.id.as_str()),
            Some(mb_group_id)
        );
        // Two pairs: MB release JSON + MB release-group JSON.
        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[0].0, "musicbrainz");
        assert_eq!(pairs[1].0, "musicbrainz_release_group");
    }

    #[tokio::test]
    async fn test_fetch_mb_xref_no_backlink_returns_none() {
        let discogs_id = "fetch-mb-xref-miss-1";
        seed_discogs_url_lookup(discogs_id, None);

        let result = fetch_mb_xref(discogs_id).await;

        assert!(
            result.is_none(),
            "expected None when MB has no back-link, got Some"
        );
    }

    #[tokio::test]
    async fn test_fetch_mb_xref_release_without_group_still_returns_response() {
        // The mapper's identity-emission gate is checked at the mapper
        // layer (it only emits an MB identity row when `release_group`
        // is present). `fetch_mb_xref` itself returns whatever the MB
        // API gave us — the absence of a release group is not a
        // fetch-time failure.
        let discogs_id = "fetch-mb-xref-no-rg";
        let mb_release_id = "mb-release-no-rg";

        seed_discogs_url_lookup(discogs_id, Some(mb_release_id.to_string()));
        seed_release_cache(
            mb_release_id,
            (
                make_mb_response(mb_release_id, None),
                empty_external_urls(),
                r#"{"id":"mb-release-no-rg"}"#.to_string(),
            ),
        );

        let result = fetch_mb_xref(discogs_id).await;

        let (response, pairs) = result.expect("expected response even without release_group");
        assert_eq!(response.id, mb_release_id);
        assert!(response.release_group.is_none());
        // Only one pair (no release-group JSON to fetch).
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].0, "musicbrainz");
    }
}
