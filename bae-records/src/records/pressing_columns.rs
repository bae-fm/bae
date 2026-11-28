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

use bae_store::read::{column_conversion_error, json_column};
use bae_types::pressing::{
    Country, DiscogsDetail, MediaCount, Packaging, PressingFacts, Region, ReleaseArea,
    ReleaseLabel, ReleaseStatus,
};
use rusqlite::Row;

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

/// The SQL of the library release `release_id`'s labels as one `labels`
/// column: its `release_labels` rows in order, each with the name of the
/// library label it names as that label shows, as the JSON array
/// [`read_labels`] reads.
pub(super) fn release_labels_sql(release_id: &str) -> String {
    format!(
        "(SELECT json_group_array(json_object( \
                 'name', shown.name, 'catalog_number', placed.catalog_number) \
                 ORDER BY placed.position) \
          FROM release_labels placed \
          LEFT JOIN label_shown_as shown_as ON shown_as.label_id = placed.label_id \
          LEFT JOIN labels shown ON shown.id = shown_as.shown_label_id \
          WHERE placed.release_id = {release_id})"
    )
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

/// Read a `labels` column: every label, each with its catalog number.
pub(super) fn read_labels(row: &Row, column: &str) -> rusqlite::Result<Vec<ReleaseLabel>> {
    json_column(row, column)
}

/// The SQL of the library release `release_id`'s barcodes as one `barcodes`
/// column: its `release_barcodes` rows in order, as the JSON array
/// [`read_barcodes`] reads.
pub(super) fn release_barcodes_sql(release_id: &str) -> String {
    format!(
        "(SELECT json_group_array(b.barcode ORDER BY b.position) \
          FROM release_barcodes b WHERE b.release_id = {release_id})"
    )
}

/// Read a `barcodes` column, a list of barcode rows read as one JSON array of
/// strings: every barcode, in order.
pub fn read_barcodes(row: &Row, column: &str) -> rusqlite::Result<Vec<String>> {
    json_column(row, column)
}

/// Read the facts a row's columns hold. `prefix` is what a joined query puts
/// before each column's name; a single-table query passes `""`.
pub fn read_facts(row: &Row, prefix: &str) -> rusqlite::Result<PressingFacts> {
    let mut facts = read_facts_without_media(row, prefix)?;
    facts.media = read_media(row, &format!("{prefix}media"))?;
    Ok(facts)
}

/// Media as a `media` column holds them: a JSON array of carriers and counts.
#[cfg(desktop)]
pub fn media_column(media: &[MediaCount]) -> String {
    serde_json::to_string(media).expect("media counts serialize")
}

/// The SQL of the library release `release_id`'s media as one `media`
/// column: its `release_media` rows in order, as the JSON array
/// [`read_media`] reads.
pub fn release_media_sql(release_id: &str) -> String {
    format!(
        "(SELECT json_group_array(json_object('medium', m.medium, 'count', m.count) \
                ORDER BY m.position) \
          FROM release_media m WHERE m.release_id = {release_id})"
    )
}

/// The SQL of the library release `release_id`'s Discogs details as one
/// `discogs_details` column: its `release_discogs_details` rows in order, as
/// a JSON array of keys.
pub(super) fn release_discogs_details_sql(release_id: &str) -> String {
    format!(
        "(SELECT json_group_array(d.detail ORDER BY d.position) \
          FROM release_discogs_details d WHERE d.release_id = {release_id})"
    )
}

/// Read a `media` column: the carriers and their counts.
pub fn read_media(row: &Row, column: &str) -> rusqlite::Result<Vec<MediaCount>> {
    json_column(row, column)
}

/// Read every fact but the media, which each reader takes from elsewhere.
fn read_facts_without_media(row: &Row, prefix: &str) -> rusqlite::Result<PressingFacts> {
    let column = |name: &str| format!("{prefix}{name}");
    let country: Option<String> = row.get(column("country").as_str())?;
    let region: Option<String> = row.get(column("region").as_str())?;
    let status: Option<String> = row.get(column("status").as_str())?;
    let packaging: Option<String> = row.get(column("packaging").as_str())?;
    let discogs_details: Vec<DiscogsDetail> = json_column(row, column("discogs_details").as_str())?;
    facts_without_media_of(
        country,
        region,
        status.as_deref(),
        packaging.as_deref(),
        discogs_details,
    )
    .map_err(|detail| column_conversion_error(row, column("region").as_str(), detail))
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
