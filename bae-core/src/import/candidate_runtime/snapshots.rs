//! Each key's [`CandidateRuntimeSnapshot`]: where the standings table says it
//! stands — the step its import is at among it — with the state the run the
//! table names last published, a library release's re-identify run, and the
//! search the runtime holds beside them.

use super::change_log::ChangeLog;
use super::{CandidateRuntime, CandidateRuntimeState, IdentificationStage, StoredRuntime};
use crate::import::candidates::{
    CandidateRuntimeSnapshot, IdentificationRuntime, ImportInFlight, SavingAnswer,
};
use crate::import::CandidateKey;
use crate::library::LibraryError;
use std::collections::HashMap;

/// `key`'s snapshot from its stored runtime and what the runtime holds for
/// it; `None` when neither has anything.
fn compose(
    key: &CandidateKey,
    stored: Option<&StoredRuntime>,
    held: Option<&CandidateRuntimeState>,
) -> Option<CandidateRuntimeSnapshot> {
    let identification = match stored.and_then(|stored| stored.standing.identification.as_ref()) {
        Some(identification) => stored_identification(key, &identification.stage, stored),
        None => stored.and_then(release_run),
    };
    let import = stored
        .and_then(|stored| stored.standing.import.as_ref())
        .map(|import| ImportInFlight { step: import.step });
    let search = held
        .and_then(|held| held.search.as_ref())
        .map(|running| running.search.clone());
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
    stored: Option<&StoredRuntime>,
) -> Option<IdentificationRuntime> {
    let published = |run| {
        let state = stored.and_then(|stored| stored.runs.get(&run)).cloned();
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
    /// Every key's snapshot from `stored`, with what the runtime holds now.
    fn compose_all(
        &self,
        stored: &HashMap<CandidateKey, StoredRuntime>,
    ) -> HashMap<CandidateKey, CandidateRuntimeSnapshot> {
        let inner = self.held();
        let keys: std::collections::HashSet<&CandidateKey> =
            stored.keys().chain(inner.runtime.keys()).collect();
        keys.into_iter()
            .filter_map(|key| {
                compose(key, stored.get(key), inner.runtime.get(key))
                    .map(|snapshot| (key.clone(), snapshot))
            })
            .collect()
    }

    /// What is in flight for `key` right now, read from the table and the
    /// runtime; `None` when nothing is.
    pub(crate) async fn snapshot(
        &self,
        key: &CandidateKey,
    ) -> Result<Option<CandidateRuntimeSnapshot>, LibraryError> {
        let stored = self.database.load_candidate_runtime(key).await?;
        let inner = self.held();
        Ok(compose(key, Some(&stored), inner.runtime.get(key)))
    }

    /// Every key with something in flight right now, read from the table and
    /// the runtime.
    pub(crate) async fn snapshots(
        &self,
    ) -> Result<HashMap<CandidateKey, CandidateRuntimeSnapshot>, LibraryError> {
        let stored = self.database.load_candidate_runtimes().await?;
        Ok(self.compose_all(&stored))
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

    /// Compose `key` again from its standing as the table was last read and
    /// what the runtime holds for it now, and tell the view's readers if that
    /// changed it.
    pub(super) fn recompose(&self, key: &CandidateKey) {
        self.view.send_if_modified(|view| {
            let inner = self.held();
            let stored = view.stored.get(key);
            view.show(key, compose(key, stored, inner.runtime.get(key)))
        });
    }

    /// Keep the view current: the standings table read live, each key whose
    /// standing a read moved composed again with what the runtime holds. A
    /// change to what the runtime holds composes its own key
    /// ([`Self::recompose`]). Runs for the library's lifetime, until the
    /// query ends with its store.
    pub(crate) async fn keep_view_current(self) {
        let mut query = self.database.subscribe_candidate_runtimes();
        while let Some(read) = query.next().await {
            match read {
                Ok(read) => {
                    self.view.send_if_modified(|view| {
                        let inner = self.held();
                        view.take_read(read, |key, stored| {
                            compose(key, stored, inner.runtime.get(key))
                        })
                    });
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

/// Every key's snapshot, as the table was last read with what the runtime
/// holds beside it, kept key by key.
#[derive(Debug, Default)]
pub(crate) struct RuntimeView {
    pub(crate) snapshots: HashMap<CandidateKey, CandidateRuntimeSnapshot>,
    /// Which keys' snapshots changed, so a reader reads only those.
    pub(crate) changes: ChangeLog,
    /// Each key's standing and runs as the tables were last read.
    stored: HashMap<CandidateKey, StoredRuntime>,
}

impl RuntimeView {
    /// Take a read of the table: each key whose standing it moved composed
    /// again by `compose`. Whether anything a reader draws changed.
    fn take_read(
        &mut self,
        standings: HashMap<CandidateKey, StoredRuntime>,
        compose: impl Fn(&CandidateKey, Option<&StoredRuntime>) -> Option<CandidateRuntimeSnapshot>,
    ) -> bool {
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
            let next = compose(&key, self.stored.get(&key));
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
    use std::cell::RefCell;

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
        }
    }

    fn read(standings: Vec<(&str, StoredRuntime)>) -> HashMap<CandidateKey, StoredRuntime> {
        standings
            .into_iter()
            .map(|(key, standing)| (crate::import::CandidateKey::test(key), standing))
            .collect()
    }

    /// A read of the table composes only the keys whose standing it moved,
    /// and names only those as changed.
    #[test]
    fn a_read_composes_only_the_keys_it_moved() {
        let composed = RefCell::new(Vec::new());
        let compose_one = |key: &CandidateKey, stored: Option<&StoredRuntime>| {
            composed.borrow_mut().push(key.clone());
            compose(key, stored, None)
        };
        let mut view = RuntimeView::default();

        assert!(view.take_read(
            read(vec![("a", importing("imp-a")), ("b", importing("imp-b"))]),
            compose_one
        ));
        let seen = view.changes.sequence();
        composed.borrow_mut().clear();

        assert!(view.take_read(read(vec![("a", importing("imp-a"))]), compose_one));
        let b = crate::import::CandidateKey::test("b");
        assert_eq!(
            *composed.borrow(),
            std::slice::from_ref(&b),
            "only the key the read moved"
        );
        assert_eq!(view.changes.since(seen).collect::<Vec<_>>(), [&b]);
        assert!(!view
            .snapshots
            .contains_key(&crate::import::CandidateKey::test("b")));

        composed.borrow_mut().clear();
        assert!(!view.take_read(read(vec![("a", importing("imp-a"))]), compose_one));
        assert!(
            composed.borrow().is_empty(),
            "a read that moves nothing composes nothing"
        );
    }
}
