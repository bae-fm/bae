//! What the person decided about a release's CUE sheets in the import pane:
//! which audio each `FILE` line describes (`release_sheet_bindings`) and which
//! side or disc each sheet is on (`release_sheet_places`). The import's commit writes them
//! from the candidate's decisions; identifying the release again reads them,
//! so its sheets bind and number as the person chose. Both tables sync with
//! the release's files.
//!
//! Its tables are `release_sheet_decisions.sql`.

pub(crate) fn synced_tables() -> Vec<coven::SyncedTable> {
    vec![
        coven::SyncedTable::new("release_sheet_bindings", coven::RowIdentity::SharedKey)
            .key_columns(["sheet_file_id", "file_reference"]),
        coven::SyncedTable::new("release_sheet_places", coven::RowIdentity::SharedKey)
            .key_columns(["sheet_file_id"]),
    ]
}

pub(crate) fn no_namespaces() -> Vec<coven::Namespace> {
    Vec::new()
}

use super::*;
use crate::folder_scanner::{CandidateFileEdits, SheetPlace, UserSheetBinding};
use bae_store::WriteStatements;

coven::write!(
    InsertSheetPlace(
        sheet_file_id: &'p str,
        place: &'static str,
        medium: Option<u32>,
        side: Option<u32>,
    ),
    r#"
    INSERT INTO release_sheet_places (sheet_file_id, place, medium, side)
    VALUES (:sheet_file_id, :place, :medium, :side)
"#
);

coven::write!(
    ReplaceSheetPlace(
        sheet_file_id: &'p str,
        place: &'static str,
        medium: Option<u32>,
        side: Option<u32>,
    ),
    r#"
    INSERT INTO release_sheet_places (sheet_file_id, place, medium, side)
    VALUES (:sheet_file_id, :place, :medium, :side)
    ON CONFLICT (sheet_file_id) DO UPDATE SET
        place = excluded.place, medium = excluded.medium, side = excluded.side
"#
);

coven::write!(
    InsertSheetBinding(
        sheet_file_id: &'p str,
        file_reference: &'p str,
        audio_file_id: Option<&'p str>,
    ),
    r#"
    INSERT INTO release_sheet_bindings (sheet_file_id, file_reference, audio_file_id)
    VALUES (:sheet_file_id, :file_reference, :audio_file_id)
"#
);

coven::write!(
    ReplaceSheetBinding(
        sheet_file_id: &'p str,
        file_reference: &'p str,
        audio_file_id: Option<&'p str>,
    ),
    r#"
    INSERT INTO release_sheet_bindings (sheet_file_id, file_reference, audio_file_id)
    VALUES (:sheet_file_id, :file_reference, :audio_file_id)
    ON CONFLICT (sheet_file_id, file_reference)
    DO UPDATE SET audio_file_id = excluded.audio_file_id
"#
);

// The decisions the person made about the release `release`'s sheets, naming
// each file by its name in the release: the JSON arrays of the bindings of
// their `FILE` lines and of the sides they are on.
coven::fragment!(
    ReleaseSheetDecisions(release: String) {
        sheet_bindings: String,
        sheet_places: String
    },
    r#"
    SELECT (SELECT json_group_array(json_object(
                       'sheet', sheet.original_filename, 'reference', binding.file_reference,
                       'audio', audio.original_filename)
                       ORDER BY sheet.original_filename, binding.file_reference)
            FROM @bae_records::records::file_fragments::ReleaseFileColumns AS sheet
            JOIN release_sheet_bindings AS binding ON binding.sheet_file_id = sheet.id
            LEFT JOIN @bae_records::records::file_fragments::ReleaseFileColumns AS audio
                   ON audio.id = binding.audio_file_id
            WHERE sheet.release_id = :release) AS sheet_bindings,
           (SELECT json_group_array(json_object(
                       'sheet', sheet.original_filename, 'place', place.place,
                       'medium', place.medium, 'side', place.side)
                       ORDER BY sheet.original_filename)
            FROM @bae_records::records::file_fragments::ReleaseFileColumns AS sheet
            JOIN release_sheet_places AS place ON place.sheet_file_id = sheet.id
            WHERE sheet.release_id = :release) AS sheet_places
"#
);

// The decisions the person made about one release's sheets.
coven::read!(
    writer OneReleaseSheetDecisions(release: String),
    r#"
    SELECT decided.*
    FROM @crate::tables::release_sheet_decisions::ReleaseSheetDecisions(
        release: :release) AS decided
"#
);

/// One binding as [`ReleaseSheetDecisions`] writes it.
#[derive(serde::Deserialize)]
struct BindingJson {
    sheet: String,
    reference: String,
    audio: Option<String>,
}

/// One sheet's side as [`ReleaseSheetDecisions`] writes it.
#[derive(serde::Deserialize)]
struct PlaceJson {
    sheet: String,
    place: String,
    medium: Option<i64>,
    side: Option<i64>,
}

impl ReleaseSheetDecisions {
    /// The decisions the rows hold. Their revision is zero, because a stored
    /// release does not track edit revisions.
    pub(crate) fn decoded(self) -> Result<CandidateFileEdits, DbError> {
        fn rows<T: serde::de::DeserializeOwned>(json: &str) -> Result<Vec<T>, DbError> {
            serde_json::from_str(json).map_err(|error| {
                DbError::unreadable_json("a release's sheet decisions", json, error)
            })
        }
        let mut edits = CandidateFileEdits::default();
        for binding in rows::<BindingJson>(&self.sheet_bindings)? {
            edits.sheet_bindings.set_reference(
                binding.sheet,
                binding.reference,
                UserSheetBinding::from_stored_file(binding.audio),
            );
        }
        for stored in rows::<PlaceJson>(&self.sheet_places)? {
            let place = SheetPlace::from_columns(
                &format!("the release_sheet_places row of {}", stored.sheet),
                &stored.place,
                stored.medium,
                stored.side,
            )?;
            edits.sheet_places.set(stored.sheet, place);
        }
        Ok(edits)
    }
}

/// Write the sheet decisions in `decided`, which name files by their path in
/// the release, as rows over the release's files `file_ids` (each path with
/// the id of its `release_files` row). A candidate's decisions name only its
/// own files, and the release holds every one of them but the audio
/// `left_out`, so a decision that names any other path is a damaged row, and
/// `store_on` fails the commit. A binding of a line to left-out audio is not
/// the release's, which has no such file.
pub(super) fn store_on(
    sql: &WriteContext<'_>,
    decided: &CandidateFileEdits,
    file_ids: &HashMap<&str, &str>,
    left_out: &[String],
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
        sql.run(&InsertSheetPlaceParams { sheet_file_id, place: kind, medium, side })?;
    }
    for (sheet, references) in decided.sheet_bindings.iter() {
        let sheet_file_id = file_id(sheet)?;
        for (reference, decision) in references.iter() {
            if decision.stored_file().is_some_and(|path| left_out.iter().any(|out| out == path)) {
                continue;
            }
            let audio_file_id = decision.stored_file().map(file_id).transpose()?;
            sql.run(&InsertSheetBindingParams {
                sheet_file_id,
                file_reference: reference,
                audio_file_id,
            })?;
        }
    }
    Ok(())
}

/// Store release editor place choices by sheet file id, replacing any
/// previous choice. The editor's save calls it
/// through `store_release_edit_sheet_decisions_on`.
pub(super) fn store_sheet_place_decisions_on<'a>(
    sql: &WriteContext<'_>,
    places: impl IntoIterator<Item = (&'a str, SheetPlace)>,
) -> Result<(), DbError> {
    for (sheet_file_id, place) in places {
        let (kind, medium, side) = place.columns();
        sql.run(&ReplaceSheetPlaceParams { sheet_file_id, place: kind, medium, side })?;
    }
    Ok(())
}

/// Store the audio files the person picked for `FILE` lines of CUE sheets in
/// a release editor, each as the line's binding: a line decided before
/// takes the new file. The editor's save calls it through
/// `store_release_edit_sheet_decisions_on`.
pub(super) fn store_sheet_binding_decisions_on(
    sql: &WriteContext<'_>,
    picks: &[bae_records::records::sheet_files::SheetFilePick],
) -> Result<(), DbError> {
    for pick in picks {
        sql.run(&ReplaceSheetBindingParams {
            sheet_file_id: &pick.sheet_file_id,
            file_reference: &pick.file_reference,
            audio_file_id: pick.audio_file_id.as_deref(),
        })?;
    }
    Ok(())
}

impl ImportTables {
    /// The decisions the person made about `release_id`'s sheets, naming each
    /// file by its path in the release, as the import's decisions did
    /// ([`ReleaseSheetDecisions`]).
    pub(crate) async fn load_release_sheet_decisions(
        &self,
        release_id: &str,
    ) -> Result<CandidateFileEdits, DbError> {
        let statement = OneReleaseSheetDecisionsParams { release: release_id.to_string() };
        self.store
            .read(move |sql| bae_store::statement_row(&sql, &statement))
            .process(|row| row.decided.decoded())
            .await
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
                store_on(&sql, &decided, &file_ids, &[])
            })
            .await
    }
}

#[cfg(test)]
#[path = "release_sheet_decisions_tests.rs"]
mod tests;
