//! One candidate's per-file decisions as rows.
//!
//! The three things a person can settle about a file — its role, what a sheet
//! describes, which disc a sheet is — are three columns of one row keyed by
//! the file's relative path. An absent row is no decision at all; a column is
//! NULL where nothing was decided about that aspect.

use super::verdict_rows::unreadable;
use super::*;
use crate::import::folder_scanner::{
    CandidateFileEdits, FileRoleChoice, SheetDisc, UserSheetBinding,
};
use std::collections::BTreeSet;

pub(super) fn delete_file_edits(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
) -> Result<(), DbError> {
    sql.execute(
        "DELETE FROM import_candidate_file_edit WHERE content_hash = ?",
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
    let decided: BTreeSet<&str> = edits
        .file_roles
        .iter()
        .map(|(file_id, _)| file_id)
        .chain(edits.sheet_bindings.iter().map(|(file_id, _)| file_id))
        .chain(edits.sheet_discs.iter().map(|(file_id, _)| file_id))
        .collect();
    for relative_path in decided {
        let role_choice = edits
            .file_roles
            .get(relative_path)
            .map(|choice| match choice {
                FileRoleChoice::Audio => "audio",
                FileRoleChoice::NotATrack => "not_a_track",
            });
        let (sheet_binding, sheet_binding_file_id) = match edits.sheet_bindings.get(relative_path) {
            None => (None, None),
            Some(UserSheetBinding::Cleared) => (Some("cleared"), None),
            Some(UserSheetBinding::Describes { file_id }) => {
                (Some("describes"), Some(file_id.as_str()))
            }
        };
        let (sheet_disc, sheet_disc_number) = match edits.sheet_discs.get(relative_path) {
            None => (None, None),
            Some(SheetDisc::Ignored) => (Some("ignored"), None),
            Some(SheetDisc::Disc { number }) => (Some("disc"), Some(number)),
        };
        sql.execute(
            "INSERT INTO import_candidate_file_edit \
                 (content_hash, relative_path, role_choice, sheet_binding, \
                  sheet_binding_file_id, sheet_disc, sheet_disc_number) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
            params![
                content_hash,
                relative_path,
                role_choice,
                sheet_binding,
                sheet_binding_file_id,
                sheet_disc,
                sheet_disc_number,
            ],
        )?;
    }
    Ok(())
}

pub(super) struct FileEditRow {
    pub(super) content_hash: String,
    relative_path: String,
    role_choice: Option<String>,
    sheet_binding: Option<String>,
    sheet_binding_file_id: Option<String>,
    sheet_disc: Option<String>,
    sheet_disc_number: Option<i64>,
}

pub(super) fn read_file_edit_row(row: &Row<'_>) -> Result<FileEditRow, DbError> {
    Ok(FileEditRow {
        content_hash: row.get("content_hash")?,
        relative_path: row.get("relative_path")?,
        role_choice: row.get("role_choice")?,
        sheet_binding: row.get("sheet_binding")?,
        sheet_binding_file_id: row.get("sheet_binding_file_id")?,
        sheet_disc: row.get("sheet_disc")?,
        sheet_disc_number: row.get("sheet_disc_number")?,
    })
}

/// Fold one stored row into the decisions being assembled for its candidate.
pub(super) fn apply_file_edit_row(
    edits: &mut CandidateFileEdits,
    row: FileEditRow,
) -> Result<(), DbError> {
    if let Some(role_choice) = row.role_choice {
        let choice = match role_choice.as_str() {
            "audio" => FileRoleChoice::Audio,
            "not_a_track" => FileRoleChoice::NotATrack,
            other => return Err(unreadable("role_choice", other)),
        };
        edits.file_roles.set(row.relative_path.clone(), choice);
    }
    if let Some(sheet_binding) = row.sheet_binding {
        let binding = match sheet_binding.as_str() {
            "cleared" => UserSheetBinding::Cleared,
            "describes" => UserSheetBinding::Describes {
                file_id: row.sheet_binding_file_id.ok_or_else(|| {
                    DbError::Message(format!(
                        "the binding stored for {} describes no file",
                        row.relative_path
                    ))
                })?,
            },
            other => return Err(unreadable("sheet_binding", other)),
        };
        edits.sheet_bindings.set(row.relative_path.clone(), binding);
    }
    if let Some(sheet_disc) = row.sheet_disc {
        let disc = match sheet_disc.as_str() {
            "ignored" => SheetDisc::Ignored,
            "disc" => {
                let number = row.sheet_disc_number.ok_or_else(|| {
                    DbError::Message(format!(
                        "the disc stored for {} states no number",
                        row.relative_path
                    ))
                })?;
                SheetDisc::Disc {
                    number: u32::try_from(number).map_err(|_| {
                        DbError::Message(format!(
                            "the disc stored for {} is numbered {number}",
                            row.relative_path
                        ))
                    })?,
                }
            }
            other => return Err(unreadable("sheet_disc", other)),
        };
        edits.sheet_discs.set(row.relative_path, disc);
    }
    Ok(())
}
