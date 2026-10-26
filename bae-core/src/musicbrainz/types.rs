//! MusicBrainz API response shapes.
//!
//! The `serde`-derived structs the client deserializes responses into, and
//! the relation-to-URL extraction helper. The request and caching logic that
//! produces these lives in the parent module.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// A relation from MusicBrainz. Release/release-group URL relations fill `url`;
/// recording and work relations fill `artist` or `work` plus typed relation fields.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct MbRelation {
    pub url: Option<MbUrlResource>,
    #[serde(rename = "target-type")]
    pub target_type: Option<String>,
    #[serde(
        rename = "type",
        default,
        deserialize_with = "crate::serde_helpers::empty_string_as_none"
    )]
    pub relation_type: Option<String>,
    pub direction: Option<String>,
    pub artist: Option<MbArtistRef>,
    pub work: Option<MbWork>,
    #[serde(
        rename = "target-credit",
        default,
        deserialize_with = "crate::serde_helpers::empty_string_as_none"
    )]
    pub target_credit: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MbUrlResource {
    pub resource: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MbArtistCredit {
    pub name: String,
    pub artist: Option<MbArtistRef>,
    /// What MusicBrainz prints after the credit (" feat. ", " & "); empty or
    /// absent for nothing.
    #[serde(default)]
    pub joinphrase: Option<String>,
}

/// Reference to a MusicBrainz artist within an artist-credit
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MbArtistRef {
    pub id: Option<String>,
    pub name: Option<String>,
    #[serde(rename = "sort-name")]
    pub sort_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MbLabelInfo {
    pub label: Option<MbLabel>,
    #[serde(
        rename = "catalog-number",
        default,
        deserialize_with = "crate::serde_helpers::empty_string_as_none"
    )]
    pub catalog_number: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MbLabel {
    #[serde(
        default,
        deserialize_with = "crate::serde_helpers::empty_string_as_none"
    )]
    pub name: Option<String>,
}

/// Every label a release is pressed under, with its catalog number, in
/// MusicBrainz's order.
pub fn release_labels(label_info: &[MbLabelInfo]) -> Vec<bae_types::pressing::ReleaseLabel> {
    bae_types::pressing::ReleaseLabel::list(label_info.iter().map(|info| {
        (
            info.label.as_ref().and_then(|label| label.name.clone()),
            info.catalog_number.clone(),
        )
    }))
}

/// Release group as embedded in a release response
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MbReleaseGroupRef {
    pub id: String,
    #[serde(rename = "first-release-date")]
    pub first_release_date: Option<String>,
    #[serde(default)]
    pub relations: Option<Vec<MbRelation>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MbWork {
    pub id: String,
    pub title: String,
    pub disambiguation: Option<String>,
    #[serde(rename = "type")]
    pub work_type: Option<String>,
    #[serde(default)]
    pub relations: Vec<MbRelation>,
}

/// A recording within a track
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MbRecording {
    pub id: Option<String>,
    pub title: Option<String>,
    #[serde(rename = "artist-credit", default)]
    pub artist_credit: Vec<MbArtistCredit>,
    #[serde(default)]
    pub relations: Vec<MbRelation>,
}

/// A track within a medium
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MbTrack {
    pub position: Option<i64>,
    pub number: Option<String>,
    pub title: Option<String>,
    pub length: Option<u64>,
    pub recording: Option<MbRecording>,
    #[serde(rename = "artist-credit", default)]
    pub artist_credit: Vec<MbArtistCredit>,
}

/// A medium (disc) within a release
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MbMedium {
    #[serde(default)]
    pub discs: Vec<MbDisc>,
    #[serde(
        default,
        deserialize_with = "crate::serde_helpers::empty_string_as_none"
    )]
    pub format: Option<String>,
    #[serde(default)]
    pub tracks: Vec<MbTrack>,
}

/// A MusicBrainz DiscID registered against a medium.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MbDisc {
    pub id: String,
}

/// What a release document says the Cover Art Archive holds for it. Every
/// endpoint that returns a whole release — the release lookup, the disc-ID
/// lookup, the release browse — carries this block, so it is the release's own
/// statement about its artwork and needs no separate call to the archive.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MbCoverArtArchive {
    /// The archive holds an image marked as this release's front cover.
    pub front: bool,
    /// The archive has taken this release's art down and serves none of it,
    /// whatever the other fields say.
    pub darkened: bool,
}

/// A full release as returned by the MB release lookup API
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MbReleaseResponse {
    pub id: String,
    pub title: String,
    /// What an editor wrote to tell this release apart from its album's
    /// others ("made in West Germany", "club edition"). Absent where none is
    /// set.
    #[serde(
        default,
        deserialize_with = "crate::serde_helpers::empty_string_as_none"
    )]
    pub disambiguation: Option<String>,
    /// The release's annotation: the free text editors write about it — its
    /// matrix, its SID codes, where it was made. Only a lookup that asks for
    /// it (`inc=annotation`) states it; the disc ID lookup cannot ask.
    /// Absent where none is written.
    #[serde(
        default,
        deserialize_with = "crate::serde_helpers::empty_string_as_none"
    )]
    pub annotation: Option<String>,
    pub date: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::serde_helpers::empty_string_as_none"
    )]
    pub country: Option<String>,
    /// How official the release is, as MusicBrainz names it ("Official",
    /// "Promotion", …). Absent where no status is set.
    #[serde(
        default,
        deserialize_with = "crate::serde_helpers::empty_string_as_none"
    )]
    pub status: Option<String>,
    /// The release's outermost packaging, as MusicBrainz names it ("Jewel
    /// Case", "Digipak", …). Absent where none is set.
    #[serde(
        default,
        deserialize_with = "crate::serde_helpers::empty_string_as_none"
    )]
    pub packaging: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::serde_helpers::empty_string_as_none"
    )]
    pub barcode: Option<String>,
    #[serde(rename = "artist-credit", default)]
    pub artist_credit: Vec<MbArtistCredit>,
    #[serde(rename = "release-group")]
    pub release_group: Option<MbReleaseGroupRef>,
    #[serde(rename = "label-info", default)]
    pub label_info: Vec<MbLabelInfo>,
    #[serde(default)]
    pub media: Vec<MbMedium>,
    #[serde(default)]
    pub relations: Vec<MbRelation>,
    #[serde(rename = "cover-art-archive")]
    pub cover_art_archive: MbCoverArtArchive,
}

impl MbReleaseResponse {
    /// Whether the Cover Art Archive serves a front image for this release: one
    /// is registered and the archive has not darkened the release's art.
    pub fn has_front_cover(&self) -> bool {
        self.cover_art_archive.front && !self.cover_art_archive.darkened
    }
}

/// Response from the disc ID lookup endpoint: each release's whole document,
/// as MusicBrainz sent it.
#[derive(Debug, Deserialize)]
pub(super) struct DiscIdResponse {
    #[serde(default)]
    pub(super) releases: Vec<Box<serde_json::value::RawValue>>,
}

/// Response from the release search endpoint
#[derive(Debug, Clone, Deserialize, Serialize)]
pub(super) struct SearchResponse {
    #[serde(default)]
    pub(super) releases: Vec<SearchRelease>,
    pub(super) error: Option<String>,
}

/// One page of what the recording search matched.
#[derive(Debug, Clone, Deserialize)]
pub(super) struct RecordingSearchResponse {
    /// How many recordings match in all, across every page.
    pub(super) count: usize,
    #[serde(default)]
    pub(super) recordings: Vec<SearchRecording>,
}

/// A recording the search matched: the ISRCs it is registered under, its
/// credit, and every release it is on, each as the release search states one.
/// A release states its own credit only where it is not the recording's.
#[derive(Debug, Clone, Deserialize)]
pub struct SearchRecording {
    #[serde(default)]
    pub isrcs: Vec<String>,
    #[serde(rename = "artist-credit", default)]
    pub artist_credit: Vec<MbArtistCredit>,
    #[serde(default)]
    pub releases: Vec<SearchRelease>,
}

/// A release in search results (less data than full lookup)
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SearchRelease {
    pub id: String,
    pub title: String,
    /// What an editor wrote to tell this release apart from its album's
    /// others. Absent where none is set.
    #[serde(
        default,
        deserialize_with = "crate::serde_helpers::empty_string_as_none"
    )]
    pub disambiguation: Option<String>,
    pub date: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::serde_helpers::empty_string_as_none"
    )]
    pub country: Option<String>,
    /// How official the release is, as MusicBrainz names it ("Official",
    /// "Promotion", …). Absent where no status is set.
    #[serde(
        default,
        deserialize_with = "crate::serde_helpers::empty_string_as_none"
    )]
    pub status: Option<String>,
    /// The release's outermost packaging, as MusicBrainz names it ("Jewel
    /// Case", "Digipak", …). Absent where none is set.
    #[serde(
        default,
        deserialize_with = "crate::serde_helpers::empty_string_as_none"
    )]
    pub packaging: Option<String>,
    /// `ws/2/release?query=` states the pressing's barcode. An empty string
    /// means the release carries none.
    #[serde(
        default,
        deserialize_with = "crate::serde_helpers::empty_string_as_none"
    )]
    pub barcode: Option<String>,
    #[serde(rename = "artist-credit", default)]
    pub artist_credit: Vec<MbArtistCredit>,
    #[serde(rename = "release-group")]
    pub release_group: Option<MbReleaseGroupRef>,
    #[serde(rename = "label-info", default)]
    pub label_info: Vec<MbLabelInfo>,
    /// Each medium's format, in medium order. The search response states the
    /// format and a track count per medium, and no tracks.
    #[serde(default)]
    pub media: Vec<MbSearchMedium>,
}

/// One medium as `ws/2/release?query=` states it.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MbSearchMedium {
    #[serde(
        default,
        deserialize_with = "crate::serde_helpers::empty_string_as_none"
    )]
    pub format: Option<String>,
}

/// Release group response (for separate fetch with url-rels)
#[derive(Debug, Clone, Deserialize)]
pub struct ReleaseGroupResponse {
    pub id: String,
    #[serde(
        default,
        deserialize_with = "crate::serde_helpers::empty_string_as_none"
    )]
    pub title: Option<String>,
    #[serde(
        rename = "first-release-date",
        default,
        deserialize_with = "crate::serde_helpers::empty_string_as_none"
    )]
    pub first_release_date: Option<String>,
    #[serde(rename = "artist-credit", default)]
    pub artist_credit: Vec<MbArtistCredit>,
    #[serde(default)]
    pub relations: Vec<MbRelation>,
}

/// One archived release-group document, parsed.
pub fn parse_release_group(json: &str) -> Result<ReleaseGroupResponse, serde_json::Error> {
    serde_json::from_str(json)
}

/// A Discogs page MusicBrainz is asked about by its address.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DiscogsPage {
    /// A release's page, which MusicBrainz releases link as themselves.
    Release(String),
    /// A master's page, which MusicBrainz release groups link as their album.
    Master(String),
}

impl DiscogsPage {
    /// The address MusicBrainz files the page's links under.
    pub(super) fn resource(&self) -> String {
        match self {
            Self::Release(id) => crate::catalog::Catalog::Discogs.release_url(id),
            Self::Master(id) => crate::catalog::Catalog::Discogs.album_url(id),
        }
    }

    /// The relationships a lookup includes to answer for this page.
    pub(super) fn include(&self) -> &'static str {
        match self {
            Self::Release(_) => "release-rels",
            Self::Master(_) => "release-group-rels",
        }
    }

    /// The MusicBrainz ids `relations` link this page from as their Discogs
    /// page — releases for a release's page, release groups for a master's —
    /// each once, in id order.
    fn linked_ids(&self, relations: Vec<UrlLookupRelation>) -> Vec<String> {
        let mut ids: Vec<_> = relations
            .into_iter()
            .filter(|relation| relation.relation_type.as_deref() == Some("discogs"))
            .filter_map(|relation| match (self, relation.target) {
                (Self::Release(_), UrlLookupTarget::Release { release }) => Some(release.id),
                (Self::Master(_), UrlLookupTarget::Group { release_group }) => {
                    Some(release_group.id)
                }
                _ => None,
            })
            .collect();
        ids.sort();
        ids.dedup();
        ids
    }

    /// The relation MusicBrainz states from the entity `id` to this page.
    #[cfg(any(test, feature = "test-utils"))]
    pub(super) fn relation_json(&self, id: &str) -> serde_json::Value {
        match self {
            Self::Release(_) => serde_json::json!({
                "type": "discogs", "target-type": "release", "release": { "id": id }
            }),
            Self::Master(_) => serde_json::json!({
                "type": "discogs", "target-type": "release_group", "release_group": { "id": id }
            }),
        }
    }
}

/// The URL endpoint's answer about one address.
#[derive(Debug, Deserialize)]
struct UrlDocument {
    relations: Vec<UrlLookupRelation>,
}

/// The URL endpoint's answer about several addresses: a document for each one
/// MusicBrainz knows, naming its address. One it does not know is left out.
#[derive(Debug, Deserialize)]
struct UrlDocuments {
    urls: Vec<UrlDocumentEntry>,
}

#[derive(Debug, Deserialize)]
struct UrlDocumentEntry {
    resource: String,
    relations: Vec<UrlLookupRelation>,
}

#[derive(Debug, Deserialize)]
struct UrlLookupRelation {
    #[serde(rename = "type")]
    relation_type: Option<String>,
    #[serde(flatten)]
    target: UrlLookupTarget,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "target-type")]
enum UrlLookupTarget {
    #[serde(rename = "release")]
    Release { release: UrlLookupEntity },
    /// MusicBrainz spells this target with an underscore in URL lookups —
    /// `"target-type": "release_group"` beside a `"release_group"` object —
    /// where every other document it serves says `release-group`.
    #[serde(rename = "release_group")]
    Group { release_group: UrlLookupEntity },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
struct UrlLookupEntity {
    id: String,
}

/// The MusicBrainz ids each of `pages` is linked from, read from the URL
/// endpoint's answer to asking about exactly those pages. One page is answered
/// with its own document; several, with the documents of the ones MusicBrainz
/// knows, and a page left out of that answer is left out of this one.
pub(super) fn parse_discogs_lookup(
    pages: &[DiscogsPage],
    json: &str,
) -> Result<HashMap<DiscogsPage, Vec<String>>, String> {
    if let [page] = pages {
        let document: UrlDocument = serde_json::from_str(json).map_err(|e| e.to_string())?;
        return Ok(HashMap::from([(
            page.clone(),
            page.linked_ids(document.relations),
        )]));
    }
    let documents: UrlDocuments = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let asked: HashMap<String, &DiscogsPage> =
        pages.iter().map(|page| (page.resource(), page)).collect();
    let mut linked = HashMap::new();
    for entry in documents.urls {
        let Some(page) = asked.get(&entry.resource) else {
            return Err(format!(
                "the answer names {}, which was not asked about",
                entry.resource
            ));
        };
        linked.insert((*page).clone(), page.linked_ids(entry.relations));
    }
    Ok(linked)
}

/// Every address a set of relations states, in relation order. A relation that
/// names an artist, a work or a recording rather than a URL states none.
pub fn relation_urls(relations: &[MbRelation]) -> impl Iterator<Item = &str> {
    relations
        .iter()
        .filter_map(|r| r.url.as_ref()?.resource.as_deref())
}
