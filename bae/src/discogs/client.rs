use crate::discogs::models::{DiscogsArtist, DiscogsRelease, DiscogsTrack};
use reqwest::{Client, Error as ReqwestError};
use serde::Deserialize;
use std::collections::HashMap;
use thiserror::Error;
use tracing::{error, warn};

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
    pub format: Option<String>,
    pub country: Option<String>,
}

impl DiscogsSearchParams {
    pub fn has_any_field(&self) -> bool {
        self.artist.is_some()
            || self.release_title.is_some()
            || self.year.is_some()
            || self.label.is_some()
            || self.catno.is_some()
            || self.barcode.is_some()
            || self.format.is_some()
            || self.country.is_some()
    }
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
    pub cover_image: Option<String>,
    pub thumb: Option<String>,
    pub master_id: Option<u64>,
    #[serde(rename = "type")]
    pub result_type: String,
}

/// Artist credit in Discogs API responses
#[derive(Debug, Deserialize, Clone)]
struct ArtistCredit {
    id: u64,
    name: String,
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
    images: Option<Vec<Image>>,
    artists: Option<Vec<ArtistCredit>>,
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
}

#[derive(Clone)]
pub struct DiscogsClient {
    client: Client,
    api_key: String,
    base_url: String,
}

impl DiscogsClient {
    pub fn new(api_key: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
            base_url: "https://api.discogs.com".to_string(),
        }
    }

    /// Flexible search using any combination of supported parameters
    pub async fn search_with_params(
        &self,
        params: &DiscogsSearchParams,
    ) -> Result<Vec<DiscogsSearchResult>, DiscogsError> {
        use tracing::{debug, info, warn};

        let url = format!("{}/database/search", self.base_url);

        let mut query_params: Vec<(&str, &str)> =
            vec![("type", "release"), ("token", &self.api_key)];

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
        if let Some(ref format) = params.format {
            query_params.push(("format", format));
        }
        if let Some(ref country) = params.country {
            query_params.push(("country", country));
        }

        info!("📡 Discogs API: GET {} with params: {:?}", url, params);

        let response = self
            .client
            .get(&url)
            .query(&query_params)
            .header("User-Agent", "bae/1.0 +https://github.com/hideselfview/bae")
            .send()
            .await?;

        let status = response.status();
        debug!("Response status: {}", status);

        if response.status().is_success() {
            let search_response: SearchResponse = response.json().await?;

            info!(
                "✓ Discogs search returned {} total result(s)",
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
        } else if response.status() == 429 {
            warn!("✗ Discogs rate limit exceeded");
            Err(DiscogsError::RateLimit)
        } else if response.status() == 401 {
            warn!("✗ Discogs invalid API key");
            Err(DiscogsError::InvalidApiKey)
        } else {
            warn!("✗ Discogs API error: {}", status);
            Err(DiscogsError::Request(
                response.error_for_status().unwrap_err(),
            ))
        }
    }

    /// Get detailed information about a specific release
    pub async fn get_release(&self, id: &str) -> Result<DiscogsRelease, DiscogsError> {
        let url = format!("{}/releases/{}", self.base_url, id);

        let mut params = HashMap::new();
        params.insert("token", &self.api_key);

        let response = self
            .client
            .get(&url)
            .query(&params)
            .header("User-Agent", "bae/1.0 +https://github.com/yourusername/bae")
            .send()
            .await?;

        if response.status().is_success() {
            let release: ReleaseResponse = response.json().await?;

            let tracklist = release
                .tracklist
                .unwrap_or_default()
                .into_iter()
                .map(|t| DiscogsTrack {
                    position: t.position,
                    title: t.title,
                    duration: t.duration,
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

            let primary_image = release.images.as_ref().and_then(|images| {
                images
                    .iter()
                    .find(|img| img.image_type == "primary")
                    .or_else(|| images.first())
            });

            let cover_image = primary_image.map(|img| img.uri.clone());
            let thumb =
                primary_image.and_then(|img| img.uri150.clone().or_else(|| Some(img.uri.clone())));

            // Some releases don't have a master_id (standalone releases)
            // The caller (import_release) will override this with the correct value anyway
            let master_id = release
                .master_id
                .map(|id| id.to_string())
                .unwrap_or_default();

            Ok(DiscogsRelease {
                id: release.id.to_string(),
                title: release.title,
                year: release.year,
                genre: release.genres.unwrap_or_default(),
                style: release.styles.unwrap_or_default(),
                format: release
                    .formats
                    .unwrap_or_default()
                    .into_iter()
                    .map(|f| f.name)
                    .collect(),
                country: release.country,
                label: Vec::new(), // Not available in detailed release
                cover_image,
                thumb,
                artists,
                tracklist,
                master_id,
            })
        } else if response.status() == 404 {
            Err(DiscogsError::NotFound)
        } else if response.status() == 429 {
            Err(DiscogsError::RateLimit)
        } else if response.status() == 401 {
            Err(DiscogsError::InvalidApiKey)
        } else {
            Err(DiscogsError::Request(
                response.error_for_status().unwrap_err(),
            ))
        }
    }
}
