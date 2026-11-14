//! Each import candidate's stored state, by the content hash of its files:
//! its state row and what hangs off it — the draft, the file decisions, the
//! verdict and its ledger, the matches, searches and lookups, the session,
//! the standing and the import that commits it. Their tables are the SQL
//! files beside the row files.

/// The candidate state's part of the library's schema. Each file runs after
/// the ones creating the tables its triggers fire on: the standings before
/// the runs a standing moving off deletes.
pub(super) const SCHEMA: bae_store::SchemaPart = bae_store::SchemaPart::new(&[
    seed_rows::SQL,
    pane_rows::SQL,
    pane_rows::VALUES_SQL,
    file_decision_rows::SQL,
    cover_rows::SQL,
    album_pick_rows::SQL,
    pane_rows::PROVENANCE_SQL,
    release_link_rows::SQL,
    session_rows::SQL,
    standing_rows::SQL,
    run_rows::SQL,
    search_rows::SQL,
    failure_rows::SQL,
    signal_rows::SQL,
    lookup_choice_rows::SQL,
    verdict_rows::SQL,
    match_rows::SQL,
    ledger_rows::SQL,
    blank_draft_rows::SQL,
]);

use super::*;

/// Why a write about a candidate's state, or the read a run starts from, was
/// refused: the candidate left the scan, was imported, or changed under the
/// request. Nothing of it was written.
#[derive(Debug, thiserror::Error)]
pub enum CandidateStateRefusal {
    /// The files hashed so hold no state: the scan holds no copy of them.
    #[error("candidate {0} has no state row")]
    NoState(String),
    /// The candidate's file decisions moved past the revision the request
    /// was made against.
    #[error("candidate {content_hash}'s file decisions changed from revision {revision}")]
    FilesChanged { content_hash: String, revision: u64 },
    /// The candidate's metadata moved past the revision the request was made
    /// against.
    #[error("candidate {content_hash}'s metadata changed from revision {revision}")]
    MetadataChanged { content_hash: String, revision: u64 },
    /// Another write of the candidate landed while this one was being stored.
    #[error("candidate {0} changed while this write was being stored")]
    Superseded(String),
    /// The candidate holds no lookup for the answer to act on: its lookup
    /// ended meanwhile.
    #[error("candidate {0} holds no lookup")]
    NoLookup(String),
    /// The key names no release the scan holds.
    #[error("{0} is no candidate a search can be of")]
    NotScanned(CandidateKey),
    /// The import does not hold the candidate: it ended, or another holds it.
    #[error("import {import_id} does not own {candidate_key}")]
    NotOwner { import_id: String, candidate_key: CandidateKey },
    /// The candidate holds no draft to identify from: it left the scan or was
    /// imported.
    #[error("candidate {0} holds no draft")]
    NoDraft(String),
}

mod album_pick_rows;
mod blank_draft_rows;
mod candidate_rows;
mod cards_read;
mod cover_rows;
mod draft_store;
pub(super) mod failure_rows;
mod file_decision_rows;
mod import_commit;
mod layout;
mod ledger_rows;
mod lookup_choice_rows;
mod lookup_failure_columns;
mod match_rows;
mod pane_rows;
mod preparation_rows;
mod release_link_rows;
mod rows;
mod run_rows;
mod search_rows;
mod seed_rows;
mod session_rows;
mod signal_rows;
mod standing_rows;
mod verdict_layout;
mod verdict_rows;

pub(super) use album_pick_rows::load_album_pick_on;
pub(super) use candidate_rows::CandidateRows;
pub(super) use cards_read::load_verdict_cards_on;
pub(super) use cover_rows::{cover_read_columns, read_cover};
use failure_rows::{delete_failure_on, load_failure_on};
pub(super) use file_decision_rows::load_candidate_file_edits_on;
use file_decision_rows::{delete_file_edits, insert_file_edits, settle_scanned_candidates};
pub(crate) use import_commit::{
    CommittingCandidate, ImportCommitGuard, ImportRows, NewArtistImages,
};
pub use import_commit::{ImportCommitError, ImportCommitRefusal};
pub(super) use pane_rows::load_pane_rows_on;
pub use pane_rows::DbCandidatePaneRows;
pub(super) use pane_rows::{read_provenance, PROVENANCE_COLUMNS};
pub use preparation_rows::PreparationRefusal;
pub(crate) use preparation_rows::{
    CandidatePaneWrite, CandidatePreparationRead, CandidateSaveExpectation, CandidateSaveExtras,
    CandidateSaved, RunVerdictSaved, ScannedCandidateKey,
};
pub(super) use release_link_rows::{read_release_link, RELEASE_LINK_COLUMNS};
pub(super) use rows::load_states_on;
pub use rows::DbImportCandidateState;
pub(super) use seed_rows::{ensure_candidate_state, forget_at_or_below, restart_state_on};
use session_rows::load_session_on;
use signal_rows::{delete_signals, store_signals};
pub(crate) use standing_rows::ImportClaim;
#[cfg(test)]
pub(crate) use standing_rows::StandingColumn;
pub use verdict_rows::{DbCandidateIdentifyResult, NewImportCandidateVerdict, VerdictPick};
// The source releases' tests store a run's releases as a release run does.
#[cfg(test)]
pub(super) use run_rows::store_release_run_on;
pub(super) use standing_rows::{
    change_identification_on, end_file_work_unless_of_on, load_standing_on, move_import_on,
    RuntimeFactColumns, RUNTIME_FACT_COLUMNS,
};

use crate::folder_scanner::StoredCandidateEdits;
use crate::CandidateKey;
use bae_catalogs::catalog::LookupCatalog;
use bae_catalogs::records::LookupRef;
use rows::load_states_rows_on;
use std::str::FromStr;
use verdict_rows::{delete_verdict, store_verdict};

impl ImportTables {
    /// Every stored `import_candidate_state` row, keyed by `content_hash`, for
    /// the tests that read back everything a write stored. A column holding a
    /// value no writer produces fails the read.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn load_import_candidate_states(
        &self,
    ) -> Result<HashMap<String, DbImportCandidateState>, DbError> {
        self.store
            .read(move |sql| load_states_rows_on(&sql, CandidateRows::All))
            .process(|process| process())
            .await
    }

    /// [`Self::load_import_candidate_states`], live: a test waits on the write
    /// it is after instead of reading again until it shows.
    #[cfg(test)]
    pub(crate) fn subscribe_import_candidate_states(
        &self,
    ) -> coven::LiveQuery<HashMap<String, DbImportCandidateState>, bae_store::DbError> {
        self.store.subscribe(|sql| rows::load_states_on(&sql, CandidateRows::All))
    }

    pub async fn load_import_candidate_state(
        &self,
        content_hash: &str,
    ) -> Result<Option<DbImportCandidateState>, DbError> {
        let content_hash = content_hash.to_string();
        self.store
            .read(move |sql| {
                Ok((load_states_rows_on(&sql, CandidateRows::One(&content_hash))?, content_hash))
            })
            .process(|(process, content_hash)| Ok(process()?.remove(&content_hash)))
            .await
    }

    /// What a run of the candidate `content_hash` starts from: its state and
    /// its draft, read together. `None` when it holds no draft: it left the
    /// scan, or this device imported its files.
    pub(crate) async fn load_candidate_run_start(
        &self,
        content_hash: &str,
    ) -> Result<Option<(DbImportCandidateState, crate::CandidateDraft)>, DbError> {
        let content_hash = content_hash.to_string();
        self.store
            .read(move |sql| {
                let stated = sql
                    .query_row(
                        "SELECT 1 FROM import_candidate_state WHERE content_hash = ?",
                        [&content_hash],
                        |_| Ok(()),
                    )
                    .optional()?
                    .is_some();
                if !stated
                    || crate::tables::import_origin::imported_release_on(&sql, &content_hash)?
                        .is_some()
                {
                    return Ok(None);
                }
                let (draft, _) = pane_rows::load_draft_and_cover_on(&sql, &content_hash)?;
                let states = load_states_rows_on(&sql, CandidateRows::One(&content_hash))?;
                Ok(Some((states, draft, content_hash)))
            })
            .process(|read| {
                let Some((states, draft, content_hash)) = read else {
                    return Ok(None);
                };
                // Found stated in this read.
                let state = states()?.remove(&content_hash).ok_or_else(|| {
                    DbError::unreadable(
                        "the stated candidate's import_candidate_state",
                        &content_hash,
                    )
                })?;
                Ok(Some((state, draft)))
            })
            .await
    }
}

/// A stored release a pick names; a catalog bae does not fetch from names
/// none.
fn lookup_reference(catalog: String, key: String) -> Result<LookupRef, DbError> {
    Ok(LookupRef::new(
        LookupCatalog::from_str(&catalog)
            .map_err(|_unknown| DbError::unreadable("a stored pick's catalog", &catalog))?,
        key,
    ))
}

#[cfg(test)]
#[path = "import_state_tests.rs"]
pub(crate) mod tests;
