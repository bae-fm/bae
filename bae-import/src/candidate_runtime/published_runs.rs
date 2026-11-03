//! What each run last published, stored beside the standings
//! (`import_identify_run`, see [`StoredRuntime`](super::StoredRuntime)), with
//! the signals the run read, so a run's pane and its settle read the same
//! rows.

use super::CandidateRuntime;
use crate::CandidateKey;
use crate::IdentifyRunId;
use bae_db::DbError;
use bae_identify::signals::Signals;
use bae_identify::{IdentifyState, RunOutcome};

impl CandidateRuntime {
    /// The signals `run` of `key` last read, as the run's rows hold them:
    /// what the settle of its answer stores beside its verdict. `None` once
    /// the run's rows are gone.
    pub(crate) async fn run_signals(
        &self,
        key: &CandidateKey,
        run: IdentifyRunId,
    ) -> Result<Option<Signals>, DbError> {
        self.tables.load_candidate_run_signals(key, run).await
    }

    /// The state `run` of the folder candidate `key` published, and the
    /// signals it last read where it read new ones, stored if the key's
    /// standing still names the run; whether they were.
    pub(crate) async fn publish_candidate_run(
        &self,
        key: &CandidateKey,
        run: IdentifyRunId,
        state: &IdentifyState,
        signals: Option<&Signals>,
    ) -> Result<bool, DbError> {
        let outcome = published_outcome(key, run, state)?;
        self.tables.publish_candidate_run(key, run, &outcome, state.read_releases(), signals).await
    }

    /// The state `run` of the library release whose re-identify key is `key`
    /// published, with the signals it last read where it read new ones, or
    /// its end with `Idle`.
    pub(crate) async fn publish_release_run(
        &self,
        key: &CandidateKey,
        run: IdentifyRunId,
        state: &IdentifyState,
        signals: Option<&Signals>,
    ) -> Result<(), DbError> {
        match RunOutcome::of(state) {
            None => self.tables.end_release_run(key, run).await,
            Some(outcome) => {
                self.tables
                    .publish_release_run(key, run, &outcome, state.read_releases(), signals)
                    .await
            }
        }
    }
}

/// How `state`, which `run` of `key` published, stands; a run publishes no
/// `Idle` while it runs.
pub(super) fn published_outcome(
    key: &CandidateKey,
    run: IdentifyRunId,
    state: &IdentifyState,
) -> Result<RunOutcome, DbError> {
    RunOutcome::of(state)
        .ok_or_else(|| DbError::Message(format!("{key}'s run {run:?} published no state to store")))
}

#[cfg(test)]
#[path = "published_runs_tests.rs"]
mod tests;
