//! The identify verdict as columns and match rows, and the read back.
//!
//! A verdict is one row of `import_candidate_state`'s identify columns plus
//! the `import_candidate_match` rows of the releases it matched, in order.

use super::*;
use crate::identify::{ResultProvenance, TerminalVerdict};
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
pub(super) struct VerdictColumns {
    pub(super) kind: &'static str,
    pub(super) track_count: Option<u32>,
}

pub(super) fn verdict_columns(verdict: &TerminalVerdict) -> VerdictColumns {
    match verdict {
        TerminalVerdict::Found { track_count, .. } => VerdictColumns {
            kind: "found",
            track_count: Some(*track_count),
        },
        TerminalVerdict::NotFoundAnywhere => VerdictColumns {
            kind: "not_found",
            track_count: None,
        },
        TerminalVerdict::ManualOnly { track_count } => VerdictColumns {
            kind: "manual_only",
            track_count: Some(*track_count),
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
        TerminalVerdict::NotFoundAnywhere | TerminalVerdict::ManualOnly { .. } => {}
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
              label, catalog_number, country, cover_url, cover_thumbnail_url, cover_label, \
              cover_source, source_group_id, source_tracks_kind, source_tracks_count, \
              source_tracks_total_ms, by_disc_id, by_barcode, matches_catalog) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
            provenance.matches_catalog,
        ],
    )?;
    Ok(())
}

pub(super) struct MatchRow {
    pub(super) content_hash: String,
    result: MetadataResult,
    provenance: ResultProvenance,
}

/// One candidate's matches, in the order they were written.
#[derive(Default)]
pub(super) struct MatchLists {
    found: Vec<(MetadataResult, ResultProvenance)>,
}

impl MatchLists {
    pub(super) fn push(&mut self, row: MatchRow) {
        self.found.push((row.result, row.provenance));
    }
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
            cover_art,
            source_group_id: row.get("source_group_id")?,
            source_tracks,
        },
        provenance: ResultProvenance {
            by_disc_id: row.get("by_disc_id")?,
            by_barcode: row.get("by_barcode")?,
            matches_catalog: row.get("matches_catalog")?,
        },
    })
}

/// Rebuild one verdict from its columns and its matches.
pub(super) fn verdict_of(
    content_hash: &str,
    kind: &str,
    track_count: Option<i64>,
    lists: MatchLists,
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
            let (matches, provenance) = lists.found.into_iter().unzip();
            Ok(TerminalVerdict::Found {
                matches,
                track_count: count_of()?,
                provenance,
            })
        }
        "not_found" => Ok(TerminalVerdict::NotFoundAnywhere),
        "manual_only" => Ok(TerminalVerdict::ManualOnly {
            track_count: count_of()?,
        }),
        other => Err(unreadable("verdict_kind", other)),
    }
}
