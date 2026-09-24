//! One candidate's runtime facts, followed as they change — what a row's live
//! state and a pane's detail read, each for its own key.

use super::ImportServiceHandle;
use crate::import::triage::{CandidateActionBasis, CandidateLiveState, TriageRuntimeFacts};
use crate::import::CandidateRuntimeChange;
use tokio::sync::broadcast;

/// One candidate's runtime facts, kept current from the runtime stream.
pub(crate) struct CandidateFactsWatch {
    key: String,
    /// The facts as they stand.
    facts: TriageRuntimeFacts,
    changes: broadcast::Receiver<CandidateRuntimeChange>,
    /// What a lagged stream re-reads the key's runtime from.
    import: ImportServiceHandle,
}

impl CandidateFactsWatch {
    pub(crate) fn facts(&self) -> &TriageRuntimeFacts {
        &self.facts
    }

    /// Wait for the key's facts to change, and return them. A change to
    /// another key, or to a part of this key's runtime the facts do not read —
    /// a progress tick within a running import — is passed over. `None` once
    /// the runtime stream has closed.
    pub(crate) async fn changed(&mut self) -> Option<TriageRuntimeFacts> {
        loop {
            let runtime = match self.changes.recv().await {
                Ok(CandidateRuntimeChange::Updated { key, runtime }) => {
                    if key != self.key {
                        continue;
                    }
                    Some(runtime)
                }
                Ok(CandidateRuntimeChange::Removed { key }) => {
                    if key != self.key {
                        continue;
                    }
                    None
                }
                Ok(CandidateRuntimeChange::Reset { mut runtimes }) => runtimes.remove(&self.key),
                Err(broadcast::error::RecvError::Lagged(count)) => {
                    tracing::warn!(
                        "{} dropped {count} runtime changes; re-reading its runtime",
                        self.key
                    );
                    self.import.candidate_runtime(&self.key)
                }
                Err(broadcast::error::RecvError::Closed) => return None,
            };
            let next = runtime
                .as_ref()
                .map(TriageRuntimeFacts::of)
                .unwrap_or_default();
            if next != self.facts {
                self.facts = next.clone();
                return Some(next);
            }
        }
    }
}

impl ImportServiceHandle {
    /// One candidate's facts as they stand, and each later change to them. The
    /// runtime stream is taken before the facts are read, so no change lands
    /// between the two.
    pub(crate) fn watch_candidate_facts(&self, key: String) -> CandidateFactsWatch {
        let (initial, changes) = self.subscribe_candidate_runtime();
        CandidateFactsWatch {
            facts: initial
                .get(&key)
                .map(TriageRuntimeFacts::of)
                .unwrap_or_default(),
            key,
            changes,
            import: self.clone(),
        }
    }

    /// What is running for one candidate, and the commands its row offers
    /// with it, now and on every change to either. A progress tick within a
    /// running import changes neither and delivers nothing.
    ///
    /// `basis` is what the row the caller draws says about the candidate in
    /// the tables; a row the list re-delivers with a different one subscribes
    /// again. Ends when the receiver is dropped.
    pub fn subscribe_candidate_live_state(
        &self,
        key: String,
        basis: CandidateActionBasis,
    ) -> tokio::sync::mpsc::UnboundedReceiver<CandidateLiveState> {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let mut watch = self.watch_candidate_facts(key);
        self.runtime_handle.spawn(async move {
            if tx
                .send(CandidateLiveState::of(&basis, watch.facts().clone()))
                .is_err()
            {
                return;
            }
            loop {
                let facts = tokio::select! {
                    () = tx.closed() => return,
                    facts = watch.changed() => match facts {
                        Some(facts) => facts,
                        None => return,
                    },
                };
                if tx.send(CandidateLiveState::of(&basis, facts)).is_err() {
                    return;
                }
            }
        });
        rx
    }
}
