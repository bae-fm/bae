use super::*;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AutomationReleaseUserEdit {
    pub album_title: String,
    pub album_artist_assignments: Vec<AutomationArtistAssignment>,
    pub album_year: Option<i32>,
    pub pressing: AutomationPressingEdit,
    pub tracks: Vec<AutomationTrackUserEdit>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AutomationPressingEdit {
    pub year: Option<i32>,
    pub format: Option<String>,
    pub label: Option<String>,
    pub catalog_number: Option<String>,
    pub country: Option<String>,
    pub barcode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AutomationTrackUserEdit {
    pub title: String,
    pub side: Option<i32>,
    pub track_number: Option<i32>,
    pub artist_assignments: AutomationTrackArtistAssignments,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationArtistAssignment {
    Picked { artist: AutomationExistingArtist },
    Credit { credit: AutomationArtistCredit },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AutomationExistingArtist {
    pub artist_id: String,
    pub name: String,
    pub sort_name: Option<String>,
    pub musicbrainz_artist_id: Option<String>,
    pub discogs_artist_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AutomationArtistCredit {
    pub name: String,
    pub sort_name: Option<String>,
    pub musicbrainz_artist_id: Option<String>,
    pub discogs_artist_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationTrackArtistAssignments {
    AlbumArtists,
    Explicit {
        assignments: Vec<AutomationArtistAssignment>,
    },
}
