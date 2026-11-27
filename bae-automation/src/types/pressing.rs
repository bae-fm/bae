//! What a pressing is, as automation reads and writes it: bae-library's typed
//! facts, serialized in the keys bae stores them under. The values cross
//! as bae-library's own types, so a key outside a vocabulary fails to parse
//! rather than turning into text; the schemas below list each vocabulary's
//! keys for a client to read.

use super::*;
use bae_types::pressing::{
    Country, DiscogsDetail, MediaCount, Medium, Packaging, Region, ReleaseArea, ReleaseStatus,
};

/// One choice of what a candidate's or a library release's pressing is, from
/// bae's vocabulary.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationPressingFactEdit {
    Area {
        #[schemars(schema_with = "area_schema")]
        area: Option<ReleaseArea>,
    },
    /// How many of one medium the pressing holds: zero removes the medium, a
    /// medium the pressing lacks is added after the others, and the other
    /// counts stay as they are.
    MediumCount {
        #[schemars(schema_with = "medium_schema")]
        medium: Medium,
        count: u32,
    },
    Status {
        #[schemars(schema_with = "status_schema")]
        status: Option<ReleaseStatus>,
    },
    Packaging {
        #[schemars(schema_with = "packaging_schema")]
        packaging: Option<Packaging>,
    },
    /// Add one Discogs detail; one the pressing holds stays where it is.
    AddDiscogsDetail {
        #[schemars(schema_with = "discogs_detail_schema")]
        detail: DiscogsDetail,
    },
    /// Remove one Discogs detail.
    RemoveDiscogsDetail {
        #[schemars(schema_with = "discogs_detail_schema")]
        detail: DiscogsDetail,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CandidatePressingFactInput {
    pub candidate_key: String,
    pub fact: AutomationPressingFactEdit,
}

/// `Vec<bae_types::pressing::ReleaseLabel>` as it serializes: each label's
/// name and catalog number, at least one of them.
pub(crate) fn labels_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "type": "array",
        "items": {
            "type": "object",
            "properties": {
                "name": { "type": "string" },
                "catalog_number": { "type": "string" },
            },
            "anyOf": [{ "required": ["name"] }, { "required": ["catalog_number"] }],
            "additionalProperties": false,
        },
    })
}

/// Where a pressing was released: `{"Country": "<ISO 3166-1 code>"}`,
/// `{"Region": "<region key>"}`, or null.
fn area_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    let countries: Vec<&str> = Country::all().map(Country::code).collect();
    schemars::json_schema!({
        "oneOf": [
            { "type": "null" },
            {
                "type": "object",
                "properties": { "Country": { "type": "string", "enum": countries } },
                "required": ["Country"],
            },
            {
                "type": "object",
                "properties": { "Region": { "type": "string", "enum": keys(Region::ALL, Region::key) } },
                "required": ["Region"],
            },
        ],
    })
}

/// A medium's key.
fn medium_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({ "type": "string", "enum": keys(Medium::ALL, Medium::key) })
}

fn status_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    nullable_key(keys(ReleaseStatus::ALL, ReleaseStatus::key))
}

fn packaging_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    nullable_key(keys(Packaging::ALL, Packaging::key))
}

/// A Discogs detail's key.
fn discogs_detail_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "type": "string",
        "enum": keys(DiscogsDetail::ALL, DiscogsDetail::key),
    })
}

fn keys<T: Copy>(all: &[T], key: fn(T) -> &'static str) -> Vec<&'static str> {
    all.iter().map(|value| key(*value)).collect()
}

fn nullable_key(keys: Vec<&'static str>) -> schemars::Schema {
    schemars::json_schema!({
        "oneOf": [
            { "type": "null" },
            { "type": "string", "enum": keys },
        ],
    })
}

/// What a library release is called where a list names it. Mirrors
/// `bae_views::album_detail::ReleaseName`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationReleaseName {
    Named { name: String },
    Described { year: Option<i32>, media: Vec<MediaCount> },
    Numbered { number: i64 },
}

impl AutomationReleaseName {
    pub(crate) fn from_core(name: bae_views::album_detail::ReleaseName) -> Self {
        match name {
            bae_views::album_detail::ReleaseName::Named(name) => Self::Named { name },
            bae_views::album_detail::ReleaseName::Described { year, media } => {
                Self::Described { year, media }
            }
            bae_views::album_detail::ReleaseName::Numbered(number) => Self::Numbered { number },
        }
    }
}

impl AutomationPressingFactEdit {
    pub(crate) fn into_core(self) -> bae_import::PressingFactEdit {
        use bae_import::PressingFactEdit;
        match self {
            Self::Area { area } => PressingFactEdit::Area(area),
            Self::MediumCount { medium, count } => {
                PressingFactEdit::MediumCount(MediaCount { medium, count })
            }
            Self::Status { status } => PressingFactEdit::Status(status),
            Self::Packaging { packaging } => PressingFactEdit::Packaging(packaging),
            Self::AddDiscogsDetail { detail } => PressingFactEdit::AddDiscogsDetail(detail),
            Self::RemoveDiscogsDetail { detail } => PressingFactEdit::RemoveDiscogsDetail(detail),
        }
    }
}
