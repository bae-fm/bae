//! A pressing's facts as the columns every table that stores them keeps them
//! in: `country` or `region`, `media` (a JSON array of carriers and counts),
//! `status`, `packaging`, and `discogs_details` (a JSON array of keys) — and
//! its `labels`, a JSON array of each label's name and catalog number.
//!
//! One writer and one reader, so every table spells the facts the same way
//! and a value no vocabulary names fails the read instead of turning into
//! nothing.

use super::read::column_conversion_error;
use crate::pressing::{
    Country, DiscogsDetail, MediaCount, Packaging, PressingFacts, Region, ReleaseArea,
    ReleaseLabel, ReleaseStatus,
};
use coven::rusqlite::Row;

/// One pressing's facts as column values.
pub(super) struct FactColumns {
    pub(super) country: Option<&'static str>,
    pub(super) region: Option<&'static str>,
    pub(super) media: String,
    pub(super) status: Option<&'static str>,
    pub(super) packaging: Option<&'static str>,
    pub(super) discogs_details: String,
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
    pub(super) fn of(facts: &PressingFacts) -> Self {
        let (country, region) = area_columns(facts.area);
        Self {
            country,
            region,
            media: serde_json::to_string(&facts.media).expect("media counts serialize"),
            status: facts.status.map(ReleaseStatus::key),
            packaging: facts.packaging.map(Packaging::key),
            discogs_details: serde_json::to_string(&facts.discogs_details)
                .expect("details serialize"),
        }
    }
}

/// A pressing's labels as their `labels` column holds them.
pub(super) fn labels_column(labels: &[ReleaseLabel]) -> String {
    serde_json::to_string(labels).expect("labels serialize")
}

/// Read a `labels` column: every label, each with its catalog number.
pub(super) fn read_labels(row: &Row, column: &str) -> coven::rusqlite::Result<Vec<ReleaseLabel>> {
    let labels: String = row.get(column)?;
    serde_json::from_str::<Vec<ReleaseLabel>>(&labels).map_err(|error| {
        column_conversion_error(row, column, format!("labels {labels:?}: {error}"))
    })
}

/// Read the facts a row's columns hold. `prefix` is what a joined query puts
/// before each column's name; a single-table query passes `""`.
pub(super) fn read_facts(row: &Row, prefix: &str) -> coven::rusqlite::Result<PressingFacts> {
    let mut facts = read_facts_without_media(row, prefix)?;
    facts.media = read_media(row, &format!("{prefix}media"))?;
    Ok(facts)
}

/// Read a `media` column: the carriers and their counts.
pub(super) fn read_media(row: &Row, column: &str) -> coven::rusqlite::Result<Vec<MediaCount>> {
    let media: String = row.get(column)?;
    serde_json::from_str::<Vec<MediaCount>>(&media)
        .map_err(|error| column_conversion_error(row, column, format!("media {media:?}: {error}")))
}

/// Read every fact but the media, for a table that keeps a record's media in
/// rows of their own.
pub(super) fn read_facts_without_media(
    row: &Row,
    prefix: &str,
) -> coven::rusqlite::Result<PressingFacts> {
    let column = |name: &str| format!("{prefix}{name}");
    let country: Option<String> = row.get(column("country").as_str())?;
    let region: Option<String> = row.get(column("region").as_str())?;
    let area = area_of(country, region)
        .map_err(|detail| column_conversion_error(row, &column("region"), detail))?;
    let status = keyed(row, &column("status"), ReleaseStatus::from_key)?;
    let packaging = keyed(row, &column("packaging"), Packaging::from_key)?;
    let details_column = column("discogs_details");
    let details: String = row.get(details_column.as_str())?;
    let discogs_details =
        serde_json::from_str::<Vec<DiscogsDetail>>(&details).map_err(|error| {
            column_conversion_error(
                row,
                &details_column,
                format!("discogs_details {details:?}: {error}"),
            )
        })?;
    Ok(PressingFacts {
        area,
        media: Vec::new(),
        status,
        packaging,
        discogs_details,
    })
}

/// A nullable column holding one vocabulary's key.
pub(super) fn keyed<T>(
    row: &Row,
    column: &str,
    from_key: fn(&str) -> Option<T>,
) -> coven::rusqlite::Result<Option<T>> {
    let key: Option<String> = row.get(column)?;
    key.map(|key| {
        from_key(&key)
            .ok_or_else(|| column_conversion_error(row, column, format!("{column} {key:?}")))
    })
    .transpose()
}
