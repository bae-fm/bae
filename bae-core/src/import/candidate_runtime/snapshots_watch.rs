//! Every candidate's runtime for a reader that draws each key: all of it
//! first, then each key that changed since it last read.

use super::{CandidateRuntime, CandidateRuntimeChange, RuntimeView};
use std::sync::Arc;
use tokio::sync::watch;

pub struct RuntimeSnapshotsWatch {
    /// What the reader was last told; `None` before the first read.
    told: Option<Arc<RuntimeView>>,
    view: watch::Receiver<Arc<RuntimeView>>,
}

impl RuntimeSnapshotsWatch {
    pub(crate) fn of(runtime: &CandidateRuntime) -> Self {
        Self {
            told: None,
            view: runtime.watch_view(),
        }
    }

    /// Every key's runtime as one `Reset` on the first call; after that, wait
    /// for keys to change and name each one. `None` once the runtime is gone.
    pub async fn next(&mut self) -> Option<Vec<CandidateRuntimeChange>> {
        let Some(told) = &mut self.told else {
            let view = self.view.borrow_and_update().clone();
            self.told = Some(view.clone());
            return Some(vec![CandidateRuntimeChange::Reset {
                runtimes: view.snapshots.clone(),
            }]);
        };
        loop {
            self.view.changed().await.ok()?;
            let next = self.view.borrow_and_update().clone();
            let mut changes: Vec<CandidateRuntimeChange> = told
                .snapshots
                .keys()
                .filter(|key| !next.snapshots.contains_key(*key))
                .map(|key| CandidateRuntimeChange::Removed { key: key.clone() })
                .collect();
            changes.extend(
                next.snapshots
                    .iter()
                    .filter(|(key, runtime)| told.snapshots.get(*key) != Some(*runtime))
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
