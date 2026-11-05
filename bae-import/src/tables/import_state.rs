//! Each import candidate's stored state, by the content hash of its files:
//! its state row and what hangs off it — the draft, the file decisions, the
//! verdict and its ledger, the matches, searches and lookups, the session,
//! the standing and the import that commits it. Their tables are the SQL
//! files beside the row files, which every device's library holds.

/// The candidate state's part of the library's schema. Each file runs after
/// the ones creating the tables its triggers fire on: the standings before
/// the runs a standing moving off deletes.
pub(super) const SCHEMA: bae_db::SchemaPart = bae_db::SchemaPart::new(&[
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
]);

#[cfg(desktop)]
use super::*;
#[cfg(desktop)]
use bae_db::client::query::CandidateRows;

mod album_pick_rows;
mod cover_rows;
#[cfg(desktop)]
mod draft_store;
pub(super) mod failure_rows;
mod file_decision_rows;
#[cfg(desktop)]
mod import_commit;
mod ledger_rows;
mod lookup_choice_rows;
#[cfg(desktop)]
mod lookup_failure_columns;
mod match_rows;
mod pane_rows;
#[cfg(desktop)]
mod preparation_rows;
mod release_link_rows;
#[cfg(desktop)]
mod rows;
mod run_rows;
mod search_rows;
mod seed_rows;
mod session_rows;
mod signal_rows;
mod standing_rows;
mod verdict_rows;

#[cfg(desktop)]
pub(super) use album_pick_rows::load_album_pick_on;
#[cfg(desktop)]
pub(super) use cover_rows::{cover_read_columns, read_cover};
#[cfg(desktop)]
use failure_rows::{delete_failure_on, load_failure_on};
#[cfg(desktop)]
pub(super) use file_decision_rows::load_candidate_file_edits_on;
#[cfg(desktop)]
use file_decision_rows::{delete_file_edits, insert_file_edits, settle_scanned_candidates};
#[cfg(desktop)]
pub use import_commit::ImportCommitError;
#[cfg(desktop)]
pub(crate) use import_commit::{
    CommittingCandidate, ImportCommitGuard, ImportRows, NewArtistImages,
};
#[cfg(desktop)]
pub(super) use pane_rows::load_pane_rows_on;
#[cfg(desktop)]
pub use pane_rows::DbCandidatePaneRows;
#[cfg(desktop)]
pub(super) use pane_rows::{read_provenance, PROVENANCE_COLUMNS};
#[cfg(desktop)]
pub(crate) use preparation_rows::{
    CandidatePaneWrite, CandidatePreparationRead, CandidateSaveExpectation, CandidateSaveExtras,
    CandidateSaved, RunVerdictSaved, ScannedCandidateKey,
};
#[cfg(desktop)]
pub(super) use release_link_rows::{read_release_link, RELEASE_LINK_COLUMNS};
#[cfg(desktop)]
pub(super) use rows::load_states_on;
#[cfg(desktop)]
pub use rows::DbImportCandidateState;
#[cfg(desktop)]
pub(super) use seed_rows::{
    ensure_candidate_state, forget_at_or_below, restart_state_on, CandidateStateSeed,
};
#[cfg(desktop)]
use session_rows::load_session_on;
#[cfg(desktop)]
use signal_rows::{delete_signals, store_signals};
#[cfg(desktop)]
pub(crate) use standing_rows::ImportClaim;
#[cfg(desktop)]
pub use verdict_rows::{DbCandidateIdentifyResult, NewImportCandidateVerdict, VerdictPick};
// The source releases' tests store a run's releases as a release run does.
#[cfg(test)]
pub(super) use run_rows::store_release_run_on;
#[cfg(desktop)]
pub(super) use standing_rows::{
    change_identification_on, end_file_work_unless_of_on, load_standing_on, move_import_on,
    RuntimeFactColumns, RUNTIME_FACT_COLUMNS,
};

#[cfg(desktop)]
use crate::folder_scanner::StoredCandidateEdits;
#[cfg(desktop)]
use crate::CandidateKey;
#[cfg(desktop)]
use bae_catalogs::catalog::LookupCatalog;
#[cfg(desktop)]
use bae_catalogs::records::LookupRef;
#[cfg(desktop)]
use rows::load_states_rows_on;
#[cfg(desktop)]
use std::str::FromStr;
#[cfg(desktop)]
use verdict_rows::{delete_verdict, store_verdict};

#[cfg(desktop)]
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
}

/// A stored release a pick names; a catalog bae does not fetch from names
/// none.
#[cfg(desktop)]
fn lookup_reference(catalog: String, key: String) -> Result<LookupRef, DbError> {
    Ok(LookupRef::new(LookupCatalog::from_str(&catalog).map_err(DbError::Message)?, key))
}

#[cfg(test)]
#[path = "import_state_tests.rs"]
pub(crate) mod tests;
