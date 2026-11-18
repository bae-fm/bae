//! What the person decided about a release's CUE sheets in the import pane:
//! which audio each `FILE` line describes (`release_sheet_bindings`) and which
//! disc each sheet is (`release_sheet_discs`). The import's commit writes them
//! from the candidate's decisions; identifying the release again reads them,
//! so its sheets bind and number as the person chose. Both tables sync with
//! the release's files.
//!
//! Its tables are `release_sheet_decisions.sql`.

/// The release sheet decisions' part of the library's schema.
pub(super) const SCHEMA: bae_store::SchemaPart =
    bae_store::SchemaPart::new(&[include_str!("release_sheet_decisions.sql")])
        .synced(synced_tables, no_namespaces);

fn synced_tables() -> Vec<coven::SyncedTable> {
    vec![
        coven::SyncedTable::new("release_sheet_bindings", coven::RowIdentity::SharedKey)
            .key_columns(["sheet_file_id", "file_reference"]),
        coven::SyncedTable::new("release_sheet_discs", coven::RowIdentity::SharedKey)
            .key_columns(["sheet_file_id"]),
    ]
}

fn no_namespaces() -> Vec<coven::Namespace> {
    Vec::new()
}

use super::*;
use crate::folder_scanner::{CandidateFileEdits, SheetDisc, UserSheetBinding};

/// Write the sheet decisions in `decided`, which name files by their path in
/// the release, as rows over the release's files `file_ids` (each path with
/// the id of its `release_files` row). When a decision names a path that is
/// not one of the release's files, the scan reads it as describing nothing,
/// so `store_on` writes no row for it.
pub(super) fn store_on(
    sql: &CovenWrite<'_>,
    decided: &CandidateFileEdits,
    file_ids: &HashMap<&str, &str>,
) -> Result<(), DbError> {
    for (sheet, disc) in decided.sheet_discs.iter() {
        let Some(sheet_file_id) = file_ids.get(sheet) else {
            continue;
        };
        let (kind, number) = disc.columns();
        sql.execute(
            "INSERT INTO release_sheet_discs (sheet_file_id, disc, disc_number) VALUES (?, ?, ?)",
            params![sheet_file_id, kind, number],
        )?;
    }
    for (sheet, references) in decided.sheet_bindings.iter() {
        let Some(sheet_file_id) = file_ids.get(sheet) else {
            continue;
        };
        for (reference, decision) in references.iter() {
            let audio_file_id = match decision.stored_file() {
                Some(file_id) => match file_ids.get(file_id) {
                    Some(audio_file_id) => Some(*audio_file_id),
                    None => continue,
                },
                None => None,
            };
            sql.execute(
                "INSERT INTO release_sheet_bindings (sheet_file_id, file_reference, audio_file_id) \
                 VALUES (?, ?, ?)",
                params![sheet_file_id, reference, audio_file_id],
            )?;
        }
    }
    Ok(())
}

impl ImportTables {
    /// The decisions the person made about `release_id`'s sheets, naming each
    /// file by its path in the release, as the import's decisions did. The
    /// returned edits have revision zero, because a stored release does not
    /// track edit revisions.
    pub(crate) async fn load_release_sheet_decisions(
        &self,
        release_id: &str,
    ) -> Result<CandidateFileEdits, DbError> {
        let release_id = release_id.to_string();
        let (binding_rows, disc_rows) = self
            .store
            .read(move |sql| {
                let bindings = sql.query(
                    "SELECT sheet.original_filename, binding.file_reference, \
                            audio.original_filename \
                     FROM release_files AS sheet \
                     JOIN release_sheet_bindings AS binding ON binding.sheet_file_id = sheet.id \
                     LEFT JOIN release_files AS audio ON audio.id = binding.audio_file_id \
                     WHERE sheet.release_id = ?",
                    [&release_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, Option<String>>(2)?,
                        ))
                    },
                )?;
                let discs = sql.query(
                    "SELECT sheet.original_filename, disc.disc, disc.disc_number \
                     FROM release_files AS sheet \
                     JOIN release_sheet_discs AS disc ON disc.sheet_file_id = sheet.id \
                     WHERE sheet.release_id = ?",
                    [&release_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, Option<i64>>(2)?,
                        ))
                    },
                )?;
                Ok((bindings, discs))
            })
            .await?;
        let mut edits = CandidateFileEdits::default();
        for (sheet, reference, audio) in binding_rows {
            edits.sheet_bindings.set_reference(
                sheet,
                reference,
                UserSheetBinding::from_stored_file(audio),
            );
        }
        for (sheet, kind, number) in disc_rows {
            let disc = SheetDisc::from_columns(
                &format!("the release_sheet_discs row of {sheet}"),
                &kind,
                number,
            )?;
            edits.sheet_discs.set(sheet, disc);
        }
        Ok(edits)
    }
}

#[cfg(test)]
impl ImportTables {
    /// Store `decided`, naming files by path, over `file_ids` (path to the id
    /// of its `release_files` row), as the import's commit stores them.
    pub(crate) async fn store_release_sheet_decisions_for_test(
        &self,
        decided: CandidateFileEdits,
        file_ids: HashMap<String, String>,
    ) {
        self.store
            .write(move |sql| {
                let file_ids: HashMap<&str, &str> =
                    file_ids.iter().map(|(path, id)| (path.as_str(), id.as_str())).collect();
                store_on(&sql, &decided, &file_ids)
            })
            .await
            .unwrap();
    }
}

#[cfg(test)]
#[path = "release_sheet_decisions_tests.rs"]
mod tests;
