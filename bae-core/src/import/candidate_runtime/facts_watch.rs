//! Every candidate's runtime facts, read again as the runtime changes — what
//! the import list joins to its rows, and what its filter entries narrow by
//! and count.

use super::{CandidateRuntime, CandidateRuntimeSnapshot, Revisions};
use crate::import::triage::TriageRuntimeFacts;
use std::collections::HashMap;
use tokio::sync::watch;

/// The runtime facts of every candidate the runtime holds.
pub(crate) struct RuntimeFactsWatch {
    facts: HashMap<String, TriageRuntimeFacts>,
    revisions: watch::Receiver<Revisions>,
    /// The runtime revision the facts were last read at.
    read_at: u64,
    runtime: CandidateRuntime,
}

impl RuntimeFactsWatch {
    /// Every candidate's facts in `runtime` as they stand, and each later
    /// change to them.
    pub(crate) fn of(runtime: &CandidateRuntime) -> Self {
        let revisions = runtime.watch_revisions();
        let read_at = revisions.borrow().runtime;
        Self {
            facts: facts_of(&runtime.all()),
            revisions,
            read_at,
            runtime: runtime.clone(),
        }
    }

    pub(crate) fn facts(&self) -> &HashMap<String, TriageRuntimeFacts> {
        &self.facts
    }

    /// Wait for any candidate's facts to change. A change to a part of a
    /// runtime the facts do not read is passed over. `false` once the runtime
    /// is gone.
    pub(crate) async fn changed(&mut self) -> bool {
        loop {
            if self.revisions.changed().await.is_err() {
                return false;
            }
            let revision = self.revisions.borrow_and_update().runtime;
            if revision == self.read_at {
                continue;
            }
            self.read_at = revision;
            let next = facts_of(&self.runtime.all());
            if next != self.facts {
                self.facts = next;
                return true;
            }
        }
    }
}

fn facts_of(
    runtimes: &HashMap<String, CandidateRuntimeSnapshot>,
) -> HashMap<String, TriageRuntimeFacts> {
    runtimes
        .iter()
        .map(|(key, runtime)| (key.clone(), TriageRuntimeFacts::of(runtime)))
        .collect()
}
