//! Each key's [`CandidateRuntimeSnapshot`]: where the standings table says it
//! stands — the step its import is at among it — with the state the run the
//! table names last published, a library release's re-identify run, and its
//! typed search, each read from its own table.

use super::stored_standing::{Admission, SaveFailure};
use super::{CandidateRuntime, IdentificationStage, StoredRuntime};
use crate::types::ImportStep;
use crate::CandidateKey;
use bae_identify::PublishedRun;
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

/// What is in flight for one key. An entry exists only while one of its
/// parts is `Some`; all of them `None` is the absence of an entry, not a
/// value.
///
/// Where a folder candidate's identification and import stand is read from
/// the standings table, the only place it is held; the state a run published,
/// a library release's re-identify run included, from the runs table beside
/// it; the search from the search tables.
#[derive(Debug, Clone, PartialEq)]
pub struct CandidateRuntimeSnapshot {
    pub identification: Option<IdentificationRuntime>,
    /// The running import: claimed, preparing, or partway through a phase.
    pub import: Option<ImportInFlight>,
    /// The typed search a person submitted for this candidate, as its sources
    /// land. `None` before one is submitted and after the files it was of
    /// change.
    ///
    /// It is stored rather than held by the pane because its sources land one
    /// at a time and the pane is rebuilt from stored state: a value the pane
    /// owned would be lost on every redraw, and lost outright if the person
    /// looked at another candidate while a provider was still answering. Each
    /// landing is a write to the rows this is read from, so what a surface
    /// draws cannot disagree with what a landing wrote.
    pub search: Option<crate::candidate_search::CandidateSearch>,
}

impl CandidateRuntimeSnapshot {
    /// What identification is doing for the key, as its pane shows it: the
    /// run in flight or the answer being written, else the wait for a run.
    /// `None` when nothing is identifying the key, and its pane shows the
    /// stored verdict; a write that failed reaches the pane through the row.
    pub fn identification_in_flight(&self) -> Option<IdentificationInFlight> {
        match self.identification.as_ref()? {
            IdentificationRuntime::Queued(_) => Some(IdentificationInFlight::Queued),
            IdentificationRuntime::Running(state) => {
                Some(IdentificationInFlight::Run(state.clone()))
            }
            IdentificationRuntime::Saving(answer) => {
                Some(IdentificationInFlight::Run(answer.state.clone()))
            }
            IdentificationRuntime::SaveFailed(_) => None,
        }
    }
}

/// Where one key's identification stands right now.
#[derive(Debug, Clone, PartialEq)]
pub enum IdentificationRuntime {
    /// Planned for this key and not started.
    Queued(Admission),
    /// The latest state the run in flight published — its answer, once it
    /// reaches one, until the standings table says the answer is being
    /// written.
    Running(PublishedRun),
    /// The answer a run reached, while it is being written.
    Saving(SavingAnswer),
    /// Why the last write of an answer did not land. Cleared by the next run
    /// of this key.
    SaveFailed(SaveFailure),
}

/// A run's answer while it is being written, and what was asked of it.
#[derive(Debug, Clone, PartialEq)]
pub struct SavingAnswer {
    /// Always answered.
    pub state: PublishedRun,
    /// A person asked, while the release this answer picks unattended was
    /// being read, for the candidate to be imported once the answer stores
    /// it. Only an answer that picks unattended carries the ask, and it goes
    /// with the answer.
    pub import_asked: bool,
}

/// What identification is doing for a key right now.
#[derive(Debug, Clone, PartialEq)]
pub enum IdentificationInFlight {
    /// On the identification queue, its run not started.
    Queued,
    /// The latest state a run published: in flight, or the answer being
    /// written.
    Run(PublishedRun),
}

/// The step a running import is at. It ends with the import: a finished one
/// leaves the runtime and reads back off the release row it wrote, or the
/// failure row it wrote. How far through the step it is moves many times a
/// second, so it is the key's percent in
/// [`CandidateRuntime::watch_import_percents`](crate::CandidateRuntime::watch_import_percents).
#[derive(Debug, Clone, PartialEq)]
pub struct ImportInFlight {
    pub step: ImportStep,
}

#[cfg(test)]
#[path = "snapshots_tests.rs"]
mod tests;
