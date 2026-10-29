//! One candidate's sheet decisions as rows: which disc each sheet is, and
//! which audio each of its `FILE` references describes. An absent row is no
//! decision at all.

use super::verdict_rows::unreadable;
use super::*;
use crate::import::folder_scanner::{CandidateFileEdits, SheetDisc, UserSheetBinding};

pub(super) fn delete_file_edits(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
) -> Result<(), DbError> {
    sql.execute(
        "DELETE FROM import_candidate_sheet_reference WHERE content_hash = ?",
        [content_hash],
    )?;
    sql.execute(
        "DELETE FROM import_candidate_sheet_disc WHERE content_hash = ?",
        [content_hash],
    )?;
    Ok(())
}

/// Write every decision `edits` holds. The caller has already cleared what
/// stood under this hash, so this is always writing into empty space.
pub(super) fn insert_file_edits(
    sql: &SqlContext<'_, '_>,
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
            edits
                .sheet_bindings
                .set_reference(stored.sheet_id, stored.file_reference, decision);
        }
        Ok(edits)
    }
}
