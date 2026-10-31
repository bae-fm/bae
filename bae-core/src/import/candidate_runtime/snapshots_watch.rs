//! Every candidate's runtime for a reader that draws each key: all of it
//! first, then each key that changed since it last read.

use super::{CandidateRuntime, CandidateRuntimeChange, RuntimeView};
use tokio::sync::watch;

pub struct RuntimeSnapshotsWatch {
    /// How far through the view's changes the reader has been told; `None`
    /// before the first read.
    seen: Option<u64>,
    view: watch::Receiver<RuntimeView>,
}

impl RuntimeSnapshotsWatch {
    pub(crate) fn of(runtime: &CandidateRuntime) -> Self {
        Self {
            seen: None,
            view: runtime.watch_view(),
        }
    }

    /// Every key's runtime as one `Reset` on the first call; after that, wait
    /// for keys to change and name each one, read from the keys the view
    /// says moved. `None` once the runtime is gone.
    pub async fn next(&mut self) -> Option<Vec<CandidateRuntimeChange>> {
        let Some(seen) = &mut self.seen else {
            let view = self.view.borrow_and_update();
            self.seen = Some(view.changes.sequence());
            return Some(vec![CandidateRuntimeChange::Reset {
                runtimes: view.snapshots.clone(),
            }]);
        };
        loop {
            self.view.changed().await.ok()?;
            let view = self.view.borrow_and_update();
            let changes: Vec<CandidateRuntimeChange> = view
                .changes
                .since(*seen)
                .map(|key| match view.snapshots.get(key) {
                    Some(runtime) => CandidateRuntimeChange::Updated {
                        key: key.clone(),
                        runtime: runtime.clone(),
                    },
                    None => CandidateRuntimeChange::Removed { key: key.clone() },
                })
                .collect();
            *seen = view.changes.sequence();
            if !changes.is_empty() {
                return Some(changes);
            }
        }
    }
}
