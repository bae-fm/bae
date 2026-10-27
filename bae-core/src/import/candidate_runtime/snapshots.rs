//! Each key's [`CandidateRuntimeSnapshot`]: where the standings table says it
//! stands, with what the runtime holds beside it — the state the run the
//! table names last published, how far its import has got, its search — and
//! a library release's re-identify run, which only the runtime holds.

use super::change_log::ChangeLog;
use super::{CandidateRuntime, CandidateRuntimeState, IdentificationStage, StoredStanding};
use crate::import::candidates::{
    CandidateRuntimeSnapshot, IdentificationRuntime, ImportInFlight, SavingAnswer,
};
use crate::import::triage::ImportStanding;
use crate::library::LibraryError;
use std::collections::HashMap;

/// `key`'s snapshot from its stored standing and what the runtime holds for
/// it; `None` when neither has anything.
fn compose(
    key: &str,
    stored: Option<&StoredStanding>,
    held: Option<&CandidateRuntimeState>,
) -> Option<CandidateRuntimeSnapshot> {
    let identification = match stored.and_then(|stored| stored.identification.as_ref()) {
        Some(identification) => stored_identification(key, &identification.stage, held),
        None => release_run(held),
    };
    let import = stored
        .and_then(|stored| stored.import.as_ref())
        .map(|import| {
            held.and_then(|held| held.import.as_ref())
                .filter(|report| {
                    report.import_id == import.import_id
                        && ImportStanding::of_step(report.in_flight.step) == import.standing
                })
                .map(|report| report.in_flight.clone())
                // A claim no report has moved yet, or a report of the step
                // before the table's: the standing's first step.
                .unwrap_or(ImportInFlight {
                    progress_percent: None,
                    step: import.standing.first_step(),
                })
        });
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
    key: &str,
    stage: &IdentificationStage,
    held: Option<&CandidateRuntimeState>,
) -> Option<IdentificationRuntime> {
    let published = |run| {
        let state = held
            .and_then(|held| held.runs.get(&run))
            .map(|published| published.state.clone());
        if state.is_none() {
            // A run records what it published before the table names it,
            // and forgets it only once the table no longer does.
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
/// latest answer. A folder candidate's runs are drawn only through the table.
fn release_run(held: Option<&CandidateRuntimeState>) -> Option<IdentificationRuntime> {
    let latest_first: Vec<_> = held?
        .runs
        .values()
        .rev()
        .filter(|published| !published.of_candidate)
        .collect();
    if let Some(running) = latest_first
        .iter()
        .find(|published| !published.state.is_terminal())
    {
        return Some(IdentificationRuntime::Running(running.state.clone()));
    }
    latest_first.first().map(|answered| {
        IdentificationRuntime::Saving(SavingAnswer {
            state: answered.state.clone(),
            import_asked: false,
        })
    })
}

impl CandidateRuntime {
    /// Every key's snapshot from `stored`, with what the runtime holds now.
    fn compose_all(
        &self,
        stored: &HashMap<String, StoredStanding>,
    ) -> HashMap<String, CandidateRuntimeSnapshot> {
        let inner = self.inner.lock().unwrap();
        let keys: std::collections::HashSet<&String> =
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
        key: &str,
    ) -> Result<Option<CandidateRuntimeSnapshot>, LibraryError> {
        let stored = self.database.load_candidate_standing(key).await?;
        let inner = self.inner.lock().unwrap();
        Ok(compose(key, Some(&stored), inner.runtime.get(key)))
    }

    /// Every key with something in flight right now, read from the table and
    /// the runtime.
    pub(crate) async fn snapshots(
        &self,
    ) -> Result<HashMap<String, CandidateRuntimeSnapshot>, LibraryError> {
        let stored = self.database.load_candidate_standings().await?;
        Ok(self.compose_all(&stored))
    }

    /// What is in flight for `key` as the live read last read the table, with
    /// what the runtime holds now, for a reader that cannot wait for a read of
    /// its own.
    pub(crate) fn latest_snapshot(&self, key: &str) -> Option<CandidateRuntimeSnapshot> {
        self.view.borrow().snapshots.get(key).cloned()
    }

    /// Every key's snapshot and the identification count as the live read
    /// reads them, now and as they change; [`RuntimeView::changes`] names
    /// the keys each change moved.
    pub(crate) fn watch_view(&self) -> tokio::sync::watch::Receiver<RuntimeView> {
        self.view.subscribe()
    }

    /// Compose `key` again from its standing as the table was last read and
    /// what the runtime holds for it now, and tell the view's readers if that
    /// changed it.
    pub(super) fn recompose(&self, key: &str) {
        self.view.send_if_modified(|view| {
            let inner = self.inner.lock().unwrap();
            let next = compose(key, view.stored.get(key), inner.runtime.get(key));
            view.show(key, next)
        });
    }

    /// Keep the view current: the standings table read live, each key whose
    /// standing a read moved composed again with what the runtime holds. A
    /// change to what the runtime holds composes its own key
    /// ([`Self::recompose`]). Runs for the library's lifetime, until the
    /// query ends with its store.
    pub(crate) async fn keep_view_current(self) {
        let mut query = self.database.subscribe_candidate_standings();
        while let Some(read) = query.next().await {
            match read {
                Ok(read) => {
                    self.view.send_if_modified(|view| {
                        let inner = self.inner.lock().unwrap();
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

/// Every key's snapshot, and the two counts, as the table was last read with
/// what the runtime holds beside it, kept key by key.
#[derive(Debug, Default)]
pub(crate) struct RuntimeView {
    pub(crate) snapshots: HashMap<String, CandidateRuntimeSnapshot>,
    /// How far the batch of identifications in flight has got: ended of
    /// admitted; `(0, 0)` is none.
    pub(crate) identification_progress: (u32, u32),
    /// How many keys an import owns, as the table counts them.
    pub(crate) importing: u32,
    /// Which keys' snapshots changed, so a reader reads only those.
    pub(crate) changes: ChangeLog,
    /// Each key's standing as the table was last read.
    stored: HashMap<String, StoredStanding>,
}

impl RuntimeView {
    /// Take a read of the table: its counts, and each key whose standing it
    /// moved composed again by `compose`. Whether anything a reader draws
    /// changed.
    fn take_read(
        &mut self,
        read: crate::db::StandingsRead,
        compose: impl Fn(&str, Option<&StoredStanding>) -> Option<CandidateRuntimeSnapshot>,
    ) -> bool {
        let mut changed = read.identification_progress != self.identification_progress
            || read.imports_in_flight != self.importing;
        self.identification_progress = read.identification_progress;
        self.importing = read.imports_in_flight;
        let moved: Vec<String> = self
            .stored
            .keys()
            .filter(|key| !read.standings.contains_key(*key))
            .chain(
                read.standings
                    .iter()
                    .filter(|(key, standing)| self.stored.get(*key) != Some(*standing))
                    .map(|(key, _)| key),
            )
            .cloned()
            .collect();
        self.stored = read.standings;
        for key in moved {
            let next = compose(&key, self.stored.get(&key));
            changed |= self.show(&key, next);
        }
        changed
    }

    /// Show `next` as `key`'s snapshot; whether that changed it.
    fn show(&mut self, key: &str, next: Option<CandidateRuntimeSnapshot>) -> bool {
        if self.snapshots.get(key) == next.as_ref() {
            return false;
        }
        match next {
            Some(next) => {
                self.snapshots.insert(key.to_string(), next);
            }
            None => {
                self.snapshots.remove(key);
            }
        }
        self.changes.record(key);
        true
    }
}

impl ImportStanding {
    /// Where an import at `step` stands.
    pub(crate) fn of_step(step: crate::import::ImportStep) -> Self {
        use crate::import::{ImportPhase, ImportStep, PrepareStep};
        match step {
            ImportStep::Preparing(PrepareStep::Queued) => Self::Queued,
            ImportStep::Preparing(
                PrepareStep::ValidatingSourceFiles | PrepareStep::FetchingArtistPictures,
            )
            | ImportStep::Running(ImportPhase::ReadingFiles | ImportPhase::MeasuringLoudness) => {
                Self::Running
            }
            ImportStep::Running(ImportPhase::Finalizing) => Self::Writing,
        }
    }

    /// The step an import reaching this standing starts it with.
    pub(crate) fn first_step(self) -> crate::import::ImportStep {
        use crate::import::{ImportPhase, ImportStep, PrepareStep};
        match self {
            Self::Queued => ImportStep::Preparing(PrepareStep::Queued),
            Self::Running => ImportStep::Preparing(PrepareStep::ValidatingSourceFiles),
            Self::Writing => ImportStep::Running(ImportPhase::Finalizing),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::candidate_runtime::StoredImport;
    use std::cell::RefCell;

    fn importing(import_id: &str) -> StoredStanding {
        StoredStanding {
            identification: None,
            import: Some(StoredImport {
                import_id: import_id.to_string(),
                standing: ImportStanding::Running,
            }),
        }
    }

    fn read(
        standings: Vec<(&str, StoredStanding)>,
        imports_in_flight: u32,
    ) -> crate::db::StandingsRead {
        crate::db::StandingsRead {
            standings: standings
                .into_iter()
                .map(|(key, standing)| (key.to_string(), standing))
                .collect(),
            identification_progress: (0, 0),
            imports_in_flight,
        }
    }

    /// A read of the table composes only the keys whose standing it moved,
    /// names only those as changed, and takes the import count it read.
    #[test]
    fn a_read_composes_only_the_keys_it_moved() {
        let composed = RefCell::new(Vec::new());
        let compose_one = |key: &str, stored: Option<&StoredStanding>| {
            composed.borrow_mut().push(key.to_string());
            compose(key, stored, None)
        };
        let mut view = RuntimeView::default();

        assert!(view.take_read(
            read(
                vec![("a", importing("imp-a")), ("b", importing("imp-b"))],
                2
            ),
            compose_one
        ));
        assert_eq!(view.importing, 2);
        let seen = view.changes.sequence();
        composed.borrow_mut().clear();

        assert!(view.take_read(read(vec![("a", importing("imp-a"))], 1), compose_one));
        assert_eq!(*composed.borrow(), ["b"], "only the key the read moved");
        assert_eq!(view.changes.since(seen).collect::<Vec<_>>(), ["b"]);
        assert!(!view.snapshots.contains_key("b"));
        assert_eq!(view.importing, 1);

        composed.borrow_mut().clear();
        assert!(!view.take_read(read(vec![("a", importing("imp-a"))], 1), compose_one));
        assert!(
            composed.borrow().is_empty(),
            "a read that moves nothing composes nothing"
        );
    }
}
