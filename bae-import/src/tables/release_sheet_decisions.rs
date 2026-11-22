//! What the person decided about a release's CUE sheets in the import pane:
//! which audio each `FILE` line describes (`release_sheet_bindings`) and which
//! side or disc each sheet is on (`release_sheet_places`). The import's commit writes them
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
        coven::SyncedTable::new("release_sheet_places", coven::RowIdentity::SharedKey)
            .key_columns(["sheet_file_id"]),
    ]
}

fn no_namespaces() -> Vec<coven::Namespace> {
    Vec::new()
}

use super::*;
use crate::folder_scanner::{CandidateFileEdits, SheetPlace, UserSheetBinding};

/// Write the sheet decisions in `decided`, which name files by their path in
/// the release, as rows over the release's files `file_ids` (each path with
/// the id of its `release_files` row). A candidate's decisions name only its
/// own files, and the release holds every one of them, so a decision that
/// names any other path is a damaged row, and `store_on` fails the commit.
pub(super) fn store_on(
    sql: &CovenWrite<'_>,
    decided: &CandidateFileEdits,
    file_ids: &HashMap<&str, &str>,
) -> Result<(), DbError> {
    let file_id = |path: &str| {
        file_ids
            .get(path)
            .copied()
            .ok_or_else(|| DbError::unreadable("a candidate's sheet decisions", path))
    };
    for (sheet, place) in decided.sheet_places.iter() {
        let sheet_file_id = file_id(sheet)?;
        let (kind, medium, side) = place.columns();
        sql.execute(
            "INSERT INTO release_sheet_places (sheet_file_id, place, medium, side) \
             VALUES (?, ?, ?, ?)",
            params![sheet_file_id, kind, medium, side],
        )?;
    }
    for (sheet, references) in decided.sheet_bindings.iter() {
        let sheet_file_id = file_id(sheet)?;
        for (reference, decision) in references.iter() {
            let audio_file_id = decision.stored_file().map(file_id).transpose()?;
            sql.execute(
                "INSERT INTO release_sheet_bindings (sheet_file_id, file_reference, audio_file_id) \
                 VALUES (?, ?, ?)",
                params![sheet_file_id, reference, audio_file_id],
            )?;
        }
    }
    Ok(())
}

/// Store release editor place choices by sheet file id, replacing any
/// previous choice. The editor's save calls it
/// through `store_release_edit_sheet_decisions_on`.
pub(super) fn store_sheet_place_decisions_on<'a>(
    sql: &CovenWrite<'_>,
    places: impl IntoIterator<Item = (&'a str, SheetPlace)>,
) -> Result<(), DbError> {
    for (sheet_file_id, place) in places {
        let (kind, medium, side) = place.columns();
        sql.execute(
            "INSERT INTO release_sheet_places (sheet_file_id, place, medium, side) \
             VALUES (?, ?, ?, ?) \
             ON CONFLICT (sheet_file_id) DO UPDATE SET place = excluded.place, \
                 medium = excluded.medium, side = excluded.side",
            params![sheet_file_id, kind, medium, side],
        )?;
    }
    Ok(())
}

/// Store the audio files the person picked for `FILE` lines of CUE sheets in
/// a release editor, each as the line's binding: a line decided before
/// takes the new file. The editor's save calls it through
/// `store_release_edit_sheet_decisions_on`.
pub(super) fn store_sheet_binding_decisions_on(
    sql: &CovenWrite<'_>,
    picks: &[bae_records::records::sheet_files::SheetFilePick],
) -> Result<(), DbError> {
    for pick in picks {
        sql.execute(
            "INSERT INTO release_sheet_bindings (sheet_file_id, file_reference, audio_file_id) \
             VALUES (?, ?, ?) \
             ON CONFLICT (sheet_file_id, file_reference) \
             DO UPDATE SET audio_file_id = excluded.audio_file_id",
            params![pick.sheet_file_id, pick.file_reference, pick.audio_file_id],
        )?;
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
        let (binding_rows, place_rows) = self
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
                let places = sql.query(
                    "SELECT sheet.original_filename, place.place, place.medium, place.side \
                     FROM release_files AS sheet \
                     JOIN release_sheet_places AS place ON place.sheet_file_id = sheet.id \
                     WHERE sheet.release_id = ?",
                    [&release_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, Option<i64>>(2)?,
                            row.get::<_, Option<i64>>(3)?,
                        ))
                    },
                )?;
                Ok((bindings, places))
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
        for (sheet, kind, medium, side) in place_rows {
            let place = SheetPlace::from_columns(
                &format!("the release_sheet_places row of {sheet}"),
                &kind,
                medium,
                side,
            )?;
            edits.sheet_places.set(sheet, place);
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
    ) -> Result<(), DbError> {
        self.store
            .write(move |sql| {
                let file_ids: HashMap<&str, &str> =
                    file_ids.iter().map(|(path, id)| (path.as_str(), id.as_str())).collect();
                store_on(&sql, &decided, &file_ids)
            })
            .await
    }
}

#[cfg(test)]
#[path = "release_sheet_decisions_tests.rs"]
mod tests;
