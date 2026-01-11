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
    pub cover_image: Option<String>,
    pub thumb: Option<String>,
    pub artists: Vec<DiscogsArtist>,
    pub tracklist: Vec<DiscogsTrack>,
    pub master_id: String,
}
/// Represents a track from Discogs
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiscogsTrack {
    pub position: String,
    pub title: String,
    pub duration: Option<String>,
}
