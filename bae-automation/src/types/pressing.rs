//! What a pressing is, as automation reads and writes it: bae-core's typed
//! facts, serialized in the keys bae stores them under. The values cross
//! as bae-core's own types, so a key outside a vocabulary fails to parse
//! rather than turning into text; the schemas below list each vocabulary's
//! keys for a client to read.

use super::*;
use bae_core::pressing::{
    Country, DiscogsDetail, MediaCount, Medium, Packaging, Region, ReleaseArea, ReleaseStatus,
};

/// One choice of what a candidate's pressing is, from bae's vocabulary.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationPressingFactEdit {
    Area {
        #[schemars(schema_with = "area_schema")]
        area: Option<ReleaseArea>,
    },
    Media {
        #[schemars(schema_with = "media_schema")]
        media: Vec<MediaCount>,
    },
    Status {
        #[schemars(schema_with = "status_schema")]
        status: Option<ReleaseStatus>,
    },
    Packaging {
        #[schemars(schema_with = "packaging_schema")]
        packaging: Option<Packaging>,
    },
    DiscogsDetails {
        #[schemars(schema_with = "discogs_details_schema")]
        discogs_details: Vec<DiscogsDetail>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CandidatePressingFactInput {
    pub candidate_key: String,
    pub fact: AutomationPressingFactEdit,
}

/// `bae_core::pressing::PressingFacts` as it serializes.
pub(crate) fn pressing_facts_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "type": "object",
        "properties": {
            "area": area_schema(generator),
            "media": media_schema(generator),
            "status": status_schema(generator),
            "packaging": packaging_schema(generator),
            "discogs_details": discogs_details_schema(generator),
        },
        "required": ["area", "media", "status", "packaging", "discogs_details"],
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

/// Each carrier with how many of it.
fn media_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "type": "array",
        "items": {
            "type": "object",
            "properties": {
                "medium": { "type": "string", "enum": keys(Medium::ALL, Medium::key) },
                "count": { "type": "integer", "minimum": 1 },
            },
            "required": ["medium", "count"],
        },
    })
}

fn status_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    nullable_key(keys(ReleaseStatus::ALL, ReleaseStatus::key))
}

fn packaging_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    nullable_key(keys(Packaging::ALL, Packaging::key))
}

fn discogs_details_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "type": "array",
        "items": { "type": "string", "enum": keys(DiscogsDetail::ALL, DiscogsDetail::key) },
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
/// `bae_core::album_detail::ReleaseName`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationReleaseName {
    Named {
        name: String,
    },
    Described {
        year: Option<i32>,
        media: Vec<MediaCount>,
    },
    Numbered {
        number: i64,
    },
}

impl AutomationReleaseName {
    pub(crate) fn from_core(name: bae_core::album_detail::ReleaseName) -> Self {
        match name {
            bae_core::album_detail::ReleaseName::Named(name) => Self::Named { name },
            bae_core::album_detail::ReleaseName::Described { year, media } => {
                Self::Described { year, media }
            }
            bae_core::album_detail::ReleaseName::Numbered(number) => Self::Numbered { number },
        }
    }
}

impl AutomationPressingFactEdit {
    pub(crate) fn into_core(self) -> bae_core::import::PressingFactEdit {
        use bae_core::import::PressingFactEdit;
        match self {
            Self::Area { area } => PressingFactEdit::Area(area),
            Self::Media { media } => PressingFactEdit::Media(media),
            Self::Status { status } => PressingFactEdit::Status(status),
            Self::Packaging { packaging } => PressingFactEdit::Packaging(packaging),
            Self::DiscogsDetails { discogs_details } => {
                PressingFactEdit::DiscogsDetails(discogs_details)
            }
        }
    }
}
