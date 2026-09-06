//! The identify verdict as its own row, its matches, and the read back.
//!
//! One `import_candidate_verdict` row holds a whole terminal verdict — found,
//! nothing anywhere, manual only, or failed — and the ordered release-match
//! rows hang off it. Deleting the row clears the matches with it, so a
//! candidate can neither hold two verdicts nor keep matches without one.

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

/// Clear whatever verdict stands under `content_hash`. The match rows go with
/// it: they hang off the verdict row.
pub(super) fn delete_verdict(sql: &SqlContext<'_, '_>, content_hash: &str) -> Result<(), DbError> {
    sql.execute(
        "DELETE FROM import_candidate_verdict WHERE content_hash = ?",
        [content_hash],
    )?;
    Ok(())
}

/// Write one whole verdict and the matches it found. The caller has already
/// cleared whatever stood under this hash.
pub(super) fn insert_verdict(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    identification: &DbCandidateIdentifyResult,
) -> Result<(), DbError> {
    let verdict = &identification.verdict;
    let (kind, track_count, matched_barcode) = match verdict {
        TerminalVerdict::Found {
            track_count,
            matched_barcode,
            ..
        } => ("found", Some(*track_count), matched_barcode.as_deref()),
        TerminalVerdict::NotFoundAnywhere => ("not_found", None, None),
        TerminalVerdict::ManualOnly { track_count } => ("manual_only", Some(*track_count), None),
        TerminalVerdict::Failed { track_count, .. } => ("failed", Some(*track_count), None),
    };
    let failures_json = match verdict {
        TerminalVerdict::Failed { failures, .. } => {
            if failures.is_empty() {
                return Err(DbError::Message(format!(
                    "failed verdict for {content_hash} contains no failed lookup"
                )));
            }
            Some(serde_json::to_string(failures).map_err(|error| {
                DbError::Message(format!(
                    "failed to serialize identify failure for {content_hash}: {error}"
                ))
            })?)
        }
        TerminalVerdict::Found { .. }
        | TerminalVerdict::NotFoundAnywhere
        | TerminalVerdict::ManualOnly { .. } => None,
    };
    let probed = i64::try_from(identification.probed_total_duration_ms).map_err(|_| {
        DbError::Message(format!(
            "candidate {content_hash} probed total exceeds SQLite's integer range"
        ))
    })?;
    sql.execute(
        "INSERT INTO import_candidate_verdict \
             (content_hash, kind, track_count, matched_barcode, failures_json, \
              probed_total_duration_ms, identified_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
        params![
            content_hash,
            kind,
            track_count,
            matched_barcode,
            failures_json,
            probed,
            identification.identified_at.to_rfc3339(),
        ],
    )?;
    insert_matches(sql, content_hash, verdict)
}

/// The matches of one verdict, written under the verdict row that found them.
fn insert_matches(
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

/// The columns of one stored verdict row.
pub(super) struct VerdictRow {
    pub(super) content_hash: String,
    pub(super) kind: String,
    pub(super) track_count: Option<i64>,
    pub(super) matched_barcode: Option<String>,
    pub(super) failures_json: Option<String>,
    pub(super) probed_total_duration_ms: i64,
    pub(super) identified_at: DateTime<Utc>,
}

pub(super) const VERDICT_COLUMNS: &str = "content_hash, kind, track_count, matched_barcode, \
     failures_json, probed_total_duration_ms, identified_at";

pub(super) fn read_verdict_row(row: &Row<'_>) -> Result<VerdictRow, DbError> {
    Ok(VerdictRow {
        content_hash: row.get("content_hash")?,
        kind: row.get("kind")?,
        track_count: row.get("track_count")?,
        matched_barcode: row.get("matched_barcode")?,
        failures_json: row.get("failures_json")?,
        probed_total_duration_ms: row.get("probed_total_duration_ms")?,
        identified_at: super::rfc3339_column(row, "identified_at")?,
    })
}

/// Rebuild what identification concluded from its row and the matches it found.
pub(super) fn identification_of(
    row: VerdictRow,
    found: StoredMatches,
) -> Result<DbCandidateIdentifyResult, DbError> {
    let VerdictRow {
        content_hash,
        kind,
        track_count,
        matched_barcode,
        failures_json,
        probed_total_duration_ms,
        identified_at,
    } = row;
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
    let verdict = match kind.as_str() {
        "found" => {
            let (matches, provenance) = found.into_iter().unzip();
            TerminalVerdict::Found {
                matches,
                track_count: count_of()?,
                provenance,
                matched_barcode,
            }
        }
        "not_found" => TerminalVerdict::NotFoundAnywhere,
        "manual_only" => TerminalVerdict::ManualOnly {
            track_count: count_of()?,
        },
        "failed" => {
            let json = failures_json.ok_or_else(|| {
                DbError::Message(format!(
                    "failed verdict for {content_hash} lists no failure"
                ))
            })?;
            let failures: Vec<IdentifyFailure> = serde_json::from_str(&json).map_err(|error| {
                DbError::Message(format!(
                    "identify failure for {content_hash} is unreadable: {error}"
                ))
            })?;
            if failures.is_empty() {
                return Err(DbError::Message(format!(
                    "identify failure for {content_hash} contains no failed lookup"
                )));
            }
            TerminalVerdict::Failed {
                failures,
                track_count: count_of()?,
            }
        }
        other => return Err(unreadable("verdict kind", other)),
    };
    Ok(DbCandidateIdentifyResult {
        verdict,
        probed_total_duration_ms: u64::try_from(probed_total_duration_ms).map_err(|_| {
            DbError::Message(format!(
                "import candidate {content_hash} holds a negative probed total"
            ))
        })?,
        identified_at,
    })
}
