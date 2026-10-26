//! Every candidate's runtime for a reader that draws each key: all of it
//! first, then each key that changed since it last read.

use super::{CandidateRuntime, CandidateRuntimeChange, CandidateRuntimeSnapshot};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::watch;

pub struct RuntimeSnapshotsWatch {
    /// What the reader was last told; `None` before the first read.
    told: Option<Arc<HashMap<String, CandidateRuntimeSnapshot>>>,
    snapshots: watch::Receiver<Arc<HashMap<String, CandidateRuntimeSnapshot>>>,
}

impl RuntimeSnapshotsWatch {
    pub(crate) fn of(runtime: &CandidateRuntime) -> Self {
        Self {
            told: None,
            snapshots: runtime.watch_snapshots(),
        }
    }

    /// Every key's runtime as one `Reset` on the first call; after that, wait
    /// for keys to change and name each one. `None` once the runtime is gone.
    pub async fn next(&mut self) -> Option<Vec<CandidateRuntimeChange>> {
        let Some(told) = &mut self.told else {
            let runtimes = self.snapshots.borrow_and_update().clone();
            self.told = Some(runtimes.clone());
            return Some(vec![CandidateRuntimeChange::Reset {
                runtimes: (*runtimes).clone(),
            }]);
        };
        loop {
            self.snapshots.changed().await.ok()?;
            let next = self.snapshots.borrow_and_update().clone();
            let mut changes: Vec<CandidateRuntimeChange> = told
                .keys()
                .filter(|key| !next.contains_key(*key))
                .map(|key| CandidateRuntimeChange::Removed { key: key.clone() })
                .collect();
            changes.extend(
                next.iter()
                    .filter(|(key, runtime)| told.get(*key) != Some(*runtime))
                    .map(|(key, runtime)| CandidateRuntimeChange::Updated {
                        key: key.clone(),
                        runtime: runtime.clone(),
                    }),
            );
            *told = next;
            if !changes.is_empty() {
                return Some(changes);
            }
        }
    }
}
