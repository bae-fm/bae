//! Every candidate's runtime facts, followed as they change — what the import
//! list's live filters read.

use super::{CandidateRuntime, CandidateRuntimeChange, CandidateRuntimeSnapshot};
use crate::import::triage::TriageRuntimeFacts;
use std::collections::HashMap;
use tokio::sync::broadcast;

/// The runtime facts of every candidate the runtime holds, kept current from
/// its stream.
pub(crate) struct RuntimeFactsWatch {
    facts: HashMap<String, TriageRuntimeFacts>,
    changes: broadcast::Receiver<CandidateRuntimeChange>,
    /// What a lagged stream re-reads every runtime from.
    runtime: CandidateRuntime,
}

impl RuntimeFactsWatch {
    /// Every candidate's facts in `runtime` as they stand, and each later
    /// change to them. The stream is taken before the facts are read, so no
    /// change lands between the two.
    pub(crate) fn of(runtime: &CandidateRuntime) -> Self {
        let changes = runtime.subscribe();
        Self {
            facts: facts_of(&runtime.all()),
            changes,
            runtime: runtime.clone(),
        }
    }

    pub(crate) fn facts(&self) -> &HashMap<String, TriageRuntimeFacts> {
        &self.facts
    }

    /// Wait for any candidate's facts to change. A change to a part of a
    /// runtime the facts do not read is passed over. `false` once the runtime
    /// stream has closed.
    pub(crate) async fn changed(&mut self) -> bool {
        loop {
            let changed = match self.changes.recv().await {
                Ok(CandidateRuntimeChange::Updated { key, runtime }) => {
                    let facts = TriageRuntimeFacts::of(&runtime);
                    self.facts.get(&key) != Some(&facts) && {
                        self.facts.insert(key, facts);
                        true
                    }
                }
                Ok(CandidateRuntimeChange::Removed { key }) => self.facts.remove(&key).is_some(),
                Ok(CandidateRuntimeChange::Reset { runtimes }) => self.replace(facts_of(&runtimes)),
                Err(broadcast::error::RecvError::Lagged(count)) => {
                    tracing::warn!("the import list dropped {count} runtime changes; re-reading");
                    self.replace(facts_of(&self.runtime.all()))
                }
                Err(broadcast::error::RecvError::Closed) => return false,
            };
            if changed {
                return true;
            }
        }
    }

    /// Take `next` as the facts; whether they differ from what was held.
    fn replace(&mut self, next: HashMap<String, TriageRuntimeFacts>) -> bool {
        let changed = next != self.facts;
        self.facts = next;
        changed
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
