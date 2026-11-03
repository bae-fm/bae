//! A candidate's file decisions as rows: which disc each sheet is, and which
//! audio each of its `FILE` references describes (an absent row is no
//! decision at all); read back by the next scan, and settled onto every
//! scanned release reading the same files.

use super::rows::FinishRead;
use super::verdict_rows::unreadable;
use super::*;
use crate::folder_scanner::{CandidateFileEdits, SheetDisc, UserSheetBinding};

pub(super) fn delete_file_edits(sql: &CovenWrite<'_>, content_hash: &str) -> Result<(), DbError> {
    sql.execute(
        "DELETE FROM import_candidate_sheet_reference WHERE content_hash = ?",
        [content_hash],
    )?;
    sql.execute("DELETE FROM import_candidate_sheet_disc WHERE content_hash = ?", [content_hash])?;
    Ok(())
}

/// Write every decision `edits` holds. The caller has already cleared what
/// stood under this hash, so this is always writing into empty space.
pub(super) fn insert_file_edits(
    sql: &CovenWrite<'_>,
    content_hash: &str,
    edits: &CandidateFileEdits,
) -> Result<(), DbError> {
    for (sheet_id, disc) in edits.sheet_discs.iter() {
        let (kind, number) = match disc {
            SheetDisc::Ignored => ("ignored", None),
            SheetDisc::Disc { number } => ("disc", Some(*number)),
        };
        sql.execute(
            "INSERT INTO import_candidate_sheet_disc \
                 (content_hash, sheet_id, disc, disc_number) VALUES (?, ?, ?, ?)",
            params![content_hash, sheet_id, kind, number],
        )?;
    }
    for (sheet_id, references) in edits.sheet_bindings.iter() {
        for (reference, decision) in references.iter() {
            let file_id = match decision {
                UserSheetBinding::Describes { file_id } => Some(file_id.as_str()),
                UserSheetBinding::Cleared => None,
            };
            sql.execute("INSERT INTO import_candidate_sheet_reference (content_hash, sheet_id, file_reference, file_id) VALUES (?, ?, ?, ?)",
                params![content_hash, sheet_id, reference, file_id])?;
        }
    }
    Ok(())
}

/// A candidate's file decisions as columns of a read that joins its state
/// row as `state`: the revision they stand at, and the sheet discs and the
/// `FILE` reference bindings riding on it as JSON arrays.
pub(super) const FILE_EDITS_COLUMNS: &str = "state.edit_revision AS edits_revision, \
     (SELECT json_group_array(json_object( \
          'sheet_id', disc.sheet_id, 'disc', disc.disc, 'disc_number', disc.disc_number) \
          ORDER BY disc.sheet_id) \
      FROM import_candidate_sheet_disc AS disc \
      WHERE disc.content_hash = state.content_hash) AS edits_sheet_discs, \
     (SELECT json_group_array(json_object( \
          'sheet_id', binding.sheet_id, 'file_reference', binding.file_reference, \
          'file_id', binding.file_id) ORDER BY binding.sheet_id, binding.file_reference) \
      FROM import_candidate_sheet_reference AS binding \
      WHERE binding.content_hash = state.content_hash) AS edits_sheet_references";

/// One candidate's [`FILE_EDITS_COLUMNS`] as SQLite hands them over, its
/// decisions still JSON.
pub(super) struct StoredFileEdits {
    content_hash: String,
    revision: i64,
    sheet_discs: String,
    sheet_references: String,
}

/// The [`FILE_EDITS_COLUMNS`] of `row`, whose state row is `content_hash`.
pub(super) fn read_file_edits(row: &Row<'_>) -> rusqlite::Result<StoredFileEdits> {
    Ok(StoredFileEdits {
        content_hash: row.get("content_hash")?,
        revision: row.get("edits_revision")?,
        sheet_discs: row.get("edits_sheet_discs")?,
        sheet_references: row.get("edits_sheet_references")?,
    })
}

/// One sheet's disc as [`FILE_EDITS_COLUMNS`] holds it.
#[derive(serde::Deserialize)]
struct StoredSheetDisc {
    sheet_id: String,
    disc: String,
    disc_number: Option<i64>,
}

/// One `FILE` reference binding as [`FILE_EDITS_COLUMNS`] holds it.
#[derive(serde::Deserialize)]
struct StoredSheetReference {
    sheet_id: String,
    file_reference: String,
    file_id: Option<String>,
}

impl StoredFileEdits {
    /// The candidate the decisions are of.
    pub(super) fn content_hash(&self) -> &str {
        &self.content_hash
    }

    pub(super) fn edits(self) -> Result<CandidateFileEdits, DbError> {
        let content_hash = &self.content_hash;
        let unreadable_json = |what: &str, json: &str, error: serde_json::Error| {
            DbError::Message(format!("{content_hash}'s stored {what} {json:?}: {error}"))
        };
        let mut edits = CandidateFileEdits {
            revision: u64::try_from(self.revision).map_err(|error| {
                DbError::Message(format!(
                    "import candidate {content_hash} has a negative edit revision: {error}"
                ))
            })?,
            ..CandidateFileEdits::default()
        };
        let discs: Vec<StoredSheetDisc> = serde_json::from_str(&self.sheet_discs)
            .map_err(|error| unreadable_json("sheet discs", &self.sheet_discs, error))?;
        for stored in discs {
            let disc = match stored.disc.as_str() {
                "ignored" => SheetDisc::Ignored,
                "disc" => {
                    let number = stored.disc_number.ok_or_else(|| {
                        DbError::Message(format!(
                            "the disc stored for {} states no number",
                            stored.sheet_id
                        ))
                    })?;
                    SheetDisc::Disc {
                        number: u32::try_from(number).map_err(|error| {
                            DbError::Message(format!(
                                "the disc stored for {} is numbered {number}: {error}",
                                stored.sheet_id
                            ))
                        })?,
                    }
                }
                other => return Err(unreadable("disc", other)),
            };
            edits.sheet_discs.set(stored.sheet_id, disc);
        }
        let references: Vec<StoredSheetReference> = serde_json::from_str(&self.sheet_references)
            .map_err(|error| unreadable_json("sheet references", &self.sheet_references, error))?;
        for stored in references {
            let decision = match stored.file_id {
                Some(file_id) => UserSheetBinding::Describes { file_id },
                None => UserSheetBinding::Cleared,
            };
            edits.sheet_bindings.set_reference(stored.sheet_id, stored.file_reference, decision);
        }
        Ok(edits)
    }
}

/// Replace every file decision held for `content_hash` with `edits`.
pub(crate) fn store_file_edits(
    sql: &CovenWrite<'_>,
    content_hash: &str,
    edits: &CandidateFileEdits,
) -> Result<(), DbError> {
    delete_file_edits(sql, content_hash)?;
    insert_file_edits(sql, content_hash, edits)
}

/// Give every candidate sharing the preparation its settled files and rebuild
/// every grouping built from one of them. Every row must still be at the
/// revision the caller prepared against.
pub(super) fn settle_scanned_candidates(
    sql: &CovenWrite<'_>,
    content_hash: &str,
    expected_revision: i64,
    next_revision: i64,
    settled_by_key: &HashMap<CandidateKey, crate::folder_scanner::CategorizedFiles>,
) -> Result<Vec<crate::folder_scanner::FolderCandidate>, DbError> {
    let scanned = sql.query(
        "SELECT watched_folder_path, path, source_kind, file_edit_revision FROM scan_candidate \
         WHERE content_hash = ? ORDER BY watched_folder_path, path",
        [content_hash],
        |row| {
            Ok((
                path_column::get(row, 0)?,
                row.get::<_, CandidateKey>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
            ))
        },
    )?;
    let mut updated_folders = HashSet::new();
    let mut updated_candidates = Vec::with_capacity(scanned.len());
    for (watched_folder_path, path, source_kind, revision) in scanned {
        if revision != expected_revision {
            return Err(DbError::Message(format!(
                "candidate {path} changed before its source decisions were stored"
            )));
        }
        match source_kind.as_str() {
            "folder" | "grouping" => {
                let settled = settled_by_key.get(&path).ok_or_else(|| {
                    DbError::Message(format!(
                        "persisted candidate {path} was missing from the settled file edit"
                    ))
                })?;
                // Deleting the file rows deletes the file-tag reading with
                // them; the files themselves did not change, so it is put
                // back as it was, at the revision it was taken at.
                let reading =
                    folder_scans::read::load_file_tag_snapshot(sql, &watched_folder_path, &path)?;
                sql.execute(
                    "DELETE FROM scan_candidate_file WHERE watched_folder_path = ? AND candidate_path = ?",
                    params![coven::path_bytes(&watched_folder_path), path],
                )?;
                folder_scans::insert_candidate_files(sql, &watched_folder_path, &path, settled)?;
                if let Some(reading) = reading {
                    folder_scans::write::replace_candidate_file_tag_snapshot(
                        sql,
                        &watched_folder_path,
                        &path,
                        &reading,
                    )?;
                }
                updated_folders.insert(path.clone());
            }
            other => return Err(DbError::Message(format!("unknown candidate source {other}"))),
        }
        let changed = sql.execute(
            "UPDATE scan_candidate SET file_edit_revision = ? \
             WHERE watched_folder_path = ? AND path = ? AND file_edit_revision = ?",
            params![
                next_revision,
                coven::path_bytes(&watched_folder_path),
                path,
                expected_revision
            ],
        )?;
        if changed != 1 {
            return Err(DbError::Message(format!(
                "candidate file decision changed {changed} persisted scan entries for {path}; \
                 expected exactly one"
            )));
        }
        // The decision makes the candidate other files than its
        // identification was of, which ends with it.
        end_file_work_unless_of_on(
            sql,
            &path,
            Some(&crate::CandidateFiles {
                content_hash: content_hash.to_string(),
                file_edit_revision: u64::try_from(next_revision).map_err(|error| {
                    DbError::Message(format!("file edit revision {next_revision}: {error}"))
                })?,
            }),
        )?;
        let stored = super::release_groupings::load_candidate_on(sql, &path)?.ok_or_else(|| {
            DbError::Message(format!(
                "candidate {path} disappeared while its source decisions were stored"
            ))
        })?;
        updated_candidates.push(stored.candidate);
    }
    if updated_folders.len() != settled_by_key.len() {
        let missing: Vec<_> = settled_by_key
            .keys()
            .filter(|key| !updated_folders.contains(*key))
            .map(CandidateKey::to_string)
            .collect();
        return Err(DbError::Message(format!(
            "candidate file decision could not update persisted scan entries: {}",
            missing.join(", ")
        )));
    }
    let settled: Vec<CandidateKey> = updated_folders.into_iter().collect();
    let regrouped = super::release_groupings::rebuild_groupings(sql, &settled, &[])?;
    updated_candidates.extend(regrouped.written.into_iter().filter_map(|item| match item {
        crate::folder_scanner::ScanItem::Valid(candidate) => Some(candidate),
        _ => None,
    }));
    Ok(updated_candidates)
}

impl ImportTables {
    /// Every candidate's user-set file decisions, keyed by `content_hash`, so a
    /// folder scan reports the roles the user settled.
    pub async fn load_stored_candidate_edits(&self) -> Result<StoredCandidateEdits, DbError> {
        self.database
            .read(move |sql| load_stored_candidate_edits_on(&sql))
            .process(|process| process())
            .await
    }

    /// Every release a scan stored reading the files `content_hash` names at
    /// file decision revision `file_edit_revision`, under whichever root, with
    /// those files in key order: what a file decision settles together.
    pub async fn load_releases_reading(
        &self,
        content_hash: &str,
        file_edit_revision: u64,
    ) -> Result<Vec<(CandidateKey, crate::folder_scanner::CategorizedFiles)>, DbError> {
        let content_hash = content_hash.to_string();
        let file_edit_revision = to_i64(file_edit_revision, "a file decision revision")?;
        self.database
            .read(move |sql| {
                let keys = sql.query(
                    "SELECT path FROM scan_candidate \
                 WHERE content_hash = ? AND file_edit_revision = ? \
                   AND kind IN ('tentative', 'valid') \
                 ORDER BY watched_folder_path, path",
                    params![content_hash, file_edit_revision],
                    |row| row.get::<_, CandidateKey>(0),
                )?;
                keys.into_iter()
                    .map(|key| {
                        folder_scans::read::load_item_by_key_rows(
                            &sql,
                            &key,
                            folder_scans::RowSources::Any,
                        )?
                        .ok_or_else(|| {
                            DbError::Message(format!("{key} vanished between its two reads"))
                        })
                    })
                    .collect::<Result<Vec<_>, DbError>>()
            })
            .process(|releases| {
                releases
                    .into_iter()
                    .map(|release| match release()?.1 {
                        folder_scans::read::StoredScanItem {
                            key,
                            item:
                                crate::folder_scanner::ScanItem::Discovered(candidate)
                                | crate::folder_scanner::ScanItem::Valid(candidate),
                            ..
                        } => Ok((key, candidate.files)),
                        folder_scans::read::StoredScanItem { key, .. } => Err(DbError::Message(
                            format!("{key} is stored as a release with files but reads as none"),
                        )),
                    })
                    .collect()
            })
            .await
    }

    /// One candidate's file decisions, by one indexed lookup, for a scan that
    /// has just computed the content hash.
    pub async fn load_candidate_file_edits(
        &self,
        content_hash: &str,
    ) -> Result<CandidateFileEdits, DbError> {
        let content_hash = content_hash.to_string();
        self.database
            .read(move |sql| load_candidate_file_edits_on(&sql, &content_hash))
            .process(|process| process())
            .await
    }
}

/// The file decisions of every candidate a decision was made for, each at its
/// revision: the candidates whose decisions stand, and those whose revision
/// moved past zero though none stands now. Any other candidate has none at
/// revision zero, which is what a scan reads where nothing is stored.
pub(crate) fn load_stored_candidate_edits_on(
    sql: &(impl bae_db::client::query::QueryOne + bae_db::client::query::QueryRows),
) -> Result<FinishRead<crate::folder_scanner::StoredCandidateEdits>, DbError> {
    let rows = sql.query(
        &format!(
            "SELECT state.content_hash, {FILE_EDITS_COLUMNS} \
             FROM import_candidate_state AS state \
             WHERE state.edit_revision > 0 \
                OR EXISTS (SELECT 1 FROM import_candidate_sheet_disc AS decided \
                           WHERE decided.content_hash = state.content_hash) \
                OR EXISTS (SELECT 1 FROM import_candidate_sheet_reference AS decided \
                           WHERE decided.content_hash = state.content_hash)"
        ),
        [],
        read_file_edits,
    )?;
    Ok(Box::new(move || {
        let edits = rows
            .into_iter()
            .map(|stored| Ok((stored.content_hash().to_string(), stored.edits()?)))
            .collect::<Result<HashMap<_, _>, DbError>>()?;
        Ok(crate::folder_scanner::StoredCandidateEdits::new(edits))
    }))
}

/// One candidate's file decisions, revision included; none at revision zero
/// for a candidate with no state row. Progressive scans call this after they
/// compute a content hash, so each emitted row performs one indexed lookup
/// instead of rereading the whole table.
pub(crate) fn load_candidate_file_edits_on(
    sql: &(impl bae_db::client::query::QueryOne + bae_db::client::query::QueryRows),
    content_hash: &str,
) -> Result<FinishRead<CandidateFileEdits>, DbError> {
    let stored = sql
        .query_row(
            &format!(
                "SELECT state.content_hash, {FILE_EDITS_COLUMNS} \
                 FROM import_candidate_state AS state WHERE state.content_hash = ?"
            ),
            [content_hash],
            read_file_edits,
        )
        .optional()?;
    Ok(Box::new(move || {
        stored.map_or_else(|| Ok(CandidateFileEdits::default()), |stored| stored.edits())
    }))
}

#[cfg(test)]
#[path = "file_decision_rows_tests.rs"]
mod tests;
