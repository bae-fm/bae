use crate::discogs::remote_cover_from_urls;
use crate::import::cover_art::RemoteCover;
use serde::{Deserialize, Serialize};

/// Artist credit from Discogs
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiscogsArtist {
    pub id: String,
    pub name: String,
}
/// Represents a Discogs release search result
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiscogsRelease {
    pub id: String,
    pub title: String,
    pub year: Option<u32>,
    pub genre: Vec<String>,
    pub style: Vec<String>,
    pub format: Vec<String>,
    pub country: Option<String>,
    pub label: Vec<String>,
    #[serde(default, deserialize_with = "crate::discogs::empty_string_as_none")]
    pub cover_image: Option<String>,
    #[serde(default, deserialize_with = "crate::discogs::empty_string_as_none")]
    pub thumb: Option<String>,
    pub catno: Option<String>,
    pub artists: Vec<DiscogsArtist>,
    pub tracklist: Vec<DiscogsTrack>,
    pub master_id: Option<String>,
}

impl DiscogsRelease {
    pub fn remote_cover(&self) -> Option<RemoteCover> {
        remote_cover_from_urls(
            self.cover_image.as_deref(),
            self.thumb.as_deref(),
            "release",
            self.id.as_str(),
        )
    }
}

/// Represents a track from Discogs
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiscogsTrack {
    pub position: String,
    pub title: String,
    pub duration: Option<String>,
    #[serde(default)]
    pub artists: Vec<DiscogsArtist>,
    /// Track type: "track", "heading", or "index"
    #[serde(default)]
    pub type_: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::MetadataSource;

    fn release_with_cover_fields(cover_image: Option<&str>, thumb: Option<&str>) -> DiscogsRelease {
        DiscogsRelease {
            id: "discogs-release-1".to_string(),
            title: "Album Title".to_string(),
            year: None,
            genre: vec![],
            style: vec![],
            format: vec![],
            country: None,
            label: vec![],
            cover_image: cover_image.map(str::to_string),
            thumb: thumb.map(str::to_string),
            catno: None,
            artists: vec![DiscogsArtist {
                id: "discogs-artist-1".to_string(),
                name: "Artist Name".to_string(),
            }],
            tracklist: vec![],
            master_id: None,
        }
    }

    #[test]
    fn remote_cover_uses_thumb_as_cover_when_cover_image_is_absent() {
        let release = release_with_cover_fields(None, Some("https://discogs.example/thumb.jpg"));

        let cover = release.remote_cover().unwrap();

        assert_eq!(cover.url, "https://discogs.example/thumb.jpg");
        assert_eq!(cover.thumbnail_url, "https://discogs.example/thumb.jpg");
        assert_eq!(cover.label, MetadataSource::Discogs.cover_source_label());
        assert_eq!(cover.source, MetadataSource::Discogs);
    }

    #[test]
    fn remote_cover_is_absent_without_cover_fields() {
        let release = release_with_cover_fields(None, None);

        assert!(release.remote_cover().is_none());
    }
}
