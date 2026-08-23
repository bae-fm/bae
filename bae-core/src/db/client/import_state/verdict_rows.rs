//! The identify verdict as columns and match rows, and the read back.
//!
//! A verdict is one row of `import_candidate_state`'s identify columns plus
//! the `import_candidate_match` rows of whichever result lists it carries: a
//! `found` list, or a conflict's `discid` and `barcode` lists.

use super::*;
use crate::identify::{GroupKey, ResultProvenance, TerminalVerdict};
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

/// The identify columns of one verdict. `group_*` is set for `found` alone and
/// `matched_barcode` for `conflict` alone, mirroring the table's CHECKs.
pub(super) struct VerdictColumns<'a> {
    pub(super) kind: &'static str,
    pub(super) track_count: Option<u32>,
    pub(super) group_source: Option<&'static str>,
    pub(super) group_id: Option<&'a str>,
    pub(super) matched_barcode: Option<&'a str>,
}

pub(super) fn verdict_columns(verdict: &TerminalVerdict) -> VerdictColumns<'_> {
    match verdict {
        TerminalVerdict::Found {
            track_count, group, ..
        } => VerdictColumns {
            kind: "found",
            track_count: Some(*track_count),
            group_source: Some(group.source.as_str()),
            group_id: Some(group.source_group_id.as_str()),
            matched_barcode: None,
        },
        TerminalVerdict::Conflict {
            matched_barcode,
            track_count,
            ..
        } => VerdictColumns {
            kind: "conflict",
            track_count: Some(*track_count),
            group_source: None,
            group_id: None,
            matched_barcode: matched_barcode.as_deref(),
        },
        TerminalVerdict::NotFoundAnywhere => VerdictColumns {
            kind: "not_found",
            track_count: None,
            group_source: None,
            group_id: None,
            matched_barcode: None,
        },
        TerminalVerdict::ManualOnly { track_count } => VerdictColumns {
            kind: "manual_only",
            track_count: Some(*track_count),
            group_source: None,
            group_id: None,
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

/// Write the result lists of one verdict. The caller has already cleared
/// whatever stood under this hash.
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
                insert_match(
                    sql,
                    content_hash,
                    "found",
                    position,
                    result,
                    Some(provenance),
                )?;
            }
        }
        TerminalVerdict::Conflict {
            discid_results,
            barcode_results,
            ..
        } => {
            for (position, result) in discid_results.iter().enumerate() {
                insert_match(sql, content_hash, "discid", position, result, None)?;
            }
            for (position, result) in barcode_results.iter().enumerate() {
                insert_match(sql, content_hash, "barcode", position, result, None)?;
            }
        }
        TerminalVerdict::NotFoundAnywhere | TerminalVerdict::ManualOnly { .. } => {}
    }
    Ok(())
}

fn insert_match(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    list: &str,
    position: usize,
    result: &MetadataResult,
    provenance: Option<&ResultProvenance>,
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
             (content_hash, list, position, source, release_id, title, artist, year, format, \
              label, catalog_number, country, cover_url, cover_thumbnail_url, cover_label, \
              cover_source, source_group_id, source_tracks_kind, source_tracks_count, \
              source_tracks_total_ms, by_disc_id, by_barcode, matches_catalog) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        params![
            content_hash,
            list,
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
            provenance.map(|provenance| provenance.by_disc_id),
            provenance.map(|provenance| provenance.by_barcode),
            provenance.map(|provenance| provenance.matches_catalog),
        ],
    )?;
    Ok(())
}

pub(super) struct MatchRow {
    pub(super) content_hash: String,
    pub(super) list: String,
    result: MetadataResult,
    by_disc_id: Option<bool>,
    by_barcode: Option<bool>,
    matches_catalog: Option<bool>,
}

/// One candidate's result lists, in the order they were written.
#[derive(Default)]
pub(super) struct MatchLists {
    found: Vec<(MetadataResult, ResultProvenance)>,
    discid: Vec<MetadataResult>,
    barcode: Vec<MetadataResult>,
}

impl MatchLists {
    pub(super) fn push(&mut self, row: MatchRow) -> Result<(), DbError> {
        match row.list.as_str() {
            "found" => {
                let missing = || {
                    DbError::Message(format!(
                        "found match {} of {} states no provenance",
                        row.result.release_id, row.content_hash
                    ))
                };
                let provenance = ResultProvenance {
                    by_disc_id: row.by_disc_id.ok_or_else(missing)?,
                    by_barcode: row.by_barcode.ok_or_else(missing)?,
                    matches_catalog: row.matches_catalog.ok_or_else(missing)?,
                };
                self.found.push((row.result, provenance));
            }
            "discid" => self.discid.push(row.result),
            "barcode" => self.barcode.push(row.result),
            other => return Err(unreadable("list", other)),
        }
        Ok(())
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
        list: row.get("list")?,
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
        by_disc_id: row.get("by_disc_id")?,
        by_barcode: row.get("by_barcode")?,
        matches_catalog: row.get("matches_catalog")?,
    })
}

/// Rebuild one verdict from its columns and its result lists.
pub(super) fn verdict_of(
    content_hash: &str,
    kind: &str,
    track_count: Option<i64>,
    group_source: Option<String>,
    group_id: Option<String>,
    matched_barcode: Option<String>,
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
            let missing = |what: &str| {
                DbError::Message(format!("found verdict for {content_hash} names no {what}"))
            };
            Ok(TerminalVerdict::Found {
                matches,
                track_count: count_of()?,
                group: GroupKey {
                    source: source_of(&group_source.ok_or_else(|| missing("group source"))?)?,
                    source_group_id: group_id.ok_or_else(|| missing("group"))?,
                },
                provenance,
            })
        }
        "conflict" => Ok(TerminalVerdict::Conflict {
            discid_results: lists.discid,
            barcode_results: lists.barcode,
            matched_barcode,
            track_count: count_of()?,
        }),
        "not_found" => Ok(TerminalVerdict::NotFoundAnywhere),
        "manual_only" => Ok(TerminalVerdict::ManualOnly {
            track_count: count_of()?,
        }),
        other => Err(unreadable("verdict_kind", other)),
    }
}
