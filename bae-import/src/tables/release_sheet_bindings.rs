//! The bindings the person made between a release's CUE sheets and its audio
//! (`release_sheet_bindings`): written in the import's commit from the
//! candidate's decisions, read when the release is identified again, so its
//! sheets bind to the audio the person chose. The table syncs with the
//! release's files.
//!
//! Its table is `release_sheet_bindings.sql`.

/// The release sheet bindings' part of the library's schema.
pub(super) const SCHEMA: bae_store::SchemaPart =
    bae_store::SchemaPart::new(&[include_str!("release_sheet_bindings.sql")])
        .synced(synced_tables, no_namespaces);

fn synced_tables() -> Vec<coven::SyncedTable> {
    vec![coven::SyncedTable::new("release_sheet_bindings", coven::RowIdentity::SharedKey)
        .key_columns(["sheet_file_id", "file_reference"])]
}

fn no_namespaces() -> Vec<coven::Namespace> {
    Vec::new()
}

use super::*;
use crate::folder_scanner::{SheetBindingEdits, UserSheetBinding};

/// Write the bindings in `decided`, which name files by their path in the
/// release, as rows over the release's files `file_ids` (each path with the
/// id of its `release_files` row). A decision naming a path that is no file of
/// the release describes nothing, as the scan reads it, and writes no row.
pub(super) fn store_on(
    sql: &CovenWrite<'_>,
    decided: &SheetBindingEdits,
    file_ids: &HashMap<&str, &str>,
) -> Result<(), DbError> {
    for (sheet, references) in decided.iter() {
        let Some(sheet_file_id) = file_ids.get(sheet) else {
            continue;
        };
        for (reference, decision) in references.iter() {
            let audio_file_id = match decision {
                UserSheetBinding::Describes { file_id } => match file_ids.get(file_id.as_str()) {
                    Some(audio_file_id) => Some(*audio_file_id),
                    None => continue,
                },
                UserSheetBinding::Cleared => None,
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
    /// The bindings the person made for `release_id`'s sheets, naming each
    /// file by its path in the release, as the import's decisions did.
    pub(crate) async fn load_release_sheet_bindings(
        &self,
        release_id: &str,
    ) -> Result<SheetBindingEdits, DbError> {
        let release_id = release_id.to_string();
        let rows = self
            .store
            .read(move |sql| {
                Ok(sql.query(
                    "SELECT sheet.original_filename, binding.file_reference, \
                            audio.original_filename \
                     FROM release_files AS sheet \
                     JOIN release_sheet_bindings AS binding ON binding.sheet_file_id = sheet.id \
                     LEFT JOIN release_files AS audio ON audio.id = binding.audio_file_id \
                     WHERE sheet.release_id = ?",
                    [release_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, Option<String>>(2)?,
                        ))
                    },
                )?)
            })
            .await?;
        let mut edits = SheetBindingEdits::default();
        for (sheet, reference, audio) in rows {
            let decision = match audio {
                Some(file_id) => UserSheetBinding::Describes { file_id },
                None => UserSheetBinding::Cleared,
            };
            edits.set_reference(sheet, reference, decision);
        }
        Ok(edits)
    }
}

#[cfg(test)]
impl ImportTables {
    /// Store `decided`, naming files by path, over `file_ids` (path to the id
    /// of its `release_files` row), as the import's commit stores them.
    pub(crate) async fn store_release_sheet_bindings_for_test(
        &self,
        decided: SheetBindingEdits,
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
#[path = "release_sheet_bindings_tests.rs"]
mod tests;
