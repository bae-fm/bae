//! Metadata search and prefetch orchestration: searching MusicBrainz and
//! Discogs for release metadata, checking Cover Art Archive for thumbnails, and
//! fetching full release details for the import confirmation step.

use crate::discogs::client::{DiscogsClient, DiscogsError, DiscogsSearchParams};
use crate::import::cover_art::RemoteCover;
use crate::import::parse_year;
use crate::import::types::{parse_catalog_url, Catalog, CatalogPage, MetadataRef};
use crate::import::ImportError;
use crate::musicbrainz::{self, MbReleaseResponse, ReleaseSearchParams, SearchRelease};
use crate::signals::LookupFailure;
use crate::util::rate_limiter::CallPriority;
use tracing::warn;

/// A metadata search result from either MusicBrainz or Discogs.
///
/// `source_group_id` carries the per-source group — MB release-group ID or
/// Discogs master ID — and is `None` when the search result surfaced no group.
///
/// A verdict's matches are these, as fetched: one `import_candidate_match`
/// row each, read back unchanged.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MetadataResult {
    pub source: Catalog,
    pub release_id: String,
    pub title: String,
    pub artist: Option<String>,
    pub year: Option<i32>,
    pub format: Option<String>,
    pub label: Option<String>,
    pub catalog_number: Option<String>,
    pub country: Option<String>,
    /// Every barcode the source states for the physical product, in the
    /// source's order and as it prints them. Empty when the source lists
    /// none. MusicBrainz states at most one; Discogs lists each code the
    /// sleeve carries. Two sources printing one code is what pairs their
    /// rows into one pressing.
    pub barcodes: Vec<String>,
    /// What the record says the pressing is made of — the evidence a medium
    /// contradiction is read from, kept apart from the display `format`.
    pub media: StatedMedia,
    /// Releases on other catalogs this record's own document names as the
    /// same release. Only a MusicBrainz release document states any.
    pub links: Vec<MetadataRef>,
    pub cover_art: Option<RemoteCover>,
    pub source_group_id: Option<String>,
    /// What the source says about this release's own tracklist — the other half
    /// of the Ready rule, which admits a single match only when the source's
    /// track count agrees with the candidate's.
    ///
    /// **`None` means nobody has asked yet** — not that the source has
    /// nothing. Search endpoints return results this way because they carry no
    /// tracklist. A MusicBrainz DiscID result instead carries `Some` immediately
    /// from the matching medium's tracks. Other results are filled when the
    /// sweep settles the lead, from the release document that settling archives.
    ///
    /// Keeping "unasked" distinct from "asked, and there is nothing" is what
    /// lets a stored verdict say whether its lead was settled: the two are
    /// written together, so a `Some` here is also the readable marker that this
    /// release's documents are stored. Collapsing them would either strand a
    /// verdict at unverified forever or re-buy the same empty answer on every
    /// launch.
    pub source_tracks: Option<SourceTracks>,
}

impl MetadataResult {
    /// The release a person chose, as a result. No lookup produced it — they
    /// found it — so it carries the release document's own facts and nothing
    /// about a signal. Its tracklist is listed because choosing a release is
    /// what archives its documents.
    pub(crate) fn of_pick(detail: &ImportSearchReleaseDetail) -> Self {
        Self {
            source: detail.source,
            release_id: detail.release_id.clone(),
            title: detail.title.clone(),
            artist: detail.artist.clone(),
            year: detail.year,
            format: detail.format.clone(),
            label: detail.label.clone(),
            catalog_number: detail.catalog_number.clone(),
            country: detail.country.clone(),
            barcodes: detail.barcode.iter().cloned().collect(),
            media: detail.media.clone(),
            links: detail.links.clone(),
            cover_art: detail.default_cover().cloned(),
            source_group_id: detail.source_group_id.clone(),
            source_tracks: Some(SourceTracks::Listed {
                count: detail.track_count,
            }),
        }
    }
}

/// What a record says a pressing is made of.
///
/// Each stated word is read in its own catalog's list of format names by
/// `crate::import::medium`; what the shape adds is whether the record
/// lists its media one by one, and so can account for all of them, or only
/// names what it is made of.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum StatedMedia {
    /// The response describes no media: a `ws/2/release?query=` result, or a
    /// Discogs search result with no `format`.
    Undescribed,
    /// One entry per medium the record lists, in its order; `None` where that
    /// medium's format is not stated. A MusicBrainz release document.
    PerMedium(Vec<Option<String>>),
    /// Format names and qualifiers as one flat list that does not say which
    /// medium each belongs to. A Discogs record.
    Descriptors(Vec<String>),
}

#[cfg(any(test, feature = "test-utils"))]
impl MetadataResult {
    /// A placeholder result: the source, the release it names, and the source
    /// group it belongs to — the three things identify's tests vary. Every
    /// other field is the empty value, so a test that cares about one of them
    /// sets it with struct-update syntax.
    pub fn for_test(source: Catalog, release_id: &str, source_group_id: Option<&str>) -> Self {
        Self {
            source,
            release_id: release_id.to_string(),
            title: "Album".to_string(),
            artist: None,
            year: None,
            format: None,
            label: None,
            catalog_number: None,
            country: None,
            barcodes: Vec::new(),
            media: StatedMedia::Undescribed,
            links: Vec::new(),
            cover_art: None,
            source_group_id: source_group_id.map(str::to_string),
            source_tracks: None,
        }
    }
}

/// What a source said about a release's tracklist, once something asked.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SourceTracks {
    /// It listed its tracks.
    Listed { count: u32 },
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
/// build a `ReleaseRecord` row from the picked release without a second fetch.
/// `media` and `links` are what the document stated about the pressing's
/// media and its counterparts on other catalogs, carried so the result a pick
/// becomes states them too.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportSearchReleaseDetail {
    pub release_id: String,
    pub source: Catalog,
    pub source_group_id: Option<String>,
    pub title: String,
    pub artist: Option<String>,
    pub year: Option<i32>,
    pub format: Option<String>,
    pub label: Option<String>,
    pub catalog_number: Option<String>,
    pub country: Option<String>,
    pub barcode: Option<String>,
    pub media: StatedMedia,
    pub links: Vec<MetadataRef>,
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
    pub side: Option<u32>,
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
    // The response lists format names and qualifiers as one flat list; which
    // medium each describes is not said.
    let media = match r.format {
        Some(descriptors) => StatedMedia::Descriptors(descriptors),
        None => StatedMedia::Undescribed,
    };
    let source_group_id = r.master_id.map(|id| id.to_string());
    MetadataResult {
        source: Catalog::Discogs,
        release_id: r.id.to_string(),
        title: album,
        artist,
        year,
        format,
        label,
        catalog_number: r.catno,
        country: r.country,
        barcodes: r.barcode,
        media,
        // A Discogs document names no counterpart on another catalog.
        links: Vec::new(),
        cover_art,
        source_group_id,
        // The Discogs search response describes no tracklist; a Discogs result
        // gets one only from a paid `get_release`.
        source_tracks: None,
    }
}

/// A MusicBrainz release's tracklist, as an `inc=recordings` response carries
/// it.
pub(crate) fn mb_source_tracks(
    r: &MbReleaseResponse,
    coverage: &crate::import::medium_coverage::MediumCoverage,
) -> SourceTracks {
    source_tracks_from_mb_tracks(
        crate::import::musicbrainz_mapper::covered_media(r, coverage)
            .flat_map(|medium| &medium.tracks),
    )
}

fn source_tracks_from_mb_tracks<'a>(
    tracks: impl Iterator<Item = &'a crate::musicbrainz::MbTrack>,
) -> SourceTracks {
    let tracks: Vec<_> = tracks.collect();
    if tracks.is_empty() {
        return SourceTracks::Nothing;
    }
    SourceTracks::Listed {
        count: tracks.len() as u32,
    }
}

fn mb_discid_release_to_metadata(discid: &str, r: MbReleaseResponse) -> Option<MetadataResult> {
    let mut matching_media = r.media.iter().filter(|medium| {
        medium
            .discs
            .iter()
            .any(|registered| registered.id == discid)
    });
    let Some(medium) = matching_media.next() else {
        warn!(
            discid,
            musicbrainz_release_id = %r.id,
            "Skipping MusicBrainz DiscID result without a matching medium"
        );
        return None;
    };
    if matching_media.next().is_some() {
        warn!(
            discid,
            musicbrainz_release_id = %r.id,
            "Skipping MusicBrainz DiscID result with multiple matching media"
        );
        return None;
    }

    let format = medium.format.clone();
    let source_tracks = Some(source_tracks_from_mb_tracks(medium.tracks.iter()));
    let pressing = crate::import::musicbrainz_mapper::pressing(&r);
    let media = mb_stated_media(&r);
    let links = mb_release_links(&r);
    let cover_art = r
        .has_front_cover()
        .then(|| RemoteCover::musicbrainz_release(&r.id));
    Some(MetadataResult {
        source: Catalog::MusicBrainz,
        release_id: r.id,
        title: r.title,
        artist: r.artist_credit.first().map(|ac| ac.name.clone()),
        year: pressing.year,
        format,
        label: pressing.label,
        catalog_number: pressing.catalog_number,
        country: pressing.country,
        barcodes: pressing.barcode.into_iter().collect(),
        media,
        links,
        cover_art,
        source_group_id: r.release_group.as_ref().map(|rg| rg.id.clone()),
        source_tracks,
    })
}

/// Every medium a MusicBrainz release document lists, each with the format it
/// states for it. The matching medium's tracks are what a disc ID answers
/// for; the pressing is made of all of them.
fn mb_stated_media(r: &MbReleaseResponse) -> StatedMedia {
    StatedMedia::PerMedium(r.media.iter().map(|medium| medium.format.clone()).collect())
}

/// The releases on other catalogs a MusicBrainz release document names as
/// the same release, in relation order. A link to an album page names an
/// album, not this pressing, and is not one of them.
fn mb_release_links(r: &MbReleaseResponse) -> Vec<MetadataRef> {
    crate::musicbrainz::relation_urls(&r.relations)
        .filter_map(parse_catalog_url)
        .filter_map(|page| match page {
            CatalogPage::Release { catalog, key } if catalog != Catalog::MusicBrainz => {
                Some(MetadataRef::new(catalog, key))
            }
            CatalogPage::Release { .. } | CatalogPage::Group { .. } => None,
        })
        .collect()
}

fn mb_discid_releases_to_metadata(
    discid: &str,
    releases: Vec<MbReleaseResponse>,
) -> Vec<MetadataResult> {
    releases
        .into_iter()
        .filter_map(|release| mb_discid_release_to_metadata(discid, release))
        .collect()
}

fn search_release_to_metadata(r: SearchRelease, cover_art: Option<RemoteCover>) -> MetadataResult {
    let (label, catalog_number) = musicbrainz::label_and_catno(&r.label_info);
    // The first medium's format, as a looked-up release's pressing reads it.
    let format = r.media.first().and_then(|medium| medium.format.clone());
    let media = StatedMedia::PerMedium(r.media.into_iter().map(|medium| medium.format).collect());
    MetadataResult {
        source: Catalog::MusicBrainz,
        release_id: r.id,
        title: r.title,
        artist: r.artist_credit.first().map(|ac| ac.name.clone()),
        year: parse_year(r.date.as_deref()),
        format,
        label,
        catalog_number,
        country: r.country,
        barcodes: r.barcode.into_iter().collect(),
        // `ws/2/release?query=…` takes no `inc`, so its response states no
        // relations and no tracks to read a length from.
        media,
        links: Vec::new(),
        cover_art,
        source_group_id: r.release_group.as_ref().map(|rg| rg.id.clone()),
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
pub(crate) async fn search_mb(
    musicbrainz: &musicbrainz::MusicBrainz,
    params: ReleaseSearchParams,
    priority: CallPriority,
) -> Result<Vec<MetadataResult>, ImportError> {
    let releases = musicbrainz
        .search_releases_with_params(&params, priority)
        .await?;

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
        ImportError::CoverArtRequest { failure, .. } => failure.clone(),
        ImportError::Discogs(error) => match error {
            DiscogsError::Transport(error) if error.is_builder() || error.is_redirect() => {
                LookupFailure::Diagnostic {
                    detail: format!("{error:?}"),
                }
            }
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

/// One provider's answer to one lookup.
pub type SourceLookup = Result<Vec<MetadataResult>, LookupFailure>;

/// A provider that failed one lookup, and how. Serialized: it rides on
/// [`crate::identify::IdentifyFailure`], which a failed verdict persists.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SourceFailure {
    pub source: Catalog,
    pub failure: LookupFailure,
}

/// A typed manual search, one of three modes. Every configured provider is
/// asked; [`search_source`] runs one of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchQuery {
    General { artist: String, album: String },
    CatalogNumber { catalog_number: String },
    Barcode { barcode: String },
}

impl SearchQuery {
    pub fn musicbrainz_params(&self) -> ReleaseSearchParams {
        match self {
            SearchQuery::General { artist, album } => ReleaseSearchParams {
                artist: Some(artist.clone()),
                album: Some(album.clone()),
                ..Default::default()
            },
            SearchQuery::CatalogNumber { catalog_number } => ReleaseSearchParams {
                catalog_number: Some(catalog_number.clone()),
                ..Default::default()
            },
            SearchQuery::Barcode { barcode } => ReleaseSearchParams {
                barcode: Some(barcode.clone()),
                ..Default::default()
            },
        }
    }

    pub fn discogs_params(&self) -> DiscogsSearchParams {
        match self {
            SearchQuery::General { artist, album } => DiscogsSearchParams {
                text: (!artist.trim().is_empty()).then(|| artist.clone()),
                release_title: Some(album.clone()),
                ..Default::default()
            },
            SearchQuery::CatalogNumber { catalog_number } => DiscogsSearchParams {
                catno: Some(catalog_number.clone()),
                ..Default::default()
            },
            SearchQuery::Barcode { barcode } => DiscogsSearchParams {
                barcode: Some(barcode.clone()),
                ..Default::default()
            },
        }
    }
}

/// Ask one provider a typed query, in the provider's own error type — for a
/// caller that reports the provider's error as it is.
pub async fn search_provider(
    library_manager: &crate::library::LibraryManager,
    source: Catalog,
    query: &SearchQuery,
    priority: CallPriority,
) -> Result<Vec<MetadataResult>, ImportError> {
    match source {
        Catalog::MusicBrainz => {
            library_manager
                .search_musicbrainz(query.musicbrainz_params(), priority)
                .await
        }
        Catalog::Discogs => {
            library_manager
                .search_discogs(query.discogs_params(), priority)
                .await
        }
        other => unreachable!("{} answers no searches", other.as_str()),
    }
}

/// Ask one provider a typed query, in the typed failure a surface renders —
/// the per-source primitive a candidate's manual search is built from.
pub async fn search_source(
    library_manager: &crate::library::LibraryManager,
    source: Catalog,
    query: &SearchQuery,
    priority: CallPriority,
) -> SourceLookup {
    search_provider(library_manager, source, query, priority)
        .await
        .map_err(|error| import_error_to_lookup_failure(&error))
}

/// The releases MusicBrainz has for a disc ID, each with its cover art. Empty
/// when the disc is unknown to MB — a settled lookup with no matches, which is
/// what `NotFound` means on this endpoint too.
pub(crate) async fn lookup_by_discid(
    musicbrainz: &musicbrainz::MusicBrainz,
    discid: &str,
    priority: CallPriority,
) -> Result<Vec<MetadataResult>, LookupFailure> {
    let releases = match musicbrainz.lookup_by_discid(discid, priority).await {
        Ok(releases) => releases,
        Err(musicbrainz::MusicBrainzError::NotFound(_)) => return Ok(Vec::new()),
        Err(e) => return Err(mb_error_to_lookup_failure(&e)),
    };

    Ok(mb_discid_releases_to_metadata(discid, releases))
}

/// A Discogs release's tracklist. Headings and index entries are not tracks;
/// nested index rows are expanded to their playable leaves when
/// `audio_durations_ms` offers no layout to fit.
pub(crate) fn discogs_source_tracks(
    release: &crate::discogs::DiscogsRelease,
    coverage: &crate::import::medium_coverage::MediumCoverage,
    audio_durations_ms: Option<&[u64]>,
) -> SourceTracks {
    let tracklist = crate::import::discogs_mapper::covered_tracklist(&release.tracklist, coverage);
    let tracks = crate::import::discogs_mapper::process_tracklist(&tracklist, audio_durations_ms);
    if tracks.is_empty() {
        return SourceTracks::Nothing;
    }
    SourceTracks::Listed {
        count: tracks.len() as u32,
    }
}

/// Build the UI-shaped `ImportSearchReleaseDetail` from a parsed MB response,
/// for the picker and the confirmation pane.
pub(crate) fn build_mb_detail(
    release_id: &str,
    mb_response: &crate::musicbrainz::MbReleaseResponse,
    coverage: &crate::import::medium_coverage::MediumCoverage,
    cover_art: Vec<RemoteCover>,
) -> Result<ImportSearchReleaseDetail, ImportError> {
    let mut side_base: u32 = 0;
    let mut tracks: Vec<ReleaseTrack> = Vec::new();

    for medium in crate::import::musicbrainz_mapper::covered_media(mb_response, coverage) {
        let sides = crate::import::musicbrainz_mapper::medium_sides(release_id, medium)?;

        for (t, &side_offset) in medium.tracks.iter().zip(&sides.offsets) {
            let side = side_offset.map(|offset| side_base + offset + 1);

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
        source: Catalog::MusicBrainz,
        source_group_id: mb_response.release_group.as_ref().map(|rg| rg.id.clone()),
        title: mb_response.title.clone(),
        artist,
        year: pressing.year,
        format: pressing.format,
        label: pressing.label,
        catalog_number: pressing.catalog_number,
        country: pressing.country,
        barcode: pressing.barcode,
        media: mb_stated_media(mb_response),
        links: mb_release_links(mb_response),
        track_count: tracks.len() as u32,
        tracks,
        cover_art,
    })
}

pub(crate) fn build_discogs_detail(
    release: &crate::discogs::DiscogsRelease,
    coverage: &crate::import::medium_coverage::MediumCoverage,
    cover_art: Vec<RemoteCover>,
    audio_durations_ms: Option<&[u64]>,
) -> ImportSearchReleaseDetail {
    let tracklist = crate::import::discogs_mapper::covered_tracklist(&release.tracklist, coverage);
    let processed =
        crate::import::discogs_mapper::process_tracklist(&tracklist, audio_durations_ms);
    let pressing = crate::import::discogs_mapper::pressing(release);

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
                side: crate::import::discogs_mapper::release_track_side(release, pt)
                    .map(|side| side as u32),
            }
        })
        .collect();

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
        source: Catalog::Discogs,
        source_group_id: release.master_id.clone(),
        title: release.title.clone(),
        artist,
        year: pressing.year,
        format: pressing.format,
        label: pressing.label,
        catalog_number: pressing.catalog_number,
        country: pressing.country,
        barcode: pressing.barcode,
        // The document's format names are one flat list, as the search
        // result's are, and a Discogs document names no counterpart.
        media: if release.format.is_empty() {
            StatedMedia::Undescribed
        } else {
            StatedMedia::Descriptors(release.format.clone())
        },
        links: Vec::new(),
        track_count: tracks.len() as u32,
        tracks,
        cover_art,
    }
}

#[cfg(test)]
#[path = "search_tests.rs"]
mod search_tests;
