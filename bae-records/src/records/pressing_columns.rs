//! A pressing's facts as the columns every table that stores them keeps them
//! in: `country` or `region`, `media` (a JSON array of carriers and counts, or
//! a library release's `release_media` rows read as one), `status`,
//! `packaging`, and `discogs_details` (a JSON array of keys, or a library
//! release's `release_discogs_details` rows read as one) — and its
//! `labels`, a list of label rows read as a JSON array of each label's name
//! and catalog number.
//!
//! One writer and one reader, so every table spells the facts the same way
//! and a value no vocabulary names fails the read instead of turning into
//! nothing.

use bae_types::pressing::{
    Country, DiscogsDetail, MediaCount, Packaging, PressingFacts, Region, ReleaseArea,
    ReleaseLabel, ReleaseStatus,
};

// The library release's labels as one JSON array of each label's name, as the
// library label it names shows it, and its catalog number, in order; `[]` for
// a release with none. [`labels_of_json`] reads it.
coven::fragment!(
    ReleaseLabels on releases { labels: String },
    r#"
    SELECT json_group_array(json_object(
               'name', shown.name, 'catalog_number', placed.catalog_number)
               ORDER BY placed.position) AS labels
    FROM release_labels AS placed
    LEFT JOIN label_shown_as AS shown_as ON shown_as.label_id = placed.label_id
    LEFT JOIN labels AS shown ON shown.id = shown_as.shown_label_id
    WHERE placed.release_id = @parent.id
"#
);

// The library release's barcodes as one JSON array, in order.
coven::fragment!(
    ReleaseBarcodes on releases { barcodes: String },
    r#"
    SELECT json_group_array(barcode.barcode ORDER BY barcode.position) AS barcodes
    FROM release_barcodes AS barcode
    WHERE barcode.release_id = @parent.id
"#
);

// The library release's media as one JSON array of each carrier and its
// count, in order.
coven::fragment!(
    ReleaseMedia on releases { media: String },
    r#"
    SELECT json_group_array(json_object('medium', counted.medium, 'count', counted.count)
               ORDER BY counted.position) AS media
    FROM release_media AS counted
    WHERE counted.release_id = @parent.id
"#
);

impl ReleaseMedia {
    /// The carriers and their counts.
    pub(crate) fn counts(&self) -> Result<Vec<MediaCount>, bae_store::DbError> {
        document("a release's media", &self.media)
    }
}

// The library release's Discogs details as one JSON array of keys, in order.
coven::fragment!(
    ReleaseDiscogsDetails on releases { discogs_details: String },
    r#"
    SELECT json_group_array(stated.detail ORDER BY stated.position) AS discogs_details
    FROM release_discogs_details AS stated
    WHERE stated.release_id = @parent.id
"#
);

/// One pressing's facts as column values.
#[derive(PartialEq)]
pub struct FactColumns {
    pub country: Option<&'static str>,
    pub region: Option<&'static str>,
    pub status: Option<&'static str>,
    pub packaging: Option<&'static str>,
    pub discogs_details: String,
}

/// An area as the `country` and `region` columns that keep it: at most one
/// of them set.
pub(super) fn area_columns(
    area: Option<ReleaseArea>,
) -> (Option<&'static str>, Option<&'static str>) {
    match area {
        Some(ReleaseArea::Country(country)) => (Some(country.code()), None),
        Some(ReleaseArea::Region(region)) => (None, Some(region.key())),
        None => (None, None),
    }
}

/// The area a `country` and a `region` column hold, or why they hold none a
/// vocabulary names.
pub(super) fn area_of(
    country: Option<String>,
    region: Option<String>,
) -> Result<Option<ReleaseArea>, String> {
    match (country, region) {
        (Some(code), None) => Country::from_code(&code)
            .map(|country| Some(ReleaseArea::Country(country)))
            .ok_or_else(|| format!("country {code:?}")),
        (None, Some(key)) => Region::from_key(&key)
            .map(|region| Some(ReleaseArea::Region(region)))
            .ok_or_else(|| format!("region {key:?}")),
        (None, None) => Ok(None),
        (Some(_), Some(_)) => Err("a row states both a country and a region".to_string()),
    }
}

impl FactColumns {
    pub fn of(facts: &PressingFacts) -> Self {
        let (country, region) = area_columns(facts.area);
        Self {
            country,
            region,
            status: facts.status.map(ReleaseStatus::key),
            packaging: facts.packaging.map(Packaging::key),
            discogs_details: serde_json::to_string(&facts.discogs_details)
                .expect("details serialize"),
        }
    }
}

/// The labels [`ReleaseLabels`] wrote into its document `json`, each with its
/// catalog number.
pub(super) fn labels_of_json(json: &str) -> Result<Vec<ReleaseLabel>, bae_store::DbError> {
    document("a release's labels", json)
}

/// The barcodes [`ReleaseBarcodes`] wrote into its document `json`.
pub(super) fn barcodes_of_json(json: &str) -> Result<Vec<String>, bae_store::DbError> {
    document("a release's barcodes", json)
}

/// A release's facts from its key columns' values and the documents
/// [`ReleaseDiscogsDetails`] and [`ReleaseMedia`] wrote.
pub(super) fn facts_of_columns(
    country: Option<String>,
    region: Option<String>,
    status: Option<&str>,
    packaging: Option<&str>,
    discogs_details: &str,
    media: &str,
) -> Result<PressingFacts, bae_store::DbError> {
    let details = document("a release's Discogs details", discogs_details)?;
    let mut facts = facts_without_media_of(country, region, status, packaging, details)
        .map_err(|reason| bae_store::DbError::unreadable("a release's pressing", reason))?;
    facts.media = document("a release's media", media)?;
    Ok(facts)
}

/// The value the JSON document `json`, named `name`, spells.
fn document<T: serde::de::DeserializeOwned>(
    name: &'static str,
    json: &str,
) -> Result<T, bae_store::DbError> {
    serde_json::from_str(json)
        .map_err(|error| bae_store::DbError::unreadable_json(name, json, error))
}

/// Media as a `media` column holds them: a JSON array of carriers and counts.
#[cfg(desktop)]
pub fn media_column(media: &[MediaCount]) -> String {
    serde_json::to_string(media).expect("media counts serialize")
}

/// Builds a record's facts, except its media, from its key columns' values,
/// for a record read from JSON instead of a row: `country`, `region`,
/// `status`, `packaging` and `discogs_details`. The error names the value no
/// vocabulary names.
pub fn facts_without_media_of(
    country: Option<String>,
    region: Option<String>,
    status: Option<&str>,
    packaging: Option<&str>,
    discogs_details: Vec<DiscogsDetail>,
) -> Result<PressingFacts, String> {
    let area = area_of(country, region)?;
    let status = status
        .map(|key| ReleaseStatus::from_key(key).ok_or_else(|| format!("status {key:?}")))
        .transpose()?;
    let packaging = packaging
        .map(|key| Packaging::from_key(key).ok_or_else(|| format!("packaging {key:?}")))
        .transpose()?;
    Ok(PressingFacts { area, media: Vec::new(), status, packaging, discogs_details })
}

// The ids of the release `:release`'s media rows, in the order of their
// places.
#[cfg(test)]
coven::read!(
    writer MediaRowsOfRelease(release: String) { position: i64 },
    r#"
    SELECT media.id, media.position FROM release_media AS media
    WHERE media.release_id = :release ORDER BY media.position
"#
);

// Each media row: its place, medium and count.
#[cfg(test)]
coven::read!(
    writer MediaRows,
    "SELECT media.id, media.position, media.medium, media.count FROM release_media AS media"
);

// The ids of the rows of the release `:release`'s barcodes, in the order of
// their places.
#[cfg(test)]
coven::read!(
    writer BarcodeRowsOfRelease(release: String),
    r#"
    SELECT barcode.id FROM release_barcodes AS barcode
    WHERE barcode.release_id = :release ORDER BY barcode.position
"#
);

// The ids of the rows of the release `:release`'s Discogs details, in the
// order of their places.
#[cfg(test)]
coven::read!(
    writer DiscogsDetailRowsOfRelease(release: String),
    r#"
    SELECT detail.id FROM release_discogs_details AS detail
    WHERE detail.release_id = :release ORDER BY detail.position
"#
);
