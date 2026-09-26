//! Cancelling an import a person started, while it waits for the worker or
//! while it runs.
//!
//! An import writes nothing durable until its last step, which writes the
//! whole release in one transaction. So everything before that step can be
//! dropped and leaves the library as it was — and that step, once begun, is
//! not interrupted: a cancel that arrives then is refused and the import
//! completes. The registry is where the two sides agree which of those an
//! import is in, under one lock, so a cancel and the start of the write can
//! never both win.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

/// Where one import stands, as far as cancelling it goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    /// Claimed and sent to the worker, which has not reached it.
    Waiting,
    /// The worker is preparing and reading it; nothing is written yet.
    Running,
    /// Its release is being written. It can no longer be cancelled.
    Writing,
}

struct Entry {
    import_id: String,
    stage: Stage,
    token: CancellationToken,
}

/// What asking to cancel one import did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CancelOutcome {
    /// Nothing is importing the candidate.
    NotImporting,
    /// It was waiting for the worker and now never starts. Whoever cancelled
    /// it says so, since no worker will.
    CancelledWaiting { import_id: String },
    /// It was running; the worker drops it and says so.
    CancelledRunning,
    /// Its release is already being written, and the write completes.
    Writing,
}

/// How a run the worker took up ended.
pub(crate) enum ImportRunEnd<T> {
    /// The work ran to its end, whatever it returned.
    Ran(T),
    /// It was cancelled while it waited; whoever cancelled it said so.
    CancelledWaiting,
    /// It was cancelled while it ran, and dropped before it wrote anything.
    CancelledRunning,
}

/// Every import between its claim and its end, by candidate key. One per
/// candidate: a candidate is claimed by one import at a time.
#[derive(Clone, Default)]
pub(crate) struct ImportCancels {
    entries: Arc<Mutex<HashMap<String, Entry>>>,
    /// Holds every run after it starts, until a test lets it through — so a
    /// test cancels an import that is genuinely running.
    #[cfg(test)]
    running_gate: Arc<Mutex<Option<Arc<tokio::sync::Semaphore>>>>,
}

impl ImportCancels {
    /// An import of `candidate_key` was claimed and is on its way to the
    /// worker.
    pub(crate) fn register(&self, candidate_key: &str, import_id: &str) {
        self.entries.lock().unwrap().insert(
            candidate_key.to_string(),
            Entry {
                import_id: import_id.to_string(),
                stage: Stage::Waiting,
                token: CancellationToken::new(),
            },
        );
    }

    /// The import never reached the worker.
    pub(crate) fn forget(&self, candidate_key: &str) {
        self.entries.lock().unwrap().remove(candidate_key);
    }

    /// Run the worker's `work` for `candidate_key`'s import: skipped when it
    /// was cancelled while it waited, and dropped when it is cancelled before
    /// it begins writing. Its entry ends with it.
    pub(crate) async fn run<T>(
        &self,
        candidate_key: &str,
        work: impl std::future::Future<Output = T>,
    ) -> ImportRunEnd<T> {
        let token = {
            let mut entries = self.entries.lock().unwrap();
            match entries.get_mut(candidate_key) {
                Some(entry) if !entry.token.is_cancelled() => {
                    entry.stage = Stage::Running;
                    entry.token.clone()
                }
                _ => return ImportRunEnd::CancelledWaiting,
            }
        };
        #[cfg(test)]
        let work = {
            let gate = self.running_gate.lock().unwrap().clone();
            async move {
                if let Some(gate) = gate {
                    let _ = gate.acquire().await;
                }
                work.await
            }
        };
        let end = tokio::select! {
            biased;
            _ = token.cancelled() => ImportRunEnd::CancelledRunning,
            result = work => ImportRunEnd::Ran(result),
        };
        self.forget(candidate_key);
        end
    }

    /// The import is about to write its release. Refused when it was
    /// cancelled first; after this, a cancel is.
    pub(crate) fn begin_writing(&self, candidate_key: &str) -> Result<(), crate::import::ImportError> {
        let mut entries = self.entries.lock().unwrap();
        match entries.get_mut(candidate_key) {
            Some(entry) if !entry.token.is_cancelled() => {
                entry.stage = Stage::Writing;
                Ok(())
            }
            _ => Err(crate::import::ImportError::ImportCancelled),
        }
    }

    pub(crate) fn cancel(&self, candidate_key: &str) -> CancelOutcome {
        let mut entries = self.entries.lock().unwrap();
        let Some(entry) = entries.get(candidate_key) else {
            return CancelOutcome::NotImporting;
        };
        match entry.stage {
            Stage::Writing => CancelOutcome::Writing,
            Stage::Running => {
                entry.token.cancel();
                CancelOutcome::CancelledRunning
            }
            Stage::Waiting => {
                entry.token.cancel();
                let entry = entries
                    .remove(candidate_key)
                    .expect("the entry was just read");
                CancelOutcome::CancelledWaiting {
                    import_id: entry.import_id,
                }
            }
        }
    }

    /// Cancel every import that is not already writing, and report the
    /// waiting ones by candidate key and import id — whoever cancelled them
    /// says they ended.
    pub(crate) fn cancel_all(&self) -> Vec<(String, String)> {
        let keys: Vec<String> = self.entries.lock().unwrap().keys().cloned().collect();
        keys.into_iter()
            .filter_map(|key| match self.cancel(&key) {
                CancelOutcome::CancelledWaiting { import_id } => Some((key, import_id)),
                CancelOutcome::NotImporting
                | CancelOutcome::CancelledRunning
                | CancelOutcome::Writing => None,
            })
            .collect()
    }

    /// Hold every run that starts from now on until [`Self::release_runs`].
    #[cfg(test)]
    pub(crate) fn hold_runs(&self) {
        *self.running_gate.lock().unwrap() = Some(Arc::new(tokio::sync::Semaphore::new(0)));
    }

    #[cfg(test)]
    pub(crate) fn release_runs(&self) {
        if let Some(gate) = self.running_gate.lock().unwrap().take() {
            gate.close();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_waiting_import_cancelled_never_runs() {
        let cancels = ImportCancels::default();
        cancels.register("Album", "import-1");

        assert_eq!(
            cancels.cancel("Album"),
            CancelOutcome::CancelledWaiting {
                import_id: "import-1".to_string()
            }
        );
        let end = cancels
            .run("Album", async { panic!("a cancelled import never runs") })
            .await;
        assert!(matches!(end, ImportRunEnd::<()>::CancelledWaiting));
        assert_eq!(cancels.cancel("Album"), CancelOutcome::NotImporting);
    }

    #[tokio::test]
    async fn a_running_import_cancelled_is_dropped_before_it_writes() {
        let cancels = ImportCancels::default();
        cancels.register("Album", "import-1");
        let (started_tx, started) = tokio::sync::oneshot::channel();
        let run = {
            let cancels = cancels.clone();
            tokio::spawn(async move {
                cancels
                    .run("Album", async {
                        started_tx.send(()).unwrap();
                        std::future::pending::<()>().await
                    })
                    .await
            })
        };
        started.await.unwrap();

        assert_eq!(cancels.cancel("Album"), CancelOutcome::CancelledRunning);
        assert!(matches!(
            run.await.unwrap(),
            ImportRunEnd::CancelledRunning
        ));
        assert_eq!(cancels.cancel("Album"), CancelOutcome::NotImporting);
    }

    #[tokio::test]
    async fn an_import_writing_its_release_is_not_cancelled() {
        let cancels = ImportCancels::default();
        cancels.register("Album", "import-1");
        let writing = cancels.clone();
        let end = cancels
            .run("Album", async move {
                writing.begin_writing("Album").unwrap();
                writing.cancel("Album")
            })
            .await;

        assert!(matches!(end, ImportRunEnd::Ran(CancelOutcome::Writing)));
    }

    #[test]
    fn a_cancelled_import_cannot_begin_writing() {
        let cancels = ImportCancels::default();
        cancels.register("Album", "import-1");
        cancels.entries.lock().unwrap().get_mut("Album").unwrap().stage = Stage::Running;

        assert_eq!(cancels.cancel("Album"), CancelOutcome::CancelledRunning);
        assert!(cancels.begin_writing("Album").is_err());
    }

    #[test]
    fn cancelling_everything_spares_only_what_is_writing() {
        let cancels = ImportCancels::default();
        cancels.register("Album 1", "import-1");
        cancels.register("Album 2", "import-2");
        cancels.register("Album 3", "import-3");
        cancels.entries.lock().unwrap().get_mut("Album 2").unwrap().stage = Stage::Running;
        cancels.entries.lock().unwrap().get_mut("Album 3").unwrap().stage = Stage::Writing;

        assert_eq!(
            cancels.cancel_all(),
            vec![("Album 1".to_string(), "import-1".to_string())]
        );
        assert_eq!(cancels.cancel("Album 3"), CancelOutcome::Writing);
        assert!(cancels.begin_writing("Album 2").is_err());
    }
}
