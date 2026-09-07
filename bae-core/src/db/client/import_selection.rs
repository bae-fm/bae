//! Selected-key reads for bulk actions. No list windows, files or editor documents.

use super::*;
use crate::import::selection::{
    ImportCandidateActionTarget, ImportSelectionProjection, SelectedCandidateFacts,
};
use std::path::Path;

pub(super) fn load_import_selection_on(
    sql: &SqlReadContext<'_>,
    keys: &BTreeSet<String>,
) -> Result<ImportSelectionProjection, DbError> {
    let mut selected = Vec::with_capacity(keys.len());
    for key in keys {
        let row = sql.query_row(
            "SELECT c.watched_folder_path, c.name, c.kind, c.source_kind, c.content_hash, \
                    c.file_edit_revision, s.edit_revision, \
                    EXISTS(SELECT 1 FROM candidate_combination_member WHERE candidate_key = c.path) \
             FROM scan_candidate c LEFT JOIN import_candidate_state s ON s.content_hash = c.content_hash \
             WHERE c.path = ? AND c.kind != 'invalid'",
            [key], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?, row.get::<_, String>(4)?, row.get::<_, i64>(5)?, row.get::<_, Option<i64>>(6)?, row.get::<_, bool>(7)?)),
        ).optional()?;
        let Some((root, name, kind, source, hash, revision, state_revision, combined)) = row else {
            continue;
        };
        let settled = match kind.as_str() {
            "valid" => true,
            "tentative" => false,
            other => return Err(folder_scans::columns::unreadable("candidate kind", other)),
        };
        let (is_folder, skipped, failure, actionable) = match source.as_str() {
            "folder" => {
                let relative =
                    crate::import::watched_folder::candidate_relative_path(&root, Path::new(key))
                        .map_err(|error| DbError::Message(error.to_string()))?;
                let skipped = sql.query_row("SELECT EXISTS(SELECT 1 FROM skipped_import_candidates WHERE watched_folder_path = ? AND relative_candidate_path = ?)", params![root, relative], |row| row.get(0))?;
                (true, skipped, None, settled && !combined)
            }
            "combination" => {
                let (skipped, error) = sql.query_row(
                    "SELECT skipped, error FROM candidate_combination WHERE candidate_key = ?",
                    [key],
                    |row| Ok((row.get::<_, bool>(0)?, row.get::<_, Option<String>>(1)?)),
                )?;
                let actionable = error.is_none();
                (false, skipped, error, actionable)
            }
            other => return Err(folder_scans::columns::unreadable("candidate source", other)),
        };
        let imported = sql
            .query_row(
                "SELECT id, album_id FROM releases WHERE content_hash = ? LIMIT 1",
                [&hash],
                |row| {
                    Ok(crate::import::ImportedRelease {
                        release_id: row.get(0)?,
                        album_id: row.get(1)?,
                    })
                },
            )
            .optional()?;
        let failure = match failure {
            Some(error) => Some(error),
            None => sql
                .query_row(
                    "SELECT error FROM import_candidate_failure WHERE content_hash = ?",
                    [&hash],
                    |row| row.get(0),
                )
                .optional()?,
        };
        let (provenance, answer) = if state_revision == Some(revision) {
            let provenance = import_state::load_provenance_on(sql, Some(&hash))?
                .remove(&hash)
                .map(|(provenance, _)| provenance);
            let summary = import_list::load_verdict_summaries_on(sql, Some(&hash))?.remove(&hash);
            let answer = match summary {
                None => None,
                Some((summary, probed)) => {
                    let checks: Vec<_> = summary
                        .lead
                        .as_ref()
                        .filter(|_| summary.pressing_count == 1)
                        .map(|lead| LibraryCheck {
                            release_id: lead.release_id.clone(),
                            source: lead.source,
                            source_group_id: lead.source_group_id.clone(),
                        })
                        .into_iter()
                        .collect();
                    let statuses = identity::check_releases_in_library_on(sql, &checks)?;
                    Some(crate::identify::classify_summary(
                        &summary,
                        probed,
                        statuses.first(),
                    ))
                }
            };
            (provenance, answer)
        } else {
            (None, None)
        };
        selected.push(SelectedCandidateFacts {
            target: ImportCandidateActionTarget {
                key: key.clone(),
                display_name: name,
            },
            actionable,
            is_folder,
            skipped,
            imported,
            failure,
            provenance,
            answer,
            metadata_valid: metadata_valid_on(sql, &hash)?,
        });
    }
    Ok(ImportSelectionProjection(selected))
}

/// The editable fields that can invalidate import readiness. Artist references
/// resolve to canonical rows, including explicit credits on nondropped tracks.
fn metadata_valid_on(sql: &SqlReadContext<'_>, hash: &str) -> Result<bool, DbError> {
    let (title, album_year, pressing_year) = sql.query_row(
        "SELECT album_title, album_year, year FROM import_candidate_edit WHERE content_hash = ?",
        [hash],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        },
    )?;
    let album_artists = artist_validity_on(sql, hash, true)?;
    let track_artists = artist_validity_on(sql, hash, false)?;
    Ok(crate::import::parse_optional_year(&album_year).is_ok()
        && crate::import::parse_optional_year(&pressing_year).is_ok()
        && crate::import::validate_release_metadata(
            &title,
            album_artists.len(),
            album_artists
                .iter()
                .chain(&track_artists)
                .any(|blank| *blank),
        )
        .is_ok())
}

fn artist_validity_on(
    sql: &SqlReadContext<'_>,
    hash: &str,
    album: bool,
) -> Result<Vec<bool>, DbError> {
    let statement = if album {
        "SELECT a.assignment_kind, a.artist_id, CASE a.assignment_kind WHEN 'existing' THEN artists.name WHEN 'new' THEN a.name END \
         FROM import_candidate_album_artist_assignment a LEFT JOIN artists ON artists.id = a.artist_id WHERE a.content_hash = ?"
    } else {
        "SELECT a.assignment_kind, a.artist_id, CASE a.assignment_kind WHEN 'existing' THEN artists.name WHEN 'new' THEN a.name END \
         FROM import_candidate_track_artist_assignment a JOIN import_candidate_track t ON t.content_hash = a.content_hash AND t.track_id = a.track_id \
         LEFT JOIN artists ON artists.id = a.artist_id WHERE a.content_hash = ? AND t.dropped = 0 AND t.artist_assignment_kind = 'explicit'"
    };
    sql.query(statement, [hash], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, Option<String>>(2)?,
        ))
    })?
    .into_iter()
    .map(|(kind, id, name)| {
        let name = name.ok_or_else(|| {
            DbError::Message("candidate artist assignment has no artist name".into())
        })?;
        match kind.as_str() {
            "new" => Ok(name.trim().is_empty()),
            "existing" => {
                let id = id.ok_or_else(|| {
                    DbError::Message("candidate artist assignment has no artist id".into())
                })?;
                Ok(id.trim().is_empty() || name.trim().is_empty())
            }
            other => Err(folder_scans::columns::unreadable(
                "artist assignment kind",
                other,
            )),
        }
    })
    .collect()
}

impl Database {
    pub(crate) fn subscribe_import_selection(
        &self,
        keys: BTreeSet<String>,
    ) -> coven::LiveQuery<ImportSelectionProjection> {
        self.inner
            .handle
            .subscribe(move |sql| load_import_selection_on(&sql, &keys).map_err(CovenError::from))
    }
}
