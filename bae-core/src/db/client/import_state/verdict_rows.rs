//! The identify verdict as columns and match rows, and the read back.
//!
//! A normal verdict is `import_candidate_state`'s identify columns plus the
//! ordered release-match rows. A failed verdict is its typed attached row;
//! both shapes are replaced by the same transaction.

use super::*;
use crate::identify::{IdentifyFailure, ResultProvenance, TerminalVerdict};
use crate::import::cover_art::RemoteCover;
use crate::import::search::{MetadataResult, SourceTracks};
use crate::import::MetadataSource;
use std::str::FromStr;

/// What a stored column holds that no writer here produces.
pub(super) fn unreadable(column: &str, stored: &str) -> DbError {
    DbError::Message(format!("import candidate column {column} holds {stored:?}"))
}

fn source_of(stored: &str) -> Result<MetadataSource, DbError> {
    MetadataSource::from_str(stored).map_err(DbError::Message)
}

/// The identify columns of one verdict. `track_count` is absent for
/// `not_found`, which counts nothing, mirroring the table's CHECKs.
pub(super) struct VerdictColumns<'a> {
    pub(super) kind: Option<&'static str>,
    pub(super) track_count: Option<u32>,
    /// The barcode the match was found through, where one was.
    pub(super) matched_barcode: Option<&'a str>,
}

pub(super) fn verdict_columns(verdict: &TerminalVerdict) -> VerdictColumns<'_> {
    match verdict {
        TerminalVerdict::Found {
            track_count,
            matched_barcode,
            ..
        } => VerdictColumns {
            kind: Some("found"),
            track_count: Some(*track_count),
            matched_barcode: matched_barcode.as_deref(),
        },
        TerminalVerdict::NotFoundAnywhere => VerdictColumns {
            kind: Some("not_found"),
            track_count: None,
            matched_barcode: None,
        },
        TerminalVerdict::ManualOnly { track_count } => VerdictColumns {
            kind: Some("manual_only"),
            track_count: Some(*track_count),
            matched_barcode: None,
        },
        TerminalVerdict::Failed { .. } => VerdictColumns {
            kind: None,
            track_count: None,
            matched_barcode: None,
        },
    }
}

pub(super) fn delete_matches(sql: &SqlContext<'_, '_>, content_hash: &str) -> Result<(), DbError> {
    sql.execute(
        "DELETE FROM import_candidate_match WHERE content_hash = ?",
        [content_hash],
    )?;
    Ok(())
}

/// Write the matches of one verdict. The caller has already cleared whatever
/// stood under this hash.
pub(super) fn insert_matches(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    verdict: &TerminalVerdict,
) -> Result<(), DbError> {
    match verdict {
        TerminalVerdict::Found {
            matches,
            provenance,
            ..
        } => {
            if matches.len() != provenance.len() {
                return Err(DbError::Message(format!(
                    "a found verdict for {content_hash} carries {} matches and {} provenance \
                     entries; they are index-aligned",
                    matches.len(),
                    provenance.len()
                )));
            }
            for (position, (result, provenance)) in
                matches.iter().zip(provenance.iter()).enumerate()
            {
                insert_match(sql, content_hash, position, result, provenance)?;
            }
        }
        TerminalVerdict::NotFoundAnywhere
        | TerminalVerdict::ManualOnly { .. }
        | TerminalVerdict::Failed { .. } => {}
    }
    Ok(())
}

pub(super) fn delete_identify_failure(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
) -> Result<(), DbError> {
    sql.execute(
        "DELETE FROM import_candidate_identify_failure WHERE content_hash = ?",
        [content_hash],
    )?;
    Ok(())
}

pub(super) fn insert_identify_failure(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    verdict: &TerminalVerdict,
    probed_total_duration_ms: i64,
    identified_at: &str,
) -> Result<(), DbError> {
    let TerminalVerdict::Failed {
        failures,
        track_count,
    } = verdict
    else {
        return Ok(());
    };
    if failures.is_empty() {
        return Err(DbError::Message(format!(
            "failed verdict for {content_hash} contains no failed lookup"
        )));
    }
    let failures_json = serde_json::to_string(failures).map_err(|error| {
        DbError::Message(format!(
            "failed to serialize identify failure for {content_hash}: {error}"
        ))
    })?;
    sql.execute(
        "INSERT INTO import_candidate_identify_failure \
             (content_hash, failures_json, track_count, probed_total_duration_ms, identified_at) \
         VALUES (?, ?, ?, ?, ?)",
        params![
            content_hash,
            failures_json,
            i64::from(*track_count),
            probed_total_duration_ms,
            identified_at,
        ],
    )?;
    Ok(())
}

pub(super) struct IdentifyFailureRow {
    pub(super) content_hash: String,
    pub(super) failures: Vec<IdentifyFailure>,
    pub(super) track_count: u32,
    pub(super) probed_total_duration_ms: u64,
    pub(super) identified_at: DateTime<Utc>,
}

pub(super) fn read_identify_failure_row(row: &Row<'_>) -> Result<IdentifyFailureRow, DbError> {
    let content_hash: String = row.get("content_hash")?;
    let failures_json: String = row.get("failures_json")?;
    let failures: Vec<IdentifyFailure> = serde_json::from_str(&failures_json).map_err(|error| {
        DbError::Message(format!(
            "identify failure for {content_hash} is unreadable: {error}"
        ))
    })?;
    if failures.is_empty() {
        return Err(DbError::Message(format!(
            "identify failure for {content_hash} contains no failed lookup"
        )));
    }
    let track_count: i64 = row.get("track_count")?;
    let probed_total_duration_ms: i64 = row.get("probed_total_duration_ms")?;
    Ok(IdentifyFailureRow {
        content_hash,
        failures,
        track_count: u32::try_from(track_count)
            .map_err(|_| DbError::Message("identify failure track count is out of range".into()))?,
        probed_total_duration_ms: u64::try_from(probed_total_duration_ms)
            .map_err(|_| DbError::Message("identify failure probed duration is negative".into()))?,
        identified_at: super::rfc3339_column(row, "identified_at")?,
    })
}

fn insert_match(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    position: usize,
    result: &MetadataResult,
    provenance: &ResultProvenance,
) -> Result<(), DbError> {
    let position = i64::try_from(position)
        .map_err(|_| DbError::Message("a match list is longer than SQLite counts".to_string()))?;
    let cover = result.cover_art.as_ref();
    let (tracks_kind, tracks_count, tracks_total_ms) = match &result.source_tracks {
        None => (None, None, None),
        Some(SourceTracks::Nothing) => (Some("nothing"), None, None),
        Some(SourceTracks::Listed {
            count,
            total_duration_ms,
        }) => (
            Some("listed"),
            Some(i64::from(*count)),
            total_duration_ms
                .map(|total| {
                    i64::try_from(total).map_err(|_| {
                        DbError::Message(
                            "a source tracklist's total exceeds SQLite's integer range".to_string(),
                        )
                    })
                })
                .transpose()?,
        ),
    };
    sql.execute(
        "INSERT INTO import_candidate_match \
             (content_hash, position, source, release_id, title, artist, year, format, \
              label, catalog_number, country, barcode, cover_url, cover_thumbnail_url, \
              cover_label, cover_source, source_group_id, source_tracks_kind, \
              source_tracks_count, source_tracks_total_ms, by_disc_id, by_barcode, by_catalog) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        params![
            content_hash,
            position,
            result.source.as_str(),
            result.release_id,
            result.title,
            result.artist,
            result.year,
            result.format,
            result.label,
            result.catalog_number,
            result.country,
            result.barcode,
            cover.map(|cover| cover.url.as_str()),
            cover.map(|cover| cover.thumbnail_url.as_str()),
            cover.map(|cover| cover.label.as_str()),
            cover.map(|cover| cover.source.as_str()),
            result.source_group_id,
            tracks_kind,
            tracks_count,
            tracks_total_ms,
            provenance.by_disc_id,
            provenance.by_barcode,
            provenance.by_catalog,
        ],
    )?;
    Ok(())
}

/// One candidate's stored matches, in the order they were written: the lead
/// first, then the rest.
pub(crate) type StoredMatches = Vec<(MetadataResult, ResultProvenance)>;

pub(super) struct MatchRow {
    pub(super) content_hash: String,
    pub(super) result: MetadataResult,
    pub(super) provenance: ResultProvenance,
}

pub(super) fn read_match_row(row: &Row<'_>) -> Result<MatchRow, DbError> {
    let cover_url: Option<String> = row.get("cover_url")?;
    let cover_source: Option<String> = row.get("cover_source")?;
    let cover_art = match (cover_url, cover_source) {
        (Some(url), Some(source)) => Some(RemoteCover {
            url,
            thumbnail_url: row.get("cover_thumbnail_url")?,
            label: row.get("cover_label")?,
            source: source_of(&source)?,
        }),
        // The table sets and clears the four cover columns together.
        _ => None,
    };
    let tracks_kind: Option<String> = row.get("source_tracks_kind")?;
    let source_tracks = match tracks_kind.as_deref() {
        None => None,
        Some("nothing") => Some(SourceTracks::Nothing),
        Some("listed") => {
            let count: i64 = row
                .get::<_, Option<i64>>("source_tracks_count")?
                .ok_or_else(|| {
                    DbError::Message("a listed source tracklist states no count".to_string())
                })?;
            let total: Option<i64> = row.get("source_tracks_total_ms")?;
            Some(SourceTracks::Listed {
                count: u32::try_from(count).map_err(|_| {
                    DbError::Message("a source tracklist's count is out of range".to_string())
                })?,
                total_duration_ms: total
                    .map(|total| {
                        u64::try_from(total).map_err(|_| {
                            DbError::Message("a source tracklist's total is negative".to_string())
                        })
                    })
                    .transpose()?,
            })
        }
        Some(other) => return Err(unreadable("source_tracks_kind", other)),
    };
    let source: String = row.get("source")?;
    Ok(MatchRow {
        content_hash: row.get("content_hash")?,
        result: MetadataResult {
            source: source_of(&source)?,
            release_id: row.get("release_id")?,
            title: row.get("title")?,
            artist: row.get("artist")?,
            year: row.get("year")?,
            format: row.get("format")?,
            label: row.get("label")?,
            catalog_number: row.get("catalog_number")?,
            country: row.get("country")?,
            barcode: row.get("barcode")?,
            cover_art,
            source_group_id: row.get("source_group_id")?,
            source_tracks,
        },
        provenance: ResultProvenance {
            by_disc_id: row.get("by_disc_id")?,
            by_barcode: row.get("by_barcode")?,
            by_catalog: row.get("by_catalog")?,
        },
    })
}

/// Rebuild one verdict from its columns and its matches.
pub(super) fn verdict_of(
    content_hash: &str,
    kind: &str,
    track_count: Option<i64>,
    matched_barcode: Option<String>,
    found: StoredMatches,
) -> Result<TerminalVerdict, DbError> {
    let count_of = || {
        track_count
            .ok_or_else(|| {
                DbError::Message(format!(
                    "verdict {kind} for {content_hash} counts no tracks"
                ))
            })
            .and_then(|count| {
                u32::try_from(count).map_err(|_| {
                    DbError::Message(format!(
                        "verdict {kind} for {content_hash} counts {count} tracks"
                    ))
                })
            })
    };
    match kind {
        "found" => {
            let (matches, provenance) = found.into_iter().unzip();
            Ok(TerminalVerdict::Found {
                matches,
                track_count: count_of()?,
                provenance,
                matched_barcode,
            })
        }
        "not_found" => Ok(TerminalVerdict::NotFoundAnywhere),
        "manual_only" => Ok(TerminalVerdict::ManualOnly {
            track_count: count_of()?,
        }),
        other => Err(unreadable("verdict_kind", other)),
    }
}
