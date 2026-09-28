//! Every candidate's runtime for a reader that draws each key: all of it
//! first, then each key that changed since it last read.

use super::{CandidateRuntime, CandidateRuntimeChange, CandidateRuntimeSnapshot, Revisions};
use std::collections::HashMap;
use tokio::sync::watch;

pub struct RuntimeSnapshotsWatch {
    /// What the reader was last told; `None` before the first read.
    told: Option<HashMap<String, CandidateRuntimeSnapshot>>,
    revisions: watch::Receiver<Revisions>,
    read_at: u64,
    runtime: CandidateRuntime,
}

impl RuntimeSnapshotsWatch {
    pub(crate) fn of(runtime: &CandidateRuntime) -> Self {
        let revisions = runtime.watch_revisions();
        let read_at = revisions.borrow().runtime;
        Self {
            told: None,
            revisions,
            read_at,
            runtime: runtime.clone(),
        }
    }

    /// Every key's runtime as one `Reset` on the first call; after that, wait
    /// for keys to change and name each one. `None` once the runtime is gone.
    pub async fn next(&mut self) -> Option<Vec<CandidateRuntimeChange>> {
        let Some(told) = &mut self.told else {
            let runtimes = self.runtime.all();
            self.told = Some(runtimes.clone());
            return Some(vec![CandidateRuntimeChange::Reset { runtimes }]);
        };
        loop {
            self.revisions.changed().await.ok()?;
            let revision = self.revisions.borrow_and_update().runtime;
            if revision == self.read_at {
                continue;
            }
            self.read_at = revision;
            let next = self.runtime.all();
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
