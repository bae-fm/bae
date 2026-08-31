use crate::discogs::remote_cover_from_urls;
use crate::import::cover_art::RemoteCover;

#[derive(Debug, Clone, PartialEq)]
pub struct DiscogsArtist {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DiscogsRoleArtist {
    pub id: Option<String>,
    pub name: String,
    pub role: String,
    pub credited_name: Option<String>,
}
/// The release-endpoint response, projected to the fields bae uses.
#[derive(Debug, Clone, PartialEq)]
pub struct DiscogsRelease {
    pub id: String,
    pub title: String,
    pub year: Option<u32>,
    pub format: Vec<String>,
    pub country: Option<String>,
    pub label: Vec<String>,
    pub cover_image: Option<String>,
    pub thumb: Option<String>,
    pub catno: Option<String>,
    pub artists: Vec<DiscogsArtist>,
    pub extraartists: Option<Vec<DiscogsRoleArtist>>,
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

#[derive(Debug, Clone, PartialEq)]
pub struct DiscogsTrack {
    pub position: String,
    pub title: String,
    pub duration: Option<String>,
    pub artists: Vec<DiscogsArtist>,
    pub extraartists: Option<Vec<DiscogsRoleArtist>>,
    /// Track type: "track", "heading", or "index"
    pub type_: String,
    /// Child entries owned by an index row. Discogs uses this shape for a
    /// suite or other grouped work whose children may be ripped separately.
    pub sub_tracks: Vec<DiscogsTrack>,
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
            extraartists: Some(vec![]),
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
