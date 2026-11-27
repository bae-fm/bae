use super::*;

/// The metadata a candidate commits with, as automation reads it.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationReleaseUserEdit {
    pub album_title: String,
    pub album_artist_assignments: Vec<AutomationCredited>,
    pub album_year: Option<i32>,
    pub pressing: AutomationPressingEdit,
    pub tracks: Vec<AutomationTrackUserEdit>,
}

/// Mirrors `bae_types::pressing::Pressing`.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationPressingEdit {
    pub year: Option<i32>,
    pub labels: Vec<bae_types::pressing::ReleaseLabel>,
    /// Every barcode the pressing is printed with, in order.
    pub barcodes: Vec<String>,
    pub facts: bae_types::pressing::PressingFacts,
}

/// A change of a library release's typed metadata. Mirrors
/// `bae_import::ReleaseMetadataEdit`; what the pressing is changes one choice
/// at a time, through `release_pressing_fact_set`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AutomationReleaseMetadataEdit {
    pub album_title: String,
    pub album_artist_assignments: Vec<AutomationCredited>,
    pub album_year: Option<i32>,
    pub pressing: AutomationTypedPressing,
    pub tracks: Vec<AutomationTrackUserEdit>,
}

/// Mirrors `bae_import::TypedPressing`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AutomationTypedPressing {
    pub year: Option<i32>,
    #[schemars(schema_with = "labels_schema")]
    pub labels: Vec<bae_types::pressing::ReleaseLabel>,
    /// Every barcode the pressing is printed with, in order.
    pub barcodes: Vec<String>,
}

/// One track's editable fields: its title and artists. Its medium, side and
/// number are the source's and not edited.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AutomationTrackUserEdit {
    pub title: String,
    pub artist_assignments: AutomationTrackArtistAssignments,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationArtistAssignment {
    /// A library artist, printed by `credit_name` (one of its other printed
    /// names) where one is given, else by its own name.
    Picked {
        artist: AutomationExistingArtist,
        credit_name: Option<AutomationCreditName>,
    },
    Credit {
        credit: AutomationArtistCredit,
    },
}

/// One place in a credit list: who is credited, and what is printed after it
/// when another credit follows (`None`: ", ").
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AutomationCredited {
    pub artist: AutomationArtistAssignment,
    pub join: Option<String>,
}

/// One of an artist's other printed names: its folded key and the name.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AutomationCreditName {
    pub key: String,
    pub name: String,
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
    /// The name the source printed for the artist, where it is another.
    pub credited_as: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationTrackArtistAssignments {
    AlbumArtists,
    Explicit { assignments: Vec<AutomationCredited> },
}
