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

pub(super) struct SheetDiscRow {
    pub(super) content_hash: String,
    sheet_id: String,
    disc: String,
    disc_number: Option<i64>,
}

pub(super) fn read_sheet_disc_row(row: &Row<'_>) -> Result<SheetDiscRow, DbError> {
    Ok(SheetDiscRow {
        content_hash: row.get("content_hash")?,
        sheet_id: row.get("sheet_id")?,
        disc: row.get("disc")?,
        disc_number: row.get("disc_number")?,
    })
}

/// Fold one stored row into the decisions being assembled for its candidate.
pub(super) fn apply_sheet_disc_row(
    edits: &mut CandidateFileEdits,
    row: SheetDiscRow,
) -> Result<(), DbError> {
    let disc = match row.disc.as_str() {
        "ignored" => SheetDisc::Ignored,
        "disc" => {
            let number = row.disc_number.ok_or_else(|| {
                DbError::Message(format!(
                    "the disc stored for {} states no number",
                    row.sheet_id
                ))
            })?;
            SheetDisc::Disc {
                number: u32::try_from(number).map_err(|_| {
                    DbError::Message(format!(
                        "the disc stored for {} is numbered {number}",
                        row.sheet_id
                    ))
                })?,
            }
        }
        other => return Err(unreadable("disc", other)),
    };
    edits.sheet_discs.set(row.sheet_id, disc);
    Ok(())
}
