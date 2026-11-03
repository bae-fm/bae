use super::*;
use bae_db::client::query::CandidateRows;

mod album_pick_rows;
mod draft_store;
pub(super) mod failure_rows;
mod file_decision_rows;
mod import_commit;
mod ledger_rows;
mod lookup_choice_rows;
mod lookup_failure_columns;
mod match_rows;
mod pane_rows;
mod preparation_rows;
mod prepared_asset_rows;
mod rows;
mod run_rows;
mod search_rows;
mod session_rows;
mod signal_rows;
mod standing_rows;
mod verdict_rows;

pub(super) use album_pick_rows::load_album_pick_on;
use failure_rows::{delete_failure_on, load_failure_on};
use file_decision_rows::{delete_file_edits, insert_file_edits, settle_scanned_candidates};
pub(super) use file_decision_rows::{load_candidate_file_edits_on, store_file_edits};
pub use import_commit::ImportCommitError;
pub(crate) use import_commit::{ImportCommitGuard, ImportRows, NewArtistImages};
pub(super) use pane_rows::{cover_read_columns, insert_draft, load_pane_rows_on, read_cover};
pub(crate) use preparation_rows::{
    CandidatePaneWrite, CandidatePreparationRead, CandidateSaveExpectation, CandidateSaveExtras,
    CandidateSaved, RunVerdictSaved, ScannedCandidateKey,
};
pub(super) use rows::{
    load_states_on, read_provenance, read_release_link, PROVENANCE_COLUMNS, RELEASE_LINK_COLUMNS,
};
use session_rows::load_session_on;
use signal_rows::{delete_signals, store_signals};
pub(crate) use standing_rows::ImportClaim;
pub(super) use standing_rows::{
    change_identification_on, end_file_work_unless_of_on, load_standing_on, move_import_on,
    RuntimeFactColumns, RUNTIME_FACT_COLUMNS,
};

use crate::folder_scanner::StoredCandidateEdits;
use crate::CandidateKey;
use rows::{insert_provenance, load_states_rows_on};
use std::collections::HashSet;
use std::path::Path;
use verdict_rows::{delete_verdict, store_verdict};

/// Move the pane inside a candidate write, without splitting the transaction.
pub(crate) fn move_pane_on(
    sql: &CovenWrite<'_>,
    content_hash: &str,
    pane_move: crate::PaneMove,
) -> Result<(), DbError> {
    session_rows::move_pane_on(sql, content_hash, pane_move)
}

/// Seed a candidate's draft from the folder's file tags, authored by the
/// pre-fill, with provenance naming the tags as its source.
pub(crate) fn insert_file_tags_draft(
    sql: &CovenWrite<'_>,
    content_hash: &str,
    draft: &crate::CandidateDraft,
) -> Result<(), DbError> {
    pane_rows::insert_draft(sql, content_hash, draft, crate::MetadataAuthor::Prefill)?;
    insert_provenance(sql, content_hash, &crate::MetadataProvenance::FileMetadata)?;
    Ok(())
}

pub(super) fn require_current_candidate(
    sql: &CovenWrite<'_>,
    watched_folder_path: &Path,
    candidate_path: &CandidateKey,
    content_hash: &str,
    expected_file_edit_revision: u64,
) -> Result<u64, DbError> {
    let current: Option<(String, i64, i64)> = sql
        .query_row(
            "SELECT content_hash, generation, file_edit_revision \
             FROM scan_candidate WHERE watched_folder_path = ? AND path = ? AND kind = 'valid'",
            params![coven::path_bytes(watched_folder_path), candidate_path],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let expected_file_edit_revision =
        to_i64(expected_file_edit_revision, "candidate file revision")?;
    let Some((current_hash, generation, current_file_edit_revision)) = current else {
        return Err(DbError::Message(format!(
            "candidate changed before metadata was stored: {candidate_path} is no longer valid"
        )));
    };
    if current_hash != content_hash || current_file_edit_revision != expected_file_edit_revision {
        return Err(DbError::Message(format!(
            "candidate changed before metadata was stored: {candidate_path} no longer names the prepared files"
        )));
    }
    u64::try_from(generation).map_err(|error| {
        DbError::Message(format!("candidate scan generation is negative: {error}"))
    })
}

/// The next scan generation. An upsert, so a missing seed row is created
/// rather than failing the scan.
pub(super) fn next_folder_scan_generation(sql: &CovenWrite<'_>) -> Result<i64, DbError> {
    let generation: i64 = sql.query_row(
        "INSERT INTO folder_scan_generation_sequence (singleton, last_generation) \
         VALUES (1, 1) \
         ON CONFLICT(singleton) DO UPDATE SET last_generation = last_generation + 1 \
         RETURNING last_generation",
        [],
        |row| row.get(0),
    )?;
    Ok(generation)
}

impl ImportTables {
    /// Every stored `import_candidate_state` row, keyed by `content_hash`, for
    /// the tests that read back everything a write stored. A column holding a
    /// value no writer produces fails the read.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn load_import_candidate_states(
        &self,
    ) -> Result<HashMap<String, DbImportCandidateState>, DbError> {
        self.database
            .read(move |sql| load_states_rows_on(&sql, CandidateRows::All))
            .process(|process| process())
            .await
    }

    pub async fn load_import_candidate_state(
        &self,
        content_hash: &str,
    ) -> Result<Option<DbImportCandidateState>, DbError> {
        let content_hash = content_hash.to_string();
        self.database
            .read(move |sql| {
                Ok((load_states_rows_on(&sql, CandidateRows::One(&content_hash))?, content_hash))
            })
            .process(|(process, content_hash)| Ok(process()?.remove(&content_hash)))
            .await
    }

    /// Whether a verdict is stored for the files at `content_hash` as their
    /// file decisions stand at `edit_revision`.
    pub(crate) async fn verdict_stored_for(
        &self,
        content_hash: &str,
        edit_revision: u64,
    ) -> Result<bool, DbError> {
        let content_hash = content_hash.to_string();
        let edit_revision = to_i64(edit_revision, "a candidate's file edit revision")?;
        self.database
            .read(move |sql| {
                Ok(sql.query_row(
                    "SELECT EXISTS (SELECT 1 FROM import_candidate_state AS state \
                     JOIN import_candidate_verdict AS verdict \
                       ON verdict.content_hash = state.content_hash \
                     WHERE state.content_hash = ? AND state.edit_revision = ?)",
                    params![content_hash, edit_revision],
                    |row| row.get(0),
                )?)
            })
            .await
    }
}

/// The stored medium conflict of a verdict row.
pub(super) fn medium_conflict_of(
    kind: Option<String>,
) -> Result<Option<bae_identify::MediumConflict>, DbError> {
    let Some(kind) = kind else {
        return Ok(None);
    };
    Ok(Some(match kind.as_str() {
        "cd_rip" => bae_identify::MediumConflict::CdRip,
        "not_cd_audio" => bae_identify::MediumConflict::NotCdAudio,
        other => return Err(verdict_rows::unreadable("medium_conflict", other)),
    }))
}

#[cfg(test)]
#[path = "import_state_tests.rs"]
pub(crate) mod tests;
