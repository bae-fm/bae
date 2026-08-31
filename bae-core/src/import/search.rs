//! Metadata search and prefetch orchestration: searching MusicBrainz and
//! Discogs for release metadata, checking Cover Art Archive for thumbnails, and
//! fetching full release details for the import confirmation step.

use crate::discogs::client::{DiscogsClient, DiscogsError, DiscogsSearchParams};
use crate::import::cover_art::RemoteCover;
use crate::import::parse_year;
use crate::import::types::MetadataSource;
use crate::import::ImportError;
use crate::musicbrainz::{self, MbReleaseResponse, ReleaseSearchParams, SearchRelease};
use crate::signals::LookupFailure;
use crate::util::rate_limiter::CallPriority;

/// A metadata search result from either MusicBrainz or Discogs.
///
/// `source_group_id` carries the per-source group — MB release-group ID or
/// Discogs master ID — and is `None` when the search result surfaced no group.
///
/// A verdict's matches are these, as fetched: one `import_candidate_match`
/// row each, read back unchanged.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
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
    /// What the source says about this release's own tracklist — the other half
    /// of the Ready rule, which admits a single match only when the source's
    /// count and total length agree with the candidate's.
    ///
    /// **`None` means nobody has asked yet** — not that the source has
    /// nothing. A result arrives this way from every lookup identification
    /// makes: the disc-ID endpoint carries track lengths but not the rest of
    /// what opening the candidate needs, and the search endpoint takes no `inc`
    /// and returns no `tracks` array at all. It is filled when the sweep settles
    /// the lead, from the release document that settling archives.
    ///
    /// Keeping "unasked" distinct from "asked, and there is nothing" is what
    /// lets a stored verdict say whether its lead was settled: the two are
    /// written together, so a `Some` here is also the readable marker that this
    /// release's documents are stored. Collapsing them would either strand a
    /// verdict at unverified forever or re-buy the same empty answer on every
    /// launch.
    pub source_tracks: Option<SourceTracks>,
}

/// What a source said about a release's tracklist, once something asked.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SourceTracks {
    /// It listed its tracks.
    Listed {
        count: u32,
        /// Sum of the tracks' lengths in milliseconds. `None` when any of them
        /// has no length: a partial sum understates the total and would read as
        /// a duration disagreement, which is a wrong answer where the honest
        /// one is "not known". A MusicBrainz release states lengths only when
        /// asked with `inc=recordings`, and a Discogs tracklist can carry
        /// untimed entries, so a count with no total is ordinary.
        total_duration_ms: Option<u64>,
    },
    /// It answered and listed nothing — a release id it has since merged away,
    /// or one with no media. There is nothing left to ask, so a verdict
    /// carrying this is finished rather than waiting on a top-up.
    Nothing,
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
/// `country` and `barcode` are pressing-level fields the user can review or
/// override in the edit-metadata form before commit. `source_group_id` carries
/// the per-source group (MB release-group ID or Discogs master ID) so the UI can
/// build a `ReleaseIdentity` row from the picked release without a second fetch.
#[derive(Debug, Clone, PartialEq)]
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
    pub fn default_cover(&self) -> Option<&RemoteCover> {
        self.cover_art.first()
    }
}

/// A track within a release detail.
#[derive(Debug, Clone, PartialEq)]
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
        // The Discogs search response describes no tracklist; a Discogs result
        // gets one only from a paid `get_release`.
        source_tracks: None,
    }
}

/// A MusicBrainz release's tracklist, as an `inc=recordings` response carries
/// it.
pub(crate) fn mb_source_tracks(r: &MbReleaseResponse) -> SourceTracks {
    let tracks: Vec<&crate::musicbrainz::MbTrack> =
        r.media.iter().flat_map(|medium| &medium.tracks).collect();
    if tracks.is_empty() {
        return SourceTracks::Nothing;
    }
    SourceTracks::Listed {
        count: tracks.len() as u32,
        total_duration_ms: tracks.iter().map(|t| t.length).sum::<Option<u64>>(),
    }
}

fn mb_release_to_metadata(r: MbReleaseResponse, cover_art: Option<RemoteCover>) -> MetadataResult {
    let pressing = crate::import::musicbrainz_mapper::pressing(&r);
    // Free here: this response came from an `inc=recordings` endpoint, so the
    // tracklist is already on the wire — a disc-ID match costs no second call to
    // reach the Ready rule.
    let source_tracks = Some(mb_source_tracks(&r));
    MetadataResult {
        source: MetadataSource::MusicBrainz,
        release_id: r.id,
        title: r.title,
        artist: r.artist_credit.first().map(|ac| ac.name.clone()),
        year: pressing.year,
        format: pressing.format,
        label: pressing.label,
        catalog_number: pressing.catalog_number,
        country: pressing.country,
        cover_art,
        source_group_id: r.release_group.as_ref().map(|rg| rg.id.clone()),
        source_tracks,
    }
}

fn search_release_to_metadata(r: SearchRelease, cover_art: Option<RemoteCover>) -> MetadataResult {
    let (label, catalog_number) = musicbrainz::label_and_catno(&r.label_info);
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
        // `ws/2/release?query=…` takes no `inc`, so its response carries no
        // `tracks` array to read a count or a length from.
        source_tracks: None,
    }
}

/// Search MusicBrainz for metadata matching the provider params.
///
/// Each result carries the archive's address for that release's front image.
/// The search endpoint takes no `inc` and returns no `cover-art-archive` block,
/// so nothing here states whether the archive holds one — the thumbnail fetch
/// the result card makes is what answers that, and it is the same request the
/// card would make anyway.
pub async fn search_mb(
    params: ReleaseSearchParams,
    priority: CallPriority,
) -> Result<Vec<MetadataResult>, ImportError> {
    let releases = musicbrainz::search_releases_with_params(&params, priority).await?;

    Ok(releases
        .into_iter()
        .map(|r| {
            let cover_art = RemoteCover::musicbrainz_release(&r.id);
            search_release_to_metadata(r, Some(cover_art))
        })
        .collect())
}

/// Search Discogs for metadata matching the provider params.
pub async fn search_discogs(
    client: &DiscogsClient,
    params: DiscogsSearchParams,
    priority: CallPriority,
) -> Result<Vec<MetadataResult>, DiscogsError> {
    let results = client.search_with_params(&params, priority).await?;
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
fn mb_error_to_lookup_failure(e: &musicbrainz::MusicBrainzError) -> LookupFailure {
    use musicbrainz::MusicBrainzError;
    match e {
        MusicBrainzError::Network(_) => LookupFailure::Network,
        MusicBrainzError::Timeout => LookupFailure::Timeout,
        MusicBrainzError::Provider { status } => LookupFailure::Provider { status: *status },
        MusicBrainzError::NotFound(_) | MusicBrainzError::Other(_) => LookupFailure::Diagnostic {
            detail: e.to_string(),
        },
    }
}

/// Preserve provider failures while lifting the import service's typed error
/// into the identify state machine.
pub(crate) fn import_error_to_lookup_failure(error: &ImportError) -> LookupFailure {
    match error {
        ImportError::MusicBrainz(error) => mb_error_to_lookup_failure(error),
        ImportError::Discogs(error) => match error {
            DiscogsError::Transport(error) if error.is_timeout() => LookupFailure::Timeout,
            DiscogsError::Transport(_) => LookupFailure::Network,
            DiscogsError::Provider(status) => LookupFailure::Provider {
                status: Some(status.as_u16()),
            },
            DiscogsError::RateLimit => LookupFailure::Provider { status: Some(429) },
            DiscogsError::InvalidApiKey => LookupFailure::Provider { status: Some(401) },
            DiscogsError::NotFound | DiscogsError::Serialization(_) => LookupFailure::Diagnostic {
                detail: error.to_string(),
            },
        },
        _ => LookupFailure::Diagnostic {
            detail: error.to_string(),
        },
    }
}

/// Combine every configured provider's answer. `None` means Discogs was not a
/// configured source; `Some(Err(_))` means it was part of this lookup and did
/// not answer, so the combined lookup is incomplete.
pub(crate) fn merge_provider_results(
    mut musicbrainz: Vec<MetadataResult>,
    discogs: Option<Result<Vec<MetadataResult>, LookupFailure>>,
) -> Result<Vec<MetadataResult>, LookupFailure> {
    if let Some(discogs) = discogs {
        musicbrainz.extend(discogs?);
    }
    Ok(musicbrainz)
}

/// The releases MusicBrainz has for a disc ID, each with its cover art. Empty
/// when the disc is unknown to MB — a settled lookup with no matches, which is
/// what `NotFound` means on this endpoint too.
pub async fn lookup_by_discid(
    discid: &str,
    priority: CallPriority,
) -> Result<Vec<MetadataResult>, LookupFailure> {
    let releases = match musicbrainz::lookup_by_discid(discid, priority).await {
        Ok(releases) => releases,
        Err(musicbrainz::MusicBrainzError::NotFound(_)) => return Ok(Vec::new()),
        Err(e) => return Err(mb_error_to_lookup_failure(&e)),
    };

    Ok(releases
        .into_iter()
        .map(|r| {
            // Unlike the search endpoint, this one returns whole release
            // documents, so each states whether the archive holds a front image
            // for it.
            let cover_art = r
                .has_front_cover()
                .then(|| RemoteCover::musicbrainz_release(&r.id));
            mb_release_to_metadata(r, cover_art)
        })
        .collect())
}

/// A Discogs release's tracklist. Headings and index entries are not tracks and
/// Nested index rows are expanded to their playable leaves when no local audio
/// layout is available. A playable row with no parseable duration leaves the
/// total unknown while the count still stands.
pub(crate) fn discogs_source_tracks(release: &crate::discogs::DiscogsRelease) -> SourceTracks {
    let tracks = crate::import::discogs_mapper::process_tracklist(&release.tracklist);
    source_tracks_from_discogs_layout(&tracks)
}

pub(crate) fn discogs_source_tracks_for_audio(
    release: &crate::discogs::DiscogsRelease,
    audio_durations_ms: &[u64],
) -> SourceTracks {
    let tracks = crate::import::discogs_mapper::process_tracklist_for_audio(
        &release.tracklist,
        audio_durations_ms,
    );
    source_tracks_from_discogs_layout(&tracks)
}

fn source_tracks_from_discogs_layout(
    tracks: &[crate::import::discogs_mapper::ProcessedTrack<'_>],
) -> SourceTracks {
    if tracks.is_empty() {
        return SourceTracks::Nothing;
    }
    SourceTracks::Listed {
        count: tracks.len() as u32,
        total_duration_ms: tracks.iter().map(|track| track.duration_ms).sum(),
    }
}

/// Parse a Discogs-style duration string ("3:45") to milliseconds.
pub fn parse_duration_to_ms(duration: &str) -> Option<u64> {
    crate::import::discogs_mapper::parse_duration_to_ms(duration)
}

/// Build the UI-shaped `ImportSearchReleaseDetail` from a parsed MB response,
/// for the picker and the confirmation pane.
pub(crate) fn build_mb_detail(
    release_id: &str,
    mb_response: &crate::musicbrainz::MbReleaseResponse,
    cover_art: Vec<RemoteCover>,
) -> Result<ImportSearchReleaseDetail, ImportError> {
    let mut side_base: u32 = 0;
    let mut tracks: Vec<ReleaseTrack> = Vec::new();

    for medium in &mb_response.media {
        let sides = crate::import::musicbrainz_mapper::medium_sides(release_id, medium)?;

        for (t, &side_offset) in medium.tracks.iter().zip(&sides.offsets) {
            let side = side_base + side_offset + 1;

            tracks.push(ReleaseTrack {
                title: crate::import::musicbrainz_mapper::track_title(release_id, t)?,
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

    // The detail's pressing fields are the release's pressing, read through the
    // same projection the commit maps — the picker shows what the import stores.
    let pressing = crate::import::musicbrainz_mapper::pressing(mb_response);
    let artist = mb_response.artist_credit.first().map(|ac| ac.name.clone());

    Ok(ImportSearchReleaseDetail {
        release_id: mb_response.id.clone(),
        source: MetadataSource::MusicBrainz,
        source_group_id: mb_response.release_group.as_ref().map(|rg| rg.id.clone()),
        title: mb_response.title.clone(),
        artist,
        year: pressing.year,
        format: pressing.format,
        label: pressing.label,
        catalog_number: pressing.catalog_number,
        country: pressing.country,
        barcode: pressing.barcode,
        track_count: tracks.len() as u32,
        tracks,
        cover_art,
    })
}

/// Build the UI-shaped `ImportSearchReleaseDetail` from a parsed Discogs
/// release, for the picker and the confirmation pane.
pub(crate) fn build_discogs_detail(
    release: &crate::discogs::DiscogsRelease,
    cover_art: Vec<RemoteCover>,
) -> ImportSearchReleaseDetail {
    let processed = crate::import::discogs_mapper::process_tracklist(&release.tracklist);
    build_discogs_detail_with_tracks(release, cover_art, &processed)
}

pub(crate) fn build_discogs_detail_for_audio(
    release: &crate::discogs::DiscogsRelease,
    cover_art: Vec<RemoteCover>,
    audio_durations_ms: &[u64],
) -> ImportSearchReleaseDetail {
    let processed = crate::import::discogs_mapper::process_tracklist_for_audio(
        &release.tracklist,
        audio_durations_ms,
    );
    build_discogs_detail_with_tracks(release, cover_art, &processed)
}

fn build_discogs_detail_with_tracks(
    release: &crate::discogs::DiscogsRelease,
    cover_art: Vec<RemoteCover>,
    processed: &[crate::import::discogs_mapper::ProcessedTrack<'_>],
) -> ImportSearchReleaseDetail {
    let format_string = if release.format.is_empty() {
        None
    } else {
        Some(release.format.join(", "))
    };

    let tracks: Vec<ReleaseTrack> = processed
        .iter()
        .map(|pt| {
            let artist = pt
                .source_tracks
                .iter()
                .find_map(|track| track.artists.first())
                .map(|artist| artist.name.clone());
            ReleaseTrack {
                title: pt.title.clone(),
                artist,
                duration_ms: pt.duration_ms,
                position: pt.position.clone(),
                side: pt.side as u32,
            }
        })
        .collect();

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

#[cfg(test)]
#[path = "search_tests.rs"]
mod search_tests;
