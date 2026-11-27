//! A candidate's file decisions as rows: which side or disc each sheet is on, and which
//! audio each of its `FILE` references describes (an absent row is no
//! decision at all); read back by the next scan, and settled onto every
//! scanned release reading the same files.
//! Its tables are `file_decision_rows.sql`, which every device's library holds.

use super::*;
use crate::folder_scanner::{CandidateFileEdits, SheetPlace, UserSheetBinding};
use crate::tables::FinishRead;
use bae_store::WriteStatements as _;

sqlive_compose::write!(
    DeleteSheetReferences(content_hash: &'p str),
    r#"
    DELETE FROM import_candidate_sheet_reference WHERE content_hash = :content_hash
"#
);

sqlive_compose::write!(
    DeleteSheetPlaces(content_hash: &'p str),
    r#"
    DELETE FROM import_candidate_sheet_place WHERE content_hash = :content_hash
"#
);

sqlive_compose::write!(
    InsertCandidateSheetPlace(
        content_hash: &'p str,
        sheet_id: &'p str,
        place: &'static str,
        medium: Option<u32>,
        side: Option<u32>,
    ),
    r#"
    INSERT INTO import_candidate_sheet_place (content_hash, sheet_id, place, medium, side)
    VALUES (:content_hash, :sheet_id, :place, :medium, :side)
"#
);

sqlive_compose::write!(
    InsertCandidateSheetReference(
        content_hash: &'p str,
        sheet_id: &'p str,
        file_reference: &'p str,
        file_id: Option<&'p str>,
    ),
    r#"
    INSERT INTO import_candidate_sheet_reference (content_hash, sheet_id, file_reference, file_id)
    VALUES (:content_hash, :sheet_id, :file_reference, :file_id)
"#
);

pub(super) fn delete_file_edits(sql: &CovenWrite<'_>, content_hash: &str) -> Result<(), DbError> {
    sql.run(&DeleteSheetReferencesParams { content_hash })?;
    sql.run(&DeleteSheetPlacesParams { content_hash })?;
    Ok(())
}

/// Write every decision `edits` holds. The caller has already cleared what
/// stood under this hash, so this is always writing into empty space.
pub(super) fn insert_file_edits(
    sql: &CovenWrite<'_>,
    content_hash: &str,
    edits: &CandidateFileEdits,
) -> Result<(), DbError> {
    for (sheet_id, place) in edits.sheet_places.iter() {
        let (kind, medium, side) = place.columns();
        sql.run(&InsertCandidateSheetPlaceParams {
            content_hash,
            sheet_id,
            place: kind,
            medium,
            side,
        })?;
    }
    for (sheet_id, references) in edits.sheet_bindings.iter() {
        for (reference, decision) in references.iter() {
            sql.run(&InsertCandidateSheetReferenceParams {
                content_hash,
                sheet_id,
                file_reference: reference,
                file_id: decision.stored_file(),
            })?;
        }
    }
    Ok(())
}

// The file decisions of each candidate in `keys` that has a state row, with
// the revision they stand at: the places the user put its sheets on, and the
// `FILE` reference bindings they decided.
sqlive_compose::fragment!(
    FileEdits with keys(content_hash TEXT) {
        struct StoredSheetPlace {
            sheet_id: String,
            place: String,
            medium: Option<i64>,
            side: Option<i64>
        }
        struct StoredSheetReference {
            sheet_id: String,
            file_reference: String,
            file_id: Option<String>
        }
        places: json<Vec<StoredSheetPlace>>,
        bindings: json<Vec<StoredSheetReference>>
    },
    r#"
    SELECT state.content_hash, state.edit_revision,
        (SELECT json_group_array(json_object('sheet_id', place.sheet_id, 'place', place.place,
                    'medium', place.medium, 'side', place.side) ORDER BY place.sheet_id)
         FROM import_candidate_sheet_place AS place
         WHERE place.content_hash = state.content_hash) AS places,
        (SELECT json_group_array(json_object('sheet_id', binding.sheet_id,
                    'file_reference', binding.file_reference, 'file_id', binding.file_id)
                    ORDER BY binding.sheet_id, binding.file_reference)
         FROM import_candidate_sheet_reference AS binding
         WHERE binding.content_hash = state.content_hash) AS bindings
    FROM keys
    JOIN import_candidate_state AS state ON state.content_hash = keys.content_hash
"#
);

sqlive_compose::read!(
    writer FileEditsOf(content_hashes: String),
    r#"
    SELECT decided.*
    FROM @crate::tables::import_state::file_decision_rows::FileEdits(
        keys: (SELECT given.value AS content_hash
               FROM json_each(:content_hashes) AS given)) AS decided
"#
);

// The content hash of every candidate a decision was made for: those that
// hold one, and those whose revision moved past zero though none stands now.
sqlive_compose::fragment!(
    DecidedFileHashes,
    r#"
    SELECT state.content_hash
    FROM import_candidate_state AS state
    WHERE state.edit_revision > 0
       OR EXISTS (SELECT 1 FROM import_candidate_sheet_place AS placed
                  WHERE placed.content_hash = state.content_hash)
       OR EXISTS (SELECT 1 FROM import_candidate_sheet_reference AS bound
                  WHERE bound.content_hash = state.content_hash)
"#
);

impl FileEdits {
    /// The candidate's decisions, as the scan and its state hold them.
    pub(super) fn edits(self) -> Result<CandidateFileEdits, DbError> {
        let content_hash = &self.content_hash;
        let mut edits = CandidateFileEdits {
            revision: to_u64(self.edit_revision, &format!("{content_hash}'s file edit revision"))?,
            ..CandidateFileEdits::default()
        };
        for stored in self.places {
            let place = SheetPlace::from_columns(
                &format!("the import_candidate_sheet_place row of {}", stored.sheet_id),
                &stored.place,
                stored.medium,
                stored.side,
            )?;
            edits.sheet_places.set(stored.sheet_id, place);
        }
        for stored in self.bindings {
            edits.sheet_bindings.set_reference(
                stored.sheet_id,
                stored.file_reference,
                UserSheetBinding::from_stored_file(stored.file_id),
            );
        }
        Ok(edits)
    }
}

/// Each candidate's decisions in `rows`, keyed by its content hash.
pub(crate) fn edits_by_hash(
    rows: impl IntoIterator<Item = FileEdits>,
) -> Result<HashMap<String, CandidateFileEdits>, DbError> {
    rows.into_iter().map(|row| Ok((row.content_hash.clone(), row.edits()?))).collect()
}

/// The request of [`FileEditsOf`] for `content_hashes`.
fn file_edits_of(content_hashes: &[String]) -> Result<FileEditsOfParams, DbError> {
    let content_hashes = serde_json::to_string(content_hashes)
        .map_err(|error| DbError::encode("the content hashes to read decisions of", error))?;
    Ok(FileEditsOfParams { content_hashes })
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
) -> Result<Vec<crate::folder_scanner::FolderCandidate>, crate::ImportError> {
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
        // Settled above, in this write, as a release reading the files.
        let stored = super::release_groupings::load_candidate_entry_on(sql, path)?
            .unwrap_or_else(|| panic!("{path}, settled in this write, reads as no release"));
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
    /// The file decisions of each of `content_hashes`, in one read. A hash
    /// nothing is stored for reads as no decisions at revision zero.
    pub async fn load_candidate_file_edits_of(
        &self,
        content_hashes: &[String],
    ) -> Result<HashMap<String, CandidateFileEdits>, DbError> {
        let request = file_edits_of(content_hashes)?;
        self.store
            .read(move |sql| {
                let rows = statement_rows(&sql, &request)?;
                Ok(Box::new(move || edits_by_hash(rows.into_iter().map(|row| row.decided)))
                    as FinishRead<HashMap<String, CandidateFileEdits>>)
            })
            .process(|process| process())
            .await
    }
}

/// One candidate's file decisions, revision included; none at revision zero
/// for a candidate with no state row. Progressive scans call this after they
/// compute a content hash, so each emitted row performs one indexed lookup
/// instead of rereading the whole table.
pub(crate) fn load_candidate_file_edits_on(
    sql: &impl bae_store::WriterRead,
    content_hash: &str,
) -> Result<FinishRead<CandidateFileEdits>, DbError> {
    let request = file_edits_of(&[content_hash.to_string()])?;
    let stored = statement_optional(sql, &request)?;
    Ok(Box::new(move || {
        stored.map_or_else(|| Ok(CandidateFileEdits::default()), |row| row.decided.edits())
    }))
}

#[cfg(test)]
impl ImportTables {
    /// Store `edits` as the decisions of the candidate `content_hash`. The
    /// caller must create the candidate's state row first.
    pub(crate) async fn store_file_edits_for_test(
        &self,
        content_hash: &str,
        edits: CandidateFileEdits,
    ) {
        let content_hash = content_hash.to_string();
        self.store.write(move |sql| store_file_edits(&sql, &content_hash, &edits)).await.unwrap();
    }
}

#[cfg(test)]
#[path = "file_decision_rows_tests.rs"]
mod tests;
