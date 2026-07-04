//! MusicBrainz API response shapes.
//!
//! The `serde`-derived structs the client deserializes responses into,
//! `ExternalUrls`, and the relation-to-URL extraction helper. The request
//! and caching logic that produces these lives in the parent module.

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

/// Artist credit entry
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

/// Label info entry
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
    pub format: Option<String>,
    #[serde(default)]
    pub tracks: Vec<MbTrack>,
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
}

impl MbReleaseResponse {
    /// Extract ExternalUrls from release relations and release-group relations
    pub(super) fn extract_external_urls(&self) -> ExternalUrls {
        let mut urls = ExternalUrls {
            discogs_release_url: None,
        };

        // Extract from release-level relations
        extract_urls_from_relations(&self.relations, &mut urls);

        // Extract from release-group relations (if present inline)
        if let Some(rg) = &self.release_group {
            if let Some(rg_relations) = &rg.relations {
                extract_urls_from_relations(rg_relations, &mut urls);
            }
        }

        urls
    }

    /// Count total tracks across all media
    pub fn track_count(&self) -> usize {
        self.media.iter().map(|m| m.tracks.len()).sum()
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

/// Extract external URLs from a list of relations into the target struct
pub(super) fn extract_urls_from_relations(relations: &[MbRelation], urls: &mut ExternalUrls) {
    for relation in relations {
        let Some(url_obj) = &relation.url else {
            continue;
        };
        let Some(resource) = &url_obj.resource else {
            continue;
        };

        if resource.contains("discogs.com/release/") && urls.discogs_release_url.is_none() {
            urls.discogs_release_url = Some(resource.clone());
            break;
        }
    }
}

/// External URLs extracted from MusicBrainz relationships
#[derive(Debug, Clone)]
pub struct ExternalUrls {
    pub discogs_release_url: Option<String>,
}
