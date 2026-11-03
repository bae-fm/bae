//! Each key's [`CandidateRuntimeSnapshot`]: where the standings table says it
//! stands — the step its import is at among it — with the state the run the
//! table names last published, a library release's re-identify run, and its
//! typed search, each read from its own table.

use super::{CandidateRuntime, IdentificationStage, StoredRuntime};
use crate::candidates::{
    CandidateRuntimeSnapshot, IdentificationRuntime, ImportInFlight, SavingAnswer,
};
use crate::CandidateKey;
use std::collections::HashMap;

/// `key`'s snapshot from its stored runtime; `None` when nothing is in
/// flight for it.
pub(crate) fn compose(
    key: &CandidateKey,
    stored: &StoredRuntime,
) -> Option<CandidateRuntimeSnapshot> {
    let identification = match stored.standing.identification.as_ref() {
        Some(identification) => stored_identification(key, &identification.stage, stored),
        None => release_run(stored),
    };
    let import = stored.standing.import.as_ref().map(|import| ImportInFlight { step: import.step });
    let search = stored.search.clone();
    if identification.is_none() && import.is_none() && search.is_none() {
        return None;
    }
    Some(CandidateRuntimeSnapshot { identification, import, search })
}

/// A folder candidate's identification as the table holds it, with the
/// state its run last published.
fn stored_identification(
    key: &CandidateKey,
    stage: &IdentificationStage,
    stored: &StoredRuntime,
) -> Option<IdentificationRuntime> {
    let published = |run| {
        let state = stored.runs.get(&run).cloned();
        if state.is_none() {
            // A run's first state is stored in the write that starts it, and
            // goes in the write that moves the standing off it.
            tracing::error!(
                "candidate runtime: {key}'s run {run:?} is stored with nothing it published"
            );
        }
        state
    };
    match stage {
        IdentificationStage::Queued { admission } => {
            Some(IdentificationRuntime::Queued(*admission))
        }
        IdentificationStage::Running { run } => published(*run).map(IdentificationRuntime::Running),
        IdentificationStage::Finalizing { run } => published(*run).map(|state| {
            IdentificationRuntime::Saving(SavingAnswer { state, import_asked: false })
        }),
        IdentificationStage::ReadingPick { run, import_asked } => published(*run).map(|state| {
            IdentificationRuntime::Saving(SavingAnswer { state, import_asked: *import_asked })
        }),
        IdentificationStage::Failed { failure } => {
            Some(IdentificationRuntime::SaveFailed(failure.clone()))
        }
    }
}

/// A library release's re-identify run: its latest run in flight, else its
/// latest answer. A key with no identification standing has rows only of a
/// release's runs: a folder candidate's run has its row only while the
/// standing names it.
fn release_run(stored: &StoredRuntime) -> Option<IdentificationRuntime> {
    let latest_first: Vec<_> = stored.runs.values().rev().collect();
    if let Some(running) = latest_first.iter().find(|published| !published.outcome.is_answered()) {
        return Some(IdentificationRuntime::Running((*running).clone()));
    }
    latest_first.first().map(|answered| {
        IdentificationRuntime::Saving(SavingAnswer {
            state: (*answered).clone(),
            import_asked: false,
        })
    })
}

impl CandidateRuntime {
    /// What is in flight for `key` right now, read from the tables; `None`
    /// when nothing is.
    pub(crate) async fn snapshot(
        &self,
        key: &CandidateKey,
    ) -> Result<Option<CandidateRuntimeSnapshot>, crate::ImportError> {
        let stored = self.tables.load_candidate_runtime(key).await?;
        Ok(compose(key, &stored))
    }

    /// Every key with something in flight right now, read from the tables.
    pub(crate) async fn snapshots(
        &self,
    ) -> Result<HashMap<CandidateKey, CandidateRuntimeSnapshot>, crate::ImportError> {
        let stored = self.tables.load_candidate_runtimes().await?;
        Ok(stored
            .iter()
            .filter_map(|(key, stored)| {
                compose(key, stored).map(|snapshot| (key.clone(), snapshot))
            })
            .collect())
    }

    /// Each running import's percent, by key, as its import last reported.
    pub(crate) fn percents(&self) -> HashMap<CandidateKey, u32> {
        self.percents.borrow().clone()
    }

    /// Each key's import percent, now and as it moves.
    pub fn watch_import_percents(&self) -> ImportPercentsWatch {
        ImportPercentsWatch(self.percents.subscribe())
    }
}

/// Each key's import percent, now and as it moves, from the candidate
/// runtime that made it.
pub struct ImportPercentsWatch(tokio::sync::watch::Receiver<HashMap<CandidateKey, u32>>);

impl ImportPercentsWatch {
    /// Each running import's percent now, by key; the value seen from here
    /// on.
    pub fn percents(&mut self) -> HashMap<CandidateKey, u32> {
        self.0.borrow_and_update().clone()
    }

    /// Wait until the percents differ from the ones last seen; an error once
    /// the runtime is gone.
    pub async fn changed(&mut self) -> Result<(), tokio::sync::watch::error::RecvError> {
        self.0.changed().await
    }

    /// Wait until the percents satisfy `done`; an error once the runtime is
    /// gone.
    #[cfg(test)]
    pub(crate) async fn wait_for(
        &mut self,
        done: impl FnMut(&HashMap<CandidateKey, u32>) -> bool,
    ) -> Result<(), tokio::sync::watch::error::RecvError> {
        self.0.wait_for(done).await.map(drop)
    }
}

#[cfg(test)]
#[path = "snapshots_tests.rs"]
mod tests;
