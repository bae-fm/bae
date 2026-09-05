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
    pub covers: Vec<RemoteCover>,
    pub catno: Option<String>,
    pub artists: Vec<DiscogsArtist>,
    pub extraartists: Option<Vec<DiscogsRoleArtist>>,
    pub tracklist: Vec<DiscogsTrack>,
    pub master_id: Option<String>,
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
