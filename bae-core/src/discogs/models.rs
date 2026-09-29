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
    pub formats: Vec<DiscogsFormat>,
    pub country: Option<String>,
    /// Every label with its catalog number, in Discogs's order.
    pub labels: Vec<crate::pressing::ReleaseLabel>,
    pub covers: Vec<RemoteCover>,
    pub barcode: Option<String>,
    pub artists: Vec<DiscogsArtist>,
    pub extraartists: Option<Vec<DiscogsRoleArtist>>,
    pub tracklist: Vec<DiscogsTrack>,
    pub master_id: Option<String>,
    /// The name of every company the release credits — who pressed, made or
    /// distributed it — in Discogs's order, whatever its role.
    pub companies: Vec<String>,
    /// What each of its identifiers other than a barcode reads — matrix /
    /// runout, rights society, label code, price code, SID codes, SPARS,
    /// other — in Discogs's order.
    pub identifiers: Vec<String>,
    /// The release's notes: the prose an editor wrote about it, as Discogs
    /// holds it. Absent where none is written.
    pub notes: Option<String>,
}

/// One entry of a release's `formats`, as Discogs states it: a name from
/// its format list, how many of that medium the release holds, descriptions
/// from its description list, and what an editor wrote beside them. The
/// search and release endpoints state the same shape.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct DiscogsFormat {
    pub name: String,
    /// The count, as the string Discogs writes it.
    #[serde(default = "one")]
    pub qty: String,
    #[serde(default)]
    pub descriptions: Vec<String>,
    /// Free text beside the format ("Small label", "Club Edition"); absent
    /// where none is written.
    #[serde(
        default,
        deserialize_with = "crate::serde_helpers::empty_string_as_none"
    )]
    pub text: Option<String>,
}

fn one() -> String {
    "1".to_string()
}

/// Album metadata stated by a master, independent of any particular pressing.
#[derive(Debug, Clone, PartialEq)]
pub struct DiscogsMaster {
    pub title: Option<String>,
    pub year: Option<u32>,
    pub artists: Vec<DiscogsArtist>,
    pub covers: Vec<RemoteCover>,
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
