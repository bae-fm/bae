use crate::discogs::models::{
    DiscogsArtist, DiscogsMaster, DiscogsRelease, DiscogsRoleArtist, DiscogsTrack,
};
use crate::discogs::remote_cover_from_urls;
use crate::import::cover_art::RemoteCover;
use crate::retry::retry_with_backoff_if;
use crate::util::http::{is_cacheable, CachedResponse, Http};
use crate::util::rate_limiter::{CallPriority, RateLimiter};
use crate::util::session_cache::{SessionCache, PROVIDER_RESPONSE_CAPACITY};
use reqwest::{Error as ReqwestError, StatusCode};
use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tracing::{debug, warn};

const DISCOGS_REQUEST_INTERVAL: Duration = Duration::from_secs(1);
const DISCOGS_RETRY_ATTEMPTS: u32 = 3;

/// Where every Discogs request goes.
const API_BASE_URL: &str = "https://api.discogs.com";

/// Discogs as bae asks it, whatever key a request carries: the transport
/// requests go out on, the one rate limit every request waits for, and the
/// answers already had. The app builds one when it starts; each
/// [`DiscogsClient`] carries a key and asks through it.
///
/// A release, master or artist document does not vary with the key that asked
/// for it, so the answers are kept here rather than per client. What a key
/// itself is worth does, which is why the key check does not read from here.
pub struct Discogs {
    http: Http,
    limiter: RateLimiter,
    /// Every stable answer kept, keyed by the full request URL.
    responses: SessionCache<CachedResponse>,
}

impl Discogs {
    pub fn new(http: Http) -> Self {
        Self::with_interval(http, DISCOGS_REQUEST_INTERVAL)
    }

    /// One whose requests are not spaced: a test's fake service has no rate
    /// to keep to.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn for_test(http: Http) -> Self {
        Self::with_interval(http, Duration::ZERO)
    }

    fn with_interval(http: Http, interval: Duration) -> Self {
        Self {
            http,
            limiter: RateLimiter::new(interval),
            responses: SessionCache::new("Discogs response cache", PROVIDER_RESPONSE_CAPACITY),
        }
    }

    /// Put `body` where a request for `url` would look for it.
    #[cfg(any(test, feature = "test-utils"))]
    fn seed_response(&self, url: &str, status: u16, body: String) {
        self.responses.put(
            crate::util::http::response_key(url),
            CachedResponse { status, body },
        );
    }

    /// Pre-populate a release document, so a test can drive `prepare_release`
    /// without an HTTP call. `raw_json` is the endpoint's own answer: it is
    /// what gets archived, what a later projection replays from, and what the
    /// client parses here, so those three cannot disagree.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn seed_release_cache(&self, id: &str, raw_json: String) {
        self.seed_response(&release_url(id), 200, raw_json);
    }

    /// Pre-populate a master document, for a synthetic `DiscogsRelease` that
    /// carries a `master_id` — the worker's cross-reference fetch then resolves
    /// through it.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn seed_master_cache(&self, master_id: &str, raw_json: String) {
        self.seed_response(&master_url(master_id), 200, raw_json);
    }

    /// Pre-populate an artist document whose image list yields `image_url`.
    /// `None` is the 404 the endpoint gives for an artist it does not have,
    /// which the image lookup reads as "no image".
    #[cfg(any(test, feature = "test-utils"))]
    pub fn seed_artist_image_response(&self, artist_id: &str, image_url: Option<String>) {
        let url = artist_url(artist_id);
        match image_url {
            Some(uri) => self.seed_response(
                &url,
                200,
                serde_json::json!({ "images": [{ "type": "primary", "uri": uri }] }).to_string(),
            ),
            None => self.seed_response(&url, 404, String::new()),
        }
    }
}

fn release_url(id: &str) -> String {
    format!("{API_BASE_URL}/releases/{id}")
}

fn master_url(master_id: &str) -> String {
    format!("{API_BASE_URL}/masters/{master_id}")
}

fn artist_url(artist_id: &str) -> String {
    format!("{API_BASE_URL}/artists/{artist_id}")
}

#[derive(Error, Debug)]
pub enum DiscogsError {
    /// The request never reached a usable response — connection, DNS, timeout, a
    /// dropped or unreadable body. Transport-level and worth retrying.
    // reqwest's Display omits the source; Debug retains the actual cause.
    #[error("Discogs transport error: {0:?}")]
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

/// The body on a success, and the error the status names otherwise.
fn classify_discogs_response(response: CachedResponse) -> Result<String, DiscogsError> {
    if response.is_success() {
        return Ok(response.body);
    }

    // The status was read off a real response, so it is a code `StatusCode`
    // holds.
    match StatusCode::from_u16(response.status).expect("a response status is in range") {
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
    /// Words matched anywhere on a release, sent as Discogs's `q`. An artist
    /// goes here rather than in Discogs's `artist` filter, which matches only
    /// the artist's main name: a record credited as "The Wailing Wailers" is
    /// filed under "The Wailers", and one tagged "The Melvins" under
    /// "Melvins", so the filter finds neither by the name on the record.
    pub text: Option<String>,
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
    /// Every barcode Discogs holds for the pressing, in the order it lists
    /// them. Absent from the response for a release with none.
    #[serde(default)]
    pub barcode: Vec<String>,
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
    #[serde(default, deserialize_with = "optional_master_id")]
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
    #[serde(
        default,
        deserialize_with = "crate::serde_helpers::empty_string_as_none"
    )]
    country: Option<String>,
    labels: Option<Vec<LabelResponse>>,
    images: Option<Vec<Image>>,
    artists: Option<Vec<ArtistCredit>>,
    extraartists: Option<Vec<ExtraArtistCredit>>,
    tracklist: Option<Vec<TrackResponse>>,
    #[serde(default, deserialize_with = "optional_master_id")]
    master_id: Option<u64>,
    identifiers: Option<Vec<Identifier>>,
}

#[derive(Debug, Deserialize)]
struct Identifier {
    #[serde(rename = "type")]
    kind: String,
    #[serde(
        default,
        deserialize_with = "crate::serde_helpers::empty_string_as_none"
    )]
    value: Option<String>,
}

/// Discogs uses zero when a release has no master record.
fn optional_master_id<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<u64>::deserialize(deserializer)?.filter(|id| *id != 0))
}

#[derive(Debug, Deserialize)]
struct MasterResponse {
    id: u64,
    #[serde(
        default,
        deserialize_with = "crate::serde_helpers::empty_string_as_none"
    )]
    title: Option<String>,
    year: Option<u32>,
    artists: Option<Vec<ArtistCredit>>,
    images: Option<Vec<Image>>,
}
#[derive(Debug, Deserialize)]
struct Format {
    #[serde(
        default,
        deserialize_with = "crate::serde_helpers::empty_string_as_none"
    )]
    name: Option<String>,
}
#[derive(Debug, Deserialize)]
struct Image {
    #[serde(rename = "type")]
    image_type: String,
    #[serde(
        default,
        deserialize_with = "crate::serde_helpers::empty_string_as_none"
    )]
    uri: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::serde_helpers::empty_string_as_none"
    )]
    uri150: Option<String>,
}

/// Discogs uses the same image array on releases and masters. Keep every
/// usable image, with the primary image first and provider order otherwise.
fn image_covers(images: Option<Vec<Image>>, entity: &str, id: u64) -> Vec<RemoteCover> {
    let mut images: Vec<Image> = images.into_iter().flatten().collect();
    images.sort_by_key(|image| image.image_type != "primary");
    let mut covers = Vec::new();
    for (index, image) in images.into_iter().enumerate() {
        if let Some(mut cover) =
            remote_cover_from_urls(image.uri.as_deref(), image.uri150.as_deref(), entity, id)
        {
            cover.label = format!("Discogs · [{entity}{id}] · {}", index + 1);
            crate::import::cover_art::push_unique_cover(&mut covers, cover);
        }
    }
    covers
}

/// Artwork from an archived master document, using the release image parser.
pub(crate) fn parse_discogs_master_covers(
    raw_json: &str,
) -> Result<Vec<RemoteCover>, DiscogsError> {
    Ok(parse_discogs_master_json(raw_json)?.covers)
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
    #[serde(default)]
    sub_tracks: Vec<TrackResponse>,
}
#[derive(Debug, Deserialize)]
struct LabelResponse {
    #[serde(
        default,
        deserialize_with = "crate::serde_helpers::empty_string_as_none"
    )]
    name: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::serde_helpers::empty_string_as_none"
    )]
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

fn track_to_model(track: TrackResponse) -> DiscogsTrack {
    DiscogsTrack {
        position: track.position,
        title: track.title,
        duration: track.duration,
        artists: track
            .artists
            .into_iter()
            .map(|artist| DiscogsArtist {
                id: artist.id.to_string(),
                name: artist.name,
            })
            .collect(),
        extraartists: track.extraartists.map(|extraartists| {
            extraartists
                .into_iter()
                .filter_map(extra_artist_to_model)
                .collect()
        }),
        type_: if track.type_.is_empty() {
            "track".to_string()
        } else {
            track.type_
        },
        sub_tracks: track.sub_tracks.into_iter().map(track_to_model).collect(),
    }
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
        .map(track_to_model)
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
    let covers = image_covers(release.images, "r", release.id);
    let master_id = release.master_id.map(|id| id.to_string());
    let labels = release.labels.unwrap_or_default();
    let label_names: Vec<String> = labels
        .iter()
        .filter_map(|label| label.name.clone())
        .collect();
    let catno = labels.first().and_then(|l| l.catno.clone());
    let formats = release.formats.unwrap_or_default();
    // The draft has one barcode field. Keep the first supplied Barcode value
    // in provider order, including its printed spaces and punctuation.
    let barcode = release
        .identifiers
        .into_iter()
        .flatten()
        .filter(|identifier| identifier.kind == "Barcode")
        .find_map(|identifier| identifier.value);

    Ok(DiscogsRelease {
        id: release.id.to_string(),
        title: release.title,
        // Discogs encodes an unknown year as zero.
        year: release.year.filter(|year| *year != 0),
        format: formats
            .into_iter()
            .filter_map(|format| format.name)
            .collect(),
        country: release.country,
        label: label_names,
        catno,
        barcode,
        covers,
        artists,
        extraartists,
        tracklist,
        master_id,
    })
}

/// Parse the master's own album metadata for both fresh fetches and archive replay.
pub fn parse_discogs_master_json(raw_json: &str) -> Result<DiscogsMaster, DiscogsError> {
    let master: MasterResponse = serde_json::from_str(raw_json)?;
    Ok(DiscogsMaster {
        title: master.title,
        year: master.year.filter(|year| *year != 0),
        artists: master
            .artists
            .into_iter()
            .flatten()
            .map(|artist| DiscogsArtist {
                id: artist.id.to_string(),
                name: artist.name,
            })
            .collect(),
        covers: image_covers(master.images, "m", master.id),
    })
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
/// the library manager's Discogs operation session.
pub type DiscogsValidationObserver = std::sync::Arc<dyn Fn(DiscogsKeySignal) + Send + Sync>;

/// Discogs asked with one key.
pub struct DiscogsClient {
    discogs: Arc<Discogs>,
    api_key: String,
    observer: Option<DiscogsValidationObserver>,
}
impl DiscogsClient {
    /// A client with no validation observer — for validating a candidate key
    /// before it's stored (the save path interprets that result directly).
    pub fn new(discogs: Arc<Discogs>, api_key: String) -> Self {
        Self::build(discogs, api_key, None)
    }

    /// A client that reports each call's outcome to `observer`, so a stored key
    /// re-validates as it's used.
    pub fn with_observer(
        discogs: Arc<Discogs>,
        api_key: String,
        observer: DiscogsValidationObserver,
    ) -> Self {
        Self::build(discogs, api_key, Some(observer))
    }

    fn build(
        discogs: Arc<Discogs>,
        api_key: String,
        observer: Option<DiscogsValidationObserver>,
    ) -> Self {
        Self {
            discogs,
            api_key,
            observer,
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
        self.discogs
            .http
            .get(url)
            .header("Authorization", format!("Discogs token={}", self.api_key))
            .timeout(crate::util::http::API_TIMEOUT)
    }

    /// One request, straight to the wire: wait for the rate-limit slot, send,
    /// read the whole body. Only the token check sends this way — its answer is
    /// about the key in the `Authorization` header, which no URL names, so a
    /// kept answer would be an answer to a different question.
    async fn send(
        &self,
        request: reqwest::Request,
        priority: CallPriority,
    ) -> Result<CachedResponse, DiscogsError> {
        self.discogs.limiter.wait(priority).await;
        let response = self.discogs.http.execute(request).await?;
        let status = response.status().as_u16();
        let body = response.text().await?;
        Ok(CachedResponse { status, body })
    }

    /// Every content request's send point: a URL that already has an answer is
    /// answered from the cache without waiting for a rate-limit slot; otherwise
    /// the request is made and a stable answer kept. Returns the body on a
    /// success, and the error the status names otherwise.
    async fn get_cached(
        &self,
        request: reqwest::RequestBuilder,
        priority: CallPriority,
    ) -> Result<String, DiscogsError> {
        let request = request.build()?;
        let key = request.url().to_string();

        if let Some(cached) = self.discogs.responses.get_cloned(&key) {
            debug!("Discogs response cache hit for {}", key);
            return classify_discogs_response(cached);
        }

        let response = self.send(request, priority).await?;
        if is_cacheable(response.status) {
            self.discogs.responses.put(key, response.clone());
        }
        classify_discogs_response(response)
    }

    /// Check the API token with a request cheap enough to throw away.
    pub async fn validate_token(&self, priority: CallPriority) -> Result<(), DiscogsError> {
        let url = format!("{API_BASE_URL}/database/search");
        let query_params = [("per_page", "1")];

        retry_with_backoff_if(
            DISCOGS_RETRY_ATTEMPTS,
            "Discogs token validation",
            should_retry_discogs,
            crate::retry::linear_backoff,
            || async {
                let request = self.get(&url).query(&query_params).build()?;
                classify_discogs_response(self.send(request, priority).await?).map(|_| ())
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
        let url = format!("{API_BASE_URL}/database/search");
        let mut query_params: Vec<(&str, &str)> = vec![("type", "release")];
        if let Some(ref text) = params.text {
            query_params.push(("q", text));
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
                let body = self
                    .get_cached(self.get(&url).query(&query_params), priority)
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
                serde_json::from_str(&body).map_err(DiscogsError::Serialization)
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
        let url = release_url(id);
        retry_with_backoff_if(
            DISCOGS_RETRY_ATTEMPTS,
            "Discogs release fetch",
            should_retry_discogs,
            crate::retry::linear_backoff,
            || async {
                let raw_json = self.get_cached(self.get(&url), priority).await?;
                let release = parse_discogs_release_json(&raw_json)?;
                Ok((release, raw_json))
            },
        )
        .await
    }

    /// The master's album metadata and raw JSON, without fetching any pressing.
    pub async fn get_master(
        &self,
        master_id: &str,
        priority: CallPriority,
    ) -> Result<(DiscogsMaster, String), DiscogsError> {
        self.observed(self.get_master_inner(master_id, priority))
            .await
    }

    async fn get_master_inner(
        &self,
        master_id: &str,
        priority: CallPriority,
    ) -> Result<(DiscogsMaster, String), DiscogsError> {
        let url = master_url(master_id);
        retry_with_backoff_if(
            DISCOGS_RETRY_ATTEMPTS,
            "Discogs master fetch",
            should_retry_discogs,
            crate::retry::linear_backoff,
            || async {
                let raw_json = self.get_cached(self.get(&url), priority).await?;
                let master = parse_discogs_master_json(&raw_json)?;
                Ok((master, raw_json))
            },
        )
        .await
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
        let url = artist_url(artist_id);
        let Some(body) = retry_with_backoff_if(
            DISCOGS_RETRY_ATTEMPTS,
            "Discogs artist fetch",
            should_retry_discogs,
            crate::retry::linear_backoff,
            || async {
                match self.get_cached(self.get(&url), priority).await {
                    Ok(body) => Ok(Some(body)),
                    Err(DiscogsError::NotFound) => {
                        warn!(
                            discogs_artist_id = %artist_id,
                            "Discogs artist image lookup returned not found"
                        );
                        Ok(None)
                    }
                    Err(error) => Err(error),
                }
            },
        )
        .await?
        else {
            return Ok(None);
        };
        let json: serde_json::Value = serde_json::from_str(&body)?;
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

#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;
