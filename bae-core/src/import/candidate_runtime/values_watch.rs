//! The values the import sidebar draws off the runtime: each candidate's
//! extracted signals and the two counts, told as they stand and then as they
//! change.

use super::{CandidateRuntime, Revisions, RuntimeView};
use crate::signals::Signals;
use tokio::sync::watch;

/// One value that changed.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum RuntimeValue {
    Signals { key: String, signals: Signals },
    /// How many of the running batch of identifications have ended, of how
    /// many; `(0, 0)` is none running.
    IdentificationProgress { identified: u32, total: u32 },
    /// How many imports are waiting for the worker or running.
    ImportsInFlight { count: u32 },
}

pub(crate) struct RuntimeValuesWatch {
    revisions: watch::Receiver<Revisions>,
    /// How many keys an import owns, and the identification count.
    view: watch::Receiver<RuntimeView>,
    progress: (u32, u32),
    importing: u32,
    /// How far through the signals' changes the reader has been told.
    signals_seen: u64,
    runtime: CandidateRuntime,
}

impl RuntimeValuesWatch {
    /// The values, told first as they stand and then as they change.
    pub(crate) fn of(runtime: &CandidateRuntime) -> Self {
        let mut revisions = runtime.watch_revisions();
        revisions.mark_changed();
        let mut view = runtime.watch_view();
        view.mark_changed();
        Self {
            revisions,
            view,
            progress: (0, 0),
            importing: 0,
            signals_seen: 0,
            runtime: runtime.clone(),
        }
    }

    /// Wait for values to change, and name each. `None` once the runtime is
    /// gone.
    pub(crate) async fn next(&mut self) -> Option<Vec<RuntimeValue>> {
        loop {
            let mut changed = Vec::new();
            tokio::select! {
                read = self.revisions.changed() => {
                    read.ok()?;
                    self.revisions.borrow_and_update();
                    let (seen, moved) = self.runtime.signals_since(self.signals_seen);
                    self.signals_seen = seen;
                    changed.extend(
                        moved
                            .into_iter()
                            .map(|(key, signals)| RuntimeValue::Signals { key, signals }),
                    );
                }
                read = self.view.changed() => {
                    read.ok()?;
                    let (progress, importing) = {
                        let view = self.view.borrow_and_update();
                        (view.identification_progress, view.importing)
                    };
                    if progress != self.progress {
                        self.progress = progress;
                        changed.push(RuntimeValue::IdentificationProgress {
                            identified: self.progress.0,
                            total: self.progress.1,
                        });
                    }
                    if importing != self.importing {
                        self.importing = importing;
                        changed.push(RuntimeValue::ImportsInFlight { count: importing });
                    }
                }
            }
            if !changed.is_empty() {
                return Some(changed);
            }
        }
    }
}
