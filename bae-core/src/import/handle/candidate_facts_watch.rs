//! One candidate's runtime facts, followed as they change — what a row's live
//! state and a pane's detail read, each for its own key.

use super::ImportServiceHandle;
use crate::import::triage::{CandidateActionBasis, CandidateLiveState, TriageRuntimeFacts};
use crate::import::candidate_runtime::{Revisions, RuntimeFactsWatch};
use tokio::sync::watch;

/// One candidate's runtime facts, read again whenever the runtime changes.
pub(crate) struct CandidateFactsWatch {
    key: String,
    /// The facts as they stand.
    facts: TriageRuntimeFacts,
    revisions: watch::Receiver<Revisions>,
    /// The runtime revision the facts were last read at.
    read_at: u64,
    import: ImportServiceHandle,
}

impl CandidateFactsWatch {
    pub(crate) fn facts(&self) -> &TriageRuntimeFacts {
        &self.facts
    }

    /// Watch `key` from now on, its facts read as they stand.
    pub(crate) fn set_key(&mut self, key: String) {
        self.facts = self.read(&key);
        self.key = key;
    }

    fn read(&self, key: &str) -> TriageRuntimeFacts {
        self.import
            .candidate_runtime(key)
            .as_ref()
            .map(TriageRuntimeFacts::of)
            .unwrap_or_default()
    }

    /// Wait for the key's facts to change, and return them. A change to
    /// another key, or to a part of this key's runtime the facts do not read —
    /// a progress tick within a running import — is passed over. `None` once
    /// the runtime is gone.
    pub(crate) async fn changed(&mut self) -> Option<TriageRuntimeFacts> {
        loop {
            self.revisions.changed().await.ok()?;
            let revision = self.revisions.borrow_and_update().runtime;
            if revision == self.read_at {
                continue;
            }
            self.read_at = revision;
            let next = self.read(&self.key);
            if next != self.facts {
                self.facts = next.clone();
                return Some(next);
            }
        }
    }
}

impl ImportServiceHandle {
    /// Every candidate's facts as they stand, and each later change to them.
    pub(crate) fn watch_runtime_facts(&self) -> RuntimeFactsWatch {
        RuntimeFactsWatch::of(&self.runtime)
    }

    /// One candidate's facts as they stand, and each later change to them. The
    /// runtime stream is taken before the facts are read, so no change lands
    /// between the two.
    pub(crate) fn watch_candidate_facts(&self, key: String) -> CandidateFactsWatch {
        let revisions = self.runtime.watch_revisions();
        let read_at = revisions.borrow().runtime;
        let mut watch = CandidateFactsWatch {
            facts: TriageRuntimeFacts::default(),
            key: String::new(),
            revisions,
            read_at,
            import: self.clone(),
        };
        watch.set_key(key);
        watch
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
