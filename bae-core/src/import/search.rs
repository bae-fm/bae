//! Metadata search and prefetch orchestration.
//!
//! These functions coordinate searching MusicBrainz and Discogs for release
//! metadata, checking Cover Art Archive for thumbnails, and fetching full
//! release details for the import confirmation step.
//!
//! Extracted from the bridge layer to keep all import-related business logic
//! in bae-core.

use crate::discogs::client::{DiscogsClient, DiscogsError, DiscogsSearchParams};
use crate::import::cover_art::{CoverArtArchiveClient, RemoteCover};
use crate::import::types::MetadataSource;
use crate::musicbrainz::{self, DiscIdRelease, ReleaseSearchParams, SearchRelease};
use crate::retry::retry_with_backoff;
use crate::util::format::compute_track_labels;
use tracing::warn;

/// A unified metadata search result from either MusicBrainz or Discogs.
///
/// This is a core type that both the bridge and desktop layers can convert
/// to their own display types.
///
/// `source_group_id` carries the per-source group: MB release-group ID for
/// MusicBrainz results, Discogs master ID for Discogs results. `None` when
/// the search result didn't surface a group.
#[derive(Debug, Clone)]
pub struct MetadataResult {
    pub source: MetadataSource,
    pub release_id: String,
    pub title: String,
    pub artist: Option<String>,
    pub year: Option<i32>,
    pub format: Option<String>,
    pub label: Option<String>,
    pub catalog_number: Option<String>,
    pub country: Option<String>,
    pub cover_art: Option<RemoteCover>,
    pub source_group_id: Option<String>,
}

impl From<&MetadataResult> for crate::db::LibraryCheck {
    fn from(r: &MetadataResult) -> Self {
        crate::db::LibraryCheck {
            release_id: r.release_id.clone(),
            source: r.source,
            source_group_id: r.source_group_id.clone(),
        }
    }
}

/// Full release details for the confirmation step.
///
/// `country` and `barcode` are pressing-level fields surfaced for the
/// edit-metadata form — the user can review or override them before
/// commit. `source_group_id` carries the per-source group (MB
/// release-group ID or Discogs master ID) so the UI can build a
/// `ReleaseIdentity` row directly from the picked release without a
/// second fetch.
#[derive(Debug, Clone)]
pub struct ImportSearchReleaseDetail {
    pub release_id: String,
    pub source: MetadataSource,
    pub source_group_id: Option<String>,
    pub title: String,
    pub artist: Option<String>,
    pub year: Option<i32>,
    pub format: Option<String>,
    pub label: Option<String>,
    pub catalog_number: Option<String>,
    pub country: Option<String>,
    pub barcode: Option<String>,
    pub track_count: u32,
    pub tracks: Vec<ReleaseTrack>,
    pub cover_art: Vec<RemoteCover>,
}

impl ImportSearchReleaseDetail {
    /// Does the source's `track_count` disagree with the user's local
    /// track count? `None` (count not known yet) returns `false`: can't
    /// mismatch against an unknown.
    pub fn track_count_mismatch(&self, local_track_count: Option<u32>) -> bool {
        local_track_count.is_some_and(|local| self.track_count != local)
    }

    pub fn default_cover(&self) -> Option<&RemoteCover> {
        self.cover_art.first()
    }
}

/// A track within a release detail.
#[derive(Debug, Clone)]
pub struct ReleaseTrack {
    pub title: String,
    pub artist: Option<String>,
    pub duration_ms: Option<u64>,
    /// Pre-formatted duration, e.g. "3:07".
    pub duration_label: String,
    pub position: String,
    pub side: u32,
    /// Human-readable side label: "Side A", "Disc 2", or empty for single-side digital
    pub side_label: String,
    /// Human-readable track position: "A1", "1", "1-2", etc.
    pub position_label: String,
}

/// Disc ID lookup result.
#[derive(Debug)]
pub enum DiscIdResult {
    NoMatches,
    SingleMatch(Box<MetadataResult>),
    MultipleMatches(Vec<MetadataResult>),
}

async fn search_covers(
    cover_art_archive: &CoverArtArchiveClient,
    release_ids: &[String],
) -> Vec<Option<RemoteCover>> {
    futures::future::join_all(
        release_ids
            .iter()
            .map(|release_id| cover_art_archive.fetch_release(release_id)),
    )
    .await
}

/// Convert a Discogs search result to a MetadataResult.
pub fn discogs_search_result_to_metadata(
    r: crate::discogs::client::DiscogsSearchResult,
) -> MetadataResult {
    // Search titles use "Artist - Album"; split once. No separator means the
    // whole title is the album and the artist is unknown.
    let (artist, album) = match crate::discogs::split_title(&r.title) {
        Some((artist, album)) => {
            let artist = if artist.is_empty() {
                None
            } else {
                Some(artist.to_string())
            };
            (artist, album.to_string())
        }
        None => (None, r.title.clone()),
    };
    let year = r.year.as_ref().and_then(|y| y.parse::<i32>().ok());
    let format = r.format.as_ref().map(|f| f.join(", "));
    let label = r.label.as_ref().and_then(|l| l.first().cloned());
    let cover_art = r.remote_cover();
    let source_group_id = r.master_id.map(|id| id.to_string());
    MetadataResult {
        source: MetadataSource::Discogs,
        release_id: r.id.to_string(),
        title: album,
        artist,
        year,
        format,
        label,
        catalog_number: r.catno,
        country: r.country,
        cover_art,
        source_group_id,
    }
}

fn parse_year(date: Option<&str>) -> Option<i32> {
    date?.split('-').next()?.parse().ok()
}

fn disc_id_release_to_metadata(r: DiscIdRelease, cover_art: Option<RemoteCover>) -> MetadataResult {
    let (label, catalog_number) = r
        .label_info
        .first()
        .map(|li| {
            (
                li.label.as_ref().and_then(|l| l.name.clone()),
                li.catalog_number.clone(),
            )
        })
        .unwrap_or((None, None));
    MetadataResult {
        source: MetadataSource::MusicBrainz,
        release_id: r.id,
        title: r.title,
        artist: r.artist_credit.first().map(|ac| ac.name.clone()),
        year: parse_year(r.date.as_deref()),
        format: r.media.first().and_then(|m| m.format.clone()),
        label,
        catalog_number,
        country: r.country,
        cover_art,
        source_group_id: r.release_group.as_ref().map(|rg| rg.id.clone()),
    }
}

fn search_release_to_metadata(r: SearchRelease, cover_art: Option<RemoteCover>) -> MetadataResult {
    let (label, catalog_number) = r
        .label_info
        .first()
        .map(|li| {
            (
                li.label.as_ref().and_then(|l| l.name.clone()),
                li.catalog_number.clone(),
            )
        })
        .unwrap_or((None, None));
    MetadataResult {
        source: MetadataSource::MusicBrainz,
        release_id: r.id,
        title: r.title,
        artist: r.artist_credit.first().map(|ac| ac.name.clone()),
        year: parse_year(r.date.as_deref()),
        format: None,
        label,
        catalog_number,
        country: r.country,
        cover_art,
        source_group_id: r.release_group.as_ref().map(|rg| rg.id.clone()),
    }
}

async fn musicbrainz_releases_to_metadata(
    cover_art_archive: &CoverArtArchiveClient,
    releases: Vec<SearchRelease>,
) -> Vec<MetadataResult> {
    let release_ids: Vec<String> = releases.iter().map(|r| r.id.clone()).collect();
    let covers = search_covers(cover_art_archive, &release_ids).await;

    releases
        .into_iter()
        .zip(covers)
        .map(|(r, cover_art)| search_release_to_metadata(r, cover_art))
        .collect()
}

/// Search MusicBrainz for metadata matching given criteria.
/// Includes cover art checks from the Cover Art Archive.
pub async fn search_musicbrainz(
    cover_art_archive: &CoverArtArchiveClient,
    artist: String,
    album: String,
    year: Option<String>,
    label: Option<String>,
) -> Result<Vec<MetadataResult>, String> {
    let params = ReleaseSearchParams {
        artist: Some(artist),
        album: Some(album),
        year,
        label,
        ..Default::default()
    };
    let releases = retry_with_backoff(3, "MusicBrainz search", || {
        musicbrainz::search_releases_with_params(&params)
    })
    .await
    .map_err(|e| format!("MusicBrainz search failed: {e}"))?;

    Ok(musicbrainz_releases_to_metadata(cover_art_archive, releases).await)
}

/// Search Discogs for metadata matching given criteria.
pub async fn search_discogs(
    client: &DiscogsClient,
    artist: String,
    album: String,
    year: Option<String>,
    label: Option<String>,
) -> Result<Vec<MetadataResult>, DiscogsError> {
    let params = DiscogsSearchParams {
        artist: Some(artist),
        release_title: Some(album),
        year,
        label,
        ..Default::default()
    };
    let results = client.search_with_params(&params).await?;
    Ok(results
        .into_iter()
        .map(discogs_search_result_to_metadata)
        .collect())
}

/// Search by catalog number on MusicBrainz.
pub async fn search_mb_by_catalog_number(
    cover_art_archive: &CoverArtArchiveClient,
    catalog_number: String,
) -> Result<Vec<MetadataResult>, String> {
    let params = ReleaseSearchParams {
        catalog_number: Some(catalog_number),
        ..Default::default()
    };
    let releases = retry_with_backoff(3, "MusicBrainz catalog search", || {
        musicbrainz::search_releases_with_params(&params)
    })
    .await
    .map_err(|e| format!("MusicBrainz search failed: {e}"))?;

    Ok(musicbrainz_releases_to_metadata(cover_art_archive, releases).await)
}

/// Search by catalog number on Discogs.
pub async fn search_discogs_by_catalog_number(
    client: &DiscogsClient,
    catalog_number: String,
) -> Result<Vec<MetadataResult>, DiscogsError> {
    let params = DiscogsSearchParams {
        catno: Some(catalog_number),
        ..Default::default()
    };
    let results = client.search_with_params(&params).await?;
    Ok(results
        .into_iter()
        .map(discogs_search_result_to_metadata)
        .collect())
}

/// Search by barcode on MusicBrainz.
pub async fn search_mb_by_barcode(
    cover_art_archive: &CoverArtArchiveClient,
    barcode: String,
) -> Result<Vec<MetadataResult>, String> {
    let params = ReleaseSearchParams {
        barcode: Some(barcode),
        ..Default::default()
    };
    let releases = retry_with_backoff(3, "MusicBrainz barcode search", || {
        musicbrainz::search_releases_with_params(&params)
    })
    .await
    .map_err(|e| format!("MusicBrainz search failed: {e}"))?;

    Ok(musicbrainz_releases_to_metadata(cover_art_archive, releases).await)
}

/// Search by barcode on Discogs.
pub async fn search_discogs_by_barcode(
    client: &DiscogsClient,
    barcode: String,
) -> Result<Vec<MetadataResult>, DiscogsError> {
    let params = DiscogsSearchParams {
        barcode: Some(barcode),
        ..Default::default()
    };
    let results = client.search_with_params(&params).await?;
    Ok(results
        .into_iter()
        .map(discogs_search_result_to_metadata)
        .collect())
}

/// Lookup releases by MusicBrainz disc ID.
pub async fn lookup_by_discid(
    cover_art_archive: &CoverArtArchiveClient,
    discid: &str,
) -> Result<DiscIdResult, String> {
    let result = retry_with_backoff(3, "MusicBrainz DiscID lookup", || {
        musicbrainz::lookup_by_discid(discid)
    })
    .await;

    match result {
        Ok((releases, _external_urls)) => {
            if releases.is_empty() {
                return Ok(DiscIdResult::NoMatches);
            }

            let release_ids: Vec<String> = releases.iter().map(|r| r.id.clone()).collect();
            let covers = search_covers(cover_art_archive, &release_ids).await;

            let results: Vec<MetadataResult> = releases
                .into_iter()
                .zip(covers)
                .map(|(r, cover_art)| disc_id_release_to_metadata(r, cover_art))
                .collect();

            if results.len() == 1 {
                Ok(DiscIdResult::SingleMatch(Box::new(
                    results.into_iter().next().unwrap(),
                )))
            } else {
                Ok(DiscIdResult::MultipleMatches(results))
            }
        }
        Err(musicbrainz::MusicBrainzError::NotFound(_)) => Ok(DiscIdResult::NoMatches),
        Err(e) => Err(format!("DiscID lookup failed: {e}")),
    }
}

/// Parse a Discogs-style duration string ("3:45") to milliseconds.
pub fn parse_duration_to_ms(duration: &str) -> Option<u64> {
    let parts: Vec<&str> = duration.split(':').collect();
    match parts.len() {
        2 => {
            let mins: u64 = parts[0].parse().ok()?;
            let secs: u64 = parts[1].parse().ok()?;
            Some((mins * 60 + secs) * 1000)
        }
        3 => {
            let hours: u64 = parts[0].parse().ok()?;
            let mins: u64 = parts[1].parse().ok()?;
            let secs: u64 = parts[2].parse().ok()?;
            Some((hours * 3600 + mins * 60 + secs) * 1000)
        }
        _ => None,
    }
}

/// Build the UI-shaped `ImportSearchReleaseDetail` from a parsed MB
/// response. Called by `prepare_mb_release` after the MB fetch returns,
/// so the `PreparedRelease` carries the UI detail and the commit-side
/// parsed data in one pass.
fn build_mb_detail(
    release_id: &str,
    mb_response: &crate::musicbrainz::MbReleaseResponse,
    cover_art: Vec<RemoteCover>,
) -> ImportSearchReleaseDetail {
    let year = parse_year(mb_response.date.as_deref());

    let mut side_base: u32 = 0;
    let mut tracks: Vec<ReleaseTrack> = Vec::new();

    for medium in &mb_response.media {
        let is_multi_side = medium
            .format
            .as_deref()
            .is_some_and(|f| f.contains("Vinyl") || f.contains("Cassette"));

        let mut max_side_offset: u32 = 0;

        let medium_base_letter = if is_multi_side {
            medium
                .tracks
                .iter()
                .filter_map(|t| t.number.as_deref()?.chars().next())
                .filter(|c| c.is_ascii_alphabetic())
                .map(|c| c.to_ascii_uppercase() as u32)
                .min()
                .unwrap_or('A' as u32)
        } else {
            'A' as u32
        };

        for t in &medium.tracks {
            let side_offset = if is_multi_side {
                let parsed = t
                    .number
                    .as_deref()
                    .and_then(|n| n.chars().next())
                    .filter(|c| c.is_ascii_alphabetic())
                    .map(|c| (c.to_ascii_uppercase() as u32) - medium_base_letter);
                match parsed {
                    Some(offset) => offset,
                    None => {
                        warn!(
                            release_id = release_id,
                            track_number = ?t.number,
                            "multi-side medium track missing side letter; defaulting to side 0"
                        );
                        0
                    }
                }
            } else {
                0
            };
            if side_offset > max_side_offset {
                max_side_offset = side_offset;
            }

            let side = side_base + side_offset + 1;

            tracks.push(ReleaseTrack {
                title: t
                    .title
                    .clone()
                    .or_else(|| t.recording.as_ref().and_then(|r| r.title.clone()))
                    .unwrap_or_default(),
                artist: t.artist_credit.first().map(|ac| ac.name.clone()),
                duration_ms: t.length,
                duration_label: crate::util::format::format_duration_label_unsigned(t.length),
                position: t
                    .number
                    .clone()
                    .unwrap_or_else(|| t.position.map(|p| p.to_string()).unwrap_or_default()),
                side,
                side_label: String::new(),
                position_label: String::new(),
            });
        }

        side_base += if is_multi_side {
            max_side_offset + 1
        } else {
            1
        };
    }

    let has_multiple_sides = tracks
        .iter()
        .map(|t| t.side)
        .collect::<std::collections::HashSet<_>>()
        .len()
        > 1;
    let format = mb_response.media.first().and_then(|m| m.format.clone());
    let format_str = format.as_deref();
    for track in &mut tracks {
        let (side_label, position_label) =
            compute_track_labels(format_str, track.side as i32, None, has_multiple_sides);
        track.side_label = side_label;
        track.position_label = if track.position.is_empty() {
            position_label
        } else {
            track.position.clone()
        };
    }

    let artist = mb_response.artist_credit.first().map(|ac| ac.name.clone());
    let (label, catalog_number) = mb_response
        .label_info
        .first()
        .map(|li| {
            (
                li.label.as_ref().and_then(|l| l.name.clone()),
                li.catalog_number.clone(),
            )
        })
        .unwrap_or((None, None));

    ImportSearchReleaseDetail {
        release_id: mb_response.id.clone(),
        source: MetadataSource::MusicBrainz,
        source_group_id: mb_response.release_group.as_ref().map(|rg| rg.id.clone()),
        title: mb_response.title.clone(),
        artist,
        year,
        format,
        label,
        catalog_number,
        country: mb_response.country.clone(),
        barcode: mb_response.barcode.clone(),
        track_count: tracks.len() as u32,
        tracks,
        cover_art,
    }
}

/// Prefetch path for MusicBrainz: pure MB fetch + picker/confirm detail. No
/// Discogs cross-ref (the picker doesn't render any cross-source data) and
/// no DB-shape mapping. Pairs with `commit_mb_release`, which composes the
/// same `fetch_mb_response` with cross-ref enrichment and DB mapping.
pub async fn prefetch_mb_release(
    cover_art_archive: &CoverArtArchiveClient,
    release_id: &str,
) -> Result<ImportSearchReleaseDetail, String> {
    let (response, _, _) = crate::import::musicbrainz_mapper::fetch_mb_response(release_id).await?;
    let release_group_id = response.release_group.as_ref().map(|rg| rg.id.as_str());
    let cover_art = cover_art_archive
        .fetch_candidates(Some(response.id.as_str()), release_group_id)
        .await;
    Ok(build_mb_detail(release_id, &response, cover_art))
}

/// Commit path for MusicBrainz: fetch + Discogs cross-ref + map to DB shape.
/// Cross-ref runs only here because pressing-level Discogs IDs are commit-only
/// — the picker never reads them. The worker consumes `parsed` and
/// `metadata_pairs`; no picker detail is built.
pub async fn commit_mb_release(
    library_manager: &crate::library::LibraryManager,
    release_id: &str,
    discogs_client: Option<&DiscogsClient>,
) -> Result<crate::import::folder_scanner::PreparedRelease, String> {
    let (response, external_urls, mut metadata_pairs) =
        crate::import::musicbrainz_mapper::fetch_mb_response(release_id).await?;
    let discogs_release = if let Some(client) = discogs_client {
        match crate::discogs::client::fetch_discogs_xref(client, &external_urls).await {
            Some((release, pairs)) => {
                metadata_pairs.extend(pairs);
                Some(release)
            }
            None => None,
        }
    } else {
        None
    };
    let parsed = crate::import::musicbrainz_mapper::map_mb_response_to_db(
        &response,
        None,
        discogs_release,
        library_manager.clock().as_ref(),
        library_manager.ids().as_ref(),
    )?;
    Ok(crate::import::folder_scanner::PreparedRelease {
        source: MetadataSource::MusicBrainz,
        release_id: release_id.to_string(),
        parsed,
        metadata_pairs,
    })
}

/// Build the UI-shaped `ImportSearchReleaseDetail` from a parsed
/// Discogs release. Shared between the prefetch shim and
/// `prepare_discogs_release`.
fn build_discogs_detail(release: &crate::discogs::DiscogsRelease) -> ImportSearchReleaseDetail {
    let processed = crate::import::discogs_mapper::process_tracklist(&release.tracklist);

    let format_string = if release.format.is_empty() {
        None
    } else {
        Some(release.format.join(", "))
    };
    let has_multiple_sides = processed
        .iter()
        .map(|pt| pt.side)
        .collect::<std::collections::HashSet<_>>()
        .len()
        > 1;

    let tracks: Vec<ReleaseTrack> = processed
        .iter()
        .map(|pt| {
            let source_track = release.tracklist.iter().find(|t| {
                pt.original_positions.iter().any(|p| p == &t.position) && t.type_ != "heading"
            });

            let (side_label, position_label) = compute_track_labels(
                format_string.as_deref(),
                pt.side,
                Some(pt.track_number),
                has_multiple_sides,
            );

            let duration_ms = source_track
                .and_then(|t| t.duration.as_ref())
                .and_then(|d| parse_duration_to_ms(d));
            ReleaseTrack {
                title: pt.title.clone(),
                artist: source_track.and_then(|t| t.artists.first().map(|a| a.name.clone())),
                duration_ms,
                duration_label: crate::util::format::format_duration_label_unsigned(duration_ms),
                position: pt.position.clone(),
                side: pt.side as u32,
                side_label,
                position_label,
            }
        })
        .collect();

    let mut cover_art = Vec::new();
    if let Some(cover) = release.remote_cover() {
        cover_art.push(cover);
    }

    let year = release.year.map(|y| y as i32);
    let artist = release
        .artists
        .iter()
        .map(|a| a.name.clone())
        .collect::<Vec<_>>()
        .join(", ");
    let artist = if artist.is_empty() {
        None
    } else {
        Some(artist)
    };

    ImportSearchReleaseDetail {
        release_id: release.id.clone(),
        source: MetadataSource::Discogs,
        source_group_id: release.master_id.clone(),
        title: release.title.clone(),
        artist,
        year,
        format: format_string,
        label: release.label.first().cloned(),
        catalog_number: release.catno.clone(),
        country: release.country.clone(),
        // The `DiscogsRelease` model doesn't carry a barcode field, so
        // the Discogs confirmation detail has no barcode to surface.
        barcode: None,
        track_count: tracks.len() as u32,
        tracks,
        cover_art,
    }
}

/// Prefetch path for Discogs: pure Discogs fetch + picker/confirm detail.
/// No MB cross-ref (the picker doesn't render any cross-source data) and
/// no DB-shape mapping. Pairs with `commit_discogs_release`, which composes
/// the same `fetch_discogs_release` with cross-ref enrichment and DB mapping.
pub async fn prefetch_discogs_release(
    client: &DiscogsClient,
    release_id: &str,
) -> Result<ImportSearchReleaseDetail, String> {
    let (discogs_release, _master_year, _metadata_pairs) =
        crate::import::commit::fetch_discogs_release(client, release_id)
            .await
            .map_err(|e| e.to_string())?;
    Ok(build_discogs_detail(&discogs_release))
}

/// Commit path for Discogs: fetch + MB cross-ref + map to DB shape.
/// Cross-ref runs only here because pressing-level MB IDs are commit-only
/// — the picker never reads them. The worker consumes `parsed` and
/// `metadata_pairs`; no picker detail is built.
pub async fn commit_discogs_release(
    client: &DiscogsClient,
    release_id: &str,
    clock: &dyn crate::clock::Clock,
    ids: &dyn crate::id_provider::IdProvider,
) -> Result<crate::import::folder_scanner::PreparedRelease, String> {
    let (parsed, metadata_pairs) =
        crate::import::commit::fetch_and_map_discogs(client, release_id, clock, ids)
            .await
            .map_err(|e| e.to_string())?;
    Ok(crate::import::folder_scanner::PreparedRelease {
        source: MetadataSource::Discogs,
        release_id: release_id.to_string(),
        parsed,
        metadata_pairs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discogs::client::DiscogsSearchResult;
    use crate::musicbrainz::{MbArtistCredit, MbMedium, MbReleaseGroupRef, MbReleaseResponse};

    fn result_with_title(title: &str) -> DiscogsSearchResult {
        DiscogsSearchResult {
            id: 1,
            title: title.to_string(),
            year: None,
            genre: None,
            style: None,
            format: None,
            country: None,
            label: None,
            catno: None,
            barcode: None,
            cover_image: None,
            thumb: None,
            master_id: None,
            result_type: "release".to_string(),
        }
    }

    #[test]
    fn discogs_title_splits_into_artist_and_album() {
        let m = discogs_search_result_to_metadata(result_with_title("Artist Name - Album Title"));
        assert_eq!(m.artist.as_deref(), Some("Artist Name"));
        assert_eq!(m.title, "Album Title");
    }

    #[test]
    fn discogs_title_without_separator_is_all_album() {
        let m = discogs_search_result_to_metadata(result_with_title("Just A Title"));
        assert_eq!(m.artist, None);
        assert_eq!(m.title, "Just A Title");
    }

    #[test]
    fn discogs_title_with_empty_artist_drops_to_none() {
        let m = discogs_search_result_to_metadata(result_with_title(" - Album Title"));
        assert_eq!(m.artist, None);
        assert_eq!(m.title, "Album Title");
    }

    #[test]
    fn discogs_search_result_carries_remote_cover_pair() {
        let mut result = result_with_title("Artist Name - Album Title");
        result.cover_image = Some("https://discogs.example/full.jpg".to_string());
        result.thumb = Some("https://discogs.example/thumb.jpg".to_string());

        let metadata = discogs_search_result_to_metadata(result);

        assert_eq!(
            metadata.cover_art,
            Some(RemoteCover {
                url: "https://discogs.example/full.jpg".to_string(),
                thumbnail_url: "https://discogs.example/thumb.jpg".to_string(),
                label: MetadataSource::Discogs.cover_source_label().to_string(),
                source: MetadataSource::Discogs,
            })
        );
    }

    #[test]
    fn mb_detail_uses_supplied_cover_art_archive_candidates() {
        let response = MbReleaseResponse {
            id: "mb-release-1".to_string(),
            title: "Album Title".to_string(),
            date: None,
            country: None,
            barcode: None,
            artist_credit: vec![MbArtistCredit {
                name: "Artist Name".to_string(),
                artist: None,
            }],
            release_group: Some(MbReleaseGroupRef {
                id: "mb-group-1".to_string(),
                first_release_date: None,
                relations: None,
            }),
            label_info: vec![],
            media: vec![MbMedium {
                format: Some("CD".to_string()),
                tracks: vec![],
            }],
            relations: vec![],
        };
        let cover_art = vec![RemoteCover {
            url: "https://caa.example/cover.jpg".to_string(),
            thumbnail_url: "https://caa.example/thumb.jpg".to_string(),
            label: MetadataSource::MusicBrainz.cover_source_label().to_string(),
            source: MetadataSource::MusicBrainz,
        }];

        let detail = build_mb_detail("mb-release-1", &response, cover_art.clone());

        assert_eq!(detail.cover_art, cover_art);
    }
}
