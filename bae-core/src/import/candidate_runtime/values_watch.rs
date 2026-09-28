//! The values the import sidebar draws off the runtime: each candidate's
//! extracted signals and the two counts, told as they change.

use super::{CandidateRuntime, Revisions};
use crate::signals::Signals;
use std::collections::HashMap;
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
    read_at: Revisions,
    progress: (u32, u32),
    importing: u32,
    signals: HashMap<String, Signals>,
    runtime: CandidateRuntime,
}

impl RuntimeValuesWatch {
    /// The values as they stand, told only once they change.
    pub(crate) fn of(runtime: &CandidateRuntime) -> Self {
        let revisions = runtime.watch_revisions();
        let read_at = *revisions.borrow();
        Self {
            revisions,
            read_at,
            progress: runtime.identification_progress(),
            importing: runtime.imports_in_flight(),
            signals: runtime.all_signals(),
            runtime: runtime.clone(),
        }
    }

    /// Wait for values to change, and name each. `None` once the runtime is
    /// gone.
    pub(crate) async fn next(&mut self) -> Option<Vec<RuntimeValue>> {
        loop {
            self.revisions.changed().await.ok()?;
            let revisions = *self.revisions.borrow_and_update();
            let mut changed = Vec::new();
            if revisions.runtime != self.read_at.runtime {
                let progress = self.runtime.identification_progress();
                if progress != self.progress {
                    self.progress = progress;
                    changed.push(RuntimeValue::IdentificationProgress {
                        identified: progress.0,
                        total: progress.1,
                    });
                }
                let importing = self.runtime.imports_in_flight();
                if importing != self.importing {
                    self.importing = importing;
                    changed.push(RuntimeValue::ImportsInFlight { count: importing });
                }
            }
            if revisions.signals != self.read_at.signals {
                let signals = self.runtime.all_signals();
                changed.extend(
                    signals
                        .iter()
                        .filter(|(key, signals)| self.signals.get(*key) != Some(*signals))
                        .map(|(key, signals)| RuntimeValue::Signals {
                            key: key.clone(),
                            signals: signals.clone(),
                        }),
                );
                self.signals = signals;
            }
            self.read_at = revisions;
            if !changed.is_empty() {
                return Some(changed);
            }
        }
    }
}
