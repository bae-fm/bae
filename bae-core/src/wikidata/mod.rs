//! Wikidata entity client.
//!
//! One Wikidata item states an album's identifier in catalog after catalog,
//! which is how bae reaches the catalogs it never asks anything: MusicBrainz
//! editors link a Wikidata item far more often than they link each streaming
//! service and review site separately.
//!
//! [`Wikidata`] fetches one item's entity document — rate-limited, timed out
//! and retried the way the MusicBrainz and Discogs clients are — and this
//! module reads the catalog pages out of its claims. The app builds one
//! [`Wikidata`] when it starts and hands it down.

use std::collections::BTreeMap;
use std::time::Duration;

use serde::Deserialize;
use thiserror::Error;
use tracing::{debug, warn};

use crate::import::{Catalog, CatalogPage};
use crate::retry::RetryPolicy;
use crate::util::http::{is_cacheable, CachedResponse, Http};
use crate::util::rate_limiter::{CallPriority, RateLimiter};
use crate::util::session_cache::{SessionCache, PROVIDER_RESPONSE_CAPACITY};

/// Where every Wikidata request goes.
const BASE_URL: &str = "https://www.wikidata.org";

/// One request a second, the pace the MusicBrainz and Discogs clients keep.
const REQUEST_INTERVAL: Duration = Duration::from_secs(1);

/// Where one item's entity document is fetched from. Named once, because the
/// response cache is keyed by URL: the request and anything putting an answer
/// where the request will look for it have to agree on it.
fn entity_url(item: &str) -> String {
    format!("{BASE_URL}/wiki/Special:EntityData/{item}.json")
}

/// Wikidata as bae asks it: the transport requests go out on, the rate limit
/// they wait for, and the answers already had. Wikimedia's user-agent policy
/// requires a request to identify what is making it, which the transport's
/// client already does for every bae request.
pub struct Wikidata {
    http: Http,
    limiter: RateLimiter,
    /// Every stable answer kept, keyed by the full request URL.
    responses: SessionCache<CachedResponse>,
}

/// A Wikidata lookup failure, keeping the same wire-level distinctions the
/// MusicBrainz client keeps: a transport failure that produced no HTTP
/// response, a timeout, or an HTTP error *response* carrying a status. `Other`
/// is local/internal detail, never Wikidata's verdict.
#[derive(Debug, Error)]
pub enum WikidataError {
    /// Wikidata has no such item — a 404, which is its answer for an id that
    /// was deleted or never existed.
    #[error("No Wikidata item {0}")]
    NotFound(String),
    /// The request never reached a response — connection refused, DNS failure,
    /// a dropped body. Carries the underlying error for logging.
    #[error("Wikidata network error: {0}")]
    Network(String),
    /// The request timed out before a response arrived.
    #[error("Wikidata request timed out")]
    Timeout,
    /// Wikidata returned an HTTP error response. `status` is the HTTP status
    /// code when one was observed (`None` when reqwest classified a send error
    /// as carrying a status we couldn't read).
    #[error("Wikidata returned an error response (status {status:?})")]
    Provider { status: Option<u16> },
    /// A local/internal failure (JSON parsing, body read).
    #[error("Wikidata API error: {0}")]
    Other(String),
}

impl WikidataError {
    /// Classify a `reqwest::Error` from a send or body read. A timeout is its
    /// own variant; an error carrying an HTTP status is a `Provider` response;
    /// everything else (connection, DNS, dropped body) is transport `Network`.
    fn from_reqwest(e: reqwest::Error) -> Self {
        if e.is_timeout() {
            WikidataError::Timeout
        } else if let Some(status) = e.status() {
            WikidataError::Provider {
                status: Some(status.as_u16()),
            }
        } else {
            WikidataError::Network(e.to_string())
        }
    }
}

/// Retry only what a retry can fix. `NotFound` is Wikidata's answer about an
/// item, not a fault. `Other` is local (a JSON parse): the same bytes will
/// parse the same way.
fn should_retry(error: &WikidataError) -> bool {
    match error {
        WikidataError::Network(_) | WikidataError::Timeout => true,
        // No readable status means reqwest classified a send error as carrying
        // one it couldn't produce — repeat it like any transport failure.
        WikidataError::Provider { status } => status.is_none_or(|status| {
            reqwest::StatusCode::from_u16(status).is_ok_and(crate::retry::is_transient_status)
        }),
        WikidataError::NotFound(_) | WikidataError::Other(_) => false,
    }
}

/// How Wikidata is asked again: a 429 or a 5xx is Wikimedia shedding load, so
/// the waits double from one second, jittered, as MusicBrainz's do.
const RETRY: RetryPolicy =
    RetryPolicy::exponential(4, Duration::from_secs(1), Duration::from_secs(10));

/// Wrap one request in the client's own retry policy — a caller shouldn't have
/// to know which of these failures are worth repeating. (MusicBrainz and
/// Discogs do the same.)
async fn wikidata_retry<F, Fut, T>(label: &str, f: F) -> Result<T, WikidataError>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, WikidataError>>,
{
    crate::retry::retry_with_backoff_if(RETRY, label, should_retry, f).await
}

fn wikidata_body(response: CachedResponse) -> Result<String, WikidataError> {
    if response.is_success() {
        Ok(response.body)
    } else {
        Err(WikidataError::Provider {
            status: Some(response.status),
        })
    }
}

impl Wikidata {
    pub fn new(http: Http) -> Self {
        Self::with_interval(http, REQUEST_INTERVAL)
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
            responses: SessionCache::new("Wikidata response cache", PROVIDER_RESPONSE_CAPACITY),
        }
    }

    /// One Wikidata GET, answered from the cache when this URL already has an
    /// answer — without waiting for a rate-limit slot — and otherwise sent,
    /// kept when the answer is stable, and returned.
    async fn get(&self, url: &str, priority: CallPriority) -> Result<String, WikidataError> {
        let request = self
            .http
            .get(url)
            .header("Accept", "application/json")
            .timeout(crate::util::http::API_TIMEOUT)
            .build()
            .map_err(WikidataError::from_reqwest)?;
        let key = request.url().to_string();

        if let Some(cached) = self.responses.get_cloned(&key) {
            debug!("Wikidata response cache hit for {}", key);
            return wikidata_body(cached);
        }

        self.limiter.wait(priority).await;
        let response = self
            .http
            .execute(request)
            .await
            .map_err(WikidataError::from_reqwest)?;
        let status = response.status().as_u16();
        let body = response.text().await.map_err(WikidataError::from_reqwest)?;
        let response = CachedResponse { status, body };

        if !response.is_success() {
            warn!(
                "Wikidata error response ({} from {}): {}",
                status, key, response.body
            );
        }
        if is_cacheable(status) {
            self.responses.put(key, response.clone());
        }
        wikidata_body(response)
    }

    /// Pre-populate an item's answer, so a test can drive the archival path
    /// without an HTTP call. `raw_json` is the endpoint's own answer: it is
    /// what gets archived, what a later projection replays from, and what the
    /// client parses here, so those three cannot disagree. `None` is the 404
    /// Wikidata gives for an item it does not have.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn seed_entity_cache(&self, item: &str, raw_json: Option<String>) {
        let url = entity_url(item);
        let (status, body) = match raw_json {
            Some(json) => (200, json),
            None => (404, String::new()),
        };
        self.responses.put(
            crate::util::http::response_key(&url),
            CachedResponse { status, body },
        );
    }

    /// One item's entity document, as Wikidata returned it.
    ///
    /// The document is what gets archived, and every later read of this item's
    /// identifiers replays from it — so it is parsed here before it is handed
    /// back. A body that is not an entity document fails this fetch rather than
    /// becoming a stored row that fails every read of it afterwards.
    pub async fn fetch_entity(
        &self,
        item: &str,
        priority: CallPriority,
    ) -> Result<String, WikidataError> {
        wikidata_retry("Wikidata entity fetch", || {
            self.fetch_entity_once(item, priority)
        })
        .await
    }

    async fn fetch_entity_once(
        &self,
        item: &str,
        priority: CallPriority,
    ) -> Result<String, WikidataError> {
        let url = entity_url(item);
        debug!("Wikidata API request: {}", url);

        let raw_json = match self.get(&url, priority).await {
            Ok(body) => body,
            Err(WikidataError::Provider { status: Some(404) }) => {
                return Err(WikidataError::NotFound(item.to_string()));
            }
            Err(error) => return Err(error),
        };

        parse_entity(&raw_json)
            .map_err(|e| WikidataError::Other(format!("Failed to parse JSON: {}", e)))?;
        Ok(raw_json)
    }
}

/// The Wikidata properties whose values are another catalog's key for the same
/// album, and which of that catalog's pages the value names.
///
/// Each id was read off its own property page on wikidata.org, where the
/// formatter URL agrees with the address [`Catalog::album_url`] builds from
/// the same key:
///
/// - `P436` MusicBrainz release group ID — `https://musicbrainz.org/release-group/$1`
/// - `P1954` Discogs master ID — `https://www.discogs.com/en/master/$1`
/// - `P1729` AllMusic album ID — `https://www.allmusic.com/album/$1`
/// - `P8392` Rate Your Music release ID — `https://rateyourmusic.com/release/$1/`
/// - `P6217` Genius album ID — `https://genius.com/albums/$1`
/// - `P2205` Spotify album ID — `https://open.spotify.com/album/$1`
/// - `P2281` Apple Music album ID — `https://music.apple.com/album/$1`
/// - `P2723` Deezer album ID — `https://www.deezer.com/album/$1`
///
/// Rate Your Music and Genius each publish a second identifier property that
/// is not an album's: `P5404` names an artist and `P6218` names a Genius page
/// of any kind, so neither belongs here.
///
/// Musik-Sammler and Bandcamp are absent because Wikidata states no property
/// for the page a record of theirs keys on. Musik-Sammler has an artist id
/// (`P9965`) and no album one; Bandcamp release ID (`P11354`) is the numeric
/// id of an embedded player widget, not the artist-subdomain album address.
/// Both catalogs still arrive through MusicBrainz's own url-rels.
const CATALOG_PROPERTIES: &[(&str, Catalog)] = &[
    ("P436", Catalog::MusicBrainz),
    ("P1954", Catalog::Discogs),
    ("P1729", Catalog::AllMusic),
    ("P8392", Catalog::RateYourMusic),
    ("P6217", Catalog::Genius),
    ("P2205", Catalog::Spotify),
    ("P2281", Catalog::AppleMusic),
    ("P2723", Catalog::Deezer),
];

/// One Wikidata item, reduced to what bae reads from it: the item's own id and
/// the string value of every claim it states.
///
/// Labels, descriptions, sitelinks and qualifiers are dropped. What the album
/// *is* bae reads from the catalogs it asks; what Wikidata adds is where else
/// the album is described.
#[derive(Debug)]
pub struct WikidataEntity {
    item: String,
    claims: BTreeMap<String, Vec<String>>,
}

impl WikidataEntity {
    /// The catalog pages this item names: the item's own page, then one per
    /// identifier it states for a catalog bae knows, in `CATALOG_PROPERTIES`
    /// order.
    ///
    /// A property no catalog of bae's answers to is skipped — an item states
    /// dozens, most of them about the album rather than about where to read
    /// more of it.
    pub fn catalog_pages(&self) -> Vec<CatalogPage> {
        let mut pages = vec![CatalogPage::Group {
            catalog: Catalog::Wikidata,
            key: self.item.clone(),
        }];
        for (property, catalog) in CATALOG_PROPERTIES {
            let Some(values) = self.claims.get(*property) else {
                continue;
            };
            pages.extend(values.iter().map(|value| CatalogPage::Group {
                catalog: *catalog,
                key: value.clone(),
            }));
        }
        pages
    }
}

/// One archived entity document, parsed.
///
/// `Special:EntityData` answers with the one item it was asked for, keyed by
/// its own id — which is the canonical id when the request followed a
/// redirect, so the pages read out of it name the item Wikidata publishes
/// rather than the id that led here.
pub fn parse_entity(json: &str) -> Result<WikidataEntity, serde_json::Error> {
    use serde::de::Error as _;

    let document: EntityDocument = serde_json::from_str(json)?;
    let (item, entity) = document
        .entities
        .into_iter()
        .next()
        .ok_or_else(|| serde_json::Error::custom("entity document names no item"))?;
    Ok(WikidataEntity {
        item,
        claims: entity
            .claims
            .into_iter()
            .map(|(property, statements)| {
                let values = statements
                    .into_iter()
                    .filter_map(|statement| statement.mainsnak.datavalue)
                    // A claim's value is a bare string only for the
                    // external-id and string datatypes; a date, a quantity and
                    // a link to another item are objects, and a `somevalue` or
                    // `novalue` snak carries no value at all. None of those is
                    // a catalog's key.
                    .filter_map(|datavalue| match datavalue.value {
                        serde_json::Value::String(value) => Some(value),
                        _ => None,
                    })
                    .collect();
                (property, values)
            })
            .collect(),
    })
}

#[derive(Deserialize)]
struct EntityDocument {
    entities: BTreeMap<String, DocumentEntity>,
}

#[derive(Deserialize)]
struct DocumentEntity {
    #[serde(default)]
    claims: BTreeMap<String, Vec<Statement>>,
}

#[derive(Deserialize)]
struct Statement {
    mainsnak: Snak,
}

#[derive(Deserialize)]
struct Snak {
    datavalue: Option<DataValue>,
}

#[derive(Deserialize)]
struct DataValue {
    value: serde_json::Value,
}

#[cfg(test)]
mod tests;
