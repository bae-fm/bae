//! Metadata search and prefetch orchestration: searching MusicBrainz and
//! Discogs for release metadata, checking Cover Art Archive for thumbnails, and
//! fetching full release details for the import confirmation step.

use crate::album_links::AlbumLinks;
use crate::catalog::{parse_catalog_url, Catalog, CatalogPage, LookupCatalog};
use crate::discogs::client::{DiscogsClient, DiscogsError, DiscogsSearchParams};
use crate::musicbrainz::{self, MbReleaseResponse, ReleaseSearchParams, SearchRelease};
use crate::parse_year;
use crate::providers::CatalogSession;
use crate::rate_limiter::CallClass;
use crate::records::{LookupRef, MetadataRef};
use crate::remote_image::RemoteCover;
use crate::CatalogError;
use crate::{Failure, LookupFailure};
use bae_types::pressing::{
    DiscogsDetail, Packaging, PressingFacts, ReleaseArea, ReleaseLabel, ReleaseStatus, StatedMedia,
};
use tracing::warn;

mod failure;
mod query;
use failure::mb_failure;
pub use failure::{lookup_or_internal, provider_failure, remote_image_failure};
pub use query::{CatalogNumberSearch, ProviderQuery, SearchQuery};

/// A metadata search result from either MusicBrainz or Discogs.
///
/// `source_group_id` carries the per-source group — MB release-group ID or
/// Discogs master ID — and is `None` when the search result surfaced no group.
///
/// A verdict's matches are these, as fetched: one `import_candidate_match`
/// row each, read back unchanged.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MetadataResult {
    pub source: LookupCatalog,
    pub release_id: String,
    pub title: String,
    pub artist: Option<String>,
    pub year: Option<i32>,
    /// Every label the record states, with its catalog number; a Discogs
    /// search result states only the first.
    pub labels: Vec<ReleaseLabel>,
    pub area: Option<ReleaseArea>,
    pub status: Option<ReleaseStatus>,
    pub packaging: Option<Packaging>,
    pub discogs_details: Vec<DiscogsDetail>,
    /// Every barcode the source states for the physical product, in the
    /// source's order and as it prints them. Empty when the source lists
    /// none. MusicBrainz states at most one; Discogs lists each code the
    /// sleeve carries. Two sources printing one code is what pairs their
    /// rows into one pressing.
    pub barcodes: Vec<String>,
    /// What the record says the pressing is made of, in the record's own
    /// shape: the evidence a medium contradiction is read from, and what
    /// [`Self::facts`] counts.
    pub media: StatedMedia,
    /// Releases on other catalogs this record's own document names as the
    /// same release. Only a MusicBrainz release document states any. A
    /// search result's response carries no relations, so it states none until
    /// its document is read.
    pub links: Vec<MetadataRef>,
    /// What the record says about its cover: a stated cover, an unstated
    /// address when it said nothing (a MusicBrainz search result), or `None`
    /// when it states it has none (a release document whose archive block has
    /// no front, a Discogs result listing no image).
    pub cover_art: Option<RemoteCover>,
    pub source_group_id: Option<String>,
    /// The albums on the other lookup catalog a statement names as this
    /// record's album — what puts two catalogs' albums on one card.
    pub album_links: AlbumLinks,
    /// What the source says about this release's own tracklist, read against
    /// the folder's audio: what rules out a row that holds other tracks than
    /// the folder, and the other half of the auto-import check. `None` until
    /// the release's document is read, which an identification run does for
    /// every row it offers before it settles. No
    /// lookup's answer states it: a search result lists no tracklist, and the
    /// medium a disc ID names is one disc of what the folder may hold, where
    /// the document is read against all of the folder's audio.
    pub source_tracks: Option<SourceTracks>,
    /// Why this record's full document could not be read when the run offered
    /// its row, which then states what the result said. `None` when it was
    /// read, or never asked for.
    pub document_failure: Option<LookupFailure>,
    /// The year the release's album first came out, as its release group or
    /// master states it. A search result states none; its full document
    /// does.
    pub album_first_year: Option<i32>,
    /// The title of each track its full document lists for the audio, in
    /// order. Empty where the document was not read, or leaves a track
    /// untitled. A search result states none. Only a run's ranking reads
    /// them, so a stored verdict keeps none and its releases read back
    /// without them.
    pub track_titles: Vec<String>,
    /// What the record writes about which pressing it is, in free text: a
    /// MusicBrainz release's disambiguation, and once its full document is
    /// read, each line of its annotation; a Discogs release's format text,
    /// and once its full document is read, each line of its notes, its
    /// company names and its identifiers too (see `discogs_release_notes`).
    /// Only a run's ranking reads them, so a stored verdict keeps none and
    /// its releases read back without them.
    pub notes: Vec<String>,
}

impl MetadataResult {
    /// What the record says the pressing is, as a surface shows it.
    /// The release this result names.
    pub fn release(&self) -> LookupRef {
        LookupRef::new(self.source, self.release_id.clone())
    }

    pub fn facts(&self) -> PressingFacts {
        PressingFacts {
            area: self.area,
            media: self.media.counts(),
            status: self.status,
            packaging: self.packaging,
            discogs_details: self.discogs_details.clone(),
        }
    }

    /// The release a person chose, as a result. No lookup produced it — they
    /// found it — so it carries the release document's own facts and nothing
    /// about a signal. `source_tracks` is its tracklist as the pick read it
    /// against the folder's audio: choosing a release is what fetches and
    /// stores it.
    pub fn of_pick(detail: &ImportSearchReleaseDetail, source_tracks: SourceTracks) -> Self {
        Self {
            source: detail.source,
            release_id: detail.release_id.clone(),
            title: detail.title.clone(),
            artist: detail.artist.clone(),
            year: detail.year,
            labels: detail.labels.clone(),
            area: detail.facts.area,
            status: detail.facts.status,
            packaging: detail.facts.packaging,
            discogs_details: detail.facts.discogs_details.clone(),
            barcodes: detail.barcodes.clone(),
            media: detail.media.clone(),
            links: detail.links.clone(),
            cover_art: detail.default_cover().cloned(),
            source_group_id: detail.source_group_id.clone(),
            // One release a person chose is the whole list; there is no other
            // catalog's album on it to join.
            album_links: AlbumLinks::NotAsked,
            source_tracks: Some(source_tracks),
            document_failure: None,
            album_first_year: None,
            track_titles: Vec::new(),
            notes: Vec::new(),
        }
    }
}

impl MetadataResult {
    /// A stored release, as a result: what its documents state, with no
    /// lookup behind it. Its tracklist is read against the folder's audio
    /// where its document is applied.
    pub fn of_release(release: &crate::source_release::SourceRelease) -> Self {
        let pressing = release.pressing();
        Self {
            source: release.release().catalog,
            release_id: release.release().key.clone(),
            title: release.metadata.album.title.clone(),
            artist: release.artist_line(),
            year: pressing.year,
            labels: pressing.labels.clone(),
            area: pressing.facts.area,
            status: pressing.facts.status,
            packaging: pressing.facts.packaging,
            discogs_details: pressing.facts.discogs_details.clone(),
            barcodes: pressing.barcodes.clone(),
            media: release.stated_media(),
            links: release.links(),
            cover_art: release.covers().first().cloned(),
            source_group_id: release.source_group_id.clone(),
            album_links: release.album_links(),
            source_tracks: None,
            document_failure: None,
            album_first_year: release.metadata.album.first_year,
            track_titles: Vec::new(),
            notes: release.notes.clone(),
        }
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl MetadataResult {
    /// A placeholder result: the source, the release it names, and the source
    /// group it belongs to — the three things identify's tests vary. Every
    /// other field is the empty value, so a test that cares about one of them
    /// sets it with struct-update syntax.
    pub fn for_test(
        source: LookupCatalog,
        release_id: &str,
        source_group_id: Option<&str>,
    ) -> Self {
        Self {
            source,
            release_id: release_id.to_string(),
            title: "Album".to_string(),
            artist: None,
            year: None,
            labels: Vec::new(),
            area: None,
            status: None,
            packaging: None,
            discogs_details: Vec::new(),
            barcodes: Vec::new(),
            media: StatedMedia::Undescribed,
            links: Vec::new(),
            cover_art: None,
            source_group_id: source_group_id.map(str::to_string),
            album_links: AlbumLinks::NotAsked,
            source_tracks: None,
            document_failure: None,
            album_first_year: None,
            track_titles: Vec::new(),
            notes: Vec::new(),
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
    /// It lists tracks for the audio that cannot be read: one with no title,
    /// or an album credited to no one. Nothing it says can be applied.
    Unreadable,
}

/// Full release details for the confirmation step.
///
/// `facts` and `barcodes` are pressing-level fields the user can review or
/// override in the edit-metadata form before commit. `source_group_id` carries
/// the per-source group (MB release-group ID or Discogs master ID) so the UI can
/// build a `ReleaseRecord` row from the picked release without a second fetch.
/// `media` and `links` are what the document stated about the pressing's
/// media and its counterparts on other catalogs, carried so the result a pick
/// becomes states them too.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportSearchReleaseDetail {
    pub release_id: String,
    pub source: LookupCatalog,
    pub source_group_id: Option<String>,
    pub title: String,
    pub artist: Option<String>,
    pub year: Option<i32>,
    pub labels: Vec<ReleaseLabel>,
    pub barcodes: Vec<String>,
    /// What the pressing is, with what the linked documents supplied where
    /// the release states nothing.
    pub facts: PressingFacts,
    /// What the release's own document says it is made of.
    pub media: StatedMedia,
    pub links: Vec<MetadataRef>,
    pub cover_art: Vec<RemoteCover>,
}

impl ImportSearchReleaseDetail {
    pub fn default_cover(&self) -> Option<&RemoteCover> {
        self.cover_art.first()
    }
}

/// Convert a Discogs search result to a MetadataResult.
///
/// Its labels are the first label alone, the only one the response pairs
/// with a number, so a release found by a later label's number does not
/// answer a catalog-number search.
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
    let cover_art = r.remote_cover();
    let labels =
        ReleaseLabel::list([(r.label.and_then(|names| names.into_iter().next()), r.catno)]);
    let release_id = r.id.to_string();
    let notes = discogs_notes(&r.formats);
    let formats = crate::discogs::formats::read(&release_id, &r.formats);
    let area = r.country.as_deref().and_then(|name| crate::discogs_mapper::area(&release_id, name));
    let source_group_id = r.master_id.map(|id| id.to_string());
    MetadataResult {
        source: LookupCatalog::Discogs,
        release_id,
        title: album,
        artist,
        year,
        labels,
        area,
        status: formats.status,
        packaging: formats.packaging,
        discogs_details: formats.details,
        barcodes: r.barcode,
        media: formats.media,
        // A Discogs document names no counterpart on another catalog.
        links: Vec::new(),
        cover_art,
        source_group_id,
        // A Discogs document names no counterpart on another catalog.
        album_links: AlbumLinks::NotAsked,
        // The Discogs search response describes no tracklist; a Discogs result
        // gets one only from a paid `get_release`.
        source_tracks: None,
        document_failure: None,
        album_first_year: None,
        track_titles: Vec::new(),
        notes,
    }
}

/// What a Discogs release's own document writes about which pressing it is,
/// in order: its format entries' text, each line of its notes, the names of
/// the companies it credits, then what each identifier other than a barcode
/// reads — a barcode is its own field.
pub(crate) fn discogs_release_notes(release: &crate::discogs::DiscogsRelease) -> Vec<String> {
    discogs_notes(&release.formats)
        .into_iter()
        .chain(release.notes.iter().flat_map(|notes| lines(notes)))
        .chain(release.companies.iter().cloned())
        .chain(release.identifiers.iter().cloned())
        .collect()
}

/// What a MusicBrainz release's own document writes about which pressing it
/// is: its disambiguation, then each line of its annotation. A Disc ID
/// lookup's releases state no annotation, and an identify run reads them from
/// that answer, so theirs is the disambiguation alone until a pick reads
/// their own documents.
pub fn musicbrainz_release_notes(release: &MbReleaseResponse) -> Vec<String> {
    release
        .disambiguation
        .iter()
        .cloned()
        .chain(release.annotation.iter().flat_map(|annotation| lines(annotation)))
        .collect()
}

/// A note of prose, one line at a time, each trimmed, blank lines dropped.
/// Each line is a note of its own, so the one a row is named for is the line
/// that says so, not the whole text around it.
fn lines(prose: &str) -> impl Iterator<Item = String> + '_ {
    prose.lines().map(str::trim).filter(|line| !line.is_empty()).map(str::to_string)
}

/// What a Discogs release's format entries write beside their names, in
/// order: the part of its notes a search result carries too.
fn discogs_notes(formats: &[crate::discogs::DiscogsFormat]) -> Vec<String> {
    formats.iter().filter_map(|format| format.text.clone()).collect()
}

/// A release a disc-ID lookup answered, when exactly one of its mediums is the
/// disc: a release that registers the disc on none of them, or on several,
/// is skipped.
fn mb_discid_release_to_metadata(discid: &str, r: MbReleaseResponse) -> Option<MetadataResult> {
    let matching_media = r
        .media
        .iter()
        .filter(|medium| medium.discs.iter().any(|registered| registered.id == discid))
        .count();
    match matching_media {
        1 => {}
        0 => {
            warn!(
                discid,
                musicbrainz_release_id = %r.id,
                "Skipping MusicBrainz DiscID result without a matching medium"
            );
            return None;
        }
        _ => {
            warn!(
                discid,
                musicbrainz_release_id = %r.id,
                "Skipping MusicBrainz DiscID result with multiple matching media"
            );
            return None;
        }
    }

    let notes = musicbrainz_release_notes(&r);
    let (pressing, media) = crate::musicbrainz_mapper::pressing(&r);
    let links = release_links_of(&r.relations);
    let cover_art = crate::remote_image::musicbrainz_release_cover(&r);
    Some(MetadataResult {
        source: LookupCatalog::MusicBrainz,
        release_id: r.id,
        title: r.title,
        artist: musicbrainz_credit_line(&r.artist_credit),
        year: pressing.year,
        labels: pressing.labels,
        area: pressing.facts.area,
        status: pressing.facts.status,
        packaging: pressing.facts.packaging,
        discogs_details: pressing.facts.discogs_details,
        barcodes: pressing.barcodes,
        media,
        links,
        cover_art,
        source_group_id: r.release_group.as_ref().map(|rg| rg.id.clone()),
        // Read from the group's own document once the list it lands on holds
        // the other catalog's releases too.
        album_links: AlbumLinks::NotAsked,
        source_tracks: None,
        document_failure: None,
        album_first_year: None,
        track_titles: Vec::new(),
        notes,
    })
}

/// The artist line a MusicBrainz credit list prints: each credited name, then
/// what MusicBrainz prints after it. None for a release crediting no one.
fn musicbrainz_credit_line(credits: &[crate::musicbrainz::MbArtistCredit]) -> Option<String> {
    if credits.is_empty() {
        return None;
    }
    let joins: Vec<Option<String>> = credits
        .iter()
        .map(|credit| bae_types::credits::stated_join(credit.joinphrase.as_deref()))
        .collect();
    Some(bae_types::credits::credit_line(
        credits.iter().zip(&joins).map(|(credit, join)| (credit.name.as_str(), join.as_deref())),
    ))
}

/// The releases on other catalogs a MusicBrainz release's relations name as
/// the same release, in relation order. A link to an album page names an
/// album, not this pressing, and is not one of them.
pub(crate) fn release_links_of(relations: &[crate::musicbrainz::MbRelation]) -> Vec<MetadataRef> {
    crate::musicbrainz::relation_urls(relations)
        .filter_map(parse_catalog_url)
        .filter_map(|page| match page {
            CatalogPage::Release { catalog, key } if catalog != Catalog::MusicBrainz => {
                Some(MetadataRef::new(catalog, key))
            }
            CatalogPage::Release { .. } | CatalogPage::Group { .. } => None,
        })
        .collect()
}

/// The releases a disc-ID lookup answered that hold the disc, each with its
/// document as the answer states it.
fn mb_discid_releases_to_metadata(
    discid: &str,
    releases: Vec<(MbReleaseResponse, String)>,
) -> Vec<(MetadataResult, crate::payloads::DiscIdRelease)> {
    releases
        .into_iter()
        .filter_map(|(release, json)| {
            let answered = crate::payloads::DiscIdRelease::new(release.id.clone(), json);
            Some((mb_discid_release_to_metadata(discid, release)?, answered))
        })
        .collect()
}

fn search_release_to_metadata(r: SearchRelease, cover_art: Option<RemoteCover>) -> MetadataResult {
    let labels = musicbrainz::release_labels(&r.label_info);
    let (facts, media) = crate::musicbrainz::pressing::read(crate::musicbrainz::pressing::Stated {
        release_id: &r.id,
        country: r.country.as_deref(),
        status: r.status.as_deref(),
        packaging: r.packaging.as_deref(),
        media: r.media.iter().map(|medium| medium.format.as_deref()),
    });
    MetadataResult {
        source: LookupCatalog::MusicBrainz,
        release_id: r.id,
        title: r.title,
        artist: musicbrainz_credit_line(&r.artist_credit),
        year: parse_year(r.date.as_deref()),
        labels,
        area: facts.area,
        status: facts.status,
        packaging: facts.packaging,
        discogs_details: facts.discogs_details,
        barcodes: r.barcode.into_iter().collect(),
        // `ws/2/release?query=…` takes no `inc`, so its response states no
        // relations and no tracks to read a length from.
        media,
        links: Vec::new(),
        cover_art,
        source_group_id: r.release_group.as_ref().map(|rg| rg.id.clone()),
        // Read from the group's own document once the list it lands on holds
        // the other catalog's releases too.
        album_links: AlbumLinks::NotAsked,
        source_tracks: None,
        document_failure: None,
        album_first_year: None,
        track_titles: Vec::new(),
        notes: r.disambiguation.into_iter().collect(),
    }
}

/// Search MusicBrainz for metadata matching the provider params.
///
/// Each result carries the archive's address for that release's front image,
/// unstated: the search endpoint takes no `inc` and returns no
/// `cover-art-archive` block, so the result says nothing about whether the
/// archive holds one. A stated cover from another record is preferred over it
/// wherever covers are picked ([`crate::remote_image::offered_covers`]),
/// and the release's own document states it once a pick fetches it.
pub(crate) async fn search_mb(
    musicbrainz: &musicbrainz::MusicBrainz,
    params: ReleaseSearchParams,
    class: CallClass,
) -> Result<Vec<MetadataResult>, CatalogError> {
    let releases = musicbrainz.search_releases_with_params(&params, class).await?;

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
    class: CallClass,
) -> Result<Vec<MetadataResult>, DiscogsError> {
    let results = client.search_with_params(&params, class).await?;
    Ok(results.into_iter().map(discogs_search_result_to_metadata).collect())
}

/// One provider's answer to one lookup.
pub type SourceLookup = Result<Vec<MetadataResult>, Failure>;

/// A provider that failed one lookup, and how. Serialized: a failed
/// identification verdict persists it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SourceFailure {
    pub source: Catalog,
    pub failure: LookupFailure,
}

/// Ask one provider a typed query, in the provider's own error type — for a
/// caller that reports the provider's error as it is.
pub async fn search_provider(
    session: &CatalogSession,
    source: LookupCatalog,
    query: &impl ProviderQuery,
    class: CallClass,
) -> Result<Vec<MetadataResult>, CatalogError> {
    let mut results = match source {
        LookupCatalog::MusicBrainz => {
            session.search_musicbrainz(query.musicbrainz_params(), class).await?
        }
        LookupCatalog::Discogs => session.search_discogs(query.discogs_params(), class).await?,
    };
    query.keep_answers(&mut results);
    Ok(results)
}

/// Ask one provider a typed query, in the typed failure a surface renders —
/// the per-source primitive a candidate's manual search is built from.
pub async fn search_source(
    session: &CatalogSession,
    source: LookupCatalog,
    query: &impl ProviderQuery,
    class: CallClass,
) -> SourceLookup {
    search_provider(session, source, query, class).await.map_err(|error| {
        lookup_or_internal(
            provider_failure(&error),
            &format!("asking {} a search for {query:?}", source.as_str()),
            &error,
        )
    })
}

/// The releases MusicBrainz has for a disc ID, each with its cover art and its
/// document as the answer states it, which an identify run reads the release
/// from. Empty when the disc is unknown to MB — a settled lookup with no
/// matches, which is what `NotFound` means on this endpoint too.
pub(crate) async fn lookup_by_discid(
    musicbrainz: &musicbrainz::MusicBrainz,
    discid: &str,
    class: CallClass,
) -> Result<Vec<(MetadataResult, crate::payloads::DiscIdRelease)>, Failure> {
    let releases = match musicbrainz.lookup_by_discid(discid, class).await {
        Ok(releases) => releases,
        Err(musicbrainz::MusicBrainzError::NotFound(_)) => return Ok(Vec::new()),
        Err(e) => {
            return Err(mb_failure(
                &e,
                &format!("reading MusicBrainz's answer for Disc ID {discid}"),
            ))
        }
    };

    Ok(mb_discid_releases_to_metadata(discid, releases))
}

/// Every release MusicBrainz has a recording registered under any of `isrcs`
/// on, each once, in the order the search answered, with its cover art.
/// `isrcs` are codes as [`crate::isrc::code`] reads them; a recording the
/// search returned under none of them answers nothing.
pub(crate) async fn lookup_by_isrcs(
    musicbrainz: &musicbrainz::MusicBrainz,
    isrcs: &[String],
    class: CallClass,
) -> Result<Vec<MetadataResult>, Failure> {
    let recordings =
        musicbrainz.search_recordings_by_isrcs(isrcs, class).await.map_err(|error| {
            mb_failure(&error, &format!("reading MusicBrainz's answer for ISRCs {isrcs:?}"))
        })?;
    Ok(isrc_releases_to_metadata(isrcs, recordings))
}

fn isrc_releases_to_metadata(
    isrcs: &[String],
    recordings: Vec<musicbrainz::SearchRecording>,
) -> Vec<MetadataResult> {
    let mut seen = std::collections::HashSet::new();
    let mut results = Vec::new();
    for recording in recordings {
        let asked = recording
            .isrcs
            .iter()
            .filter_map(|stated| crate::isrc::code(stated))
            .any(|code| isrcs.contains(&code));
        if !asked {
            continue;
        }
        for mut release in recording.releases {
            if !seen.insert(release.id.clone()) {
                continue;
            }
            if release.artist_credit.is_empty() {
                release.artist_credit = recording.artist_credit.clone();
            }
            let cover_art = RemoteCover::musicbrainz_release(&release.id);
            results.push(search_release_to_metadata(release, Some(cover_art)));
        }
    }
    results
}

#[cfg(test)]
#[path = "search_tests.rs"]
mod tests;
