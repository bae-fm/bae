use super::state::ImportContext;
use crate::ui::components::import::SearchSource;
use bae_core::discogs::client::DiscogsSearchParams;
use bae_core::import::cover_art::fetch_cover_art_from_archive;
use bae_core::import::MatchCandidate;
use bae_core::musicbrainz::{search_releases_with_params, ReleaseSearchParams};
use dioxus::prelude::*;
use tracing::{debug, info, warn};
impl ImportContext {
    /// General search by artist, album, year, label
    pub async fn search_general(
        &self,
        source: SearchSource,
        artist: String,
        album: String,
        year: String,
        label: String,
    ) -> Result<Vec<MatchCandidate>, String> {
        let metadata = self.detected_metadata().read().clone();
        match source {
            SearchSource::MusicBrainz => {
                let params = ReleaseSearchParams {
                    artist: non_empty(artist),
                    album: non_empty(album),
                    year: non_empty(year),
                    label: non_empty(label),
                    catalog_number: None,
                    barcode: None,
                    format: None,
                    country: None,
                };
                info!("🎵 MusicBrainz general search: {:?}", params);
                search_mb_and_rank(params, metadata).await
            }
            SearchSource::Discogs => {
                let client = self.get_discogs_client()?;
                let params = DiscogsSearchParams {
                    artist: non_empty(artist),
                    release_title: non_empty(album),
                    year: non_empty(year),
                    label: non_empty(label),
                    catno: None,
                    barcode: None,
                    format: None,
                    country: None,
                };
                info!("🔍 Discogs general search: {:?}", params);
                search_discogs_and_rank(&client, params, metadata).await
            }
        }
    }
    /// Search by catalog number only
    pub async fn search_by_catalog_number(
        &self,
        source: SearchSource,
        catalog_number: String,
    ) -> Result<Vec<MatchCandidate>, String> {
        let metadata = self.detected_metadata().read().clone();
        match source {
            SearchSource::MusicBrainz => {
                let params = ReleaseSearchParams {
                    artist: None,
                    album: None,
                    year: None,
                    label: None,
                    catalog_number: Some(catalog_number),
                    barcode: None,
                    format: None,
                    country: None,
                };
                info!("🎵 MusicBrainz catalog number search: {:?}", params);
                search_mb_and_rank(params, metadata).await
            }
            SearchSource::Discogs => {
                let client = self.get_discogs_client()?;
                let params = DiscogsSearchParams {
                    artist: None,
                    release_title: None,
                    year: None,
                    label: None,
                    catno: Some(catalog_number),
                    barcode: None,
                    format: None,
                    country: None,
                };
                info!("🔍 Discogs catalog number search: {:?}", params);
                search_discogs_and_rank(&client, params, metadata).await
            }
        }
    }
    /// Search by barcode only
    pub async fn search_by_barcode(
        &self,
        source: SearchSource,
        barcode: String,
    ) -> Result<Vec<MatchCandidate>, String> {
        let metadata = self.detected_metadata().read().clone();
        match source {
            SearchSource::MusicBrainz => {
                let params = ReleaseSearchParams {
                    artist: None,
                    album: None,
                    year: None,
                    label: None,
                    catalog_number: None,
                    barcode: Some(barcode),
                    format: None,
                    country: None,
                };
                info!("🎵 MusicBrainz barcode search: {:?}", params);
                search_mb_and_rank(params, metadata).await
            }
            SearchSource::Discogs => {
                let client = self.get_discogs_client()?;
                let params = DiscogsSearchParams {
                    artist: None,
                    release_title: None,
                    year: None,
                    label: None,
                    catno: None,
                    barcode: Some(barcode),
                    format: None,
                    country: None,
                };
                info!("🔍 Discogs barcode search: {:?}", params);
                search_discogs_and_rank(&client, params, metadata).await
            }
        }
    }
}
/// Convert empty string to None
fn non_empty(s: String) -> Option<String> {
    if s.trim().is_empty() {
        None
    } else {
        Some(s)
    }
}
/// Search MusicBrainz and rank results
async fn search_mb_and_rank(
    params: ReleaseSearchParams,
    metadata: Option<bae_core::import::FolderMetadata>,
) -> Result<Vec<MatchCandidate>, String> {
    match search_releases_with_params(&params).await {
        Ok(releases) => {
            info!("✓ MusicBrainz search returned {} result(s)", releases.len());
            for (i, release) in releases.iter().enumerate().take(5) {
                info!(
                    "   {}. {} - {} (release_id: {})",
                    i + 1,
                    release.artist,
                    release.title,
                    release.release_id
                );
            }
            let mut candidates = if let Some(ref meta) = metadata {
                use bae_core::import::rank_mb_matches;
                rank_mb_matches(meta, releases)
            } else {
                releases
                    .into_iter()
                    .map(|release| MatchCandidate {
                        source: bae_core::import::MatchSource::MusicBrainz(release),
                        confidence: 50.0,
                        match_reasons: vec!["Manual search result".to_string()],
                        cover_art_url: None,
                    })
                    .collect()
            };
            let cover_art_futures: Vec<_> = candidates
                .iter()
                .map(|candidate| {
                    let release_id = match &candidate.source {
                        bae_core::import::MatchSource::MusicBrainz(release) => {
                            release.release_id.clone()
                        }
                        _ => String::new(),
                    };
                    async move {
                        if !release_id.is_empty() {
                            debug!("Fetching cover art for release {}", release_id);
                            fetch_cover_art_from_archive(&release_id).await
                        } else {
                            None
                        }
                    }
                })
                .collect();
            let cover_art_results = futures::future::join_all(cover_art_futures).await;
            for (candidate, cover_url) in candidates.iter_mut().zip(cover_art_results.into_iter()) {
                if candidate.cover_art_url.is_none() {
                    candidate.cover_art_url = cover_url;
                }
            }
            Ok(candidates)
        }
        Err(e) => {
            warn!("✗ MusicBrainz search failed: {}", e);
            Err(format!("MusicBrainz search failed: {}", e))
        }
    }
}
/// Search Discogs and rank results
async fn search_discogs_and_rank(
    client: &bae_core::discogs::client::DiscogsClient,
    params: DiscogsSearchParams,
    metadata: Option<bae_core::import::FolderMetadata>,
) -> Result<Vec<MatchCandidate>, String> {
    match client.search_with_params(&params).await {
        Ok(results) => {
            info!("✓ Discogs search returned {} result(s)", results.len());
            let candidates = if let Some(ref meta) = metadata {
                use bae_core::import::rank_discogs_matches;
                rank_discogs_matches(meta, results)
            } else {
                results
                    .into_iter()
                    .map(|result| MatchCandidate {
                        source: bae_core::import::MatchSource::Discogs(result),
                        confidence: 50.0,
                        match_reasons: vec!["Manual search result".to_string()],
                        cover_art_url: None,
                    })
                    .collect()
            };
            Ok(candidates)
        }
        Err(e) => {
            warn!("✗ Discogs search failed: {}", e);
            Err(format!("Discogs search failed: {}", e))
        }
    }
}
