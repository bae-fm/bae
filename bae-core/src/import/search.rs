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
use crate::import::parse_year;
use crate::import::types::MetadataSource;
use crate::import::ImportError;
use crate::musicbrainz::{self, MbReleaseResponse, ReleaseSearchParams, SearchRelease};
use crate::retry::retry_with_backoff;
use crate::signals::LookupFailure;

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
    /// Raw position string as the metadata source reports it ("A1", "1",
    /// "1-2", or arbitrary prose like "Bonus"). The import preview shows it
    /// verbatim; it is not the structured library-display position.
    pub position: String,
    pub side: u32,
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
        Some((artist, album)) => (artist.map(str::to_string), album.to_string()),
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

fn mb_release_to_metadata(r: MbReleaseResponse, cover_art: Option<RemoteCover>) -> MetadataResult {
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

/// Search MusicBrainz for metadata matching the provider params.
pub async fn search_mb(
    cover_art_archive: &CoverArtArchiveClient,
    params: ReleaseSearchParams,
) -> Result<Vec<MetadataResult>, ImportError> {
    let releases = retry_with_backoff(3, "MusicBrainz search", || {
        musicbrainz::search_releases_with_params(&params)
    })
    .await?;

    Ok(musicbrainz_releases_to_metadata(cover_art_archive, releases).await)
}

/// Search Discogs for metadata matching the provider params.
pub async fn search_discogs(
    client: &DiscogsClient,
    params: DiscogsSearchParams,
) -> Result<Vec<MetadataResult>, DiscogsError> {
    let results = client.search_with_params(&params).await?;
    Ok(results
        .into_iter()
        .map(discogs_search_result_to_metadata)
        .collect())
}

/// Map a MusicBrainz wire failure to the typed `LookupFailure` the identify
/// pipeline carries. The wire-level variants pass through structured (the HTTP
/// status is preserved); a local/internal MB error becomes opaque `Diagnostic`
/// detail. `NotFound` never reaches here — callers map it to "no matches"
/// before this is called.
fn mb_error_to_lookup_failure(e: musicbrainz::MusicBrainzError) -> LookupFailure {
    use musicbrainz::MusicBrainzError;
    match e {
        MusicBrainzError::Network(_) => LookupFailure::Network,
        MusicBrainzError::Timeout => LookupFailure::Timeout,
        MusicBrainzError::Provider { status } => LookupFailure::Provider { status },
        MusicBrainzError::NotFound(_) | MusicBrainzError::Other(_) => LookupFailure::Diagnostic {
            detail: e.to_string(),
        },
    }
}

/// Lookup releases by MusicBrainz disc ID.
pub async fn lookup_by_discid(
    cover_art_archive: &CoverArtArchiveClient,
    discid: &str,
) -> Result<DiscIdResult, LookupFailure> {
    let result = retry_with_backoff(3, "MusicBrainz DiscID lookup", || {
        musicbrainz::lookup_by_discid(discid)
    })
    .await;

    match result {
        Ok(releases) => {
            if releases.is_empty() {
                return Ok(DiscIdResult::NoMatches);
            }

            let release_ids: Vec<String> = releases.iter().map(|r| r.id.clone()).collect();
            let covers = search_covers(cover_art_archive, &release_ids).await;

            let results: Vec<MetadataResult> = releases
                .into_iter()
                .zip(covers)
                .map(|(r, cover_art)| mb_release_to_metadata(r, cover_art))
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
        Err(e) => Err(mb_error_to_lookup_failure(e)),
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
) -> Result<ImportSearchReleaseDetail, ImportError> {
    let year = parse_year(mb_response.date.as_deref());

    let mut side_base: u32 = 0;
    let mut tracks: Vec<ReleaseTrack> = Vec::new();

    for medium in &mb_response.media {
        let sides = crate::import::musicbrainz_mapper::medium_sides(release_id, medium)?;

        for (t, &side_offset) in medium.tracks.iter().zip(&sides.offsets) {
            let side = side_base + side_offset + 1;

            tracks.push(ReleaseTrack {
                title: t
                    .title
                    .clone()
                    .or_else(|| t.recording.as_ref().and_then(|r| r.title.clone()))
                    .unwrap_or_default(),
                artist: t.artist_credit.first().map(|ac| ac.name.clone()),
                duration_ms: t.length,
                position: t
                    .number
                    .clone()
                    .unwrap_or_else(|| t.position.map(|p| p.to_string()).unwrap_or_default()),
                side,
            });
        }

        side_base += sides.side_span;
    }

    let format = mb_response.media.first().and_then(|m| m.format.clone());
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

    Ok(ImportSearchReleaseDetail {
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
    })
}

/// Prefetch path for MusicBrainz: pure MB fetch + picker/confirm detail. No
/// Discogs cross-ref (the picker doesn't render any cross-source data) and
/// no DB-shape mapping. Pairs with `commit_mb_release`, which composes the
/// same `fetch_mb_response` with cross-ref enrichment and DB mapping.
pub async fn prefetch_mb_release(
    cover_art_archive: &CoverArtArchiveClient,
    release_id: &str,
) -> Result<ImportSearchReleaseDetail, ImportError> {
    let (response, _, _) = crate::import::musicbrainz_mapper::fetch_mb_response(release_id).await?;
    let release_group_id = response.release_group.as_ref().map(|rg| rg.id.as_str());
    let cover_art = cover_art_archive
        .fetch_candidates(Some(response.id.as_str()), release_group_id)
        .await;
    build_mb_detail(release_id, &response, cover_art)
}

/// Commit-ready release data: the parsed DB-shape album/release/tracks plus
/// the raw JSON pairs for archival. Produced by `commit_mb_release` /
/// `commit_discogs_release` on the worker side. Prefetch returns
/// `ImportSearchReleaseDetail` directly and never builds this struct — the
/// picker doesn't need the DB shape.
#[derive(Debug, Clone)]
pub struct PreparedRelease {
    pub parsed: crate::import::ParsedAlbum,
    /// `(source_name, raw_json)` pairs for the `release_metadata` table.
    pub metadata_pairs: Vec<(String, String)>,
}

/// Commit path for MusicBrainz: fetch + Discogs cross-ref + map to DB shape.
/// Cross-ref runs only here because pressing-level Discogs IDs are commit-only
/// — the picker never reads them. The worker consumes `parsed` and
/// `metadata_pairs`; no picker detail is built.
pub async fn commit_mb_release(
    library_manager: &crate::library::LibraryManager,
    release_id: &str,
    discogs_client: Option<&DiscogsClient>,
) -> Result<PreparedRelease, ImportError> {
    let (response, discogs_url, mut metadata_pairs) =
        crate::import::musicbrainz_mapper::fetch_mb_response(release_id).await?;
    let discogs_release = match (discogs_client, discogs_url.as_deref()) {
        (Some(client), Some(url)) => {
            match crate::discogs::client::fetch_discogs_xref(client, url).await {
                Some((release, pairs)) => {
                    metadata_pairs.extend(pairs);
                    Some(release)
                }
                None => None,
            }
        }
        _ => None,
    };
    let parsed = crate::import::musicbrainz_mapper::map_mb_response_to_db(
        &response,
        None,
        discogs_release,
        library_manager.clock().as_ref(),
        library_manager.ids().as_ref(),
    )?;
    Ok(PreparedRelease {
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

    let tracks: Vec<ReleaseTrack> = processed
        .iter()
        .map(|pt| {
            let source_track_index = pt
                .source_track_indices
                .first()
                .expect("processed Discogs track has a source track");
            let source_track = &release.tracklist[*source_track_index];
            let duration_ms = source_track
                .duration
                .as_ref()
                .and_then(|d| parse_duration_to_ms(d));
            ReleaseTrack {
                title: pt.title.clone(),
                artist: source_track.artists.first().map(|a| a.name.clone()),
                duration_ms,
                position: pt.position.clone(),
                side: pt.side as u32,
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
) -> Result<ImportSearchReleaseDetail, ImportError> {
    let (discogs_release, _master_year, _metadata_pairs) =
        crate::import::commit::fetch_discogs_release(client, release_id).await?;
    Ok(build_discogs_detail(&discogs_release))
}

/// Commit path for Discogs: fetch + MB cross-ref + map to DB shape.
/// Cross-ref runs only here because pressing-level MB IDs are commit-only
/// — the picker never reads them. The worker consumes `parsed` and
/// `metadata_pairs`; no picker detail is built.
pub async fn commit_discogs_release(
    client: &DiscogsClient,
    release_id: &str,
    clock: &dyn coven::Clock,
    ids: &dyn coven::IdProvider,
) -> Result<PreparedRelease, ImportError> {
    let (parsed, metadata_pairs) =
        crate::import::commit::fetch_and_map_discogs(client, release_id, clock, ids).await?;
    Ok(PreparedRelease {
        parsed,
        metadata_pairs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discogs::client::DiscogsSearchResult;
    use crate::discogs::{DiscogsArtist, DiscogsRelease, DiscogsTrack};
    use crate::import::{IdentityChoice, MetadataRef};
    use crate::musicbrainz::{
        MbArtistCredit, MbMedium, MbRecording, MbReleaseGroupRef, MbReleaseResponse, MbTrack,
    };
    use coven::{FixedClock, SequentialIdProvider};

    fn test_clock() -> FixedClock {
        FixedClock(
            chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
        )
    }

    fn make_mb_track(number: &str, title: &str) -> MbTrack {
        MbTrack {
            position: None,
            number: Some(number.to_string()),
            title: None,
            length: None,
            recording: Some(MbRecording {
                id: None,
                title: Some(title.to_string()),
                artist_credit: vec![],
                relations: vec![],
            }),
            artist_credit: vec![],
        }
    }

    fn response_with_media(media: Vec<MbMedium>) -> MbReleaseResponse {
        MbReleaseResponse {
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
            media,
            relations: vec![],
        }
    }

    fn result_with_title(title: &str) -> DiscogsSearchResult {
        DiscogsSearchResult {
            id: 1,
            title: title.to_string(),
            year: None,
            format: None,
            country: None,
            label: None,
            catno: None,
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
        let response = response_with_media(vec![MbMedium {
            format: Some("CD".to_string()),
            tracks: vec![make_mb_track("1", "Track Title")],
        }]);
        let cover_art = vec![RemoteCover {
            url: "https://caa.example/cover.jpg".to_string(),
            thumbnail_url: "https://caa.example/thumb.jpg".to_string(),
            label: MetadataSource::MusicBrainz.cover_source_label().to_string(),
            source: MetadataSource::MusicBrainz,
        }];

        let detail = build_mb_detail("mb-release-1", &response, cover_art.clone()).unwrap();

        assert_eq!(detail.cover_art, cover_art);
    }

    /// Vinyl side numbering runs continuously across media: medium 1 (A/B) is
    /// sides 1-2, medium 2 (C/D) is sides 3-4. Same shared assignment the DB
    /// mapper uses.
    #[test]
    fn mb_detail_numbers_vinyl_sides_across_media() {
        let response = response_with_media(vec![
            MbMedium {
                format: Some("12\" Vinyl".to_string()),
                tracks: vec![
                    make_mb_track("A1", "Track A1"),
                    make_mb_track("B1", "Track B1"),
                ],
            },
            MbMedium {
                format: Some("12\" Vinyl".to_string()),
                tracks: vec![
                    make_mb_track("C1", "Track C1"),
                    make_mb_track("D1", "Track D1"),
                ],
            },
        ]);

        let detail = build_mb_detail("mb-release-1", &response, vec![]).unwrap();
        let sides: Vec<u32> = detail.tracks.iter().map(|t| t.side).collect();
        assert_eq!(sides, vec![1, 2, 3, 4]);
    }

    #[test]
    fn parse_duration_to_ms_handles_mm_ss_and_hh_mm_ss() {
        // (input, expected)
        let ok: &[(&str, u64)] = &[
            ("0:00", 0),
            ("3:45", 225_000),
            ("59:59", 3_599_000),
            ("1:02:03", 3_723_000),
            ("0:00:30", 30_000),
        ];
        for (input, expected) in ok {
            assert_eq!(parse_duration_to_ms(input), Some(*expected), "{input}");
        }

        // Wrong shape or non-numeric parts yield None.
        for input in ["", "45", "3:45:67:89", "a:b", "3:xy", ":", "1::2"] {
            assert_eq!(parse_duration_to_ms(input), None, "{input}");
        }
    }

    /// A multi-side medium track without a leading side letter is malformed MB
    /// data. The search detail path propagates the error rather than bucketing
    /// the track onto side 0.
    #[test]
    fn mb_detail_errors_on_multi_side_track_without_side_letter() {
        let response = response_with_media(vec![MbMedium {
            format: Some("12\" Vinyl".to_string()),
            tracks: vec![
                make_mb_track("A1", "Track A1"),
                make_mb_track("1", "Numeric-only Track"),
            ],
        }]);

        let err = build_mb_detail("mb-release-1", &response, vec![])
            .expect_err("expected error for vinyl track without side letter");
        assert!(
            matches!(&err, ImportError::SourceData { detail, .. } if detail.contains("no side letter")),
            "unexpected error: {}",
            err
        );
    }

    /// Assert the commit plane and the editor-seed plane assign identical
    /// `(side, track_number)` to every track: the commit rows from
    /// `map_*_to_db` and the editor seed from a detail →
    /// `shape_user_edit_from_search_detail(Exact)` line up position for
    /// position.
    fn assert_planes_agree_on_numbering(
        parsed: &crate::import::ParsedAlbum,
        detail: &ImportSearchReleaseDetail,
        release_ref: MetadataRef,
    ) {
        let user_edit = crate::import::shape_user_edit_from_search_detail(
            detail,
            &IdentityChoice::Exact { release_ref },
        );
        let commit: Vec<(i32, Option<i32>)> = parsed
            .tracks
            .iter()
            .map(|t| (t.side, t.track_number))
            .collect();
        let seed: Vec<(i32, Option<i32>)> = user_edit
            .tracks
            .iter()
            .map(|t| (t.side, t.track_number))
            .collect();
        assert_eq!(commit, seed);
    }

    /// The commit plane (`map_mb_response_to_db`) and the editor-seed plane
    /// (`build_mb_detail` → `shape_user_edit_from_search_detail`) agree on
    /// `(side, track_number)` for every track of a multi-side vinyl release:
    /// both number through `per_side_positions` over side values from the same
    /// `medium_sides` derivation.
    #[test]
    fn mb_commit_and_editor_seed_agree_on_side_and_number() {
        let response = response_with_media(vec![
            MbMedium {
                format: Some("12\" Vinyl".to_string()),
                tracks: vec![
                    make_mb_track("A1", "Track A1"),
                    make_mb_track("A2", "Track A2"),
                    make_mb_track("B1", "Track B1"),
                ],
            },
            MbMedium {
                format: Some("12\" Vinyl".to_string()),
                tracks: vec![
                    make_mb_track("C1", "Track C1"),
                    make_mb_track("D1", "Track D1"),
                ],
            },
        ]);
        let clock = test_clock();
        let ids = SequentialIdProvider::new("mb");
        let parsed = crate::import::musicbrainz_mapper::map_mb_response_to_db(
            &response, None, None, &clock, &ids,
        )
        .unwrap();
        let detail = build_mb_detail("mb-release-1", &response, vec![]).unwrap();

        assert_planes_agree_on_numbering(
            &parsed,
            &detail,
            MetadataRef::new("mb-release-1", MetadataSource::MusicBrainz),
        );
    }

    /// The same numbering agreement over a multi-disc Discogs release, whose
    /// sides come from `process_tracklist`.
    #[test]
    fn discogs_commit_and_editor_seed_agree_on_side_and_number() {
        let make_track = |position: &str, title: &str| DiscogsTrack {
            position: position.to_string(),
            title: title.to_string(),
            duration: None,
            artists: vec![],
            type_: "track".to_string(),
            extraartists: None,
        };
        let release = DiscogsRelease {
            id: "discogs-release-1".to_string(),
            title: "Album Title".to_string(),
            year: Some(2024),
            format: vec![],
            country: None,
            label: vec![],
            cover_image: None,
            thumb: None,
            catno: None,
            artists: vec![DiscogsArtist {
                id: "artist-1".to_string(),
                name: "Artist Name".to_string(),
            }],
            extraartists: Some(vec![]),
            tracklist: vec![
                make_track("1-1", "Disc 1 Track 1"),
                make_track("1-2", "Disc 1 Track 2"),
                make_track("2-1", "Disc 2 Track 1"),
                make_track("2-2", "Disc 2 Track 2"),
            ],
            master_id: None,
        };
        let clock = test_clock();
        let ids = SequentialIdProvider::new("d");
        let parsed =
            crate::import::discogs_mapper::map_discogs_to_db(&release, None, None, &clock, &ids)
                .unwrap();
        let detail = build_discogs_detail(&release);

        assert_planes_agree_on_numbering(
            &parsed,
            &detail,
            MetadataRef::new("discogs-release-1", MetadataSource::Discogs),
        );
    }
}
