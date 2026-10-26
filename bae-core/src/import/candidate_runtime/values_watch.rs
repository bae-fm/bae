//! The values the import sidebar draws off the runtime: each candidate's
//! extracted signals and the two counts, told as they stand and then as they
//! change.

use super::{CandidateRuntime, Revisions, RuntimeView};
use crate::signals::Signals;
use std::collections::HashMap;
use std::sync::Arc;
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
    /// Every key's snapshot, which says which of them an import owns, and the
    /// identification count.
    view: watch::Receiver<Arc<RuntimeView>>,
    progress: (u32, u32),
    importing: u32,
    signals: HashMap<String, Signals>,
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
            signals: HashMap::new(),
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
                read = self.view.changed() => {
                    read.ok()?;
                    let view = self.view.borrow_and_update().clone();
                    if view.identification_progress != self.progress {
                        self.progress = view.identification_progress;
                        changed.push(RuntimeValue::IdentificationProgress {
                            identified: self.progress.0,
                            total: self.progress.1,
                        });
                    }
                    let importing = view
                        .snapshots
                        .values()
                        .filter(|snapshot| snapshot.import.is_some())
                        .count() as u32;
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
