//! A pressing's facts as the columns every table that stores them keeps them
//! in: `country` or `region`, `media` (a JSON array of carriers and counts, or
//! a library release's `release_media` rows read as one), `status`,
//! `packaging`, and `discogs_details` (a JSON array of keys) — and its
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

/// One pressing's facts as column values.
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
          LEFT JOIN labels shown ON shown.id = {shown} \
          WHERE placed.release_id = {release_id})",
        shown = super::label_resolution::shown_label_id("placed.label_id"),
    )
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

/// Read a `media` column: the carriers and their counts.
pub fn read_media(row: &Row, column: &str) -> rusqlite::Result<Vec<MediaCount>> {
    json_column(row, column)
}

/// Read every fact but the media, for a table that keeps a record's media in
/// rows of their own.
pub fn read_facts_without_media(row: &Row, prefix: &str) -> rusqlite::Result<PressingFacts> {
    let column = |name: &str| format!("{prefix}{name}");
    let country: Option<String> = row.get(column("country").as_str())?;
    let region: Option<String> = row.get(column("region").as_str())?;
    let area = area_of(country, region)
        .map_err(|detail| column_conversion_error(row, column("region").as_str(), detail))?;
    let status = keyed(row, &column("status"), ReleaseStatus::from_key)?;
    let packaging = keyed(row, &column("packaging"), Packaging::from_key)?;
    let discogs_details: Vec<DiscogsDetail> = json_column(row, column("discogs_details").as_str())?;
    Ok(PressingFacts { area, media: Vec::new(), status, packaging, discogs_details })
}

/// A nullable column holding one vocabulary's key.
pub fn keyed<T>(
    row: &Row,
    column: &str,
    from_key: fn(&str) -> Option<T>,
) -> rusqlite::Result<Option<T>> {
    let key: Option<String> = row.get(column)?;
    key.map(|key| {
        from_key(&key)
            .ok_or_else(|| column_conversion_error(row, column, format!("{column} {key:?}")))
    })
    .transpose()
}
