//! A candidate's file decisions as rows: which disc each sheet is, and which
//! audio each of its `FILE` references describes (an absent row is no
//! decision at all); read back by the next scan, and settled onto every
//! scanned release reading the same files.
//! Its tables are `file_decision_rows.sql`, which every device's library holds.

pub(super) const SQL: &str = include_str!("file_decision_rows.sql");

use super::*;
use crate::folder_scanner::{CandidateFileEdits, SheetDisc, UserSheetBinding};
use crate::tables::FinishRead;

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
            revision: to_u64(self.revision, &format!("{content_hash}'s file edit revision"))?,
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
                        number: to_u32(
                            number,
                            &format!("the disc number stored for {}", stored.sheet_id),
                        )?,
                    }
                }
                other => {
                    return Err(DbError::unreadable("import_candidate_sheet_disc.disc", other))
                }
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
pub(super) fn store_file_edits(
    sql: &CovenWrite<'_>,
    content_hash: &str,
    edits: &CandidateFileEdits,
) -> Result<(), DbError> {
    delete_file_edits(sql, content_hash)?;
    insert_file_edits(sql, content_hash, edits)
}

/// Give every scanned release sharing the preparation its settled files,
/// end the file work of each, which was of other files, and rebuild every
/// grouping built from one of them. Every row must still be at the revision
/// the caller prepared against.
pub(super) fn settle_scanned_candidates(
    sql: &CovenWrite<'_>,
    content_hash: &str,
    expected_revision: i64,
    next_revision: i64,
    settled_by_key: &HashMap<CandidateKey, crate::folder_scanner::CategorizedFiles>,
) -> Result<Vec<crate::folder_scanner::FolderCandidate>, DbError> {
    let settled = folder_scans::settle_candidate_files(
        sql,
        content_hash,
        expected_revision,
        next_revision,
        settled_by_key,
    )?;
    let decided = crate::CandidateFiles {
        content_hash: content_hash.to_string(),
        file_edit_revision: to_u64(next_revision, "a file edit revision")?,
    };
    let mut updated_candidates = Vec::with_capacity(settled.len());
    for path in &settled {
        // The decision makes the candidate other files than its
        // identification was of, which ends with it.
        end_file_work_unless_of_on(sql, path, Some(&decided))?;
        let stored = super::release_groupings::load_candidate_on(sql, path)?.ok_or_else(|| {
            DbError::Message(format!(
                "candidate {path} disappeared while its source decisions were stored"
            ))
        })?;
        updated_candidates.push(stored.candidate);
    }
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
