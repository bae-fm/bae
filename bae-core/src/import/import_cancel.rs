//! Cancelling an import a person started, while it waits for the worker or
//! while it runs.
//!
//! An import writes nothing durable until its last step, which writes the
//! whole release in one transaction — and with it clears the failure an
//! earlier attempt left. So everything before that step can be dropped and
//! leaves the library as it was — and that step, once begun, is not
//! interrupted: a cancel that arrives then is refused and the import
//! completes. The registry is where the two sides agree which of those an
//! import is in, under one lock, so a cancel and the start of the write can
//! never both win.
//!
//! Dropping the work is not quite the end of it. Tracks being measured decode
//! on blocking threads that nothing aborts; the streams they read stop when
//! the dropped work lets go of them, so each ends at its next read, and what
//! one reports on its way out names an import that no longer holds the
//! candidate, which the candidate's runtime does not record. The next import
//! starts from the queue.

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

/// How an import's work stopped short of its end.
#[derive(Debug)]
pub(crate) enum ImportStop {
    /// It was cancelled before it began writing its release.
    Cancelled,
    Failed(crate::import::ImportError),
}

impl<E> From<E> for ImportStop
where
    crate::import::ImportError: From<E>,
{
    fn from(error: E) -> Self {
        Self::Failed(error.into())
    }
}

/// How a run the worker took up ended.
pub(crate) enum ImportRunEnd<T> {
    /// The work ran to its end or failed.
    Ran(Result<T, crate::import::ImportError>),
    /// It was cancelled while it waited; whoever cancelled it said so.
    CancelledWaiting,
    /// It was cancelled while it ran, before it wrote anything.
    CancelledRunning,
}

/// Every import between its claim and its end, by candidate key. One per
/// candidate: a candidate is claimed by one import at a time.
///
/// Each entry is its import's own, by import id. An import cancelled while it
/// waited is gone from here at once, and the candidate can be imported again
/// before the worker reaches the command it left in the queue; that command
/// must find nothing of its own and be skipped, not take up the entry of the
/// import that followed it.
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

    /// The import `import_id` of `candidate_key` never reached the worker.
    pub(crate) fn forget(&self, candidate_key: &str, import_id: &str) {
        let mut entries = self.entries.lock().unwrap();
        if entries
            .get(candidate_key)
            .is_some_and(|entry| entry.import_id == import_id)
        {
            entries.remove(candidate_key);
        }
    }

    /// Run the worker's `work` for the import `import_id` of `candidate_key`:
    /// skipped when it was cancelled while it waited, and dropped when it is
    /// cancelled before it begins writing. Its entry ends with it.
    pub(crate) async fn run<T>(
        &self,
        candidate_key: &str,
        import_id: &str,
        work: impl std::future::Future<Output = Result<T, ImportStop>>,
    ) -> ImportRunEnd<T> {
        let token = {
            let mut entries = self.entries.lock().unwrap();
            match entries.get_mut(candidate_key) {
                Some(entry) if entry.import_id == import_id && !entry.token.is_cancelled() => {
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
            result = work => match result {
                Ok(value) => ImportRunEnd::Ran(Ok(value)),
                Err(ImportStop::Failed(error)) => ImportRunEnd::Ran(Err(error)),
                Err(ImportStop::Cancelled) => ImportRunEnd::CancelledRunning,
            },
        };
        self.forget(candidate_key, import_id);
        end
    }

    /// The import `import_id` is about to write its release. Refused when it
    /// was cancelled first; after this, a cancel is.
    pub(crate) fn begin_writing(
        &self,
        candidate_key: &str,
        import_id: &str,
    ) -> Result<(), ImportStop> {
        let mut entries = self.entries.lock().unwrap();
        match entries.get_mut(candidate_key) {
            Some(entry) if entry.import_id == import_id && !entry.token.is_cancelled() => {
                entry.stage = Stage::Writing;
                Ok(())
            }
            _ => Err(ImportStop::Cancelled),
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
            .run("Album", "import-1", async {
                panic!("a cancelled import never runs")
            })
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
                    .run("Album", "import-1", async {
                        started_tx.send(()).unwrap();
                        std::future::pending::<Result<(), ImportStop>>().await
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
            .run("Album", "import-1", async move {
                writing.begin_writing("Album", "import-1")?;
                Ok(writing.cancel("Album"))
            })
            .await;

        assert!(matches!(end, ImportRunEnd::Ran(Ok(CancelOutcome::Writing))));
    }

    #[test]
    fn a_cancelled_import_cannot_begin_writing() {
        let cancels = ImportCancels::default();
        cancels.register("Album", "import-1");
        cancels.entries.lock().unwrap().get_mut("Album").unwrap().stage = Stage::Running;

        assert_eq!(cancels.cancel("Album"), CancelOutcome::CancelledRunning);
        assert!(matches!(
            cancels.begin_writing("Album", "import-1"),
            Err(ImportStop::Cancelled)
        ));
    }

    /// A cancel that lands between the work's last await and its write is
    /// refused at the write, and the run ends cancelled, not failed.
    #[tokio::test]
    async fn a_run_refused_its_write_ends_cancelled() {
        let cancels = ImportCancels::default();
        cancels.register("Album", "import-1");
        let writing = cancels.clone();
        let end = cancels
            .run("Album", "import-1", async move {
                writing.cancel("Album");
                writing.begin_writing("Album", "import-1")
            })
            .await;

        assert!(matches!(end, ImportRunEnd::CancelledRunning));
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
        assert!(matches!(
            cancels.begin_writing("Album 2", "import-2"),
            Err(ImportStop::Cancelled)
        ));
    }

    /// A command cancelled while it waited finds the entry of the import that
    /// followed it and leaves it alone: it is skipped, and the import that
    /// holds the candidate now runs when the worker reaches it.
    #[tokio::test]
    async fn a_command_cancelled_while_waiting_leaves_the_next_import_alone() {
        let cancels = ImportCancels::default();
        cancels.register("Album", "import-1");
        cancels.cancel("Album");
        cancels.register("Album", "import-2");

        let end = cancels
            .run("Album", "import-1", async {
                panic!("the cancelled command never runs")
            })
            .await;
        assert!(matches!(end, ImportRunEnd::<()>::CancelledWaiting));
        assert!(matches!(
            cancels.begin_writing("Album", "import-1"),
            Err(ImportStop::Cancelled)
        ));
        cancels.forget("Album", "import-1");

        let end = cancels.run("Album", "import-2", async { Ok("ran") }).await;
        assert!(matches!(end, ImportRunEnd::Ran(Ok("ran"))));
    }
}
