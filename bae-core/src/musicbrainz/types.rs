//! MusicBrainz API response shapes.
//!
//! The `serde`-derived structs the client deserializes responses into, and
//! the relation-to-URL extraction helper. The request and caching logic that
//! produces these lives in the parent module.

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
    #[serde(rename = "catalog-number")]
    pub catalog_number: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MbLabel {
    pub name: Option<String>,
}

/// The label name and catalog number a release is pressed under: MusicBrainz
/// lists one entry per label involved, and we take the first. Read by every
/// projection of a release — the search result, the picker detail, and the
/// committed pressing — so they cannot pick different labels.
pub fn label_and_catno(label_info: &[MbLabelInfo]) -> (Option<String>, Option<String>) {
    label_info
        .first()
        .map(|li| {
            (
                li.label.as_ref().and_then(|l| l.name.clone()),
                li.catalog_number.clone(),
            )
        })
        .unwrap_or((None, None))
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
    pub date: Option<String>,
    pub country: Option<String>,
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

    /// The Discogs release URL from this release's url-rels, falling back to
    /// the inline release-group relations when the release-level ones carry
    /// none.
    pub fn discogs_release_url(&self) -> Option<String> {
        first_discogs_release_url(&self.relations).or_else(|| {
            self.release_group
                .as_ref()
                .and_then(|rg| rg.relations.as_deref())
                .and_then(first_discogs_release_url)
        })
    }
}

/// Response from the disc ID lookup endpoint
#[derive(Debug, Clone, Deserialize)]
pub(super) struct DiscIdResponse {
    #[serde(default)]
    pub(super) releases: Vec<MbReleaseResponse>,
}

/// Response from the release search endpoint
#[derive(Debug, Clone, Deserialize, Serialize)]
pub(super) struct SearchResponse {
    #[serde(default)]
    pub(super) releases: Vec<SearchRelease>,
    pub(super) error: Option<String>,
}

/// A release in search results (less data than full lookup)
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SearchRelease {
    pub id: String,
    pub title: String,
    pub date: Option<String>,
    pub country: Option<String>,
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
}

/// Release group response (for separate fetch with url-rels)
#[derive(Debug, Clone, Deserialize)]
pub(super) struct ReleaseGroupResponse {
    #[serde(default)]
    pub(super) relations: Vec<MbRelation>,
}

/// Response from the MB URL lookup endpoint (used for Discogs -> MB cross-reference)
#[derive(Debug, Deserialize)]
pub(super) struct UrlLookupResponse {
    #[serde(default)]
    pub(super) relations: Vec<UrlLookupRelation>,
}

#[derive(Debug, Deserialize)]
pub(super) struct UrlLookupRelation {
    #[serde(rename = "type")]
    pub(super) relation_type: Option<String>,
    pub(super) release: Option<UrlLookupRelease>,
}

#[derive(Debug, Deserialize)]
pub(super) struct UrlLookupRelease {
    pub(super) id: Option<String>,
}

/// The first Discogs release URL among a set of MB relations, if any.
pub(super) fn first_discogs_release_url(relations: &[MbRelation]) -> Option<String> {
    relations
        .iter()
        .filter_map(|r| r.url.as_ref()?.resource.as_deref())
        .find(|resource| resource.contains("discogs.com/release/"))
        .map(str::to_string)
}
