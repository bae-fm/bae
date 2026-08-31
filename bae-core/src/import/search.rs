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
        Err(e) => return Err(mb_error_to_lookup_failure(e)),
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
mod tests {
    use super::*;
    use crate::discogs::client::DiscogsSearchResult;
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
            cover_art_archive: crate::musicbrainz::MbCoverArtArchive {
                front: true,
                darkened: false,
            },
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

    /// The picker's pressing fields are the pressing the commit stores: one
    /// projection, read by both. They used to be re-derived side by side.
    #[test]
    fn mb_detail_pressing_matches_the_committed_pressing() {
        let mut response = response_with_media(vec![MbMedium {
            format: Some("CD".to_string()),
            tracks: vec![make_mb_track("1", "Track Title")],
        }]);
        response.date = Some("1996-05-04".to_string());
        response.country = Some("JP".to_string());
        response.barcode = Some("4988006757486".to_string());
        response.label_info = vec![crate::musicbrainz::MbLabelInfo {
            label: Some(crate::musicbrainz::MbLabel {
                name: Some("Toshiba EMI".to_string()),
            }),
            catalog_number: Some("TOCP-8556".to_string()),
        }];

        let detail = build_mb_detail("mb-release-1", &response, vec![]).unwrap();
        let parsed = crate::import::musicbrainz_mapper::map_mb_response_to_db(
            &response,
            None,
            None,
            &test_clock(),
            &SequentialIdProvider::new("mb"),
        )
        .unwrap();
        let committed = parsed.release.pressing;

        assert_eq!(detail.year, committed.year);
        assert_eq!(detail.format, committed.format);
        assert_eq!(detail.label, committed.label);
        assert_eq!(detail.catalog_number, committed.catalog_number);
        assert_eq!(detail.country, committed.country);
        assert_eq!(detail.barcode, committed.barcode);
    }

    /// The picker's track titles resolve exactly as the commit mapper's do —
    /// recording title first, the track's own title only as the fallback. The two
    /// used to read the pair in opposite orders, so a release whose track and
    /// recording titles differ showed one title in the picker and committed the
    /// other.
    #[test]
    fn mb_detail_track_title_prefers_the_recording_title() {
        let mut track = make_mb_track("1", "Recording Title");
        track.title = Some("Track Title".to_string());
        let fallback = MbTrack {
            position: None,
            number: Some("2".to_string()),
            title: Some("Only A Track Title".to_string()),
            length: None,
            recording: Some(MbRecording {
                id: None,
                title: None,
                artist_credit: vec![],
                relations: vec![],
            }),
            artist_credit: vec![],
        };
        let response = response_with_media(vec![MbMedium {
            format: Some("CD".to_string()),
            tracks: vec![track, fallback],
        }]);

        let detail = build_mb_detail("mb-release-1", &response, vec![]).unwrap();
        let titles: Vec<&str> = detail.tracks.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(titles, vec!["Recording Title", "Only A Track Title"]);

        let parsed = crate::import::musicbrainz_mapper::map_mb_response_to_db(
            &response,
            None,
            None,
            &test_clock(),
            &SequentialIdProvider::new("mb"),
        )
        .unwrap();
        let committed: Vec<&str> = parsed.tracks.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(titles, committed);
    }

    /// A track with no title anywhere fails the prefetch rather than rendering as
    /// an empty row the user can't tell from a real one — the same error the
    /// commit mapper raises.
    #[test]
    fn mb_detail_errors_on_track_without_any_title() {
        let response = response_with_media(vec![MbMedium {
            format: Some("CD".to_string()),
            tracks: vec![MbTrack {
                position: None,
                number: Some("1".to_string()),
                title: None,
                length: None,
                recording: Some(MbRecording {
                    id: None,
                    title: None,
                    artist_credit: vec![],
                    relations: vec![],
                }),
                artist_credit: vec![],
            }],
        }]);

        let err = build_mb_detail("mb-release-1", &response, vec![])
            .expect_err("expected error for a title-less track");
        assert!(
            matches!(&err, ImportError::SourceData { detail, .. } if detail.contains("has no track title")),
            "unexpected error: {err}"
        );
    }

    fn nested_discogs_release() -> crate::discogs::DiscogsRelease {
        crate::discogs::client::parse_discogs_release_json(
            &serde_json::json!({
                "id": 123,
                "title": "Album Title",
                "artists": [{ "id": 1, "name": "Artist Name" }],
                "tracklist": [
                    {
                        "position": "",
                        "type_": "index",
                        "title": "Suite Title",
                        "duration": "5:00",
                        "sub_tracks": [
                            {
                                "position": "1a",
                                "type_": "track",
                                "title": "Movement One",
                                "duration": "2:00"
                            },
                            {
                                "position": "1b",
                                "type_": "track",
                                "title": "Movement Two",
                                "duration": "3:00"
                            }
                        ]
                    },
                    {
                        "position": "2",
                        "type_": "track",
                        "title": "Track Title",
                        "duration": "4:00"
                    }
                ]
            })
            .to_string(),
        )
        .expect("nested Discogs tracklist parses")
    }

    #[test]
    fn discogs_detail_includes_nested_index_tracks() {
        let release = nested_discogs_release();

        let detail = build_discogs_detail(&release, Vec::new());
        let titles: Vec<&str> = detail
            .tracks
            .iter()
            .map(|track| track.title.as_str())
            .collect();

        assert_eq!(
            titles,
            vec![
                "Suite Title: Movement One",
                "Suite Title: Movement Two",
                "Track Title"
            ]
        );
    }

    #[test]
    fn discogs_detail_collapses_an_index_for_one_matching_audio_file() {
        let release = nested_discogs_release();

        let detail = build_discogs_detail_for_audio(&release, Vec::new(), &[300_000, 240_000]);
        let titles: Vec<&str> = detail
            .tracks
            .iter()
            .map(|track| track.title.as_str())
            .collect();

        assert_eq!(titles, vec!["Suite Title", "Track Title"]);
    }

    #[test]
    fn discogs_detail_selects_each_index_layout_from_ordered_durations() {
        let release = crate::discogs::client::parse_discogs_release_json(
            &serde_json::json!({
                "id": 456,
                "title": "Album Title",
                "artists": [{ "id": 1, "name": "Artist Name" }],
                "tracklist": [
                    {
                        "position": "",
                        "type_": "index",
                        "title": "Suite One",
                        "duration": "3:00",
                        "sub_tracks": [
                            { "position": "1a", "type_": "track", "title": "Part One", "duration": "1:00" },
                            { "position": "1b", "type_": "track", "title": "Part Two", "duration": "2:00" }
                        ]
                    },
                    {
                        "position": "",
                        "type_": "index",
                        "title": "Suite Two",
                        "duration": "9:00",
                        "sub_tracks": [
                            { "position": "2a", "type_": "track", "title": "Part Three", "duration": "4:00" },
                            { "position": "2b", "type_": "track", "title": "Part Four", "duration": "5:00" }
                        ]
                    }
                ]
            })
            .to_string(),
        )
        .expect("two nested Discogs indexes parse");

        let detail =
            build_discogs_detail_for_audio(&release, Vec::new(), &[60_000, 120_000, 540_000]);
        let titles: Vec<&str> = detail
            .tracks
            .iter()
            .map(|track| track.title.as_str())
            .collect();

        assert_eq!(
            titles,
            vec!["Suite One: Part One", "Suite One: Part Two", "Suite Two"]
        );
    }

    #[test]
    fn nested_index_durations_align_after_preceding_tracks() {
        let release = crate::discogs::client::parse_discogs_release_json(
            &serde_json::json!({
                "id": 789,
                "title": "Album Title",
                "artists": [{ "id": 1, "name": "Artist Name" }],
                "tracklist": [
                    { "position": "1", "type_": "track", "title": "Opening Track", "duration": "10:00" },
                    {
                        "position": "",
                        "type_": "index",
                        "title": "Grouped Work",
                        "sub_tracks": [
                            {
                                "position": "",
                                "type_": "index",
                                "title": "Suite One",
                                "duration": "3:00",
                                "sub_tracks": [
                                    { "position": "2a", "type_": "track", "title": "Part One", "duration": "1:00" },
                                    { "position": "2b", "type_": "track", "title": "Part Two", "duration": "2:00" }
                                ]
                            },
                            {
                                "position": "",
                                "type_": "index",
                                "title": "Suite Two",
                                "duration": "9:00",
                                "sub_tracks": [
                                    { "position": "3a", "type_": "track", "title": "Part Three", "duration": "4:00" },
                                    { "position": "3b", "type_": "track", "title": "Part Four", "duration": "5:00" }
                                ]
                            }
                        ]
                    }
                ]
            })
            .to_string(),
        )
        .expect("nested Discogs indexes parse");

        let detail = build_discogs_detail_for_audio(
            &release,
            Vec::new(),
            &[600_000, 60_000, 120_000, 540_000],
        );
        let titles: Vec<&str> = detail
            .tracks
            .iter()
            .map(|track| track.title.as_str())
            .collect();

        assert_eq!(
            titles,
            vec![
                "Opening Track",
                "Grouped Work: Suite One: Part One",
                "Grouped Work: Suite One: Part Two",
                "Grouped Work: Suite Two"
            ]
        );
    }
}
