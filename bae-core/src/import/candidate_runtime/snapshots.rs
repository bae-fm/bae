//! Each key's [`CandidateRuntimeSnapshot`]: where the standings table says it
//! stands — the step its import is at among it — with the state the run the
//! table names last published, a library release's re-identify run, and its
//! typed search, each read from its own table.

use super::change_log::ChangeLog;
use super::{CandidateRuntime, IdentificationStage, StoredRuntime};
use crate::import::candidates::{
    CandidateRuntimeSnapshot, IdentificationRuntime, ImportInFlight, SavingAnswer,
};
use crate::import::CandidateKey;
use crate::library::LibraryError;
use std::collections::HashMap;

/// `key`'s snapshot from its stored runtime; `None` when nothing is in
/// flight for it.
fn compose(key: &CandidateKey, stored: &StoredRuntime) -> Option<CandidateRuntimeSnapshot> {
    let identification = match stored.standing.identification.as_ref() {
        Some(identification) => stored_identification(key, &identification.stage, stored),
        None => release_run(stored),
    };
    let import = stored
        .standing
        .import
        .as_ref()
        .map(|import| ImportInFlight { step: import.step });
    let search = stored.search.clone();
    if identification.is_none() && import.is_none() && search.is_none() {
        return None;
    }
    Some(CandidateRuntimeSnapshot {
        identification,
        import,
        search,
    })
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
            IdentificationRuntime::Saving(SavingAnswer {
                state,
                import_asked: false,
            })
        }),
        IdentificationStage::ReadingPick { run, import_asked } => published(*run).map(|state| {
            IdentificationRuntime::Saving(SavingAnswer {
                state,
                import_asked: *import_asked,
            })
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
    if let Some(running) = latest_first
        .iter()
        .find(|published| !published.outcome.is_answered())
    {
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
    ) -> Result<Option<CandidateRuntimeSnapshot>, LibraryError> {
        let stored = self.database.load_candidate_runtime(key).await?;
        Ok(compose(key, &stored))
    }

    /// Every key with something in flight right now, read from the tables.
    pub(crate) async fn snapshots(
        &self,
    ) -> Result<HashMap<CandidateKey, CandidateRuntimeSnapshot>, LibraryError> {
        let stored = self.database.load_candidate_runtimes().await?;
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

    /// Every key's snapshot as the live read reads it, now and as it
    /// changes; [`RuntimeView::changes`] names the keys each change moved.
    pub(crate) fn watch_view(&self) -> tokio::sync::watch::Receiver<RuntimeView> {
        self.view.subscribe()
    }

    /// Each key's import percent, now and as it moves.
    pub fn watch_import_percents(
        &self,
    ) -> tokio::sync::watch::Receiver<HashMap<CandidateKey, u32>> {
        self.percents.subscribe()
    }

    /// Keep the view current: the tables read live, each key a read moved
    /// composed again. Runs for the library's lifetime, until the query ends
    /// with its store.
    pub(crate) async fn keep_view_current(self) {
        let mut query = self.database.subscribe_candidate_runtimes();
        while let Some(read) = query.next().await {
            match read {
                Ok(read) => {
                    self.view.send_if_modified(|view| view.take_read(read));
                }
                // The stream has no reader to hand the failure to: a bug,
                // reported as one. The next read that succeeds is delivered.
                Err(error) => self
                    .diagnostics
                    .report_bug("read where every import candidate stands", &error),
            }
        }
    }
}

/// Every key's snapshot, as the tables were last read, kept key by key.
#[derive(Debug, Default)]
pub(crate) struct RuntimeView {
    pub(crate) snapshots: HashMap<CandidateKey, CandidateRuntimeSnapshot>,
    /// Which keys' snapshots changed, so a reader reads only those.
    pub(crate) changes: ChangeLog,
    /// Each key's standing, runs and search as the tables were last read.
    stored: HashMap<CandidateKey, StoredRuntime>,
}

impl RuntimeView {
    /// Take a read of the tables: each key whose rows it moved composed
    /// again. Whether anything a reader draws changed.
    fn take_read(&mut self, standings: HashMap<CandidateKey, StoredRuntime>) -> bool {
        let mut changed = false;
        let moved: Vec<CandidateKey> = self
            .stored
            .keys()
            .filter(|key| !standings.contains_key(*key))
            .chain(
                standings
                    .iter()
                    .filter(|(key, standing)| self.stored.get(*key) != Some(*standing))
                    .map(|(key, _)| key),
            )
            .cloned()
            .collect();
        self.stored = standings;
        for key in moved {
            let next = self
                .stored
                .get(&key)
                .and_then(|stored| compose(&key, stored));
            changed |= self.show(&key, next);
        }
        changed
    }

    /// Show `next` as `key`'s snapshot; whether that changed it.
    fn show(&mut self, key: &CandidateKey, next: Option<CandidateRuntimeSnapshot>) -> bool {
        if self.snapshots.get(key) == next.as_ref() {
            return false;
        }
        match next {
            Some(next) => {
                self.snapshots.insert(key.clone(), next);
            }
            None => {
                self.snapshots.remove(key);
            }
        }
        self.changes.record(key);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::candidate_runtime::{StoredImport, StoredStanding};

    fn importing(import_id: &str) -> StoredRuntime {
        StoredRuntime {
            standing: StoredStanding {
                identification: None,
                import: Some(StoredImport {
                    import_id: import_id.to_string(),
                    step: crate::import::ImportStep::Preparing(
                        crate::import::PrepareStep::ValidatingSourceFiles,
                    ),
                }),
            },
            runs: Default::default(),
            search: None,
        }
    }

    fn read(standings: Vec<(&str, StoredRuntime)>) -> HashMap<CandidateKey, StoredRuntime> {
        standings
            .into_iter()
            .map(|(key, standing)| (crate::import::CandidateKey::test(key), standing))
            .collect()
    }

    /// A read of the tables names only the keys whose rows it moved as
    /// changed.
    #[test]
    fn a_read_names_only_the_keys_it_moved() {
        let mut view = RuntimeView::default();

        assert!(view.take_read(read(vec![
            ("a", importing("imp-a")),
            ("b", importing("imp-b"))
        ])));
        let seen = view.changes.sequence();

        assert!(view.take_read(read(vec![("a", importing("imp-a"))])));
        let b = crate::import::CandidateKey::test("b");
        assert_eq!(view.changes.since(seen).collect::<Vec<_>>(), [&b]);
        assert!(!view.snapshots.contains_key(&b));

        let seen = view.changes.sequence();
        assert!(!view.take_read(read(vec![("a", importing("imp-a"))])));
        assert_eq!(
            view.changes.since(seen).count(),
            0,
            "a read that moves nothing names nothing"
        );
    }
}
