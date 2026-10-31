//! Each candidate's extracted signals, as the import sidebar draws them off
//! the runtime: told as they stand and then as they change.

use super::{CandidateRuntime, Revisions};
use crate::import::CandidateKey;
use crate::signals::Signals;
use tokio::sync::watch;

/// One key's signals that changed.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RuntimeValue {
    pub(crate) key: CandidateKey,
    pub(crate) signals: Signals,
}

pub(crate) struct RuntimeValuesWatch {
    revisions: watch::Receiver<Revisions>,
    /// How far through the signals' changes the reader has been told.
    signals_seen: u64,
    runtime: CandidateRuntime,
}

impl RuntimeValuesWatch {
    /// The signals, told first as they stand and then as they change.
    pub(crate) fn of(runtime: &CandidateRuntime) -> Self {
        let mut revisions = runtime.watch_revisions();
        revisions.mark_changed();
        Self {
            revisions,
            signals_seen: 0,
            runtime: runtime.clone(),
        }
    }

    /// Wait for signals to change, and name each. `None` once the runtime is
    /// gone.
    pub(crate) async fn next(&mut self) -> Option<Vec<RuntimeValue>> {
        loop {
            self.revisions.changed().await.ok()?;
            self.revisions.borrow_and_update();
            let (seen, moved) = self.runtime.signals_since(self.signals_seen);
            self.signals_seen = seen;
            if !moved.is_empty() {
                return Some(
                    moved
                        .into_iter()
                        .map(|(key, signals)| RuntimeValue { key, signals })
                        .collect(),
                );
            }
        }
    }
}
